# Multi-Agent Group Chat

A desktop app for running a group discussion between several AI models. You configure agents (each one a provider + model + persona), put them into a session around a topic, and they talk to you and to each other. Every agent sees the whole shared transcript, so it can agree with, build on, or push back against what the others said.

Built with **Tauri 2**, a **Rust** backend, and a **React + TypeScript** frontend. Everything (agents, sessions, messages, images) is stored locally; API keys are kept in the OS credential manager, never in the database.

## Features

- **Multiple providers in one room**: OpenAI, Anthropic, DeepSeek and Qwen, behind a single provider layer. Each agent has its own model, system prompt, temperature and color.
- **Two discussion modes**, switchable at any time from the input box:
  - **小组发言 (sequential)**: every agent on the roster replies in turn, in roster order. Each agent sees what the ones before it just said. One round = everyone speaks once.
  - **点名发言 (mention)**: you pick one agent and only that agent replies, still with the full shared history as context.
- **Session roster management**: add/remove agents per session and reorder who speaks first.
- **Image input**: attach images with the file picker or paste them with Ctrl+V; several per message, with preview and remove-before-send. PNG, JPEG, GIF and WebP are accepted (checked by extension, MIME type and file header).
- **Per-agent vision switch**: vision support differs between models and changes often, so it's a checkbox on each agent (「支持图片识别」) rather than something guessed from the model name. An agent with vision turned off never receives image data; it gets a short text note that an image was attached and that it can't see it, so it can say so honestly.
- **Live updates**: the backend runs the whole round and streams each reply to the UI as it arrives. If one agent fails (bad key, HTTP error, empty reply), the error is shown inline and the round continues with the next agent.
- **Persistent history**: sessions and transcripts survive restarts and can be reopened from the session list.

## Getting started

### Prerequisites

- [Node.js](https://nodejs.org/) (LTS) and npm
- [Rust](https://www.rust-lang.org/tools/install) (stable)
- The Tauri 2 system prerequisites for your OS: <https://tauri.app/start/prerequisites/> (on Windows: Microsoft C++ Build Tools and WebView2)
- An API key for at least one supported provider

### Run in development

```bash
npm install
npm run tauri dev
```

The first run compiles the Rust backend and takes a few minutes; later runs are incremental.

### Build an installer

```bash
npm run tauri build
```

The bundle is written to `src-tauri/target/release/bundle/`.

### Run the tests

```bash
cd src-tauri
cargo test
```

The Rust unit tests cover history building (speaker labels, vision gating, missing images), system prompt assembly, provider request serialization, and image validation.

## Using the app

1. **Agent 配置 tab**: add an agent. Pick a provider, type the model name exactly as the provider expects it (e.g. `gpt-4o`, `claude-sonnet-5`, `deepseek-flash`, `qwen3.7-plus`), paste the API key, and optionally write a system prompt. Uncheck 「支持图片识别」 for models that don't accept images. When editing an existing agent, leave the API key field empty to keep the stored key.
2. **会话 tab**: create a session with a topic, then open 管理名单 to add agents and set the speaking order.
3. Type a message (and/or attach images) and send. Switch between 小组发言 and 点名发言 with the buttons next to the send button. In mention mode, choose who to address from the chips above the text box.

## How it works

### A round, step by step

1. The frontend calls `run_sequential_round` or `run_mention_round` with the text and any images.
2. The backend validates the images, then atomically marks the session as `running` (so a double click can't start two rounds).
3. Images are written to the attachments folder, and the user message plus its attachment rows are inserted in one transaction.
4. For each agent that should reply, the backend:
   - re-reads the full transcript (so each agent sees the replies from earlier in the same round),
   - builds that agent's view of the history (see below),
   - assembles its system prompt,
   - calls the provider and stores the reply.
5. Each stored message is emitted as a `message-added` event; failures are emitted as `agent-error`. The round ends with `round-complete` and the session returns to `awaiting_user`.

If the app is killed mid-round, sessions left in `running` are reset on the next startup.

### Each agent's view of the conversation

Chat APIs only know two roles, `user` and `assistant`, so the shared group transcript is rewritten per agent:

- The agent's **own** earlier messages → `assistant`, unchanged.
- **Everyone else's** messages → `user`, prefixed with the speaker's name: `[User]: ...`, `[Alice]: ...`.
- Images are attached as image content parts if the agent supports vision; otherwise they're replaced by a text note. If an image file can't be found on disk, a "[N 张图片已丢失，无法加载]" note is added instead of failing the turn.

If a model imitates the format and starts its reply with its own `[Name]:` label, that prefix is stripped before the reply is stored.

### System prompt

The system prompt sent on each turn is built from:

1. **An identity directive** (always): tells the model its name in this chat, and that `[Name]:` messages come from other participants while only `assistant` messages are its own. Without this, models tended to answer as their default identity ("I'm Claude...") and confuse themselves with other agents.
2. **The agent's own system prompt**, if it has one.
3. **Sequential-mode guidelines** (sequential mode only): keep replies short, don't repeat what others said, point out disagreements and missed angles.

### Provider layer

`providers::send_chat_message` is the single entry point for all providers. OpenAI, DeepSeek and Qwen share an OpenAI-compatible request/response format; Anthropic has its own (separate `system` field, required `max_tokens`, content blocks). A text-only message is always sent with plain-string content, exactly as before image support existed. The content-block array form is used only for messages that contain an image, so models without vision support never receive a request shape they might reject.

## Data and storage

All local data lives in the app data directory (`%APPDATA%\com.刘畅.multi-agent-gourp-chat\` on Windows):

| What | Where |
|---|---|
| Agents, sessions, rosters, messages, attachment records | `agents.db` (SQLite) |
| Attached images (original files, named `<uuid>.<ext>`) | `attachments/` |
| API keys | OS credential manager (Windows Credential Manager / macOS Keychain), service `multi-agent-group-chat` |

Tables are created on startup (`db::run_migrations`); there are no separate migration files.

| Table | Purpose |
|---|---|
| `agents` | Agent config, including `supports_vision` |
| `chat_sessions` | Topic, mode (`sequential` / `mention`), status (`idle` / `running` / `awaiting_user`) |
| `session_agents` | Which agents are in which session, and their speaking order |
| `messages` | Transcript; `agent_id` is `NULL` for user messages, `refers_to` is the mentioned agent in mention mode |
| `attachments` | Images linked to a message; stores a path relative to `attachments/`, never the image bytes |

Deleting a session removes its messages and attachment rows; the image files are left on disk. Images are stored as-is, with no size limit or compression yet.

## Project structure

```
src/                            React frontend
  App.tsx                       Tabs: 会话 (sessions) / Agent 配置 (agents)
  components/
    AgentsPage.tsx              Agent list + add/edit form
    AgentForm.tsx, AgentCard.tsx
    TestPanel.tsx               Dev-only: send one message to one agent
    SessionsPage.tsx            Session list + create
    SessionDetail.tsx           Chat room: loads history, listens to events, sends
    SessionAgentRoster.tsx      Add/remove/reorder agents in a session
    ChatInput.tsx               Text box, mode switch, mention target, image attach/paste
    MessageList.tsx             Transcript with round dividers, images, inline errors
    AttachmentImage.tsx         Loads a stored image through Tauri's asset protocol
  types/                        TS mirrors of the Rust structs (keep in sync)
  utils/image.ts                Accepted image types, File -> base64

src-tauri/src/                  Rust backend
  lib.rs                        App setup, DB pool, command registration
  db.rs                         SQLite pool, schema creation, startup cleanup
  agent.rs                      Agent struct, API key storage (keyring)
  session.rs                    Session / Message structs
  attachment.rs                 Image validation, saving and loading
  commands/
    agent.rs                    Agent CRUD + test_agent_message
    session.rs                  Session and roster commands
    chat.rs                     Rounds, history building, system prompt assembly
  providers/
    mod.rs                      ChatMessage / ContentPart, send_chat_message dispatch
    openai_compatible.rs        OpenAI, DeepSeek, Qwen
    anthropic.rs                Anthropic
```

Design notes for earlier phases are in [PHASE4_PLAN.md](PHASE4_PLAN.md) (sessions and rounds) and [PHASE5_IMAGE_PLAN.md](PHASE5_IMAGE_PLAN.md) (images and vision), both in Chinese.

## Known limitations

- Replies are not streamed token by token; each agent's reply appears once it's complete.
- The full history, including every image, is resent on every turn, so token usage grows with session length.
- Images have no size limit; an image above a provider's limit makes that agent's turn fail with an HTTP error.
- The 「Agent 配置」 test panel sends only the agent's own system prompt, without the identity directive or mode guidelines.

## Recommended IDE setup

[VS Code](https://code.visualstudio.com/) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)
