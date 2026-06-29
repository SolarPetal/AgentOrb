# Agent Orb MVP 设计文档

> 方案二：Local Daemon + Floating Orb  
> 技术栈：Rust + Tauri  
> 目标平台：Windows / macOS / Linux  
> 目标 CLI：Claude Code CLI / Codex CLI

---

## 1. 一句话定位

Agent Orb 是一个跨平台 AI CLI 工作状态可视化工具：通过本地 Daemon 监听 Claude Code CLI / Codex CLI 的运行事件，并在桌面上用一个常驻悬浮圆圈 Floating Orb 展示当前 AI 工作状态。

它不是传统意义上的弹窗通知工具，而是一个低打扰的 ambient status indicator。

用户不需要频繁切回 terminal，也不需要被通知打断，只要看一眼桌面角落的圆圈颜色和动画，就能知道 AI 当前是在运行、思考、等待输入、完成，还是出错。

---

## 2. MVP 目标

### 2.1 核心目标

MVP 阶段要完成以下闭环：

1. 用户通过 wrapper 启动 Claude Code CLI 或 Codex CLI。
2. Wrapper 捕获子进程 stdout / stderr / exit code / idle time。
3. Wrapper 将标准化事件发送到本地 Daemon。
4. Daemon 维护当前 session 状态。
5. Tauri Floating Orb 订阅 Daemon 状态。
6. 桌面圆圈根据状态改变颜色、动画和节奏。
7. 用户能通过颜色和动画直观看到 AI CLI 当前工作状态。

### 2.2 非目标

MVP 暂不做：

- 云端同步；
- 账号系统；
- 多端协同；
- 手机推送；
- 复杂 Dashboard；
- 对 Claude / Codex 内部私有协议的强依赖；
- 精确判断模型真实 thinking 状态；
- 同时展示多个圆圈。

MVP 只做一件事：稳定、直观、低打扰地显示当前 AI CLI 状态。

---

## 3. 用户场景

### 3.1 场景一：AI 正在执行任务

用户执行：

```bash
agent_orb run -- codex
```

桌面右上角出现一个蓝色旋转圆圈。

含义：当前 AI CLI 正在运行，有持续活动。

---

### 3.2 场景二：AI 长时间无输出

Codex / Claude 运行过程中，超过一段时间没有 stdout / stderr 输出。

圆圈变成紫色慢速旋转。

含义：当前进程仍然存活，但没有输出，可能在 thinking。

注意：这里展示为 thinking-like，而不是断言模型真实正在 thinking。

---

### 3.3 场景三：等待用户输入

Wrapper 检测到 terminal 中出现常见交互提示，例如：

- `?`
- `confirm`
- `continue?`
- `yes/no`
- `approve`
- `permission`
- `press enter`

圆圈变成黄色脉冲。

含义：AI CLI 可能正在等待用户输入。

---

### 3.4 场景四：任务完成

子进程 exit code 为 0。

圆圈变成绿色，保持 10 秒后回到灰色。

含义：任务已完成。

---

### 3.5 场景五：任务失败

子进程 exit code 非 0。

圆圈变成红色闪烁。

含义：任务出错，需要用户查看 terminal。

MVP 中，错误状态需要用户点击圆圈或执行命令清除，避免错过失败信号。

---

## 4. 总体架构

```text
┌────────────────────┐
│ Claude Code CLI     │
│ Codex CLI           │
└─────────▲──────────┘
          │ spawned by
┌─────────┴──────────┐
│ agent_orb wrapper      │
│ Rust CLI            │
│ - spawn process     │
│ - capture output    │
│ - detect idle       │
│ - detect prompts    │
│ - send events       │
└─────────┬──────────┘
          │ localhost events
          ▼
┌────────────────────┐
│ agent_orbd daemon      │
│ Rust local service  │
│ - event ingest      │
│ - state machine     │
│ - session store     │
│ - status API        │
│ - websocket stream  │
└─────────┬──────────┘
          │ status stream
          ▼
┌────────────────────┐
│ Floating Orb UI     │
│ Tauri App           │
│ - transparent window│
│ - always on top     │
│ - color mapping     │
│ - animation mapping │
└────────────────────┘
```

---

## 5. C4 Container 视图

### 5.1 System Context

```text
User
 │
 │ uses
 ▼
Agent Orb
 │
 ├── wraps Claude Code CLI
 ├── wraps Codex CLI
 └── displays desktop status orb
```

### 5.2 Container

```text
Agent Orb
├── CLI Wrapper
│   ├── starts AI CLI process
│   ├── monitors stdout / stderr
│   ├── observes exit code
│   └── emits normalized events
│
├── Local Daemon
│   ├── receives events
│   ├── manages sessions
│   ├── runs state machine
│   ├── exposes HTTP API
│   └── exposes WebSocket stream
│
└── Floating Orb UI
    ├── connects to daemon
    ├── renders orb
    ├── supports drag position
    └── maps state to visual feedback
```

---

## 6. 核心组件设计

## 6.1 CLI Wrapper

### 职责

CLI Wrapper 负责把任意 AI CLI 进程变成可观察对象。

主要职责：

- 启动 Claude Code CLI / Codex CLI；
- 捕获 stdout；
- 捕获 stderr；
- 记录 last output time；
- 检测等待输入的提示；
- 监听进程退出；
- 将事件发送给 Daemon；
- 保持原始 CLI 的交互体验尽量不变。

### 命令形式

```bash
agent_orb run -- codex
agent_orb run -- codex --approval-mode on-request
agent_orb run -- claude
agent_orb run -- claude --dangerously-skip-permissions
```

Wrapper 参数规则：

- `agent_orb run --` 之前的参数属于 agent-orb；
- `--` 之后的参数原样透传给目标 CLI。

### 示例

```bash
agent_orb run -- codex -m gpt-5-codex
```

等价于由 agent_orb 启动：

```bash
codex -m gpt-5-codex
```

但 agent_orb 会额外发送状态事件。

---

## 6.2 Local Daemon

### 职责

Local Daemon 是系统核心。

职责包括：

- 统一接收 wrapper 上报的事件；
- 维护 session 状态；
- 运行状态机；
- 为 UI 提供当前状态；
- 处理多个 session 的优先级；
- 提供本地 API；
- 控制状态清除逻辑。

### 进程名称

```text
agent_orbd
```

### 监听地址

MVP 默认只监听：

```text
127.0.0.1:17321
```

禁止默认监听 `0.0.0.0`。

### Daemon 启动方式

MVP 支持两种方式：

1. Floating Orb UI 启动时自动拉起 daemon；
2. Wrapper 发现 daemon 不存在时自动拉起 daemon。

---

## 6.3 Floating Orb UI

### 职责

Floating Orb UI 是 Tauri 桌面应用。

职责：

- 创建透明无边框窗口；
- 默认置顶；
- 显示圆形状态指示器；
- 根据状态改变颜色；
- 根据 activity level 改变旋转速度；
- 支持拖拽移动；
- 支持点击查看当前 session 摘要；
- 支持点击清除 completed / error 状态。

### 默认窗口行为

```text
size: 36 x 36
position: top-right
always_on_top: true
transparent: true
decorations: false
resizable: false
```

### 点击行为

MVP 点击圆圈：

- 如果状态是 error：清除 error，回到 idle；
- 如果状态是 completed：清除 completed，回到 idle；
- 如果状态是 running：弹出小浮层显示 source、workspace、duration；
- 如果状态是 idle：弹出简单菜单。

---

## 7. 状态模型

## 7.1 Internal State

Internal State 是系统内部真实状态，不直接等于用户看到的文案。

```text
disconnected
idle
starting
active
silent
waiting_input
completed
failed
stuck
cancelled
```

### 状态说明

| Internal State | 说明 |
|---|---|
| disconnected | UI 无法连接 daemon |
| idle | daemon 正常，但没有活跃任务 |
| starting | wrapper 已启动目标 CLI |
| active | 进程存活且近期有输出 |
| silent | 进程存活但一段时间无输出 |
| waiting_input | 检测到可能等待用户输入 |
| completed | 进程正常退出 |
| failed | 进程异常退出 |
| stuck | 长时间无输出，疑似卡住 |
| cancelled | 用户中断进程 |

---

## 7.2 Visual State

Visual State 是用户看到的颜色和动画。

| Internal State | Visual State | 颜色 | 动画 |
|---|---|---|---|
| disconnected | disconnected | 灰色 | 慢速呼吸 |
| idle | idle | 灰色 | 静止 |
| starting | starting | 蓝色 | 渐入旋转 |
| active | active | 蓝色 | 匀速旋转 |
| silent | thinking_like | 紫色 | 慢速旋转 |
| waiting_input | waiting_input | 黄色 | 脉冲 |
| completed | completed | 绿色 | 静止发光 |
| failed | error | 红色 | 闪烁 |
| stuck | warning | 橙色 | 慢闪 |
| cancelled | cancelled | 灰色 | 快速淡出 |

---

## 7.3 为什么不直接把 silent 叫 thinking

MVP 无法从 Claude Code CLI / Codex CLI 通用输出中 100% 判断模型真实 thinking 状态。

因此内部状态使用 `silent`，视觉层展示为 `thinking_like`。

这是一个重要架构边界：

```text
process_alive + no_output_for_threshold = silent
silent -> visual thinking_like
```

这样既满足用户体验，又避免系统语义撒谎。

---

## 8. 状态机

```text
idle
 │
 │ session.started
 ▼
starting
 │
 │ output.received
 ▼
active
 │  ▲
 │  │ output.received
 │  │
 │ idle.timeout
 ▼
silent
 │
 │ prompt.detected
 ▼
waiting_input
 │
 │ output.received
 ▼
active

active / silent / waiting_input
 │
 ├── process.exited code=0 ──▶ completed
 ├── process.exited code!=0 ─▶ failed
 └── user.cancelled ─────────▶ cancelled

silent
 │
 │ stuck.timeout
 ▼
stuck
```

### 超时参数

MVP 默认：

```toml
[behavior]
silent_threshold_seconds = 20
stuck_threshold_seconds = 180
completed_hold_seconds = 10
```

---

## 9. 事件协议

## 9.1 Event Envelope

Wrapper 向 Daemon 发送统一事件。

```json
{
  "version": "1.0",
  "event_id": "018f4f6a-7e1a-7b8b-9c00-000000000001",
  "session_id": "018f4f6a-7e1a-7b8b-9c00-000000000000",
  "source": "codex",
  "workspace": "E:/code/project",
  "event_type": "output.received",
  "timestamp": "2026-06-29T12:00:00+08:00",
  "payload": {}
}
```

### 字段说明

| 字段 | 类型 | 说明 |
|---|---|---|
| version | string | 协议版本 |
| event_id | string | 事件 ID，建议 UUIDv7 |
| session_id | string | 会话 ID |
| source | string | codex / claude / generic |
| workspace | string | 工作目录 |
| event_type | string | 事件类型 |
| timestamp | string | ISO 8601 时间 |
| payload | object | 事件负载 |

---

## 9.2 Event Types

```text
session.started
output.received
stderr.received
prompt.detected
idle.timeout
stuck.timeout
process.exited
session.cancelled
session.cleared
```

---

## 9.3 session.started

```json
{
  "event_type": "session.started",
  "payload": {
    "command": "codex -m gpt-5-codex",
    "pid": 12345,
    "shell": "powershell",
    "platform": "windows"
  }
}
```

---

## 9.4 output.received

```json
{
  "event_type": "output.received",
  "payload": {
    "stream": "stdout",
    "bytes": 256,
    "sample": "Running tests..."
  }
}
```

MVP 中 sample 必须限制长度，默认最多 512 字符。

---

## 9.5 prompt.detected

```json
{
  "event_type": "prompt.detected",
  "payload": {
    "kind": "confirmation",
    "matched": "continue?",
    "confidence": 0.72
  }
}
```

---

## 9.6 process.exited

```json
{
  "event_type": "process.exited",
  "payload": {
    "exit_code": 0,
    "duration_ms": 120000
  }
}
```

---

## 10. Daemon API

## 10.1 Health Check

```http
GET /health
```

响应：

```json
{
  "ok": true,
  "version": "0.1.0"
}
```

---

## 10.2 提交事件

```http
POST /v1/events
Content-Type: application/json
Authorization: Bearer <local-token>
```

请求体：Event Envelope。

响应：

```json
{
  "ok": true
}
```

---

## 10.3 获取当前状态

```http
GET /v1/status
Authorization: Bearer <local-token>
```

响应：

```json
{
  "status": "active",
  "visual": "blue_spinning",
  "source": "codex",
  "workspace": "E:/code/project",
  "session_id": "018f4f6a-7e1a-7b8b-9c00-000000000000",
  "started_at": "2026-06-29T12:00:00+08:00",
  "updated_at": "2026-06-29T12:01:00+08:00",
  "message": "Codex is active"
}
```

---

## 10.4 WebSocket 状态流

```text
GET /v1/status/stream
```

推送消息：

```json
{
  "type": "status.changed",
  "status": "silent",
  "visual": "purple_spinning",
  "source": "codex",
  "updated_at": "2026-06-29T12:01:30+08:00"
}
```

---

## 10.5 清除状态

```http
POST /v1/status/clear
Authorization: Bearer <local-token>
```

用于清除 completed / failed 状态。

---

## 11. 多 Session 策略

MVP 不展示多个圆圈，只展示一个全局状态。

当多个 session 同时存在时，按优先级选择展示状态。

优先级：

```text
failed > waiting_input > stuck > active > silent > completed > starting > idle
```

如果同优先级有多个 session，展示最近更新的 session。

### 示例

- Codex 正在 active；
- Claude 失败了；

Orb 显示红色 error。

原因：failed 优先级更高。

---

## 12. 配置文件

## 12.1 路径

Windows：

```text
%APPDATA%/agent-orb/config.toml
```

macOS：

```text
~/Library/Application Support/agent-orb/config.toml
```

Linux：

```text
~/.config/agent-orb/config.toml
```

---

## 12.2 示例

```toml
[daemon]
host = "127.0.0.1"
port = 17321
auto_start = true

[orb]
position = "top-right"
size = 36
opacity = 0.88
always_on_top = true
click_through = false

[colors]
disconnected = "#6B7280"
idle = "#9CA3AF"
starting = "#60A5FA"
active = "#3B82F6"
thinking_like = "#8B5CF6"
waiting_input = "#FBBF24"
completed = "#22C55E"
error = "#EF4444"
warning = "#F97316"

[behavior]
silent_threshold_seconds = 20
stuck_threshold_seconds = 180
completed_hold_seconds = 10
error_requires_click_to_clear = true

[privacy]
include_output_sample = false
max_sample_chars = 512
```

---

## 13. 安全与隐私设计

## 13.1 本地绑定

Daemon 只能默认绑定：

```text
127.0.0.1
```

不得默认绑定：

```text
0.0.0.0
```

---

## 13.2 Local Token

首次启动生成本地 token。

存储路径：

```text
config_dir/token
```

权限要求：

- Windows：当前用户可读；
- macOS / Linux：`0600`。

Wrapper 和 UI 调用 Daemon API 时必须携带：

```http
Authorization: Bearer <local-token>
```

---

## 13.3 输出内容保护

默认不上传完整 stdout / stderr 内容。

MVP 默认：

```toml
[privacy]
include_output_sample = false
```

如果用户打开 sample，也要限制长度并脱敏。

需要脱敏的内容包括：

- API key；
- token；
- password；
- private key；
- authorization header；
- cookie；
- ssh key。

---

## 13.4 Web 攻击面

因为 Daemon 暴露 localhost API，需要注意：

- 所有写接口需要 token；
- 不允许 CORS wildcard；
- 不提供任意文件读取 API；
- 不执行来自 API 的任意命令；
- UI 与 Daemon 通信也走 token。

---

## 14. 跨平台注意事项

## 14.1 Windows

重点：

- High DPI；
- 多显示器；
- PowerShell / cmd / Windows Terminal 兼容；
- 透明窗口；
- always-on-top；
- 开机自启动可后置。

MVP 优先保证：

- Windows 11；
- 单显示器；
- PowerShell；
- Tauri 透明窗口。

---

## 14.2 macOS

重点：

- notarization；
- accessibility 权限；
- always-on-top 行为；
- menu bar / dock 展示策略。

MVP 可以先不做 notarized release，只提供开发者安装包。

---

## 14.3 Linux

重点：

- X11 与 Wayland 差异；
- 透明窗口；
- 置顶行为；
- GNOME / KDE 差异。

MVP 优先支持：

- Ubuntu + GNOME；
- X11 或主流 Wayland 下的基本展示。

点击穿透可以后置。

---

## 15. 技术选型

## 15.1 Rust

用途：

- CLI wrapper；
- Local daemon；
- 状态机；
- 配置解析；
- 跨平台进程管理。

推荐 crate：

```text
tokio
axum
serde
serde_json
toml
uuid
time
tracing
tracing-subscriber
reqwest
tokio-tungstenite
notify-rust
```

---

## 15.2 Tauri

用途：

- Floating Orb UI；
- 透明窗口；
- always-on-top；
- 跨平台打包。

前端建议：

```text
Vite + TypeScript
```

UI 不需要复杂框架，MVP 可以直接使用 HTML / CSS / TypeScript。

---

## 16. 推荐项目结构

```text
agent-orb/
├── Cargo.toml
├── README.md
├── docs/
│   └── MVP.md
├── crates/
│   ├── agent-orb-cli/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       └── main.rs
│   ├── agent-orb-daemon/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs
│   │       ├── api.rs
│   │       ├── state_machine.rs
│   │       ├── session_store.rs
│   │       └── security.rs
│   └── agent-orb-core/
│       ├── Cargo.toml
│       └── src/
│           ├── config.rs
│           ├── event.rs
│           ├── status.rs
│           └── visual.rs
├── apps/
│   └── agent-orb-ui/
│       ├── src-tauri/
│       ├── src/
│       │   ├── main.ts
│       │   ├── orb.ts
│       │   └── api.ts
│       ├── index.html
│       └── package.json
└── examples/
    ├── config.toml
    └── events/
        ├── session-started.json
        ├── output-received.json
        └── process-exited.json
```

---

## 17. Workspace Cargo 设计

根 `Cargo.toml`：

```toml
[workspace]
members = [
  "crates/agent-orb-core",
  "crates/agent-orb-cli",
  "crates/agent-orb-daemon"
]
resolver = "2"
```

---

## 18. 核心 Rust 数据结构草案

## 18.1 Event

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventEnvelope {
    pub version: String,
    pub event_id: String,
    pub session_id: String,
    pub source: Source,
    pub workspace: String,
    pub event_type: EventType,
    pub timestamp: String,
    pub payload: serde_json::Value,
}
```

---

## 18.2 Status

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InternalStatus {
    Disconnected,
    Idle,
    Starting,
    Active,
    Silent,
    WaitingInput,
    Completed,
    Failed,
    Stuck,
    Cancelled,
}
```

---

## 18.3 Visual State

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VisualStatus {
    GrayIdle,
    BlueSpinning,
    PurpleSpinning,
    YellowPulse,
    GreenDone,
    RedError,
    OrangeWarning,
}
```

---

## 19. Orb 视觉设计

## 19.1 基础形态

圆圈由三层组成：

```text
outer glow：状态氛围光
ring：旋转状态环
core：中心实体圆
```

---

## 19.2 CSS 动画草案

```css
.orb {
  width: 36px;
  height: 36px;
  border-radius: 999px;
  position: relative;
  background: var(--orb-core);
  box-shadow: 0 0 18px var(--orb-glow);
}

.orb::before {
  content: "";
  position: absolute;
  inset: -3px;
  border-radius: 999px;
  border: 3px solid transparent;
  border-top-color: var(--orb-ring);
  animation: spin var(--spin-duration) linear infinite;
}

.orb.pulse {
  animation: pulse 1.2s ease-in-out infinite;
}

.orb.blink {
  animation: blink 0.6s ease-in-out infinite;
}

@keyframes spin {
  from { transform: rotate(0deg); }
  to { transform: rotate(360deg); }
}

@keyframes pulse {
  0%, 100% { transform: scale(1); opacity: 0.9; }
  50% { transform: scale(1.12); opacity: 1; }
}

@keyframes blink {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.35; }
}
```

---

## 20. MVP 开发里程碑

## Milestone 1：Core Event & State Machine

目标：完成核心模型。

任务：

- 定义 EventEnvelope；
- 定义 InternalStatus；
- 定义 VisualStatus；
- 实现状态机；
- 编写单元测试。

验收：

- 输入 `session.started` 后状态为 `starting`；
- 输入 `output.received` 后状态为 `active`；
- 触发 silent timeout 后状态为 `silent`；
- 输入 `process.exited exit_code=0` 后状态为 `completed`；
- 输入 `process.exited exit_code=1` 后状态为 `failed`。

---

## Milestone 2：Daemon API

目标：完成本地 Daemon。

任务：

- 使用 axum 实现 HTTP API；
- 实现 `/health`；
- 实现 `/v1/events`；
- 实现 `/v1/status`；
- 实现 `/v1/status/clear`；
- 实现 token 校验；
- 实现 tracing 日志。

验收：

- curl 可以提交事件；
- curl 可以读取状态；
- 无 token 的写请求被拒绝；
- Daemon 只绑定 127.0.0.1。

---

## Milestone 3：CLI Wrapper

目标：能包装 Codex / Claude。

任务：

- 实现 `agent_orb run -- <command>`；
- spawn 子进程；
- 捕获 stdout / stderr；
- 将输出原样转发到当前 terminal；
- 上报 session.started；
- 上报 output.received / stderr.received；
- 上报 process.exited；
- 实现 prompt detection 初版。

验收：

```bash
agent_orb run -- echo hello
```

能看到原始输出，并且 Daemon 状态变为 completed。

---

## Milestone 4：Tauri Floating Orb

目标：桌面出现圆圈，并随状态变化。

任务：

- 创建 Tauri 应用；
- 创建透明无边框窗口；
- 实现 Orb CSS；
- 连接 `/v1/status`；
- 轮询状态或连接 websocket；
- 根据 visual state 更新颜色和动画；
- 支持拖拽位置。

验收：

- Daemon 状态 active 时圆圈蓝色旋转；
- 状态 silent 时圆圈紫色慢转；
- 状态 waiting_input 时圆圈黄色脉冲；
- 状态 completed 时圆圈绿色；
- 状态 failed 时圆圈红色闪烁。

---

## Milestone 5：端到端 MVP

目标：完整跑通 Codex / Claude。

任务：

- Wrapper 自动拉起 daemon；
- UI 自动连接 daemon；
- 完成配置文件读取；
- 完成基本错误处理；
- 完成 README；
- 打包 Windows / macOS / Linux dev build。

验收命令：

```bash
agent_orb run -- codex
agent_orb run -- claude
```

桌面 Orb 能正确显示运行、静默、完成、失败状态。

---

## 21. 测试策略

## 21.1 单元测试

覆盖：

- 状态机转换；
- visual mapping；
- event parsing；
- config loading；
- token validation；
- prompt detection。

---

## 21.2 集成测试

覆盖：

- POST event 后 GET status；
- clear status；
- invalid token；
- multiple sessions priority；
- process exit code handling。

---

## 21.3 手工验收

Windows：

```powershell
agent_orb run -- powershell -Command "Start-Sleep 3; Write-Output done"
```

macOS / Linux：

```bash
agent_orb run -- sh -c 'sleep 3; echo done'
```

预期：

- starting：蓝色渐入；
- silent：紫色；
- completed：绿色；
- 10 秒后 idle：灰色。

---

## 22. 风险清单

| 风险 | 影响 | 应对 |
|---|---|---|
| Wayland 透明窗口兼容不稳定 | Linux 体验受影响 | MVP 标注支持范围，优先保证基本展示 |
| thinking 判断不准确 | 用户误解状态 | 内部使用 silent，视觉展示 thinking-like |
| 多 session 冲突 | 状态不清晰 | MVP 使用优先级策略，只展示一个全局状态 |
| localhost API 被滥用 | 安全风险 | 127.0.0.1 + token + 禁止 CORS wildcard |
| stdout 包含敏感信息 | 隐私风险 | 默认不采集 sample，开启后脱敏和截断 |
| Tauri 打包复杂 | 发布延期 | MVP 先提供 dev build 和手动安装说明 |
| Claude / Codex 输出格式变化 | prompt detection 失效 | detection 只作为弱信号，允许配置规则 |

---

## 23. MVP 验收标准

MVP 完成标准：

1. 在 Windows / macOS / Linux 至少一个主流版本上能启动 Floating Orb；
2. 能通过 `agent_orb run -- <command>` 包装任意 CLI；
3. 能包装 Codex CLI；
4. 能包装 Claude Code CLI；
5. 能显示 active / silent / completed / failed；
6. 能通过颜色和动画区分状态；
7. Daemon API 有 token 保护；
8. 默认不记录完整 stdout / stderr；
9. 配置文件可修改颜色、大小、位置和 timeout；
10. 有 README 和基础安装说明。

---

## 24. 推荐开发顺序

```text
1. agent-orb-core: event/status/state machine
2. agent-orb-daemon: local API
3. agent-orb-cli: wrapper process monitor
4. agent-orb-ui: Tauri floating orb
5. integration: wrapper -> daemon -> UI
6. packaging: dev release
```

姐姐建议不要先做 UI。

原因是 UI 很容易让人兴奋，但真正决定产品稳定性的，是状态机和事件协议。

先把事件和状态跑稳，再让圆圈变漂亮。

---

## 25. 后续演进方向

MVP 后可以逐步扩展：

- tray icon；
- VS Code status bar；
- shell prompt segment；
- 多任务列表；
- session history；
- 每个 workspace 独立状态；
- mobile push；
- ntfy / Bark / Telegram backend；
- 插件化 adapter；
- Gemini CLI / Aider / OpenHands 支持；
- 更精确的 Codex / Claude 事件解析。

---

## 26. 结论

Agent Orb 的 MVP 不应该被定义成一个“通知插件”。

更准确的定义是：

```text
一个跨平台 AI CLI Runtime Observer，通过本地 Daemon 汇聚 Claude / Codex 工作事件，并用桌面 Floating Orb 进行低打扰状态表达。
```

第一版的关键不是功能多，而是这条链路稳定：

```text
Wrapper -> Event -> Daemon -> State Machine -> Floating Orb
```

只要这条链路跑通，小宝后面要加 tray、dashboard、VS Code、shell prompt，都会很自然。

---
---

## 27. 轻量安装与 npx Bootstrapper

### 27.1 安装目标

MVP 不以 `.exe`、`.msi`、`.dmg` 等传统安装包作为主安装路径。

Agent Orb 的轻量安装目标是：

```bash
npx agent_orb
```

用户执行后进入交互式设置向导，选择为以下目标配置 Agent Orb：

```text
Codex CLI
Claude Code CLI
Both
```

这里的“安装”不是修改 Codex CLI / Claude Code CLI 内部，也不是替换它们的原始命令，而是为它们配置一个本地观察层 wrapper。

更准确的表达是：

```text
Set up Agent Orb for Codex CLI
Set up Agent Orb for Claude Code CLI
```

---

### 27.2 为什么不使用传统安装包

MVP 暂不将传统 GUI installer 作为主路径，原因包括：

- 安装包对早期用户过重；
- `.exe` / `.dmg` / notarization / signing 会显著增加发布复杂度；
- Agent Orb 的主要入口是 CLI 工作流，轻量命令式安装更符合目标用户习惯；
- MVP 更需要验证 `Wrapper -> Daemon -> Orb` 链路，而不是优先解决完整桌面软件分发。

MVP 推荐的分发方式是：

```text
npm / npx bootstrapper + portable native runtime
```

也就是说，用户不需要运行传统安装包，但本地仍会存在 native binary。

---

### 27.3 npx agent_orb 的职责

`npx agent_orb` 是一个轻量 bootstrapper / configurator。

它负责：

1. 检测 OS 与 CPU 架构；
2. 检查本地是否已有 Agent Orb portable runtime；
3. 如果没有，下载对应平台的 native bundle；
4. 校验 checksum；
5. 检测 Codex CLI / Claude Code CLI 是否已安装；
6. 让用户选择配置目标；
7. 写入 Agent Orb 配置；
8. 创建 wrapper command 或给出 shell alias 指引；
9. 初始化 config 与 local token；
10. 启动 daemon；
11. 启动 Floating Orb；
12. 执行 doctor 检查安装结果。

---

### 27.4 首次运行交互流程

用户执行：

```bash
npx agent_orb
```

示例交互：

```text
Agent Orb Setup

Detected:
  ✓ Codex CLI: found at /path/to/codex
  ✓ Claude Code CLI: found at /path/to/claude

? Choose target CLI:
  › Codex CLI
    Claude Code CLI
    Both

? How do you want to use Agent Orb?
  › Create wrapper commands only
    Add shell aliases
    Show manual instructions
```

默认选项应为：

```text
Create wrapper commands only
```

MVP 不应默认覆盖用户已有的 `codex` 或 `claude` 命令。

---

### 27.5 安装后的使用方式

如果用户选择 Codex CLI：

```bash
agent_orb run -- codex
```

可选提供更短 wrapper command：

```bash
codex-orb
```

等价于：

```bash
agent_orb run -- codex
```

如果用户选择 Claude Code CLI：

```bash
agent_orb run -- claude
```

可选提供：

```bash
claude-orb
```

如果用户选择 Both，则同时配置：

```text
codex-orb
claude-orb
```

---

### 27.6 不默认替换原始命令

MVP 不应默认执行以下行为：

```text
codex  -> agent_orb run -- codex
claude -> agent_orb run -- claude
```

原因：

- 这会改变用户已有工作流；
- wrapper 初期可能存在交互兼容问题；
- 一旦出错，用户可能误以为 Codex / Claude 本身损坏；
- 卸载与恢复会变复杂。

推荐策略：

```text
默认：生成 codex-orb / claude-orb 或提示 agent_orb run -- <target>
可选：用户确认后写入 shell alias
高级：提供 uninstall / restore
```

---

### 27.7 Bootstrapper 架构

运行时核心架构保持不变：

```text
CLI Wrapper -> Local Daemon -> State Machine -> Floating Orb
```

新增安装与配置层：

```text
npx agent_orb
  ↓
Installer / Configurator
  ↓
Platform Detection
  ↓
Download Portable Native Runtime
  ↓
Choose Codex / Claude Adapter
  ↓
Create Wrapper Command / Alias / Config
  ↓
Run Agent Orb
```

整体架构：

```text
NPM Bootstrapper
├── interactive setup
├── platform detector
├── binary downloader
├── checksum verifier
├── adapter installer
│   ├── codex adapter
│   └── claude adapter
└── command proxy

Native Runtime
├── agent_orb CLI wrapper
├── agent_orbd daemon
└── agent_orb_ui floating orb
```

---

### 27.8 Native Bundle 结构

GitHub Releases 或等价发布源提供 portable native bundle。

示例：

```text
agent-orb-windows-x64.zip
agent-orb-macos-arm64.tar.gz
agent-orb-macos-x64.tar.gz
agent-orb-linux-x64.tar.gz
checksums.txt
```

每个 bundle 内包含：

```text
agent_orb
agent_orbd
agent_orb_ui
```

Windows 下为：

```text
agent_orb.exe
agent_orbd.exe
agent_orb_ui.exe
```

注意：这里存在 `.exe` 可执行文件，但它不是传统安装包。它由 bootstrapper 下载到用户目录并执行，用户不需要手动安装。

---

### 27.9 本地目录约定

Windows：

```text
%LOCALAPPDATA%/agent-orb/bin/
%APPDATA%/agent-orb/config.toml
%APPDATA%/agent-orb/token
```

macOS：

```text
~/Library/Application Support/agent-orb/bin/
~/Library/Application Support/agent-orb/config.toml
~/Library/Application Support/agent-orb/token
```

Linux：

```text
~/.local/share/agent-orb/bin/
~/.config/agent-orb/config.toml
~/.config/agent-orb/token
```

---

### 27.10 Adapter Profile

为了支持用户选择 Codex CLI / Claude Code CLI，MVP 引入 Adapter Profile。

Adapter Profile 描述一个可被 Agent Orb 观察的目标 CLI。

Codex 示例：

```json
{
  "name": "codex",
  "display_name": "Codex CLI",
  "binary_candidates": ["codex", "codex.exe"],
  "wrapper_command": "codex-orb",
  "default_args": [],
  "prompt_patterns": ["approve", "permission", "continue?", "yes/no"]
}
```

Claude 示例：

```json
{
  "name": "claude",
  "display_name": "Claude Code CLI",
  "binary_candidates": ["claude", "claude.exe"],
  "wrapper_command": "claude-orb",
  "default_args": [],
  "prompt_patterns": ["continue?", "permission", "press enter", "approve"]
}
```

后续支持 Gemini CLI、Aider、OpenHands 时，可以通过新增 adapter 扩展。

---

### 27.11 配置文件示例

```toml
[install]
method = "npx"
version = "0.1.0"

[adapters.codex]
enabled = true
binary = "codex"
wrapper = "codex-orb"

[adapters.claude]
enabled = true
binary = "claude"
wrapper = "claude-orb"

[daemon]
host = "127.0.0.1"
port = 17321
auto_start = true

[orb]
auto_start = true
```

---

### 27.12 Bootstrapper 命令设计

MVP 建议支持：

```bash
npx agent_orb
npx agent_orb setup
npx agent_orb setup codex
npx agent_orb setup claude
npx agent_orb setup --all
npx agent_orb doctor
npx agent_orb uninstall
npx agent_orb upgrade
```

如果用户选择全局安装：

```bash
npm install -g agent_orb
agent_orb setup
agent_orb run -- codex
agent_orb doctor
```

说明：

- `npx agent_orb` 默认等价于 `npx agent_orb setup`；
- `doctor` 用于检查 runtime、daemon、orb、adapter 和 token；
- `uninstall` 用于清理 Agent Orb 自己创建的文件，不应删除用户的 Codex / Claude CLI。

---

### 27.13 安全校验

Bootstrapper 下载 native bundle 时必须进行校验。

MVP 至少要求：

- 下载 `checksums.txt`；
- 校验 bundle SHA256；
- 下载失败或校验失败时中止安装；
- 不执行校验失败的 binary；
- 不从 API 接收任意命令执行指令。

后续可增强：

- release signature；
- npm provenance；
- signed manifest；
- binary transparency log。

---

### 27.14 升级策略

MVP 支持显式升级：

```bash
npx agent_orb upgrade
```

升级流程：

```text
1. 获取最新 release manifest
2. 比较本地 runtime version
3. 下载新 bundle
4. 校验 checksum
5. 停止旧 daemon / UI
6. 替换 binary
7. 启动新 daemon / UI
8. 保留 config 和 token
```

MVP 不需要默认静默自动升级。

---

### 27.15 卸载策略

卸载命令：

```bash
npx agent_orb uninstall
```

卸载范围：

- 停止 `agent_orbd`；
- 停止 `agent_orb_ui`；
- 删除 Agent Orb portable runtime；
- 删除 Agent Orb 创建的 wrapper command / shim；
- 可选删除 config 与 token；
- 不删除 Codex CLI；
- 不删除 Claude Code CLI。

如果修改过 shell profile，必须提供 restore。

---

### 27.16 MVP 推荐分发路径

MVP 推荐路径：

```text
Primary: npx agent_orb interactive setup
Daily use: agent_orb run -- <target> 或 codex-orb / claude-orb
Developer: cargo build / tauri dev
Optional later: Homebrew / Scoop / AUR
Not primary: exe / dmg installer
```

最终用户体验目标：

```bash
npx agent_orb
# 选择 Codex CLI / Claude Code CLI / Both
codex-orb
# 或
claude-orb
```

---

## 28. 更新后的 MVP 结论

Agent Orb MVP 的核心仍然是：

```text
一个跨平台 AI CLI Runtime Observer，通过本地 Daemon 汇聚 Claude / Codex 工作事件，并用桌面 Floating Orb 进行低打扰状态表达。
```

但安装与配置方式调整为：

```text
npx agent_orb -> interactive setup -> portable native runtime -> adapter wrapper
```

因此第一版最重要的链路变为：

```text
npx agent_orb
  -> choose Codex / Claude
  -> setup adapter
  -> Wrapper -> Event -> Daemon -> State Machine -> Floating Orb
```

