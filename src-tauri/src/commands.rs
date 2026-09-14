use crate::agent::{self, Agent};
use sqlx::SqlitePool;
use tauri::State;

/// Insert a new agent config into SQLite and store its API key in the
/// OS credential manager.
#[tauri::command]
pub async fn add_agent(
    pool: State<'_, SqlitePool>,
    agent: Agent,
    api_key: String,
) -> Result<(), String> {
    sqlx::query(
        r#"
        INSERT INTO agents (id, name, provider, model, system_prompt, temperature, color, created_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&agent.id)
    .bind(&agent.name)
    .bind(&agent.provider)
    .bind(&agent.model)
    .bind(&agent.system_prompt)
    .bind(agent.temperature)
    .bind(&agent.color)
    .bind(agent.created_at)
    .execute(pool.inner())
    .await
    .map_err(|e| e.to_string())?;

    agent::save_api_key(&agent.id, &api_key)?;

    Ok(())
}

/// Return all configured agents (never includes API keys - those only
/// ever live in the OS credential manager).
#[tauri::command]
pub async fn list_agents(pool: State<'_, SqlitePool>) -> Result<Vec<Agent>, String> {
    sqlx::query_as::<_, Agent>(
        r#"
        SELECT id, name, provider, model, system_prompt, temperature, color, created_at
        FROM agents
        ORDER BY created_at ASC
        "#,
    )
    .fetch_all(pool.inner())
    .await
    .map_err(|e| e.to_string())
}

/// Update an existing agent's config. `api_key` is only touched when
/// `Some(..)` is provided (and non-empty), so editing name/prompt/etc.
/// doesn't force the user to re-enter their key.
#[tauri::command]
pub async fn update_agent(
    pool: State<'_, SqlitePool>,
    agent: Agent,
    api_key: Option<String>,
) -> Result<(), String> {
    sqlx::query(
        r#"
        UPDATE agents
        SET name = ?, provider = ?, model = ?, system_prompt = ?, temperature = ?, color = ?
        WHERE id = ?
        "#,
    )
    .bind(&agent.name)
    .bind(&agent.provider)
    .bind(&agent.model)
    .bind(&agent.system_prompt)
    .bind(agent.temperature)
    .bind(&agent.color)
    .bind(&agent.id)
    .execute(pool.inner())
    .await
    .map_err(|e| e.to_string())?;

    if let Some(key) = api_key {
        if !key.is_empty() {
            agent::save_api_key(&agent.id, &key)?;
        }
    }

    Ok(())
}

/// Delete an agent from both SQLite and the OS credential manager.
#[tauri::command]
pub async fn delete_agent(pool: State<'_, SqlitePool>, id: String) -> Result<(), String> {
    sqlx::query("DELETE FROM agents WHERE id = ?")
        .bind(&id)
        .execute(pool.inner())
        .await
        .map_err(|e| e.to_string())?;

    agent::delete_api_key(&id)?;

    Ok(())
}
