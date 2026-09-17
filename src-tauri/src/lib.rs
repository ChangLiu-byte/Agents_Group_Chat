mod agent;
mod commands;
mod db;
mod providers;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let handle = app.handle().clone();
            // Pool init + migration must finish before any command can run,
            // so we block on it here inside setup() rather than spawning it.
            tauri::async_runtime::block_on(async move {
                let pool = db::init_pool(&handle)
                    .await
                    .expect("failed to open agents.db");
                db::run_migrations(&pool)
                    .await
                    .expect("failed to run agents table migration");
                handle.manage(pool);
            });
            // Shared HTTP client for all provider calls - reqwest::Client
            // holds a connection pool internally and is meant to be reused
            // rather than constructed per-request.
            app.manage(reqwest::Client::new());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::add_agent,
            commands::list_agents,
            commands::update_agent,
            commands::delete_agent,
            commands::test_agent_message,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
