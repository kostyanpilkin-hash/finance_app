//! API-level test of the /push schema gate and the partial-row upsert:
//! an old client pushing a row without the v3 columns must not wipe them.

use axum::body::{to_bytes, Body};
use axum::extract::connect_info::MockConnectInfo;
use axum::http::{Request, StatusCode};
use finance_api::{router, ApiState, PairingHub};
use finance_storage::db as dbm;
use finance_storage::migrations::SCHEMA_VERSION;
use finance_storage::{repos, Actor, Entity, Source};
use serde_json::{json, Value};
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use tower::ServiceExt;

// Key store and tests share the process — install once.
static INIT: Mutex<()> = Mutex::new(());

/// All test traffic pretends to come from a LAN peer.
fn lan_req(req: Request<Body>) -> Request<Body> {
    let mut req = req;
    req.extensions_mut()
        .insert(MockConnectInfo(SocketAddr::from((
            Ipv4Addr::new(192, 168, 1, 50),
            40000,
        ))));
    req
}

fn app() -> (axum::Router, Arc<dbm::Db>) {
    let _g = INIT.lock().unwrap();
    finance_storage::secrets::install_key_store(|_| Ok(Some(vec![7u8; 32])), |_, _| Ok(()));
    let db = Arc::new(dbm::Db::open_memory(Some(&[7u8; 32])).unwrap());
    (
        router(ApiState {
            db: db.clone(),
            hub: Arc::new(PairingHub::new()),
        }),
        db,
    )
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Devices row whose token_hash matches Bearer "tok".
fn seed_device(db: &dbm::Db) {
    use sha2::Digest;
    let hash = hex(&sha2::Sha256::digest(b"tok"));
    db.with_conn(|c| {
        c.execute(
            "INSERT INTO devices (id, user_id, name, platform, token_hash, paired_at) VALUES (?1,NULL,'dev','test',?2,?3)",
            finance_storage::rusqlite::params!["dev-1", hash, dbm::now_iso()],
        )?;
        Ok(())
    })
    .unwrap();
}

fn seed_account(db: &dbm::Db) -> Value {
    let actor = Actor {
        user_id: None,
        device_id: Some(db.device_id.clone()),
        source: Source::Desktop,
    };
    db.with_conn(|c| {
        repos::account_upsert(
            c,
            db,
            &actor,
            json!({"name":"Приорбанк","kind":"card","last4":"1234","bank":"Приорбанк","paymentSystem":"visa","currency":"BYN"}),
        )
    })
    .unwrap()
}

async fn post_push(app: axum::Router, body: Value) -> (StatusCode, Value) {
    let resp = app
        .oneshot(lan_req(
            Request::post("/api/v1/push")
                .header("authorization", "Bearer tok")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        ))
        .await
        .unwrap();
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(json!({})))
}

#[tokio::test]
async fn push_without_schema_keeps_new_columns() {
    let (app, db) = app();
    seed_device(&db);
    let acc = seed_account(&db);
    // Old client payload: every column it knows, minus bank/paymentSystem.
    let mut payload = acc.clone();
    let m = payload.as_object_mut().unwrap();
    m.remove("bank");
    m.remove("paymentSystem");
    m.insert("name".into(), json!("Старое имя с клиента"));
    let body = json!({"ops": [{
        "entity": "accounts", "id": acc["id"], "op": "upsert",
        "baseVersion": acc["version"], "payload": payload,
    }]});
    let (status, resp) = post_push(app, body).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(resp["results"][0]["status"], "ok");
    let row = db
        .with_conn(|c| dbm::row_json(c, Entity::Accounts.table(), acc["id"].as_str().unwrap()))
        .unwrap()
        .unwrap();
    assert_eq!(row["name"], "Старое имя с клиента");
    assert_eq!(row["bank"], "Приорбанк");
    assert_eq!(row["paymentSystem"], "visa");
}

#[tokio::test]
async fn push_schema_gate() {
    let (app, db) = app();
    seed_device(&db);
    let acc = seed_account(&db);
    let op = json!({"ops": [{
        "entity": "accounts", "id": acc["id"], "op": "upsert",
        "baseVersion": acc["version"], "payload": acc,
    }]});

    for (client, err) in [
        (SCHEMA_VERSION - 1, "client_outdated"),
        (SCHEMA_VERSION + 1, "host_outdated"),
    ] {
        let mut body = op.clone();
        body["schemaVersion"] = json!(client);
        let (status, resp) = post_push(app.clone(), body).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(resp["error"], err);
        assert_eq!(resp["hostSchema"], SCHEMA_VERSION);
        assert_eq!(resp["clientSchema"], client);
    }

    let mut body = op.clone();
    body["schemaVersion"] = json!(SCHEMA_VERSION);
    let (status, resp) = post_push(app, body).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(resp["results"][0]["status"], "ok");
}

#[tokio::test]
async fn pull_pages_do_not_skip_changes() {
    let (app, db) = app();
    seed_device(&db);
    db.with_conn(|c| {
        let a = Actor {
            user_id: None,
            device_id: Some(db.device_id.clone()),
            source: Source::Desktop,
        };
        for n in ["A", "B", "C"] {
            repos::account_upsert(
                c,
                &db,
                &a,
                json!({"name": n, "kind": "cash", "currency": "BYN"}),
            )?;
        }
        Ok(())
    })
    .unwrap();
    let req = Request::get("/api/v1/pull?since=0&limit=2")
        .header("authorization", "Bearer tok")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(lan_req(req)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    let changes = body["changes"].as_array().unwrap();
    assert_eq!(changes.len(), 2);
    assert_eq!(body["maxSeq"], changes[1]["seq"]);
    assert_eq!(body["hasMore"], true);
}

#[tokio::test]
async fn repeated_conflicting_push_keeps_one_conflict() {
    let (app, db) = app();
    seed_device(&db);
    let acc = seed_account(&db);
    let body = json!({"ops": [{
        "entity": "accounts", "id": acc["id"], "op": "upsert",
        "baseVersion": 0, "payload": {"id": acc["id"], "name": "Local"},
    }]});
    for _ in 0..2 {
        let (status, resp) = post_push(app.clone(), body.clone()).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(resp["results"][0]["status"], "conflict");
    }
    let n: i64 = db
        .with_conn(|c| {
            Ok(c.query_row(
                "SELECT COUNT(*) FROM conflicts WHERE resolved_at IS NULL",
                [],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    assert_eq!(n, 1);
}

#[tokio::test]
async fn hello_reports_schema_version() {
    let (app, _db) = app();
    let resp = app
        .oneshot(lan_req(
            Request::get("/api/v1/hello").body(Body::empty()).unwrap(),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["schemaVersion"], SCHEMA_VERSION);
}

#[tokio::test]
async fn push_same_content_is_ok_not_conflict() {
    let (app, db) = app();
    seed_device(&db);
    let acc = seed_account(&db);
    // A replayed op carries the same data but stale base_version — must be
    // idempotent, not a conflict.
    let body = json!({"ops": [{
        "entity": "accounts", "id": acc["id"], "op": "upsert",
        "baseVersion": 0, "payload": acc,
    }]});
    let (status, resp) = post_push(app, body).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(resp["results"][0]["status"], "ok");
    let n: i64 = db
        .with_conn(|c| {
            Ok(c.query_row(
                "SELECT COUNT(*) FROM conflicts WHERE resolved_at IS NULL",
                [],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    assert_eq!(n, 0);
}

#[tokio::test]
async fn push_different_content_still_conflicts() {
    let (app, db) = app();
    seed_device(&db);
    let acc = seed_account(&db);
    let body = json!({"ops": [{
        "entity": "accounts", "id": acc["id"], "op": "upsert",
        "baseVersion": 0, "payload": {"id": acc["id"], "name": "Иное имя"},
    }]});
    let (status, resp) = post_push(app, body).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(resp["results"][0]["status"], "conflict");
}

#[tokio::test]
async fn exchange_rates_last_write_wins() {
    let (app, db) = app();
    seed_device(&db);
    // Seed one rate on the host, then push an upsert for the same date+pair
    // with a stale base_version — public data never conflicts.
    db.with_conn(|c| repos::rate_upsert(c, &db, "2030-01-01", "USD", 3000000, 1000000))
        .unwrap();
    let id = "2030-01-01|USD";
    let body = json!({"ops": [{
        "entity": "exchange_rates", "id": id, "op": "upsert",
        "baseVersion": 0,
        "payload": {"id": id, "date": "2030-01-01", "currency": "USD", "rateBynScaled": 3100000, "scale": 1000000},
    }]});
    let (status, resp) = post_push(app, body).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(resp["results"][0]["status"], "ok");
    let row = db
        .with_conn(|c| dbm::row_json(c, "exchange_rates", id))
        .unwrap()
        .unwrap();
    assert_eq!(row["rateBynScaled"], 3100000);
}
