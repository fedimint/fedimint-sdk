use std::sync::Arc;

use fedimint_client_rpc::{RpcGlobalState, RpcRequest, RpcResponse, RpcResponseHandler};
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
        let json = serde_json::to_string(&response).expect("Failed to serialize RPC response");
        self.0.on_response(json);
    }
}

async fn create_database(path: &str) -> anyhow::Result<Database> {
    tokio::fs::create_dir_all(path).await?;

    let db_path = std::path::Path::new(path).join(DB_DIR_NAME);
    let db = fedimint_rocksdb::RocksDb::build(db_path).open().await?;

    Ok(Database::new(db, Default::default()))
}
