//! API-level test of the attachment file routes: upload/download/index,
//! sha256 enforcement, dedup and auth — the file counterpart of the synced row.

use axum::body::{to_bytes, Body};
use axum::extract::connect_info::MockConnectInfo;
use axum::http::{Request, StatusCode};
use finance_api::{router, ApiState, PairingHub};
use finance_storage::db as dbm;
use serde_json::Value;
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
    let mut db = dbm::Db::open_memory(Some(&[7u8; 32])).unwrap();
    // A private attachments dir per test — files are never shared between runs.
    db.attachments_dir = std::env::temp_dir().join(format!("finance-api-att-{}", dbm::new_id()));
    let db = Arc::new(db);
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

fn sha256(b: &[u8]) -> String {
    use sha2::Digest;
    hex(&sha2::Sha256::digest(b))
}

/// Devices row whose token_hash matches Bearer "tok".
fn seed_device(db: &dbm::Db) {
    let hash = sha256(b"tok");
    db.with_conn(|c| {
        c.execute(
            "INSERT INTO devices (id, user_id, name, platform, token_hash, paired_at) VALUES (?1,NULL,'dev','test',?2,?3)",
            finance_storage::rusqlite::params!["dev-1", hash, dbm::now_iso()],
        )?;
        Ok(())
    })
    .unwrap();
}

async fn send(app: axum::Router, req: Request<Body>) -> (StatusCode, Vec<u8>) {
    let resp = app.oneshot(lan_req(req)).await.unwrap();
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (status, bytes.to_vec())
}

fn authed(req: axum::http::request::Builder) -> Request<Body> {
    req.header("authorization", "Bearer tok")
        .body(Body::empty())
        .unwrap()
}

#[tokio::test]
async fn attachments_roundtrip() {
    let (app, db) = app();
    seed_device(&db);
    let content = b"receipt-jpeg-bytes".to_vec();
    let sha = sha256(&content);

    let req = Request::put(format!("/api/v1/attachments/{sha}"))
        .header("authorization", "Bearer tok")
        .body(Body::from(content.clone()))
        .unwrap();
    let (status, _) = send(app.clone(), req).await;
    assert_eq!(status, StatusCode::OK);

    let (status, bytes) = send(
        app.clone(),
        authed(Request::get(format!("/api/v1/attachments/{sha}"))),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(bytes, content);

    let (status, bytes) = send(app, authed(Request::get("/api/v1/attachments"))).await;
    assert_eq!(status, StatusCode::OK);
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    assert!(
        body["shas"].as_array().unwrap().iter().any(|s| s == &sha),
        "index must list the uploaded sha: {body}"
    );
}

#[tokio::test]
async fn attachment_rejects_bad_name() {
    let (app, db) = app();
    seed_device(&db);
    let (status, _) = send(app, authed(Request::get("/api/v1/attachments/zzz"))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn attachment_put_hash_mismatch() {
    let (app, db) = app();
    seed_device(&db);
    // Valid sha format, but the digest of different bytes.
    let sha = sha256(b"other content");
    let req = Request::put(format!("/api/v1/attachments/{sha}"))
        .header("authorization", "Bearer tok")
        .body(Body::from(b"different bytes".to_vec()))
        .unwrap();
    let (status, _) = send(app, req).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn attachment_requires_auth() {
    let (app, db) = app();
    seed_device(&db);
    let sha = "a".repeat(64);
    let (status, _) = send(
        app,
        Request::get(format!("/api/v1/attachments/{sha}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn attachment_missing() {
    let (app, db) = app();
    seed_device(&db);
    let sha = sha256(b"never uploaded");
    let (status, _) = send(
        app,
        authed(Request::get(format!("/api/v1/attachments/{sha}"))),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn attachment_put_dedup() {
    let (app, db) = app();
    seed_device(&db);
    let content = b"same-photo".to_vec();
    let sha = sha256(&content);
    for _ in 0..2 {
        let req = Request::put(format!("/api/v1/attachments/{sha}"))
            .header("authorization", "Bearer tok")
            .body(Body::from(content.clone()))
            .unwrap();
        let (status, _) = send(app.clone(), req).await;
        assert_eq!(status, StatusCode::OK);
    }
}
