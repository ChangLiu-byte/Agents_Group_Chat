use crate::agent::Agent;
use crate::session::{now_unix, Session};
use sqlx::SqlitePool;
use tauri::State;
use uuid::Uuid;

/// Create a new chat room. Mode defaults to "sequential", status to "idle";
/// the agent roster starts empty (see [`add_agent_to_session`]).
#[tauri::command]
pub async fn create_session(pool: State<'_, SqlitePool>, topic: String) -> Result<Session, String> {
    let session = Session {
        id: Uuid::new_v4().to_string(),
        topic,
        mode: "sequential".to_string(),
        status: "idle".to_string(),
        created_at: now_unix(),
    };

    sqlx::query(
        r#"
        INSERT INTO chat_sessions (id, topic, mode, status, created_at)
        VALUES (?, ?, ?, ?, ?)
        "#,
    )
    .bind(&session.id)
    .bind(&session.topic)
    .bind(&session.mode)
    .bind(&session.status)
    .bind(session.created_at)
    .execute(pool.inner())
    .await
    .map_err(|e| e.to_string())?;

    Ok(session)
}

/// All chat rooms, most recently created first - backs the "past sessions"
/// list.
#[tauri::command]
pub async fn list_sessions(pool: State<'_, SqlitePool>) -> Result<Vec<Session>, String> {
    sqlx::query_as::<_, Session>(
        r#"
        SELECT id, topic, mode, status, created_at
        FROM chat_sessions
        ORDER BY created_at DESC
        "#,
    )
    .fetch_all(pool.inner())
    .await
    .map_err(|e| e.to_string())
}

/// A single chat room by id - used to refresh a session's `mode`/`status`
/// (e.g. after switching mode) without re-fetching the whole list.
#[tauri::command]
pub async fn get_session(pool: State<'_, SqlitePool>, session_id: String) -> Result<Session, String> {
    fetch_session(pool.inner(), &session_id).await
}

pub(crate) async fn fetch_session(pool: &SqlitePool, session_id: &str) -> Result<Session, String> {
    sqlx::query_as::<_, Session>(
        r#"
        SELECT id, topic, mode, status, created_at
        FROM chat_sessions
        WHERE id = ?
        "#,
    )
    .bind(session_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?
    .ok_or_else(|| format!("session {session_id} not found"))
}

/// Add an agent to a session's roster, placing it at the end of the
/// current speaking order.
#[tauri::command]
pub async fn add_agent_to_session(
    pool: State<'_, SqlitePool>,
    session_id: String,
    agent_id: String,
) -> Result<(), String> {
    let next_position: i64 = sqlx::query_scalar(
        r#"SELECT COALESCE(MAX(position), -1) + 1 FROM session_agents WHERE session_id = ?"#,
    )
    .bind(&session_id)
    .fetch_one(pool.inner())
    .await
    .map_err(|e| e.to_string())?;

    sqlx::query(
        r#"
        INSERT INTO session_agents (session_id, agent_id, position)
        VALUES (?, ?, ?)
        "#,
    )
    .bind(&session_id)
    .bind(&agent_id)
    .bind(next_position)
    .execute(pool.inner())
    .await
    .map_err(|e| e.to_string())?;

    Ok(())
}

/// Remove an agent from a session's roster. Does not touch past messages -
/// anything it already said stays in the transcript.
#[tauri::command]
pub async fn remove_agent_from_session(
    pool: State<'_, SqlitePool>,
    session_id: String,
    agent_id: String,
) -> Result<(), String> {
    sqlx::query(r#"DELETE FROM session_agents WHERE session_id = ? AND agent_id = ?"#)
        .bind(&session_id)
        .bind(&agent_id)
        .execute(pool.inner())
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

/// Rewrite the full speaking order for a session in one go - backs a
/// drag-and-drop (or up/down-arrow) reorder UI, which always has the
/// complete new order on hand rather than a single incremental move.
#[tauri::command]
pub async fn reorder_session_agents(
    pool: State<'_, SqlitePool>,
    session_id: String,
    ordered_agent_ids: Vec<String>,
) -> Result<(), String> {
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;

    for (position, agent_id) in ordered_agent_ids.iter().enumerate() {
        sqlx::query(
            r#"UPDATE session_agents SET position = ? WHERE session_id = ? AND agent_id = ?"#,
        )
        .bind(position as i64)
        .bind(&session_id)
        .bind(agent_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    }

    tx.commit().await.map_err(|e| e.to_string())?;

    Ok(())
}

/// Full agent configs for a session's roster, in speaking order - used to
/// render it in the UI.
#[tauri::command]
pub async fn list_session_agents(
    pool: State<'_, SqlitePool>,
    session_id: String,
) -> Result<Vec<Agent>, String> {
    fetch_roster(pool.inner(), &session_id).await
}

pub(crate) async fn fetch_roster(pool: &SqlitePool, session_id: &str) -> Result<Vec<Agent>, String> {
    sqlx::query_as::<_, Agent>(
        r#"
        SELECT a.id, a.name, a.provider, a.model, a.system_prompt, a.temperature, a.color, a.created_at
        FROM session_agents sa
        JOIN agents a ON a.id = sa.agent_id
        WHERE sa.session_id = ?
        ORDER BY sa.position ASC
        "#,
    )
    .bind(session_id)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())
}

/// Delete a session together with its roster and message history. Refused
/// while the session is "running" so an in-flight round can't be left
/// inserting messages for a session that no longer exists.
#[tauri::command]
pub async fn delete_session(
    pool: State<'_, SqlitePool>,
    session_id: String,
) -> Result<(), String> {
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;

    let status: Option<String> = sqlx::query_scalar("SELECT status FROM chat_sessions WHERE id = ?")
        .bind(&session_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;

    match status.as_deref() {
        None => return Err(format!("session {session_id} not found")),
        Some("running") => return Err("session is running, cannot delete it right now".to_string()),
        Some(_) => {}
    }

    // SQLite foreign keys aren't enforced (and have no ON DELETE CASCADE in
    // the schema), so children are deleted explicitly, child-first.
    sqlx::query("DELETE FROM messages WHERE session_id = ?")
        .bind(&session_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM session_agents WHERE session_id = ?")
        .bind(&session_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM chat_sessions WHERE id = ?")
        .bind(&session_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;

    tx.commit().await.map_err(|e| e.to_string())?;

    Ok(())
}

/// Switch a session between "sequential" and "mention" scheduling. Mention
/// mode's actual turn-taking logic isn't implemented yet - this only
/// persists the flag so the UI can be built against it now.
#[tauri::command]
pub async fn set_session_mode(
    pool: State<'_, SqlitePool>,
    session_id: String,
    mode: String,
) -> Result<(), String> {
    if mode != "sequential" && mode != "mention" {
        return Err(format!(
            "invalid mode \"{mode}\" (expected \"sequential\" or \"mention\")"
        ));
    }

    sqlx::query(r#"UPDATE chat_sessions SET mode = ? WHERE id = ?"#)
        .bind(&mode)
        .bind(&session_id)
        .execute(pool.inner())
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}
