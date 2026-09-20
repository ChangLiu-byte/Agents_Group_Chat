use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// One chat room. Persisted in `chat_sessions`; the agent roster lives in
/// the separate `session_agents` junction table (see
/// [`crate::commands::session::list_session_agents`]).
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Session {
    pub id: String,
    pub topic: String,
    /// "sequential" | "mention"
    pub mode: String,
    /// "idle" | "running" | "awaiting_user"
    pub status: String,
    /// Unix timestamp (seconds since epoch).
    pub created_at: i64,
}
