//! Pairing hub: two-phase /pair (code → pending request → host decision),
//! failed-attempt lock that burns active codes, and the LAN-only peer filter.

use axum::body::{to_bytes, Body};
use axum::extract::connect_info::MockConnectInfo;
use axum::http::{Request, StatusCode};
use finance_api::{is_lan_ip, router, ApiState, PairingHub, PAIR_MAX_FAILURES};
use finance_storage::db as dbm;
use finance_storage::migrations::SCHEMA_VERSION;
use serde_json::{json, Value};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use tower::ServiceExt;

static INIT: Mutex<()> = Mutex::new(());

fn app() -> (axum::Router, Arc<dbm::Db>, Arc<PairingHub>) {
    let _g = INIT.lock().unwrap();
    finance_storage::secrets::install_key_store(|_| Ok(Some(vec![7u8; 32])), |_, _| Ok(()));
    let db = Arc::new(dbm::Db::open_memory(Some(&[7u8; 32])).unwrap());
    let hub = Arc::new(PairingHub::new());
    (
        router(ApiState {
            db: db.clone(),
            hub: hub.clone(),
        }),
        db,
        hub,
    )
}

fn lan_req(req: Request<Body>, ip: IpAddr) -> Request<Body> {
    let mut req = req;
    req.extensions_mut()
        .insert(MockConnectInfo(SocketAddr::new(ip, 40000)));
    req
}

fn post(uri: &str, body: Value, ip: IpAddr) -> Request<Body> {
    lan_req(
        Request::post(uri)
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap(),
        ip,
    )
}

fn get(uri: &str, ip: IpAddr) -> Request<Body> {
    lan_req(Request::get(uri).body(Body::empty()).unwrap(), ip)
}

async fn send(app: &axum::Router, req: Request<Body>) -> (StatusCode, Value) {
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(json!({})))
}

const LAN: IpAddr = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 50));

/// device_pair_codes row for user "u1" — a far-future expires_at keeps the
/// RFC3339 string comparison true without a clock dependency. The users row
/// is needed because devices.user_id is a FK.
fn seed_code(db: &dbm::Db, code: &str) {
    db.with_conn(|c| {
        c.execute(
            "INSERT OR IGNORE INTO users (id, name, created_at, updated_at) VALUES ('u1','U',?1,?1)",
            [dbm::now_iso()],
        )?;
        c.execute(
            "INSERT INTO device_pair_codes (code, user_id, expires_at) VALUES (?1,'u1','9999-01-01T00:00:00Z')",
            [code],
        )?;
        Ok(())
    })
    .unwrap();
}

fn code_exists(db: &dbm::Db, code: &str) -> bool {
    db.with_conn(|c| {
        Ok(c.query_row(
            "SELECT COUNT(*) FROM device_pair_codes WHERE code=?1",
            [code],
            |r| r.get::<_, i64>(0),
        )? > 0)
    })
    .unwrap()
}

fn pair_body(code: &str) -> Value {
    json!({"code": code, "name": "Телефон", "platform": "android"})
}

#[tokio::test]
async fn pairing_approval_flow() {
    let (app, db, hub) = app();
    seed_code(&db, "123456");

    let (status, body) = send(&app, post("/api/v1/pair", pair_body("123456"), LAN)).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(body["status"], "pending");
    let rid = body["requestId"].as_str().unwrap();

    // The code is single-use and no device exists yet.
    assert!(!code_exists(&db, "123456"));
    let devices: i64 = db
        .with_conn(|c| Ok(c.query_row("SELECT COUNT(*) FROM devices", [], |r| r.get(0))?))
        .unwrap();
    assert_eq!(devices, 0);

    let (status, body) = send(
        &app,
        get(&format!("/api/v1/pair/status?requestId={rid}"), LAN),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "pending");
    assert_eq!(hub.pending().len(), 1);
    assert_eq!(hub.pending()[0].ip, "192.168.1.50");

    // Host approves → status hands out the token exactly once.
    hub.decide(&db, rid, true).unwrap();
    let (status, body) = send(
        &app,
        get(&format!("/api/v1/pair/status?requestId={rid}"), LAN),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "approved");
    let token = body["token"].as_str().unwrap();
    assert_eq!(body["userId"], "u1");
    assert!(!body["deviceId"].as_str().unwrap().is_empty());

    // The devices row's hash matches the handed-out token.
    use sha2::Digest;
    let hash = hex(&sha2::Sha256::digest(token.as_bytes()));
    let stored: String = db
        .with_conn(|c| {
            Ok(c.query_row(
                "SELECT token_hash FROM devices WHERE id=?1",
                [body["deviceId"].as_str().unwrap()],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    assert_eq!(stored, hash);

    // Second poll: already claimed → expired.
    let (status, body) = send(
        &app,
        get(&format!("/api/v1/pair/status?requestId={rid}"), LAN),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["status"], "expired");
}

#[tokio::test]
async fn pairing_deny_flow() {
    let (app, db, hub) = app();
    seed_code(&db, "654321");
    let (_, body) = send(&app, post("/api/v1/pair", pair_body("654321"), LAN)).await;
    let rid = body["requestId"].as_str().unwrap().to_string();

    hub.decide(&db, &rid, false).unwrap();
    let (status, body) = send(
        &app,
        get(&format!("/api/v1/pair/status?requestId={rid}"), LAN),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "denied");
    let devices: i64 = db
        .with_conn(|c| Ok(c.query_row("SELECT COUNT(*) FROM devices", [], |r| r.get(0))?))
        .unwrap();
    assert_eq!(devices, 0);
    // Denied is delivered once.
    let (status, _) = send(
        &app,
        get(&format!("/api/v1/pair/status?requestId={rid}"), LAN),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn pairing_attempt_limit_locks_and_burns() {
    let (app, db, hub) = app();
    seed_code(&db, "111111");

    for left in (1..PAIR_MAX_FAILURES).rev() {
        let (status, body) = send(&app, post("/api/v1/pair", pair_body("000000"), LAN)).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(body["error"], "bad_code");
        assert_eq!(body["attemptsLeft"], left);
    }
    // The 5th failure locks and burns the still-valid code.
    let (status, body) = send(&app, post("/api/v1/pair", pair_body("000000"), LAN)).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(body["error"], "pairing_locked");
    assert!(!code_exists(&db, "111111"));
    assert!(hub.locked());

    // Even the correct code is refused while locked.
    let (status, body) = send(&app, post("/api/v1/pair", pair_body("111111"), LAN)).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(body["error"], "pairing_locked");

    // A fresh code (the host reset the hub) unlocks pairing.
    hub.reset_failures();
    seed_code(&db, "222222");
    let (status, body) = send(&app, post("/api/v1/pair", pair_body("222222"), LAN)).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(body["status"], "pending");
}

#[tokio::test]
async fn pair_schema_mismatch() {
    let (app, _db, _hub) = app();
    for client in [SCHEMA_VERSION - 1, SCHEMA_VERSION + 1] {
        let mut body = pair_body("whatever");
        body["schemaVersion"] = json!(client);
        let (status, resp) = send(&app, post("/api/v1/pair", body, LAN)).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(resp["hostSchema"], SCHEMA_VERSION);
        assert_eq!(resp["clientSchema"], client);
    }
    // Matching schema proceeds to the code check.
    let mut body = pair_body("whatever");
    body["schemaVersion"] = json!(SCHEMA_VERSION);
    let (status, _) = send(&app, post("/api/v1/pair", body, LAN)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn peer_filter_blocks_non_lan() {
    let (app, _db, _hub) = app();
    for ip in [
        IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8)),
        IpAddr::V4(Ipv4Addr::new(100, 64, 1, 1)),
    ] {
        let (status, body) = send(&app, get("/api/v1/hello", ip)).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{ip}");
        assert_eq!(body["error"], "not_lan");
    }
    for ip in [
        IpAddr::V4(Ipv4Addr::new(192, 168, 1, 50)),
        IpAddr::V4(Ipv4Addr::new(10, 0, 0, 2)),
        IpAddr::V4(Ipv4Addr::new(172, 20, 0, 5)),
        IpAddr::V4(Ipv4Addr::LOCALHOST),
    ] {
        let (status, _) = send(&app, get("/api/v1/hello", ip)).await;
        assert_eq!(status, StatusCode::OK, "{ip}");
    }
}

/// 20 concurrent wrong guesses must still hit exactly 5 counted failures:
/// the whole attempt is serialized by the db mutex inside one with_conn.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pair_attempt_limit_under_concurrency() {
    let (app, db, _hub) = app();
    seed_code(&db, "333333");

    let mut set = tokio::task::JoinSet::new();
    for _ in 0..20 {
        let app = app.clone();
        set.spawn(async move {
            send(&app, post("/api/v1/pair", pair_body("000000"), LAN))
                .await
                .0
        });
    }
    let (mut bad, mut locked) = (0, 0);
    while let Some(s) = set.join_next().await {
        match s.unwrap() {
            StatusCode::FORBIDDEN => bad += 1,
            StatusCode::TOO_MANY_REQUESTS => locked += 1,
            other => panic!("unexpected status {other}"),
        }
    }
    assert_eq!(bad, 4, "expected exactly 4 bad_code responses");
    assert_eq!(locked, 16, "expected exactly 16 pairing_locked responses");

    // Locked: the once-valid code is burned and refused.
    let (status, _) = send(&app, post("/api/v1/pair", pair_body("333333"), LAN)).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert!(!code_exists(&db, "333333"));
}

/// A double decide must not mint two device rows.
#[test]
fn decide_twice_mints_one_device() {
    let _g = INIT.lock().unwrap();
    finance_storage::secrets::install_key_store(|_| Ok(Some(vec![7u8; 32])), |_, _| Ok(()));
    let db = dbm::Db::open_memory(Some(&[7u8; 32])).unwrap();
    db.with_conn(|c| {
        c.execute(
            "INSERT INTO users (id, name, created_at, updated_at) VALUES ('u1','U',?1,?1)",
            [dbm::now_iso()],
        )?;
        Ok(())
    })
    .unwrap();
    let hub = PairingHub::new();
    let id = hub.new_request("u1", "dev", "test", "192.168.1.9".into());

    hub.decide(&db, &id, true).unwrap();
    assert!(hub.decide(&db, &id, true).is_err());
    let devices: i64 = db
        .with_conn(|c| Ok(c.query_row("SELECT COUNT(*) FROM devices", [], |r| r.get(0))?))
        .unwrap();
    assert_eq!(devices, 1);
}

#[test]
fn is_lan_ip_cases() {
    let v4 = |a, b, c, d| IpAddr::V4(Ipv4Addr::new(a, b, c, d));
    for ip in [
        v4(127, 0, 0, 1),
        v4(10, 1, 2, 3),
        v4(172, 16, 0, 1),
        v4(172, 31, 255, 1),
        v4(192, 168, 0, 1),
        v4(169, 254, 5, 5),
        IpAddr::V6(Ipv6Addr::LOCALHOST),
        "fe80::1".parse().unwrap(),
        "fd12::1".parse().unwrap(),
        "::ffff:192.168.1.5".parse().unwrap(),
    ] {
        assert!(is_lan_ip(ip), "{ip} should be LAN");
    }
    for ip in [
        v4(8, 8, 8, 8),
        v4(100, 64, 0, 1),
        v4(0, 0, 0, 0),
        v4(172, 15, 0, 1),
        v4(172, 32, 0, 1),
        "2001:4860::1".parse().unwrap(),
        "::ffff:8.8.8.8".parse().unwrap(),
    ] {
        assert!(!is_lan_ip(ip), "{ip} should not be LAN");
    }
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}
