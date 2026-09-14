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

    Ok(())
}
