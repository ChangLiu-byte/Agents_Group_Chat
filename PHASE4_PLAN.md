# Phase 4 实施计划：聊天室 + 顺序模式核心循环

> 本文档仅为现状梳理 + 实施方案，**未修改任何代码**，供你 review 后再动手实现。

## 1. 现状梳理

读完现有代码后，当前结构是：

```
src-tauri/src/
  main.rs          -- 入口，调用 lib::run()
  lib.rs           -- Builder 配置、DB 连接池/迁移初始化、invoke_handler 注册
  db.rs            -- SqlitePool 初始化 + "启动时建表" 迁移（目前只有 agents 表）
  agent.rs         -- Agent struct（FromRow）+ keyring 存取（save/load/delete_api_key）
  commands.rs      -- 5 个 tauri command：add/list/update/delete_agent + test_agent_message
  providers/
    mod.rs         -- send_chat_message 统一入口，ChatMessage/Role/ProviderError 定义
    openai_compatible.rs -- OpenAI/DeepSeek/Qwen 共享实现
    anthropic.rs         -- Anthropic 专属实现（system 字段单独传，且从不发 temperature）

src/
  App.tsx                 -- 唯一页面：Agent 配置列表 + 表单 + 一个临时 TestPanel
  types/agent.ts           -- Agent / AgentFormValues 类型，与 Rust struct 手动保持同步
  components/AgentForm.tsx / AgentCard.tsx / TestPanel.tsx
```

关键观察：

- 风格约定：Rust 侧每个领域一个文件（`agent.rs` 放 struct+keyring，`commands.rs` 放所有 tauri command），错误统一 `Result<T, String>`（`.map_err(|e| e.to_string())`），SQL 用 `sqlx::query`/`query_as` 手写，不用 `sqlx::migrate!`。前端每个组件一个文件，纯 state 驱动，没有路由库、没有全局状态库，Tailwind 4，中文 UI 文案。
- `db.rs::run_migrations` 是"启动时 `CREATE TABLE IF NOT EXISTS`"风格，新表直接加在这个函数里即可，不需要引入迁移框架。
- `reqwest::Client` 和 `SqlitePool`都是通过 `app.manage()` 放的全局 state，command 里用 `State<'_, T>` 取。新 command 也应该这样拿 `client`/`pool`，另外 `run_sequential_round` 还需要 `AppHandle` 用于 `emit`。
- 一个小的命名不一致：`agent.rs` 里 keyring 的 `SERVICE_NAME` 常量实际是 `"multi-agent-group-chat"`，跟你项目描述里写的 `"multi-agent-chat"` 不一样 —— 不影响功能，只是提一下，看是否要统一（不改的话本次也不影响 Phase 4）。
- 目前完全没有"页面切换"的概念，`App.tsx` 只渲染 Agent 配置这一个页面。Session 相关 UI 需要一个新的顶层导航方式。

## 2. Part 1：聊天室创建 + 名单管理

### 2.1 数据库（`db.rs::run_migrations`）

按你给的三张表原样加进 `run_migrations`（`CREATE TABLE IF NOT EXISTS` 风格，和现有 `agents` 表一致，不引入迁移框架）。

唯一想在这里标注的技术点：`messages.created_at` 是秒级 `INTEGER`。顺序模式一轮下来，多个 agent 的回复很可能落在同一秒内，单靠 `created_at` 排序会有平局（tie）。计划：查询历史时用 `ORDER BY created_at ASC, rowid ASC` 作为次级排序键（SQLite 普通表自带隐式 `rowid`，按插入顺序单调递增），不改 schema、不加新列。这个点见第 5 节的问题列表。

### 2.2 新增 Rust 文件：`session.rs`

跟 `agent.rs` 的风格对齐，放 struct 定义：

```rust
pub struct Session {
    pub id: String,
    pub topic: String,
    pub mode: String,      // "sequential" | "mention"
    pub status: String,    // "idle" | "running" | "awaiting_user"
    pub created_at: i64,
}

pub struct Message {
    pub id: String,
    pub session_id: String,
    pub agent_id: Option<String>,   // None = 用户发的
    pub round_number: i64,
    pub content: String,
    pub refers_to: Option<String>,
    pub created_at: i64,
}
```

### 2.3 新增 Tauri commands

按你列的清单实现，全部 `Result<_, String>` 风格：

- `create_session(topic) -> Session`
- `list_sessions() -> Vec<Session>`（`ORDER BY created_at DESC`）
- `add_agent_to_session(session_id, agent_id)`：`position = COALESCE(MAX(position), -1) + 1`（子查询）
- `remove_agent_from_session(session_id, agent_id)`
- `reorder_session_agents(session_id, ordered_agent_ids: Vec<String>)`：在一个事务里按数组下标依次 `UPDATE ... SET position = ? WHERE session_id=? AND agent_id=?`
- `list_session_agents(session_id) -> Vec<Agent>`：`JOIN session_agents ON agent_id ... ORDER BY position`
- `set_session_mode(session_id, mode)`：校验 `mode` 只能是 `"sequential"` / `"mention"`，否则直接 `Err`

另外我打算加两个你清单里没写但实现上必须有的command（不是需求膨胀，纯粹是前端要用）：

- `get_session(session_id) -> Session`：单个 session 详情刷新（比如 mode/status 变化后重新拉取一次），比整页重新 `list_sessions()` 再 find 更直接。
- `list_messages(session_id) -> Vec<Message>`：打开一个 session 时把历史一次性拉回来（进入页面时用，之后靠事件增量追加，不会重复拉）。

这两个要不要加，见第5节问题。

### 2.4 文件组织

`commands.rs` 现在 145 行、5 个command。Part1+Part2 加起来大约再加 8～9 个 command，其中 `run_sequential_round` 内部逻辑本身就不短。我倾向于拆成：

```
src-tauri/src/commands/
  mod.rs      -- pub use agent::*; pub use session::*; pub use chat::*;
  agent.rs    -- 现有 5 个 agent command，原样搬过去
  session.rs  -- session/roster 管理的 7 个 command
  chat.rs     -- run_sequential_round
```

`lib.rs` 里 `tauri::generate_handler![...]` 引用路径改一下即可，行为不变。是否要这么拆见问题列表（也可以保持单文件，等它真的太长再拆）。

### 2.5 前端

新增：

- `types/session.ts`：`Session` / `Message` 类型，手动跟 Rust struct 对齐（沿用现有项目里"手动同步"的方式，不引入 codegen）。
- `components/NewSessionForm.tsx`：topic 输入框 + agent 多选（checkbox list，数据来自已有的 `list_agents`）。
- `components/AgentRoster.tsx`：session 内的 agent 名单，支持上下箭头调整顺序（见 5.5 的问题：是否要上 dnd 库）+ 删除按钮 + "添加 agent" 入口。
- `components/ModeToggle.tsx`：sequential / mention 的切换开关，调 `set_session_mode`，纯前端 state 更新，不做额外校验（mention 模式下按你的要求先不接后续逻辑）。
- `App.tsx` 改动：目前只有一个页面，需要一个最简单的顶层视图切换（不引入 react-router，纯 `useState`）：`'agents' | 'sessions' | { kind: 'session-detail', id }`。Part 1 只做到 "sessions 列表 + 新建 + 名单管理"，不渲染消息列表（那是 Part 2）。

Part 1 验收方式：创建 session → 加/删/排序 agent → 手动重新加载页面（刷新等价物）→ 数据还在，跟你要求的一致。

## 3. Part 2：顺序模式核心循环

### 3.1 关键设计决定：历史消息的 role 映射规则（**请重点 review 这一段**）

我打算用下面这个**统一、无特例**的规则，构造发给"即将发言的 agent"的 `ChatMessage[]`：

对 session 里按 `created_at ASC, rowid ASC` 排好序的每一条历史消息 `m`：

- 如果 `m.agent_id == 当前即将发言的 agent 的 id` → `role: Assistant`，`content` 原样，**不加任何前缀**（这是它自己说过的话，就应该是纯粹的 assistant turn）。
- 其他所有情况（用户发的消息，或者别的 agent 发的消息）→ 统一 `role: User`，`content` 一律加上 `"[<speaker>]: "` 前缀：
  - 用户消息：`speaker = "User"` → `"[User]: <content>"`
  - 别的 agent 消息：`speaker = <该 agent 的 name>` → `"[Alice]: <content>"`（name 从 `agents` 表按 `agent_id` 查，不是 provider/model）

也就是说：**用户和其他 agent 都走同一条路径（都映射成 role=User，且都加前缀），唯一的特例是"这是不是我自己说的"**。

为什么这么选，而不是"只给别的 agent 加前缀、用户不加"：

1. 一致性更好：agent 拿到的每一条 `role=User` 的消息都有明确的 `[谁]:` 标签，不需要靠"没有前缀的就是用户"这种隐含约定去猜——尤其是以后如果要接入 mention 模式（一条消息可能是用户 at 某个 agent，也可能是模拟"用户转述别人的话"），统一前缀不会有歧义。
2. 对模型效果几乎没有代价：多数模型对 `[User]: xxx` 这种轻量前缀不敏感，反而更容易分清"房间里现在几个说话人在说什么"，这正是多 agent 群聊场景要解决的问题。
3. 实现更简单：不需要写"如果是 user 走一个格式、如果是 agent 走另一个格式"的分支，只有"是不是我自己"这一个判断点，减少出错空间。

需要一次性把 session 里出现过的所有 `agent_id -> name` 查出来做映射（一次 `SELECT id, name FROM agents WHERE id IN (...)`，或者更简单：直接用当前 roster 的 `list_session_agents` 结果建 map，因为发言历史里出现的 agent 一定还在 roster 里，除非中途被从名单移除——如果被移除了，用 `"[已移除的Agent]"` 兜底，不报错)。

失败的 agent 回复（见 3.3）不会进入 `messages` 表，所以不会出现在这段历史里，不需要单独处理"错误消息要不要发给下一个 agent"的问题。

### 3.2 `run_sequential_round` 命令流程

```
#[tauri::command]
async fn run_sequential_round(
    session_id: String,
    user_message: String,
    pool: State<'_, SqlitePool>,
    client: State<'_, reqwest::Client>,
    app_handle: AppHandle,
) -> Result<(), String>
```

步骤：

1. **原子性地**把 status 从非 running 切到 running：
   `UPDATE chat_sessions SET status='running' WHERE id=? AND status != 'running'`，检查 `rows_affected() == 1`，否则直接返回 `Err("session already running")`。用一句 UPDATE 做"检查+切换"而不是"先 SELECT 再 UPDATE"，避免竞态（虽然桌面单用户场景下风险很小，但双击发送按钮这种情况确实可能触发）。
   同时校验 session 的 `mode == "sequential"`，否则报错（mention 模式的循环逻辑本任务不实现）。
2. 算出这一轮的 `round_number`：`SELECT COALESCE(MAX(round_number), 0) + 1 FROM messages WHERE session_id=?`。
3. 插入用户消息（`agent_id = NULL`），`emit("message-added", &message)`。
4. `SELECT * FROM session_agents JOIN agents ... WHERE session_id=? ORDER BY position` 拿到发言顺序。
5. 按顺序 `for agent in roster`（**不用 `join_all`/`tokio::spawn`，就是普通 `for` + `.await`**，保证严格串行）：
   a. 重新拉取当前完整历史（`list_messages` 内部逻辑复用）。
   b. 按 3.1 的规则转换成 `ChatMessage[]`。
   c. 从 `agents` 表拿 provider/model/system_prompt/temperature，从 keyring 拿 api_key。
   d. 调 `providers::send_chat_message(...)`。
   e. 成功：插入一条 `messages` 行（`agent_id = 当前agent`，同一个 `round_number`），`emit("message-added", &message)`。
   f. 失败：**不写入 `messages` 表**，改成 `emit("agent-error", { session_id, round_number, agent_id, agent_name, error: e.to_string() })`，然后 `continue` 到下一个 agent —— 不 abort 整轮（按你的要求）。
6. 循环结束后，`UPDATE chat_sessions SET status='awaiting_user' WHERE id=?`，`emit("round-complete", { session_id })`。

第 5f 里"错误不落库"是一个需要你确认的设计选择，好处和代价见第 5 节。

### 3.3 前端

- `components/SessionChatView.tsx`：
  - 挂载时调 `list_messages` 拉历史 + `list_session_agents` 拿 roster（用于按 `agent_id` 查 name/color 渲染气泡），再 `listen("message-added", ...)` / `listen("agent-error", ...)` / `listen("round-complete", ...)` 三个事件订阅，卸载时 `unlisten`。
  - 消息列表按 `round_number` 分组，组间加一条细分割线（nice to have）。
  - 输入框 + 发送按钮：`status === "running"` 时 disabled。发送调 `run_sequential_round`，不等它 resolve 就已经能看到用户消息（因为那条消息也是通过 `message-added` 事件来的，跟 agent 回复走同一条路径，UI 逻辑更统一）。
  - `agent-error` 事件收到后，在对应位置渲染一个临时的"❌ Alice 没有回复：<error>"气泡，这个气泡只存在于当前这次运行的前端 state 里，不落库、刷新后消失（对应 3.2 步骤 5f 的选择）。
  - round 完成后（`round-complete`）：显示"继续下一轮 / 切换模式 / 结束会话"三个按钮。"继续下一轮"就是清空输入框重新调用同一个发送路径。"结束会话"目前没有专门的后端状态（`status` 只有 idle/running/awaiting_user），大概率就是"返回 session 列表"，不需要新状态或新 command——除非你想要一个显式的 `ended` 状态，见问题列表。

## 4. Tauri 事件相关的一个待验证点

`capabilities/default.json` 目前只有 `core:default` + `opener:default`。Tauri v2 里 `emit`/前端 `listen` 属于 `core:event:*` 权限，一般包含在 `core:default` 里，但没有在这台机器上实跑验证过——实现的时候如果发现前端 `listen` 收不到事件，需要在 `default.json` 的 `permissions` 里显式加 `"core:event:default"`。先记一下，不代表现在要改。

## 5. 需要你确认的问题

1. **role 映射规则**（第 3.1 节）：用户消息和其他 agent 消息统一走 `role=User` + `"[<speaker>]: "` 前缀，当前 agent 自己的历史消息走 `role=Assistant` 不加前缀。确认这个方案，还是想要"用户不加前缀、只有别的 agent 加前缀"？
2. **失败回复不落库**（3.2 步骤 5f）：好处是不会污染发给后续 agent/下一轮的对话历史；代价是错误提示只存在于当次前端 state，刷新页面或重新打开 session 后错误就"消失"了（只是不再显示，不影响实际数据）。可以接受，还是想给 `messages` 表加一个 `is_error` 之类的列把错误也持久化？
3. **同秒内消息排序**（2.1 节）：用 `ORDER BY created_at ASC, rowid ASC` 做 tie-breaker，不改 schema。可以接受，还是想把 `created_at` 改成毫秒级以后就不需要靠 rowid？
4. **文件拆分**（2.4 节）：`commands.rs` 拆成 `commands/agent.rs` + `commands/session.rs` + `commands/chat.rs`，新增 `session.rs` 放 struct。可以这么拆，还是保持现在的单文件风格，之后再看？
5. **拖拽排序**：先用零依赖的上下箭头按钮实现 MVP，不引入拖拽库，符合你说的"don't over-engineer"。确认这个选择？
6. **页面导航**：不引入 `react-router`，`App.tsx` 用一个简单的 `useState` 三态切换（agents 配置 / session 列表 / session 详情）。确认这个选择？
7. **两个补充命令**：`get_session(session_id)` 和 `list_messages(session_id)`，你的清单里没写但前端逻辑上少不了（分别用于刷新单个 session 状态、和打开 session 时拉历史消息）。确认要加，命名也没问题？
8. **"结束会话"**：目前 schema 只有 `idle/running/awaiting_user` 三种 status，没有 `ended`。"结束会话"按钮打算做成纯前端导航（离开 session 详情页回列表），不改后端状态。这样够用吗，还是想要一个真正的终态（比如 `ended`，用于列表页区分"已结束"和"待续"的 session）？
9. 顺带一提（不影响本次实现）：`agent.rs` 里 keyring 的 `SERVICE_NAME` 是 `"multi-agent-group-chat"`，和你描述里的 `"multi-agent-chat"` 不一致，要不要顺手统一一下？

以上都确认后我再开始写代码，目前**没有改动任何现有文件**。
