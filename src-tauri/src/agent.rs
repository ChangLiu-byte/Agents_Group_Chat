use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// One configured AI agent. Everything except the API key is persisted in
/// SQLite; the API key itself lives in the OS credential manager, keyed by
/// `id` (see [`save_api_key`] / [`delete_api_key`] below).
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Agent {
    pub id: String,
    pub name: String,
    /// "openai" | "anthropic" | "deepseek" | ... kept as a free-form string
    /// so new providers can be added later without a schema migration.
    pub provider: String,
    pub model: String,
    pub system_prompt: Option<String>,
    pub temperature: f64,
    pub color: Option<String>,
    /// Unix timestamp (seconds since epoch).
    pub created_at: i64,
}

/// Service name under which all agent API keys are grouped in the OS
/// credential manager (Windows Credential Manager / macOS Keychain / ...).
const SERVICE_NAME: &str = "multi-agent-group-chat";

/// Store (or overwrite) the API key for the given agent id.
pub fn save_api_key(agent_id: &str, api_key: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(SERVICE_NAME, agent_id).map_err(|e| e.to_string())?;
    entry.set_password(api_key).map_err(|e| e.to_string())
}

/// Remove the stored API key for the given agent id, if any.
/// Not having an entry to delete is not treated as an error.
pub fn delete_api_key(agent_id: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(SERVICE_NAME, agent_id).map_err(|e| e.to_string())?;
    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}
