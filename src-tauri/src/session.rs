use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::time::{SystemTime, UNIX_EPOCH};

/// Unix timestamp in seconds, as stored in every `created_at` column.
pub fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is before the unix epoch")
        .as_secs() as i64
}

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

/// One row of `messages`: either something the user said (`agent_id` is
/// `None`) or one agent's reply. Also the payload of the `message-added`
/// event.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Message {
    pub id: String,
    pub session_id: String,
    /// `None` means the user sent this message.
    pub agent_id: Option<String>,
    /// Every message of one round (the user's message that starts it plus
    /// each agent's reply) shares the same number, starting at 1.
    pub round_number: i64,
    pub content: String,
    /// Reserved for mention mode (an agent_id another agent's reply
    /// referred to); not used yet.
    pub refers_to: Option<String>,
    /// Unix timestamp (seconds since epoch).
    pub created_at: i64,
}
