use std::sync::Arc;

use fedimint_client_rpc::{
    RpcGlobalState, RpcRequest, RpcResponse, RpcResponseHandler, RpcResponseKind,
};
use fedimint_connectors::ConnectorRegistry;
use fedimint_core::db::Database;

// Force the linker to pull in our strong sdallocx stub from sdallocx_stub.c.
// On Android, aws-lc declares sdallocx as a weak symbol. The Android linker
// resolves weak GLOB_DAT entries to the PLT stub (non-NULL) while JUMP_SLOT
// stays 0 → SIGSEGV. Our stub provides a strong definition that delegates to free().
#[cfg(target_os = "android")]
extern "C" {
    fn sdallocx(ptr: *mut std::ffi::c_void, size: usize, flags: i32);
}

#[cfg(target_os = "android")]
#[used]
static FORCE_SDALLOCX: unsafe extern "C" fn(*mut std::ffi::c_void, usize, i32) = sdallocx;

uniffi::setup_scaffolding!();

const DB_DIR_NAME: &str = "fedimint_db";

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum FedimintError {
    #[error("Database initialization failed: {msg}")]
    DatabaseError { msg: String },

    #[error("Failed to initialize networking: {msg}")]
    NetworkingError { msg: String },

    #[error("Failed to create async runtime: {msg}")]
    RuntimeError { msg: String },

    #[error("Invalid request JSON: {msg}")]
    InvalidRequest { msg: String },

    #[error("General error: {msg}")]
    General { msg: String },
}

#[uniffi::export(callback_interface)]
pub trait RpcCallback: Send + Sync {
    fn on_response(&self, response_json: String);
}

#[derive(uniffi::Object)]
pub struct RpcHandler {
    state: Arc<RpcGlobalState>,
    runtime: tokio::runtime::Runtime,
}

#[uniffi::export]
impl RpcHandler {
    #[uniffi::constructor]
    pub fn new(db_path: String) -> Result<Arc<Self>, FedimintError> {
        let runtime = tokio::runtime::Runtime::new()
            .map_err(|e| FedimintError::RuntimeError { msg: e.to_string() })?;

        let state = runtime.block_on(async {
            let connectors = ConnectorRegistry::build_from_client_env()
                .map_err(|e| FedimintError::General { msg: e.to_string() })?
                .bind()
                .await
                .map_err(|e| FedimintError::NetworkingError { msg: e.to_string() })?;
            let db = create_database(&db_path)
                .await
                .map_err(|e| FedimintError::DatabaseError { msg: e.to_string() })?;

            Ok(Arc::new(RpcGlobalState::new(connectors, db)))
        })?;

        Ok(Arc::new(Self { state, runtime }))
    }

    pub fn rpc(
        &self,
        request_json: String,
        callback: Box<dyn RpcCallback>,
    ) -> Result<(), FedimintError> {
        let request: RpcRequest = serde_json::from_str(&request_json)
            .map_err(|e| FedimintError::InvalidRequest { msg: e.to_string() })?;

        let handled = self
            .state
            .clone()
            .handle_rpc(request, CallbackWrapper(callback));

        if let Some(task) = handled.task {
            self.runtime.spawn(task);
        }

        Ok(())
    }
}

struct CallbackWrapper(Box<dyn RpcCallback>);

impl RpcResponseHandler for CallbackWrapper {
    fn handle_response(&self, response: RpcResponse) {
        // With panic = "abort" in the release profile, panicking here would
        // take down the whole host app, so degrade to an error response on
        // the (currently impossible) serialization failure instead.
        let json = serde_json::to_string(&response).unwrap_or_else(|e| {
            let fallback = RpcResponse {
                request_id: response.request_id,
                kind: RpcResponseKind::Error {
                    error: format!("Failed to serialize RPC response: {e}"),
                },
            };
            serde_json::to_string(&fallback).unwrap_or_else(|_| {
                format!(
                    r#"{{"request_id":{},"type":"error","error":"unserializable RPC response"}}"#,
                    response.request_id
                )
            })
        });
        self.0.on_response(json);
    }
}

async fn create_database(path: &str) -> anyhow::Result<Database> {
    tokio::fs::create_dir_all(path).await?;

    let db_path = std::path::Path::new(path).join(DB_DIR_NAME);
    let db = fedimint_rocksdb::RocksDb::build(db_path).open().await?;

    Ok(Database::new(db, Default::default()))
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::time::Duration;

    use serde_json::{json, Value};

    use super::*;

    const RESPONSE_TIMEOUT: Duration = Duration::from_secs(60);

    /// The standard BIP-39 English test vector.
    const TEST_MNEMONIC: [&str; 12] = [
        "abandon", "abandon", "abandon", "abandon", "abandon", "abandon", "abandon", "abandon",
        "abandon", "abandon", "abandon", "about",
    ];

    struct ChannelCallback(mpsc::Sender<String>);

    impl RpcCallback for ChannelCallback {
        fn on_response(&self, response_json: String) {
            self.0.send(response_json).expect("test receiver dropped");
        }
    }

    fn new_handler(dir: &tempfile::TempDir) -> Arc<RpcHandler> {
        RpcHandler::new(dir.path().display().to_string()).expect("failed to create handler")
    }

    /// Sends `request` and collects the JSON responses up to and including the
    /// terminating `end` message.
    fn rpc_collect(handler: &RpcHandler, request: Value) -> Vec<Value> {
        let (tx, rx) = mpsc::channel();
        handler
            .rpc(request.to_string(), Box::new(ChannelCallback(tx)))
            .expect("request was rejected");

        let mut responses = Vec::new();
        loop {
            let json = rx.recv_timeout(RESPONSE_TIMEOUT).expect("no response");
            let response: Value = serde_json::from_str(&json).expect("invalid response JSON");
            let is_end = response["type"] == "end";
            responses.push(response);
            if is_end {
                return responses;
            }
        }
    }

    #[test]
    fn rejects_invalid_request_json() {
        let dir = tempfile::tempdir().unwrap();
        let handler = new_handler(&dir);
        let (tx, _rx) = mpsc::channel();

        let result = handler.rpc("not json".to_owned(), Box::new(ChannelCallback(tx)));

        assert!(matches!(result, Err(FedimintError::InvalidRequest { .. })));
    }

    #[test]
    fn fails_on_unusable_db_path() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("occupied");
        std::fs::write(&file, b"not a directory").unwrap();

        let result = RpcHandler::new(file.display().to_string());

        assert!(matches!(result, Err(FedimintError::DatabaseError { .. })));
    }

    #[test]
    fn mnemonic_round_trips_through_db() {
        let dir = tempfile::tempdir().unwrap();
        let handler = new_handler(&dir);

        let responses = rpc_collect(
            &handler,
            json!({ "request_id": 1, "type": "has_mnemonic_set" }),
        );
        assert_eq!(
            responses,
            vec![
                json!({ "request_id": 1, "type": "data", "data": false }),
                json!({ "request_id": 1, "type": "end" }),
            ]
        );

        let responses = rpc_collect(
            &handler,
            json!({ "request_id": 2, "type": "set_mnemonic", "words": TEST_MNEMONIC }),
        );
        assert_eq!(
            responses[0]["type"], "data",
            "unexpected response: {responses:?}"
        );

        let responses = rpc_collect(&handler, json!({ "request_id": 3, "type": "get_mnemonic" }));
        assert_eq!(responses[0]["data"]["mnemonic"], json!(TEST_MNEMONIC));

        let responses = rpc_collect(
            &handler,
            json!({ "request_id": 4, "type": "has_mnemonic_set" }),
        );
        assert_eq!(responses[0]["data"], json!(true));
    }

    /// Repro for the wasm peg_in-right-after-join crash (issue #330 /
    /// fedimint#9046): same flow as the TS integration test, natively, so a
    /// panic yields a symbolized backtrace.
    #[test]
    #[ignore = "needs a running devimint federation: set REPRO_INVITE_CODE and run with --ignored"]
    fn peg_in_right_after_join() {
        let invite = std::env::var("REPRO_INVITE_CODE")
            .expect("REPRO_INVITE_CODE must hold a devimint federation invite code");

        for round in 1..=3 {
            eprintln!("=== round {round}");
            let dir = tempfile::tempdir().unwrap();
            let handler = new_handler(&dir);
            let client_name = format!("{round:0>36}");

            rpc_collect(
                &handler,
                json!({ "request_id": 1, "type": "set_mnemonic", "words": TEST_MNEMONIC }),
            );
            let responses = rpc_collect(
                &handler,
                json!({
                    "request_id": 2,
                    "type": "join_federation",
                    "invite_code": invite,
                    "force_recover": false,
                    "client_name": client_name,
                }),
            );
            assert_eq!(responses[0]["type"], "data", "join failed: {responses:?}");

            // Deliberately no delay: this is the race under investigation.
            let responses = rpc_collect(
                &handler,
                json!({
                    "request_id": 3,
                    "type": "client_rpc",
                    "client_name": client_name,
                    "module": "wallet",
                    "method": "peg_in",
                    "payload": { "extra_meta": {} },
                }),
            );
            eprintln!("peg_in responses: {responses:?}");
            assert_eq!(responses[0]["type"], "data", "peg_in failed: {responses:?}");
        }
    }

    #[test]
    fn reports_errors_as_responses() {
        let dir = tempfile::tempdir().unwrap();
        let handler = new_handler(&dir);

        let responses = rpc_collect(
            &handler,
            json!({ "request_id": 1, "type": "parse_invite_code", "invite_code": "garbage" }),
        );

        assert_eq!(responses[0]["type"], "error");
        assert_eq!(responses[1]["type"], "end");
    }
}
