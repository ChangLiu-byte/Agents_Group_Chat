# Phase 5 实施计划：图片上传 + 视觉（Vision）支持

> 本文档仅为现状梳理 + 实施方案，**未修改任何代码**，供你 review 后再动手实现。

## 1. 现状梳理

与本任务相关的现有结构：

```
src-tauri/src/
  lib.rs                 -- setup()：DB 池 + run_migrations + reset_stale_running_sessions；注册 17 个 command
  db.rs                  -- run_migrations：4 张表，全部 "CREATE TABLE IF NOT EXISTS" 风格，没有任何 ALTER
  agent.rs               -- Agent struct（FromRow + Serialize/Deserialize）+ keyring
  session.rs             -- Session / Message struct + now_unix()
  commands/
    agent.rs             -- add/list/update/delete_agent + test_agent_message
    session.rs           -- session / roster 管理，含 fetch_roster()、delete_session（事务里手动删子表）
    chat.rs              -- list_messages、run_sequential_round、run_mention_round，以及
                            build_history_for / effective_system_prompt / run_one_turn 等 helper + 单元测试
  providers/
    mod.rs               -- Role 枚举、ChatMessage { role: Role, content: String }、send_chat_message
    openai_compatible.rs -- RequestMessage { role: &str, content: &str }，content 目前永远是纯字符串
    anthropic.rs         -- 同样是 content: &str，从不发 temperature

src/
  types/agent.ts, types/session.ts   -- 与 Rust struct 手动同步
  components/AgentForm.tsx           -- 新增/编辑表单
  components/AgentsPage.tsx          -- 负责把表单值拼成 Agent 调 add_agent / update_agent
  components/ChatInput.tsx           -- 文本框 + 小组/点名切换 + 发送按钮，onSend(text)
  components/SessionDetail.tsx       -- handleSend 调 run_sequential_round / run_mention_round；监听 message-added 等事件
  components/MessageList.tsx         -- MessageBubble 纯文本渲染
```

关键观察：

- **两个发送入口，不是一个**：`run_sequential_round` 和 `run_mention_round` 都会插入用户消息，并且都通过共享的 `run_one_turn` → `generate_reply` → `build_history_for` 构造历史。所以图片参数需要加到**两个** command 上，vision gating 只需要改 `build_history_for` 这一处，两种模式自动生效。
- `build_history_for` 目前是**纯函数**（同步、无 IO），有单元测试覆盖。计划保持它纯函数：读文件/base64 放到调用它之前的 async 步骤，再把结果以 map 形式传进去，这样新逻辑也能直接单测。
- `Role` 已经是枚举（`User` / `Assistant`），不是你描述里的 `role: String`。计划保留枚举，只把 `content` 改成 `Vec<ContentPart>`。
- 你描述里的 `build_effective_system_prompt` 实际名字是 `effective_system_prompt`（chat.rs:35）。本任务不需要改它（"看不到图片"的说明按你的要求追加在消息文本里，不放进 system prompt）。
- `Agent` 的列名在 **4 处** SQL 里手写：`add_agent` 的 INSERT、`list_agents`、`test_agent_message`、`fetch_roster`（session.rs:166）。加 `supports_vision` 时都要同步，否则 `query_as::<_, Agent>` 会在运行时报 "no column found"。
- `run_migrations` 每次启动都会跑。SQLite 的 `ALTER TABLE ... ADD COLUMN` **不是幂等的**（第二次启动会报 `duplicate column name`），不能像 CREATE TABLE 那样直接写，需要先 `PRAGMA table_info(agents)` 判断列是否存在。
- `Message` 同时是 `list_messages` 的返回值和 `message-added` 事件的 payload。只要把附件挂在 `Message` 上，前端历史加载和实时事件两条路径都能自动拿到图片信息。
- `delete_session` 在事务里手动删子表（messages → session_agents → chat_sessions），新表 `attachments` 也要加进去，同时删磁盘文件。
- `tauri.conf.json` 里 `csp: null`，`capabilities/default.json` 只有 `core:default` + `opener:default`，前端没有安装 `@tauri-apps/plugin-dialog` / `plugin-fs`。
- 顺带发现一个**现有测试已经会失败**的问题：`chat.rs` 的 `sequential_mode_appends_guidelines_after_existing_prompt` 断言 `result.contains("group discussion round")`，但 `SEQUENTIAL_MODE_GUIDELINES` 已经改成中文了，这个断言必然不通过。见第 7 节问题。

## 2. 数据库（`db.rs::run_migrations`）

```rust
// 1. agents.supports_vision —— 条件 ALTER，保证重复启动不报错
let has_col: bool = sqlx::query_scalar(
    "SELECT COUNT(*) > 0 FROM pragma_table_info('agents') WHERE name = 'supports_vision'"
).fetch_one(pool).await?;
if !has_col {
    sqlx::query("ALTER TABLE agents ADD COLUMN supports_vision INTEGER NOT NULL DEFAULT 1")
        .execute(pool).await?;
}

// 2. attachments 表 —— 按你给的 schema 原样 CREATE TABLE IF NOT EXISTS
```

另外给 `attachments(message_id)` 建一个索引（`CREATE INDEX IF NOT EXISTS idx_attachments_message ON attachments(message_id)`），因为每次构造历史都要按 message 查附件。成本几乎为零，如果你觉得没必要可以去掉。

## 3. Rust 侧改动

### 3.1 `agent.rs`

`Agent` 增加字段：

```rust
/// 用户手动勾选：该模型是否接受图片输入。不从 provider/model 名推断，
/// 因为各家支持情况变化太快（例如 DeepSeek 只有 flash 系列最近才支持）。
#[serde(default = "default_true")]
pub supports_vision: bool,
```

SQLite 的 `INTEGER` 0/1 可以被 sqlx 直接 decode 成 `bool`。`#[serde(default)]` 是防御性的：万一前端某处构造 `Agent` 时漏传，默认按 true（与 DB 默认一致）。

### 3.2 新增文件：`src-tauri/src/attachment.rs`

按项目"一个领域一个文件"的风格，附件相关的 struct 和 IO helper 都放这里：

```rust
/// attachments 表的一行，也挂在 Message 上返回给前端。
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Attachment {
    pub id: String,
    pub message_id: String,
    pub file_path: String,   // 相对 attachments 目录，如 "<session_id>/<uuid>.png"
    pub mime_type: String,
    pub created_at: i64,
}

/// 前端上传时传进来的一张图。
#[derive(Debug, Deserialize)]
pub struct ImageUpload {
    pub file_name: String,     // 原始文件名，只用来取扩展名
    pub mime_type: String,
    pub data_base64: String,
}
```

helper 函数：

- `attachments_dir(app: &AppHandle) -> Result<PathBuf, String>`：`app_data_dir()/attachments`，不存在就 `create_dir_all`。
- `validate_and_decode(upload) -> Result<(ext, mime, Vec<u8>), String>`：
  - 白名单：`png → image/png`、`jpg/jpeg → image/jpeg`、`gif → image/gif`、`webp → image/webp`。扩展名和 mime 都必须在白名单里且**互相对应**，否则返回清晰的中文错误，如 `不支持的图片格式 "xxx.bmp"，仅支持 PNG / JPEG / GIF / WebP`。
  - 额外做一个零依赖的**文件头魔数校验**（PNG `89 50 4E 47`、JPEG `FF D8 FF`、GIF `GIF8`、WebP `RIFF....WEBP`），防止改了扩展名的非图片文件被发给 provider 然后得到一个难懂的 400。几行代码，见问题 4。
  - 解码失败（非法 base64）同样报错。
- `save_image(dir, session_id, ext, bytes) -> Result<String /*relative path*/, String>`：文件名 `<uuid>.<ext>`，写入 `attachments/<session_id>/`（目录方案见问题 2）。
  - 加 TODO 注释：`// TODO: 目前原样存储，没有大小限制/压缩。如果文件过大（Anthropic 单图上限 5MB 左右，OpenAI 20MB），后续可在此处加限制或缩放。`
- `load_image_base64(dir, rel_path) -> Result<String, String>`：读文件 + base64 编码，供构造历史用。拼路径前校验 `rel_path` 不含 `..`（路径是我们自己生成的，这里只是兜底）。

新增依赖：`base64 = "0.22"`（Cargo.lock 里已经作为间接依赖存在，现在改成直接依赖）。

### 3.3 `session.rs`：`Message` 挂附件

```rust
pub struct Message {
    ...现有字段...
    /// 不是 messages 表的列，由 fetch_messages 另外查出来填进去。
    #[sqlx(skip)]
    #[serde(default)]
    pub attachments: Vec<Attachment>,
}
```

用 `#[sqlx(skip)]` 而不是另起一个 `MessageWithAttachments` 类型，好处是 `list_messages`、`message-added` 事件、前端 `Message` 类型都只需要"多一个字段"，不需要换类型。

### 3.4 `providers/mod.rs`：多段内容模型

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum ContentPart {
    Text(String),
    Image { mime_type: String, base64_data: String },
}

pub struct ChatMessage {
    pub role: Role,                 // 保留现有枚举
    pub content: Vec<ContentPart>,
}

impl ChatMessage {
    /// 纯文本消息的便捷构造，test_agent_message 和单测里用，减少改动面。
    pub fn text(role: Role, text: impl Into<String>) -> Self { ... }
    pub fn has_image(&self) -> bool { ... }
}
```

`send_chat_message` 的签名不变（仍然是 `&[ChatMessage]`）。

### 3.5 `providers/openai_compatible.rs`（OpenAI / DeepSeek / Qwen）

用 `#[serde(untagged)]` 让 `content` 有两种序列化形态：

```rust
#[derive(Serialize)]
#[serde(untagged)]
enum RequestContent<'a> {
    Text(String),                 // 纯文本：保持现在的 "content": "..." 字符串
    Parts(Vec<OpenAiPart<'a>>),   // 含图片时才变成数组
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum OpenAiPart<'a> {
    Text { text: &'a str },
    ImageUrl { image_url: ImageUrl },   // { "url": "data:<mime>;base64,<data>" }
}
```

规则：`!m.has_image()` → 把所有 Text 段拼接成一个字符串走 `Text`（与当前请求体**逐字节一致**，文本场景零回归）；有图 → `Parts`。system 消息保持字符串。

这一点对不支持视觉的模型很重要：`supports_vision = false` 时历史里根本不会出现 Image 段，请求体就永远是纯字符串，老的 DeepSeek 模型不会因为"content 是数组"而 400。

### 3.6 `providers/anthropic.rs`

同样的 untagged 双形态：纯文本仍然发字符串；含图时发

```json
[{"type": "text", "text": "..."},
 {"type": "image", "source": {"type": "base64", "media_type": "<mime>", "data": "<base64>"}}]
```

Anthropic 支持的 media type 正好就是 jpeg/png/gif/webp，与白名单一致。

### 3.7 `commands/chat.rs`：历史构造 + vision gating

**(a) `fetch_messages` 同时取附件** —— 两条查询，不是 N+1：

```sql
SELECT ... FROM messages WHERE session_id = ? ORDER BY created_at ASC, rowid ASC;
SELECT a.* FROM attachments a JOIN messages m ON m.id = a.message_id
WHERE m.session_id = ? ORDER BY a.created_at ASC, a.rowid ASC;
```

在 Rust 里按 `message_id` 分组填回 `Message.attachments`。`list_messages` command 因此自动带上附件。

**(b) 新增 async 步骤 `load_images_for(agent, history, dir)`**：

- `agent.supports_vision == false` → 直接返回空 map，**完全不读文件**。
- 否则遍历 history 里所有附件，读文件 + base64，返回 `HashMap<message_id, Vec<ContentPart::Image>>`。
- 某张图文件丢失/读取失败：不让整个 agent 回合失败，改成在该消息文本后追加 `[图片已丢失，无法加载]`（见问题 6）。

**(c) `build_history_for` 保持纯函数，增加两个参数**：`supports_vision: bool`、`images: &HashMap<String, Vec<ContentPart>>`。对每条消息：

1. 照旧生成文本段（`[User]: ...` / `[Alice]: ...` / 自己的消息不加前缀）。
2. 若 `m.attachments` 非空：
   - `supports_vision == true` → `content = [Text(文本), Image, Image, ...]`
   - `supports_vision == false` → 只有一个 Text 段，文本末尾追加：
     `\n[注意：此消息附带了 N 张图片，但你不具备图片识别能力，无法看到图片内容。如与讨论相关，请在回复中说明这一局限，并仅根据文字内容回应。]`

**(d) `generate_reply` / `run_one_turn`** 串起 (b)(c)，需要多一个 attachments 目录参数 → 放进现有的 `RoundCtx`（加一个 `attachments_dir: PathBuf` 字段），不增加函数参数个数。

**(e) 两个发送 command 都加参数** `images: Option<Vec<ImageUpload>>`（前端不传时就是 None，老调用不会坏）：

```
run_sequential_round(..., user_message: String, images: Option<Vec<ImageUpload>>)
run_mention_round(..., target_agent_id, user_message, images: Option<Vec<ImageUpload>>)
```

流程（抽成一个共享 helper `insert_user_message(ctx, round, text, refers_to, uploads)`）：

1. **在 `claim_running` 之前**校验并解码所有图片（格式错误直接 `Err`，不会把 session 锁进 running 再解锁）。
2. 空消息判断改为：`文本为空 && 没有图片` 才报错（见问题 3）。
3. `claim_running` 后：先把文件写盘，再在**一个事务**里插入 `messages` 行 + 所有 `attachments` 行。事务失败则尽力删除刚写的文件，避免孤儿文件。
4. `emit("message-added", &user_row)`，此时 `user_row.attachments` 已填好，前端能立即显示缩略图。

**(f) 单元测试**：给 `build_history_for` 补 3 个测试 —— 支持视觉时图片段在文本段之后；不支持视觉时无 Image 段且文本含提示语；无附件消息两种情况下输出一致。给 openai/anthropic 的请求序列化各补一个"纯文本仍是字符串、有图变数组"的测试。现有测试里的 `ChatMessage { content: String }` 断言改成用 `ChatMessage::text(...)` 比较。

### 3.8 `commands/agent.rs` / `commands/session.rs`

- `add_agent`：INSERT 增加 `supports_vision`（你只提了 update_agent，但新增时也必须写，否则勾选框在新增时无效）。
- `update_agent`：SET 增加 `supports_vision = ?`。
- `list_agents`、`test_agent_message`、`fetch_roster` 三处 SELECT 加上该列。`list_session_agents` 走 `fetch_roster`，因此自动返回。
- `test_agent_message` 改用 `ChatMessage::text(Role::User, user_message)`，行为不变。
- `delete_session`：事务里在删 messages 之前先 `DELETE FROM attachments WHERE message_id IN (SELECT id FROM messages WHERE session_id = ?)`；commit 成功后尽力 `remove_dir_all(attachments/<session_id>)`（失败只打日志，不影响删除结果）。`delete_session` 需要多一个 `AppHandle` 参数来拿目录。

### 3.9 图片如何显示到前端（见问题 1）

推荐方案 A：**Tauri asset protocol**

- `Cargo.toml`：`tauri = { version = "2", features = ["protocol-asset"] }`
- `tauri.conf.json`：`app.security.assetProtocol = { "enable": true, "scope": ["$APPDATA/attachments/**"] }`（只开放附件目录）
- 前端：启动时 `appDataDir()` 拿一次绝对路径，`convertFileSrc(join(appData, "attachments", file_path))` 得到 `<img src>`。

优点：图片不走 IPC，历史里几十张图也不会卡；是 Tauri 官方推荐做法。
风险点：你的 `identifier` 是 `com.刘畅.multi-agent-gourp-chat`，app data 路径里有中文。`convertFileSrc` 会做 URL 编码，理论上没问题，但没在这台机器上实测过，实现时要第一时间验证。

备选方案 B：新增 `read_attachment(attachment_id) -> String`（返回 data URL），前端缩略图组件挂载时按需调用。零配置、不碰中文路径问题，但每张图都要走一次 base64 IPC。方案 A 如果验证失败就退回 B。

## 4. 前端改动

### 4.1 类型

- `types/agent.ts`：`Agent` 和 `AgentFormValues` 加 `supports_vision: boolean`；`emptyFormValues()` 默认 `true`；`agentToFormValues` 同步。
- `types/session.ts`：新增 `Attachment` 接口；`Message` 加 `attachments: Attachment[]`。

### 4.2 Agent 配置

- `AgentForm.tsx`：在 System Prompt 上方加一个 checkbox「支持图片识别」，下面一行灰色小字说明"取消勾选后，该 Agent 会收到'有图片但看不到'的文字提示，而不会收到图片本身"。
- `AgentsPage.tsx`：新增/编辑两处构造 `Agent` 的地方都带上 `supports_vision`。
- `AgentCard.tsx`（可选小改动）：不支持视觉时在 `provider · model` 后面显示一个灰色「不识图」小标签，方便一眼看出配置。

### 4.3 聊天输入（`ChatInput.tsx`）

- **支持多图**：实现上只是把"一个 pending 附件"换成"一个数组"，复杂度几乎没有增加，所以直接做多图。
- 在模式按钮左侧加一个 📎「图片」按钮，触发隐藏的 `<input type="file" accept="image/png,image/jpeg,image/gif,image/webp" multiple>`。
  - 不引入 `@tauri-apps/plugin-dialog`：HTML file input 在 Tauri webview 里本来就会弹系统文件选择框，还能直接拿到 `File` 对象，省掉额外插件、权限配置和"再从路径读文件"的一步。
- textarea 上加 `onPaste`：从 `clipboardData.items` 里取 `image/*`，支持截图直接 Ctrl+V。
- 文本框上方显示缩略图条（`URL.createObjectURL` 预览，64px 方块），每张右上角 × 删除；删除或发送后 `revokeObjectURL`。
- 前端先做一次格式过滤，不支持的格式直接提示，不加入列表（Rust 侧仍然做最终校验）。
- `canSend` 改为：`blockedReason === null && (text.trim() !== "" || images.length > 0)`。
- `onSend` 签名改为 `onSend(text: string, images: File[])`。
- （小提示，可选）当待发送列表里有图片、且名单中有 `supports_vision=false` 的 agent 时，在 `blockedReason` 那一行显示"Alice、Bob 未开启图片识别，只会收到文字提示"。

### 4.4 发送（`SessionDetail.tsx`）

`handleSend(text, files)`：用 `FileReader.readAsDataURL` 把每个 `File` 转成 base64（去掉 `data:...;base64,` 前缀），组装成 `{ file_name, mime_type, data_base64 }[]`（嵌套对象字段用 snake_case，和现在传 `Agent` 的方式一致），作为 `images` 参数传给 `run_sequential_round` / `run_mention_round`。

base64 转换函数放到新文件 `src/utils/image.ts`，连同允许的 mime 白名单常量，供 ChatInput 和 SessionDetail 共用。

### 4.5 消息显示（`MessageList.tsx`）

- 用户气泡里，文字下方渲染附件缩略图（最大 160px，`object-cover` 圆角）。文字为空时只显示图片。
- 点击缩略图打开一个简单的全屏遮罩查看原图，点击遮罩关闭（一个 `useState` + 一个 fixed 定位的 div，不引入库）。
- 新增小组件 `src/components/AttachmentImage.tsx`：负责把 `Attachment` 转成 `<img src>`（方案 A 下是 `convertFileSrc`，方案 B 下是异步调 `read_attachment`），这样切换方案只改这一个文件。
- agent 回复中"我看不到图片"的内容不做任何特殊处理。

## 5. 文件清单

**新增**

| 文件 | 内容 |
|---|---|
| `src-tauri/src/attachment.rs` | `Attachment` / `ImageUpload` struct，格式校验、保存、读取+base64 helper |
| `src/utils/image.ts` | 允许的 mime 常量、`fileToBase64` |
| `src/components/AttachmentImage.tsx` | 附件 → `<img>` 的渲染组件 |

**修改**

| 文件 | 改动 |
|---|---|
| `src-tauri/Cargo.toml` | 加 `base64`；tauri 加 `protocol-asset` feature（方案 A） |
| `src-tauri/tauri.conf.json` | `assetProtocol` 配置（方案 A） |
| `src-tauri/src/lib.rs` | `mod attachment;`（方案 B 时再注册 `read_attachment`） |
| `src-tauri/src/db.rs` | 条件 ALTER + attachments 表 + 索引 |
| `src-tauri/src/agent.rs` | `supports_vision` 字段 |
| `src-tauri/src/session.rs` | `Message.attachments` |
| `src-tauri/src/providers/mod.rs` | `ContentPart`、`ChatMessage.content: Vec<ContentPart>`、helper |
| `src-tauri/src/providers/openai_compatible.rs` | 字符串/数组双形态 content |
| `src-tauri/src/providers/anthropic.rs` | 字符串/数组双形态 content |
| `src-tauri/src/commands/chat.rs` | 附件查询、vision gating、两个发送 command 加 `images`、测试 |
| `src-tauri/src/commands/agent.rs` | INSERT/UPDATE/SELECT 加列，`test_agent_message` 用 `ChatMessage::text` |
| `src-tauri/src/commands/session.rs` | `fetch_roster` SELECT 加列；`delete_session` 清理附件行和文件 |
| `src/types/agent.ts`、`src/types/session.ts` | 新字段/新类型 |
| `src/components/AgentForm.tsx`、`AgentsPage.tsx`、`AgentCard.tsx` | 勾选框及字段传递 |
| `src/components/ChatInput.tsx` | 附件按钮、粘贴、预览、删除 |
| `src/components/SessionDetail.tsx` | 发送时带图片 |
| `src/components/MessageList.tsx` | 渲染缩略图 + 查看大图 |

不需要改：`capabilities/default.json`（`core:default` 已包含 path API；方案 A 的 asset protocol 由 `tauri.conf.json` 控制）、`NewSessionForm`、`SessionAgentRoster`、`SessionsPage`、`TestPanel`。

## 6. 已知取舍 / 以后可以再做的

- **历史图片会在每个 agent、每一轮重复发送**。一轮 4 个 agent、历史里 3 张图 = 12 次图片输入，token 成本会随会话变长明显增长。本次不处理，后续可以考虑"只附带最近 N 轮的图片，更早的替换成 `[之前的图片]` 文字占位"。
- 同一轮内每个 agent 都会重新读文件、重新 base64。几 MB 的图片开销可以忽略；需要时可以在 `RoundCtx` 里加一个本轮缓存。
- 无大小限制/压缩（按你的要求只留 TODO）。超出 provider 限制的图会导致该 agent 回合 HTTP 400，按现有逻辑显示为 `agent-error` 气泡，不会中断整轮。

## 7. 需要你确认的问题

1. **图片显示方案**（3.9 节）：推荐方案 A（asset protocol，需改 `Cargo.toml` feature + `tauri.conf.json`），若中文路径实测有问题再退回方案 B（`read_attachment` command 返回 data URL）。可以吗？还是想一开始就用零配置的方案 B？
2. **存储目录结构**：推荐按会话分子目录 `attachments/<session_id>/<uuid>.<ext>`，`file_path` 存 `"<session_id>/<uuid>.png"`（仍然是"相对 attachments 目录的路径"）。好处是 `delete_session` 可以直接整个目录删掉。还是你想要完全平铺的 `attachments/<uuid>.<ext>`？
3. **纯图片消息**：是否允许"只发图片、不写文字"？我打算允许（文本为空且有图片时，历史里的文本段就是 `[User]: `，后面跟图片 / 或跟"看不到图片"的提示）。现有逻辑是空文本直接报错。
4. **魔数校验**：除了扩展名 + mime 白名单，是否还要做文件头魔数校验（零依赖，约 15 行）？我倾向于做，能挡住"改了后缀的非图片文件"。
5. **删除会话时清理附件**：`delete_session` 同时删除 attachments 行和磁盘文件——你的需求里没提，但不做的话会留下孤儿文件和孤儿行。确认要做？
6. **图片文件丢失**（比如用户手动删了 AppData 下的文件）：打算不让该 agent 回合失败，而是在文字后追加 `[图片已丢失，无法加载]` 继续。还是你更希望直接报 `agent-error`？
7. **"看不到图片"提示语的措辞**：计划用 `[注意：此消息附带了 N 张图片，但你不具备图片识别能力，无法看到图片内容。如与讨论相关，请在回复中说明这一局限，并仅根据文字内容回应。]`。加了"如与讨论相关"是为了避免后续每一轮它都重复声明一次"我看不到图片"（历史里的旧图片每轮都会带着这句提示）。这样可以吗？
8. **AgentCard 上的「不识图」标签** 和 **ChatInput 里"某些 Agent 未开启图片识别"的提示**：两个都是可选的小改动，要不要做？
9. **现有测试 bug**：`sequential_mode_appends_guidelines_after_existing_prompt` 断言英文 `"group discussion round"`，但指引已经是中文，这个测试现在必然失败。要不要这次顺手把断言改成匹配中文（比如 `"当前处于小组讨论轮次"`）？我改 chat.rs 的测试时会跑 `cargo test`，不修的话会一直看到这个失败。

以上都确认后我再开始写代码，目前**没有改动任何现有文件**（只新增了本文档）。
