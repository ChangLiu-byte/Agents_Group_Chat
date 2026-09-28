use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use tauri::{AppHandle, Manager};

/// Open (creating if necessary) the SQLite database that lives under the
/// app's local data directory, e.g.
/// `%APPDATA%/<identifier>/agents.db` on Windows.
pub async fn init_pool(app: &AppHandle) -> Result<SqlitePool, String> {
    let app_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&app_dir).map_err(|e| e.to_string())?;

    let db_path = app_dir.join("agents.db");
    let connect_options = SqliteConnectOptions::new()
        .filename(&db_path)
        .create_if_missing(true);

    SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(connect_options)
        .await
        .map_err(|e| e.to_string())
}

/// No round can be in flight when the process has just started, so any
/// session still marked "running" was interrupted by a crash/kill. Without
/// this it would stay locked (can't send, can't delete) forever.
pub async fn reset_stale_running_sessions(pool: &SqlitePool) -> Result<(), String> {
    sqlx::query("UPDATE chat_sessions SET status = 'awaiting_user' WHERE status = 'running'")
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Create the `agents` table on startup if it doesn't already exist.
/// Simple "migrate on launch" approach - fine for a single-table app like
/// this; can be swapped for `sqlx::migrate!` later if the schema grows.
pub async fn run_migrations(pool: &SqlitePool) -> Result<(), String> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS agents (
            id             TEXT PRIMARY KEY NOT NULL,
            name           TEXT NOT NULL,
            provider       TEXT NOT NULL,
            model          TEXT NOT NULL,
            system_prompt  TEXT,
            temperature    REAL NOT NULL DEFAULT 0.7,
            color          TEXT,
            created_at     INTEGER NOT NULL
        )
        "#,
    )
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;

    // Phase 4: chat rooms ("sessions"), their agent roster, and messages.
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS chat_sessions (
            id          TEXT PRIMARY KEY NOT NULL,
            topic       TEXT NOT NULL,
            mode        TEXT NOT NULL DEFAULT 'sequential',
            status      TEXT NOT NULL DEFAULT 'idle',
            created_at  INTEGER NOT NULL
        )
        "#,
    )
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS session_agents (
            session_id  TEXT NOT NULL,
            agent_id    TEXT NOT NULL,
            position    INTEGER NOT NULL,
            PRIMARY KEY (session_id, agent_id),
            FOREIGN KEY (session_id) REFERENCES chat_sessions(id),
            FOREIGN KEY (agent_id) REFERENCES agents(id)
        )
        "#,
    )
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS messages (
            id            TEXT PRIMARY KEY NOT NULL,
            session_id    TEXT NOT NULL,
            agent_id      TEXT,
            round_number  INTEGER NOT NULL,
            content       TEXT NOT NULL,
            refers_to     TEXT,
            created_at    INTEGER NOT NULL,
            FOREIGN KEY (session_id) REFERENCES chat_sessions(id)
        )
        "#,
    )
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;

    // Phase 5: per-agent vision flag. `ADD COLUMN` isn't idempotent in
    // SQLite (a second launch would fail with "duplicate column name"), so
    // only run it when the column is actually missing. Defaults to 1 since
    // every model configured so far accepts images.
    let has_supports_vision: bool = sqlx::query_scalar(
        "SELECT COUNT(*) > 0 FROM pragma_table_info('agents') WHERE name = 'supports_vision'",
    )
    .fetch_one(pool)
    .await
    .map_err(|e| e.to_string())?;
    if !has_supports_vision {
        sqlx::query("ALTER TABLE agents ADD COLUMN supports_vision INTEGER NOT NULL DEFAULT 1")
            .execute(pool)
            .await
            .map_err(|e| e.to_string())?;
    }

    // Phase 5: images attached to (user) messages. Only the path relative to
    // the attachments folder is stored - never the bytes.
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS attachments (
            id          TEXT PRIMARY KEY,
            message_id  TEXT NOT NULL,
            file_path   TEXT NOT NULL,
            mime_type   TEXT NOT NULL,
            created_at  INTEGER NOT NULL,
            FOREIGN KEY (message_id) REFERENCES messages(id)
        )
        "#,
    )
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;

    // Every turn looks attachments up by message, so index that.
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_attachments_message ON attachments(message_id)")
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}
