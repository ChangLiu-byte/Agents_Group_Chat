use super::session::{fetch_roster, fetch_session};
use crate::agent::{self, Agent};
use crate::providers::{self, ChatMessage, Role};
use crate::session::{now_unix, Message};
use serde::Serialize;
use sqlx::SqlitePool;
use std::collections::HashMap;
use tauri::{AppHandle, Emitter, State};
use uuid::Uuid;

/// Label prefixed to the user's messages in every agent's view of the
/// transcript.
const USER_LABEL: &str = "User";
/// Label for a message whose author has since been deleted from `agents`.
const REMOVED_AGENT_LABEL: &str = "已移除的Agent";

/// Appended to each agent's own system prompt while the session is in
/// sequential ("小组发言") mode, so replies stay short enough to read as a
/// live back-and-forth instead of everyone dumping an essay in turn.
const SEQUENTIAL_MODE_GUIDELINES: &str = "\
当前处于小组讨论轮次，请遵守：
- 别人的发言前面会有 [名字]: 标签，你回复时不需要给自己加标签
- 回答控制在200字以内，直接给出你的核心判断和理由，不要客套或重复前文
- 如果你的观点与已有发言基本一致，用一句话说明“同意XX的哪个具体论点”即可，不要重新论证一遍
- 如果前面的发言中存在遗漏的角度，或存在你不同意的地方，请明确指出并说明理由。
- 不需要总结或复述问题本身
- 如果这个问题需要展开详细论证，只需指出“这里有更深入的分析空间”，用户可以随后点名你详细展开";

/// The system prompt actually sent for one turn: the agent's own prompt
/// (unmodified) plus the sequential-mode guidelines, only while the session
/// is in sequential mode. Mention mode's turn-taking isn't built yet, so
/// this only ever fires today - but it's gated on `mode` rather than
/// unconditional so it doesn't silently start applying there too once it
/// is built.
fn effective_system_prompt(agent: &Agent, mode: &str) -> Option<String> {
    if mode != "sequential" {
        return agent.system_prompt.clone();
    }
    Some(match agent.system_prompt.as_deref() {
        Some(base) => format!("{base}\n\n{SEQUENTIAL_MODE_GUIDELINES}"),
        None => SEQUENTIAL_MODE_GUIDELINES.to_string(),
    })
}

/// Payload of the `agent-error` event: one agent's turn failed, so the round
/// moves on without a reply from it. Never persisted to `messages`.
#[derive(Clone, Serialize)]
struct AgentErrorPayload {
    session_id: String,
    round_number: i64,
    agent_id: String,
    agent_name: String,
    error: String,
}

/// Payload of the `round-complete` event.
#[derive(Clone, Serialize)]
struct RoundCompletePayload {
    session_id: String,
}

/// A failed emit only means the UI misses a live update (it can re-fetch
/// from the DB), so it must never abort a round.
fn emit_event<S: Serialize + Clone>(app: &AppHandle, event: &str, payload: S) {
    if let Err(e) = app.emit(event, payload) {
        eprintln!("failed to emit \"{event}\": {e}");
    }
}

async fn fetch_messages(pool: &SqlitePool, session_id: &str) -> Result<Vec<Message>, String> {
    // `created_at` only has one-second resolution and a round produces
    // several messages within a second, so `rowid` (insertion order) breaks
    // the ties.
    sqlx::query_as::<_, Message>(
        r#"
        SELECT id, session_id, agent_id, round_number, content, refers_to, created_at
        FROM messages
        WHERE session_id = ?
        ORDER BY created_at ASC, rowid ASC
        "#,
    )
    .bind(session_id)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())
}

/// Full message history of a session in order - loaded when a session is
/// opened; later messages arrive through the `message-added` event.
#[tauri::command]
pub async fn list_messages(
    pool: State<'_, SqlitePool>,
    session_id: String,
) -> Result<Vec<Message>, String> {
    fetch_messages(pool.inner(), &session_id).await
}

async fn insert_message(
    pool: &SqlitePool,
    session_id: &str,
    agent_id: Option<&str>,
    round_number: i64,
    content: &str,
    refers_to: Option<&str>,
) -> Result<Message, String> {
    let message = Message {
        id: Uuid::new_v4().to_string(),
        session_id: session_id.to_string(),
        agent_id: agent_id.map(str::to_string),
        round_number,
        content: content.to_string(),
        refers_to: refers_to.map(str::to_string),
        created_at: now_unix(),
    };

    sqlx::query(
        r#"
        INSERT INTO messages (id, session_id, agent_id, round_number, content, refers_to, created_at)
        VALUES (?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&message.id)
    .bind(&message.session_id)
    .bind(&message.agent_id)
    .bind(message.round_number)
    .bind(&message.content)
    .bind(&message.refers_to)
    .bind(message.created_at)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;

    Ok(message)
}

/// Turn the session transcript into the `ChatMessage` list for the agent
/// `current_agent_id` that is about to speak.
///
/// - Messages that agent wrote itself -> `assistant`, content untouched.
/// - Everything else (the user, every other agent) -> `user`, with the
///   content prefixed by the speaker's label: `[User]: ...` for the user,
///   `[<agent name>]: ...` for another agent.
fn build_history_for(
    current_agent_id: &str,
    messages: &[Message],
    agent_names: &HashMap<String, String>,
) -> Vec<ChatMessage> {
    messages
        .iter()
        .map(|m| match m.agent_id.as_deref() {
            Some(id) if id == current_agent_id => ChatMessage {
                role: Role::Assistant,
                content: m.content.clone(),
            },
            Some(id) => {
                let label = agent_names
                    .get(id)
                    .map(String::as_str)
                    .unwrap_or(REMOVED_AGENT_LABEL);
                ChatMessage {
                    role: Role::User,
                    content: format!("[{label}]: {}", m.content),
                }
            }
            None => ChatMessage {
                role: Role::User,
                content: format!("[{USER_LABEL}]: {}", m.content),
            },
        })
        .collect()
}

/// Because every other speaker shows up as `[Name]: ...` in the history,
/// models tend to imitate it and begin their own reply with `[TheirName]:`.
/// Drop that one leading self-label so it doesn't end up in the stored
/// transcript (and then get echoed back through everyone's history).
fn strip_self_label(reply: &str, agent_name: &str) -> String {
    let trimmed = reply.trim();
    for colon in [":", "："] {
        if let Some(rest) = trimmed.strip_prefix(&format!("[{agent_name}]{colon}")) {
            return rest.trim_start().to_string();
        }
    }
    trimmed.to_string()
}

async fn generate_reply(
    client: &reqwest::Client,
    agent: &Agent,
    mode: &str,
    history: &[Message],
    agent_names: &HashMap<String, String>,
) -> Result<String, String> {
    let api_key = agent::load_api_key(&agent.id)?;
    let chat_messages = build_history_for(&agent.id, history, agent_names);
    let system_prompt = effective_system_prompt(agent, mode);

    let reply = providers::send_chat_message(
        client,
        &agent.provider,
        &agent.model,
        system_prompt.as_deref(),
        &chat_messages,
        &api_key,
        agent.temperature as f32,
    )
    .await
    .map_err(|e| e.to_string())?;

    let reply = strip_self_label(&reply, &agent.name);
    if reply.is_empty() {
        return Err("provider returned an empty reply".to_string());
    }
    Ok(reply)
}

async fn fetch_agent_names(pool: &SqlitePool) -> Result<HashMap<String, String>, String> {
    let rows: Vec<(String, String)> = sqlx::query_as("SELECT id, name FROM agents")
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(rows.into_iter().collect())
}

/// Look up `target_agent_id` in the session's current roster, so a mention
/// round can't be aimed at an agent that was since removed from it.
fn find_mention_target<'a>(
    roster: &'a [Agent],
    target_agent_id: &str,
) -> Result<&'a Agent, String> {
    roster
        .iter()
        .find(|a| a.id == target_agent_id)
        .ok_or_else(|| format!("agent {target_agent_id} is not in this session's roster"))
}

/// Check-and-set in one statement so two near-simultaneous triggers (e.g. a
/// double click, or switching mode mid-flight) can't both start a round.
async fn claim_running(pool: &SqlitePool, session_id: &str) -> Result<(), String> {
    let claimed = sqlx::query(
        "UPDATE chat_sessions SET status = 'running' WHERE id = ? AND status != 'running'",
    )
    .bind(session_id)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?
    .rows_affected();
    if claimed == 0 {
        return Err("session is already running a round".to_string());
    }
    Ok(())
}

async fn release_to_awaiting_user(pool: &SqlitePool, session_id: &str) -> Result<(), String> {
    sqlx::query("UPDATE chat_sessions SET status = 'awaiting_user' WHERE id = ?")
        .bind(session_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

async fn next_round_number(pool: &SqlitePool, session_id: &str) -> Result<i64, String> {
    sqlx::query_scalar("SELECT COALESCE(MAX(round_number), 0) + 1 FROM messages WHERE session_id = ?")
        .bind(session_id)
        .fetch_one(pool)
        .await
        .map_err(|e| e.to_string())
}

/// The handful of things every step of a round needs - bundled so the
/// per-turn/per-round helpers below don't each need their own four-plus
/// parameters just to reach the DB and emit events.
struct RoundCtx<'a> {
    pool: &'a SqlitePool,
    client: &'a reqwest::Client,
    app: &'a AppHandle,
    session_id: &'a str,
}

/// One agent's turn: read the latest history, ask it for a reply, and
/// either store+emit it or (on failure) emit `agent-error` without storing
/// anything. Shared by the sequential loop (called once per roster agent)
/// and mention mode (called once for the mentioned agent).
async fn run_one_turn(
    ctx: &RoundCtx<'_>,
    mode: &str,
    round_number: i64,
    agent: &Agent,
    agent_names: &HashMap<String, String>,
) -> Result<(), String> {
    let history = fetch_messages(ctx.pool, ctx.session_id).await?;

    match generate_reply(ctx.client, agent, mode, &history, agent_names).await {
        Ok(reply) => {
            let row = insert_message(
                ctx.pool,
                ctx.session_id,
                Some(&agent.id),
                round_number,
                &reply,
                None,
            )
            .await?;
            emit_event(ctx.app, "message-added", &row);
        }
        Err(error) => {
            emit_event(
                ctx.app,
                "agent-error",
                AgentErrorPayload {
                    session_id: ctx.session_id.to_string(),
                    round_number,
                    agent_id: agent.id.clone(),
                    agent_name: agent.name.clone(),
                    error,
                },
            );
        }
    }

    Ok(())
}

/// Run one full sequential round: store the user's message, then let every
/// agent on the roster reply in `position` order, streaming each message to
/// the frontend as a `message-added` event. Finishes with `round-complete`
/// and leaves the session in "awaiting_user".
///
/// A single agent failing (bad key, HTTP error, empty reply, ...) is
/// reported through an `agent-error` event and does not stop the round.
#[tauri::command]
pub async fn run_sequential_round(
    pool: State<'_, SqlitePool>,
    client: State<'_, reqwest::Client>,
    app_handle: AppHandle,
    session_id: String,
    user_message: String,
) -> Result<(), String> {
    let user_message = user_message.trim();
    if user_message.is_empty() {
        return Err("message is empty".to_string());
    }

    let pool = pool.inner();
    let client = client.inner();

    let session = fetch_session(pool, &session_id).await?;
    if session.mode != "sequential" {
        return Err(format!(
            "session is in \"{}\" mode, run_sequential_round only works in \"sequential\" mode",
            session.mode
        ));
    }

    let roster = fetch_roster(pool, &session_id).await?;
    if roster.is_empty() {
        return Err("session has no agents, add at least one before sending".to_string());
    }

    claim_running(pool, &session_id).await?;
    let ctx = RoundCtx {
        pool,
        client,
        app: &app_handle,
        session_id: &session_id,
    };

    let outcome: Result<(), String> = async {
        let round_number = next_round_number(pool, &session_id).await?;
        let user_row =
            insert_message(pool, &session_id, None, round_number, user_message, None).await?;
        emit_event(&app_handle, "message-added", &user_row);

        let agent_names = fetch_agent_names(pool).await?;

        // Strictly one agent at a time: each one has to see what the
        // previous one just said, so the history is re-read before every
        // turn (inside `run_one_turn`).
        for agent in &roster {
            run_one_turn(&ctx, &session.mode, round_number, agent, &agent_names).await?;
        }

        Ok(())
    }
    .await;

    // Leave "running" no matter how the round ended, otherwise the session
    // would stay locked.
    let released = release_to_awaiting_user(pool, &session_id).await;

    emit_event(
        &app_handle,
        "round-complete",
        RoundCompletePayload {
            session_id: session_id.clone(),
        },
    );

    outcome?;
    released?;
    Ok(())
}

/// Run one mention-mode ("点名发言") round: store the user's message with
/// `refers_to` set to the mentioned agent, then let only that one agent
/// reply - everyone else on the roster stays silent this round. The
/// mentioned agent still sees the full shared transcript (including what
/// other agents said in earlier rounds), it just doesn't get a turn of its
/// own unless it's the one mentioned.
#[tauri::command]
pub async fn run_mention_round(
    pool: State<'_, SqlitePool>,
    client: State<'_, reqwest::Client>,
    app_handle: AppHandle,
    session_id: String,
    target_agent_id: String,
    user_message: String,
) -> Result<(), String> {
    let user_message = user_message.trim();
    if user_message.is_empty() {
        return Err("message is empty".to_string());
    }

    let pool = pool.inner();
    let client = client.inner();

    let session = fetch_session(pool, &session_id).await?;
    if session.mode != "mention" {
        return Err(format!(
            "session is in \"{}\" mode, run_mention_round only works in \"mention\" mode",
            session.mode
        ));
    }

    let roster = fetch_roster(pool, &session_id).await?;
    let target = find_mention_target(&roster, &target_agent_id)?.clone();

    claim_running(pool, &session_id).await?;
    let ctx = RoundCtx {
        pool,
        client,
        app: &app_handle,
        session_id: &session_id,
    };

    let outcome: Result<(), String> = async {
        let round_number = next_round_number(pool, &session_id).await?;
        let user_row = insert_message(
            pool,
            &session_id,
            None,
            round_number,
            user_message,
            Some(&target.id),
        )
        .await?;
        emit_event(&app_handle, "message-added", &user_row);

        let agent_names = fetch_agent_names(pool).await?;

        run_one_turn(&ctx, &session.mode, round_number, &target, &agent_names).await
    }
    .await;

    let released = release_to_awaiting_user(pool, &session_id).await;

    emit_event(
        &app_handle,
        "round-complete",
        RoundCompletePayload {
            session_id: session_id.clone(),
        },
    );

    outcome?;
    released?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(agent_id: Option<&str>, content: &str) -> Message {
        Message {
            id: "id".into(),
            session_id: "s".into(),
            agent_id: agent_id.map(str::to_string),
            round_number: 1,
            content: content.into(),
            refers_to: None,
            created_at: 0,
        }
    }

    fn agent(system_prompt: Option<&str>) -> Agent {
        Agent {
            id: "a1".into(),
            name: "Alice".into(),
            provider: "openai".into(),
            model: "gpt-4o".into(),
            system_prompt: system_prompt.map(str::to_string),
            temperature: 0.7,
            color: None,
            created_at: 0,
        }
    }

    fn names() -> HashMap<String, String> {
        HashMap::from([
            ("a1".to_string(), "Alice".to_string()),
            ("a2".to_string(), "Bob".to_string()),
        ])
    }

    #[test]
    fn own_messages_are_assistant_and_unprefixed() {
        let history = [msg(None, "topic?"), msg(Some("a1"), "my take")];
        let out = build_history_for("a1", &history, &names());
        assert_eq!(out[1].role, Role::Assistant);
        assert_eq!(out[1].content, "my take");
    }

    #[test]
    fn user_and_other_agents_are_user_role_with_labels() {
        let history = [msg(None, "topic?"), msg(Some("a1"), "alice says"), msg(Some("a2"), "bob says")];
        let out = build_history_for("a2", &history, &names());
        assert_eq!(out[0].role, Role::User);
        assert_eq!(out[0].content, "[User]: topic?");
        assert_eq!(out[1].role, Role::User);
        assert_eq!(out[1].content, "[Alice]: alice says");
        assert_eq!(out[2].role, Role::Assistant);
        assert_eq!(out[2].content, "bob says");
    }

    #[test]
    fn unknown_author_gets_fallback_label() {
        let history = [msg(Some("gone"), "hello")];
        let out = build_history_for("a1", &history, &names());
        assert_eq!(out[0].content, format!("[{REMOVED_AGENT_LABEL}]: hello"));
    }

    #[test]
    fn sequential_mode_appends_guidelines_after_existing_prompt() {
        let result = effective_system_prompt(&agent(Some("You are a pirate.")), "sequential").unwrap();
        assert!(result.starts_with("You are a pirate.\n\n"));
        assert!(result.contains("group discussion round"));
    }

    #[test]
    fn sequential_mode_with_no_own_prompt_is_just_the_guidelines() {
        let result = effective_system_prompt(&agent(None), "sequential").unwrap();
        assert_eq!(result, SEQUENTIAL_MODE_GUIDELINES);
    }

    #[test]
    fn non_sequential_mode_leaves_the_prompt_untouched() {
        assert_eq!(
            effective_system_prompt(&agent(Some("You are a pirate.")), "mention"),
            Some("You are a pirate.".to_string())
        );
        assert_eq!(effective_system_prompt(&agent(None), "mention"), None);
    }

    fn agent_with_id(id: &str, name: &str) -> Agent {
        Agent {
            id: id.into(),
            name: name.into(),
            provider: "openai".into(),
            model: "gpt-4o".into(),
            system_prompt: None,
            temperature: 0.7,
            color: None,
            created_at: 0,
        }
    }

    #[test]
    fn finds_mention_target_in_roster() {
        let roster = vec![agent_with_id("a1", "Alice"), agent_with_id("a2", "Bob")];
        let found = find_mention_target(&roster, "a2").unwrap();
        assert_eq!(found.name, "Bob");
    }

    #[test]
    fn rejects_mention_target_not_in_roster() {
        let roster = vec![agent_with_id("a1", "Alice")];
        let err = find_mention_target(&roster, "gone").unwrap_err();
        assert!(err.contains("gone"));

        let empty: Vec<Agent> = vec![];
        assert!(find_mention_target(&empty, "a1").is_err());
    }

    #[test]
    fn strips_only_own_leading_label() {
        assert_eq!(strip_self_label("[Alice]: hi", "Alice"), "hi");
        assert_eq!(strip_self_label("[Alice]：hi", "Alice"), "hi");
        assert_eq!(strip_self_label("  [Alice]:   hi  ", "Alice"), "hi");
        assert_eq!(strip_self_label("[Bob]: hi", "Alice"), "[Bob]: hi");
        assert_eq!(strip_self_label("I said [Alice]: hi", "Alice"), "I said [Alice]: hi");
        assert_eq!(strip_self_label("[Alice]:", "Alice"), "");
    }
}
