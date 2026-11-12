//! API-level test of the /push schema gate and the partial-row upsert:
//! an old client pushing a row without the v3 columns must not wipe them.

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use finance_api::{router, ApiState};
use finance_storage::db as dbm;
use finance_storage::migrations::SCHEMA_VERSION;
use finance_storage::{repos, Actor, Entity, Source};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use tower::ServiceExt;

// Key store and tests share the process — install once.
static INIT: Mutex<()> = Mutex::new(());

fn app() -> (axum::Router, Arc<dbm::Db>) {
    let _g = INIT.lock().unwrap();
    finance_storage::secrets::install_key_store(|_| Ok(Some(vec![7u8; 32])), |_, _| Ok(()));
    let db = Arc::new(dbm::Db::open_memory(Some(&[7u8; 32])).unwrap());
    (router(ApiState { db: db.clone() }), db)
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
        .oneshot(
            Request::post("/api/v1/push")
                .header("authorization", "Bearer tok")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
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
async fn hello_reports_schema_version() {
    let (app, _db) = app();
    let resp = app
        .oneshot(Request::get("/api/v1/hello").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["schemaVersion"], SCHEMA_VERSION);
}
