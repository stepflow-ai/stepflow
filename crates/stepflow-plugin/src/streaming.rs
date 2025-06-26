use dashmap::DashMap;
use once_cell::sync::Lazy;
use serde_json::Value;
use tokio::sync::mpsc;
use uuid::Uuid;

/// Maps execution_id → Sender<raw chunk JSON>.
/// This global registry allows streaming chunk handlers to route chunks without acquiring locks.
pub static STREAM_CHUNK_SENDERS: Lazy<DashMap<Uuid, mpsc::Sender<Value>>> =
    Lazy::new(DashMap::new);

/// Register a chunk sender for an execution ID
pub fn register_chunk_sender(execution_id: Uuid, sender: mpsc::Sender<Value>) {
    tracing::debug!("Registering chunk sender for execution {}", execution_id);
    STREAM_CHUNK_SENDERS.insert(execution_id, sender);
}

/// Unregister a chunk sender for an execution ID (cleanup)
pub fn unregister_chunk_sender(execution_id: Uuid) {
    tracing::debug!("Unregistering chunk sender for execution {}", execution_id);
    STREAM_CHUNK_SENDERS.remove(&execution_id);
}

/// Send a chunk to the registered sender for an execution ID
pub async fn send_chunk(execution_id: Uuid, chunk: Value) -> Result<(), String> {
    if let Some(sender) = STREAM_CHUNK_SENDERS.get(&execution_id) {
        sender.send(chunk).await
            .map_err(|e| format!("stream-chunk channel closed: {}", e))
    } else {
        Err(format!("no streaming channel for exec {}", execution_id))
    }
} 