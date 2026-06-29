# Agent Orb MVP 开发计划

> 来源：`docs/AGENT_ORB_MVP.md`  
> 目标：将 MVP 设计拆解为可执行、可验收、可逐步交付的开发计划  
> 原则：先稳定链路，再完善体验；先 Core/Daemon，再 CLI/UI

---

## 1. 开发总目标

Agent Orb MVP 的开发目标，是跑通以下最小闭环：

```text
agent_orb run -- <command>
        ↓
CLI Wrapper 捕获进程事件
        ↓
Local Daemon 接收事件并更新状态机
        ↓
Floating Orb UI 获取状态
        ↓
桌面圆圈按颜色和动画反馈状态
```

MVP 第一优先级不是 UI 炫酷，而是这条链路稳定：

```text
Wrapper -> Event -> Daemon -> State Machine -> Floating Orb
```

---

## 2. 开发原则

1. **先 Core，后 UI**  
   状态机和事件协议是地基，Floating Orb 只是状态表达层。

2. **先轮询，后 WebSocket**  
   MVP 可以先用 `/v1/status` 轮询跑通链路，WebSocket 后置。

3. **先支持任意 CLI，再验证 Codex / Claude**  
   先用 `echo`、`powershell`、`sh` 这类稳定命令验证 wrapper，再接入真实 AI CLI。

4. **prompt detection 只作为弱信号**  
   `waiting_input` 不能被设计成强断言，只能是 heuristic。

5. **安全默认开启**  
   Daemon 默认只监听 `127.0.0.1`，写接口必须校验 local token。

---

## 3. 推荐里程碑

```text
Phase 0: 项目骨架与约束确认
Phase 1: agent-orb-core 核心模型与状态机
Phase 2: agent-orb-daemon 本地服务
Phase 3: agent-orb-cli Wrapper
Phase 4: Floating Orb UI
Phase 5: 端到端集成
Phase 6: 测试、文档与 Dev Build
```

---

## 4. Phase 0：项目骨架与约束确认

### 目标

建立工程结构，明确 Rust workspace、Tauri app、文档和示例事件的位置。

### 任务 0.1 初始化 Rust Workspace

任务：

- 创建根 `Cargo.toml`。
- 创建 workspace members：
  - `crates/agent-orb-core`
  - `crates/agent-orb-daemon`
  - `crates/agent-orb-cli`
- 配置 workspace resolver 为 `2`。

产出：

```text
Cargo.toml
crates/agent-orb-core/
crates/agent-orb-daemon/
crates/agent-orb-cli/
```

验收：

```bash
cargo check --workspace
```

---

### 任务 0.2 初始化 Tauri UI 工程

任务：

- 创建 `apps/agent-orb-ui`。
- 使用 `Vite + TypeScript + Tauri`。
- 暂时只显示一个普通页面。

产出：

```text
apps/agent-orb-ui/
apps/agent-orb-ui/src-tauri/
apps/agent-orb-ui/src/
```

验收：

```bash
npm install
npm run tauri dev
```

---

### 任务 0.3 创建基础文档与示例目录

任务：

- 创建 `README.md`。
- 创建 `examples/config.toml`。
- 创建示例事件 JSON：
  - `examples/events/session-started.json`
  - `examples/events/output-received.json`
  - `examples/events/process-exited.json`

验收：

- 示例 JSON 能被 `agent-orb-core` 正确解析。

---

## 5. Phase 1：agent-orb-core 核心模型与状态机

### 目标

完成事件协议、状态模型、视觉映射、状态机和配置模型。

这是 MVP 的地基，应优先完成并覆盖单元测试。

---

### 任务 1.1 定义 Source 类型

任务：

- 定义 `Source` enum：
  - `Codex`
  - `Claude`
  - `Generic`
- 支持 `serde` 序列化和反序列化。

建议文件：

```text
crates/agent-orb-core/src/source.rs
```

验收：

- JSON 中的 `"codex"` 能解析为 `Source::Codex`。
- JSON 中的 `"claude"` 能解析为 `Source::Claude`。
- 未知 source 可降级为 `Generic` 或返回明确错误，具体策略需在代码中保持一致。

---

### 任务 1.2 定义 EventType

任务：

定义事件类型：

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

建议文件：

```text
crates/agent-orb-core/src/event.rs
```

验收：

- 所有事件类型可从文档约定字符串解析。
- 未知事件返回明确错误。

---

### 任务 1.3 定义 EventEnvelope

任务：

实现事件信封结构：

```text
version
event_id
session_id
source
workspace
event_type
timestamp
payload
```

建议文件：

```text
crates/agent-orb-core/src/event.rs
```

验收：

- 能解析 `examples/events/session-started.json`。
- 能解析 `examples/events/output-received.json`。
- 能解析 `examples/events/process-exited.json`。

---

### 任务 1.4 定义 InternalStatus

任务：

定义内部状态：

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

建议文件：

```text
crates/agent-orb-core/src/status.rs
```

验收：

- `serde` 输出符合 API 约定，例如 `active`、`waiting_input`。

---

### 任务 1.5 定义 VisualStatus

任务：

定义视觉状态：

```text
disconnected
idle
starting
blue_spinning
purple_spinning
yellow_pulse
green_done
red_error
orange_warning
cancelled
```

建议文件：

```text
crates/agent-orb-core/src/visual.rs
```

验收：

- `InternalStatus::Active -> VisualStatus::BlueSpinning`
- `InternalStatus::Silent -> VisualStatus::PurpleSpinning`
- `InternalStatus::Failed -> VisualStatus::RedError`

---

### 任务 1.6 实现状态机

任务：

实现基础状态转换：

```text
idle + session.started -> starting
starting + output.received -> active
active + idle.timeout -> silent
silent + prompt.detected -> waiting_input
waiting_input + output.received -> active
active/silent/waiting_input + process.exited code=0 -> completed
active/silent/waiting_input + process.exited code!=0 -> failed
silent + stuck.timeout -> stuck
```

建议文件：

```text
crates/agent-orb-core/src/state_machine.rs
```

验收：

```bash
cargo test -p agent-orb-core
```

---

### 任务 1.7 实现配置模型

任务：

定义并加载配置：

```text
daemon
orb
colors
behavior
privacy
```

要求：

- 支持 TOML。
- 提供默认值。
- 缺失字段可回退默认值。

建议文件：

```text
crates/agent-orb-core/src/config.rs
```

验收：

- 能加载 `examples/config.toml`。
- 缺失配置项能回退默认值。

---

## 6. Phase 2：agent-orb-daemon 本地服务

### 目标

完成本地 API、状态存储、token 校验和多 session 优先级。

Daemon 是 MVP 的中枢，负责将 wrapper 事件转化为 UI 可消费的状态。

---

### 任务 2.1 创建 Daemon 启动入口

任务：

- 使用 `axum` 创建 HTTP server。
- 默认监听 `127.0.0.1:17321`。
- 禁止默认监听 `0.0.0.0`。

建议文件：

```text
crates/agent-orb-daemon/src/main.rs
```

验收：

```bash
cargo run -p agent-orb-daemon
curl http://127.0.0.1:17321/health
```

---

### 任务 2.2 实现 `/health`

任务：

返回 daemon 健康状态。

响应示例：

```json
{
  "ok": true,
  "version": "0.1.0"
}
```

验收：

```bash
curl http://127.0.0.1:17321/health
```

---

### 任务 2.3 实现 token 生成与读取

任务：

- 首次启动生成 local token。
- 保存到 config dir 下的 `token` 文件。
- 后续启动复用。
- macOS / Linux 设置 `0600` 权限。
- Windows 至少保证当前用户可读。

建议文件：

```text
crates/agent-orb-daemon/src/security.rs
```

验收：

- 首次启动会生成 token 文件。
- 第二次启动不会重置 token。

---

### 任务 2.4 实现 Authorization 校验

任务：

写接口必须校验：

```http
Authorization: Bearer <local-token>
```

影响接口：

```text
POST /v1/events
POST /v1/status/clear
```

验收：

```bash
curl -X POST http://127.0.0.1:17321/v1/events
```

预期：

```text
401 Unauthorized
```

---

### 任务 2.5 实现 SessionStore

任务：

维护 session 状态。

每个 session 至少包含：

```text
session_id
source
workspace
status
started_at
updated_at
last_output_at
exit_code
```

建议文件：

```text
crates/agent-orb-daemon/src/session_store.rs
```

验收：

- 提交两个 session 的事件后，store 能分别保存。

---

### 任务 2.6 实现多 Session 展示优先级

任务：

按优先级选择全局展示状态：

```text
failed > waiting_input > stuck > active > silent > completed > starting > idle
```

同优先级时展示最近更新的 session。

验收：

- 一个 session 为 `active`，另一个为 `failed`，全局状态为 `failed`。
- 两个 session 同为 `active`，展示最近更新的 session。

---

### 任务 2.7 实现 `POST /v1/events`

任务：

- 接收 `EventEnvelope`。
- 校验 token。
- 更新 `SessionStore`。
- 推进状态机。
- 返回 `{ "ok": true }`。

建议文件：

```text
crates/agent-orb-daemon/src/api.rs
```

验收：

```bash
curl -X POST http://127.0.0.1:17321/v1/events \
  -H "Authorization: Bearer <token>" \
  -H "Content-Type: application/json" \
  -d @examples/events/session-started.json
```

---

### 任务 2.8 实现 `GET /v1/status`

任务：

返回当前全局状态。

响应字段：

```text
status
visual
source
workspace
session_id
started_at
updated_at
message
```

验收：

```bash
curl http://127.0.0.1:17321/v1/status \
  -H "Authorization: Bearer <token>"
```

---

### 任务 2.9 实现 `POST /v1/status/clear`

任务：

- 清除 `completed` / `failed` 状态。
- 清除后回到 `idle` 或下一个优先级 session。

验收：

- `failed` 状态下调用 clear，状态不再显示 `failed`。

---

### 任务 2.10 实现 WebSocket `/v1/status/stream`

任务：

- UI 可通过 websocket 订阅状态变化。
- 推送 `status.changed` 消息。

说明：

MVP 可先用轮询实现 UI，WebSocket 不应阻塞最小闭环。

验收：

- 提交事件后 websocket 客户端收到 `status.changed`。

---

## 7. Phase 3：agent-orb-cli Wrapper

### 目标

实现 `agent_orb run -- <command>`，将任意 CLI 进程变成可观察对象。

---

### 任务 3.1 实现 CLI 参数解析

任务：

支持命令：

```bash
agent_orb run -- <command> [...args]
```

要求：

- `--` 后面的参数原样透传给目标命令。

建议 crate：

```text
clap
```

验收：

```bash
agent_orb run -- echo hello
```

目标命令被解析为：

```text
echo hello
```

---

### 任务 3.2 实现 daemon 探活

任务：

- 启动 wrapper 时先请求 `/health`。
- 如果 daemon 不存在，尝试自动拉起。
- 自动拉起失败时输出清晰错误。

验收：

- daemon 已运行时直接复用。
- daemon 未运行时 wrapper 能拉起，或明确提示失败原因。

---

### 任务 3.3 实现子进程 spawn

任务：

- 启动目标 CLI。
- 记录 pid。
- 记录 workspace。
- 生成 session_id。
- 上报 `session.started`。

验收：

```bash
agent_orb run -- echo hello
```

Daemon 收到 `session.started`。

---

### 任务 3.4 捕获 stdout 并原样转发

任务：

- 捕获子进程 stdout。
- 原样写回当前 terminal。
- 上报 `output.received`。
- 默认不包含 sample，除非配置允许。

验收：

```bash
agent_orb run -- echo hello
```

预期：

- Terminal 显示 `hello`。
- Daemon 收到 `output.received`。

---

### 任务 3.5 捕获 stderr 并原样转发

任务：

- 捕获子进程 stderr。
- 原样写回当前 terminal。
- 上报 `stderr.received`。

验收：

- 执行一个输出 stderr 的命令时，terminal 能看到原始 stderr。
- Daemon 收到 `stderr.received`。

---

### 任务 3.6 实现 exit code 监听

任务：

- 等待子进程退出。
- 获取 exit code。
- 上报 `process.exited`。
- wrapper 自身退出码尽量等于子进程退出码。

验收：

```bash
agent_orb run -- powershell -Command "exit 3"
```

预期：

- Wrapper 最终 exit code 为 `3`。
- Daemon 状态为 `failed`。

---

### 任务 3.7 实现 idle timeout 检测

任务：

- 记录 last output time。
- 超过 `silent_threshold_seconds` 后上报 `idle.timeout`。
- 避免重复高频上报。

验收：

```bash
agent_orb run -- powershell -Command "Start-Sleep 25; Write-Output done"
```

预期：

- 状态先变 `silent`。
- 输出后变回 `active` 或进入 `completed`。

---

### 任务 3.8 实现 stuck timeout 检测

任务：

- 超过 `stuck_threshold_seconds` 后上报 `stuck.timeout`。
- 只对仍存活进程生效。

验收：

- 长时间无输出进程最终变为 `stuck`。

---

### 任务 3.9 实现 prompt detection 初版

任务：

对 stdout / stderr 文本块做弱匹配。

默认规则：

```text
?
confirm
continue?
yes/no
approve
permission
press enter
```

验收：

- 模拟输出 `continue?` 后状态变为 `waiting_input`。

注意：

`prompt.detected` 是 heuristic，不应被当成真实等待输入的强证明。

---

### 任务 3.10 stdin / 交互兼容初版

任务：

- 保持目标 CLI 基础交互能力。
- 支持用户输入继续传给子进程。
- Windows PowerShell 优先。

验收：

```bash
agent_orb run -- powershell -Command "Read-Host 'continue?'"
```

预期：

- 用户可以正常输入。
- 子进程可以继续执行。

风险：

如果普通 pipe 无法满足真实 CLI 交互体验，后续可能需要引入 PTY 方案。

---

## 8. Phase 4：Floating Orb UI

### 目标

桌面出现一个可工作的状态圆圈，并能随 daemon 状态变化颜色和动画。

---

### 任务 4.1 创建透明无边框窗口

任务：

配置 Tauri window：

```text
size: 36 x 36
transparent: true
decorations: false
resizable: false
always_on_top: true
position: top-right
```

验收：

- 启动 UI 后桌面右上角出现透明背景圆圈区域。

---

### 任务 4.2 实现 Orb 基础 DOM / CSS

任务：

实现三层结构：

```text
outer glow
ring
core
```

建议文件：

```text
apps/agent-orb-ui/src/orb.ts
apps/agent-orb-ui/src/style.css
```

验收：

- 页面显示 36px 圆形 orb。

---

### 任务 4.3 实现状态到颜色映射

任务：

根据 visual state 应用 CSS variables：

```text
gray
blue
purple
yellow
green
red
orange
```

验收：

- mock 不同状态时颜色变化正确。

---

### 任务 4.4 实现动画映射

任务：

状态与动画映射：

```text
active -> 蓝色匀速旋转
silent -> 紫色慢速旋转
waiting_input -> 黄色脉冲
completed -> 绿色静止发光
failed -> 红色闪烁
stuck -> 橙色慢闪
idle -> 灰色静止
```

验收：

- 使用 mock status 能看到对应动画。

---

### 任务 4.5 实现 Daemon API Client

任务：

- 读取 local token。
- 请求 `/v1/status`。
- 处理 daemon disconnected。
- MVP 初版可每 500ms 或 1000ms 轮询。

建议文件：

```text
apps/agent-orb-ui/src/api.ts
```

验收：

- Daemon 状态变化后，Orb 在 1 秒内更新。

---

### 任务 4.6 实现点击行为

任务：

- `failed`：调用 clear。
- `completed`：调用 clear。
- `running`：显示小浮层，包含 source / workspace / duration。
- `idle`：显示简单菜单。

验收：

- `failed` 状态点击后能恢复 `idle` 或下一个优先级状态。

---

### 任务 4.7 实现拖拽位置

任务：

- 支持拖动 orb。
- 保存位置到配置或 local storage。
- 下次启动恢复。

验收：

- 拖动后重启 UI，位置保持。

---

## 9. Phase 5：端到端集成

### 目标

跑通完整链路：

```text
Wrapper -> Event -> Daemon -> State Machine -> Orb
```

---

### 任务 5.1 打通 echo happy path

任务：

```bash
agent_orb run -- echo hello
```

验收：

- Orb 显示 `starting`。
- Orb 显示 `active`。
- Orb 显示 `completed`。
- 10 秒后回 `idle`。

---

### 任务 5.2 打通 silent path

任务：

```powershell
agent_orb run -- powershell -Command "Start-Sleep 25; Write-Output done"
```

验收：

- 前期进入 `starting` / `active`。
- 20 秒后进入 `silent`，Orb 紫色慢转。
- 输出后进入 `active` 或 `completed`。
- 退出后进入 `completed`。

---

### 任务 5.3 打通 failed path

任务：

```powershell
agent_orb run -- powershell -Command "Write-Error boom; exit 1"
```

验收：

- Orb 显示红色闪烁。
- 点击后清除 failed。

---

### 任务 5.4 打通 waiting_input path

任务：

```powershell
agent_orb run -- powershell -Command "Read-Host 'continue?'"
```

验收：

- Orb 显示黄色脉冲。
- 用户输入后进程继续。

---

### 任务 5.5 打通 Codex CLI

任务：

```bash
agent_orb run -- codex
```

验收：

- 能显示 `active` / `silent` / `completed` / `failed`。
- terminal 原始交互体验尽量不变。

---

### 任务 5.6 打通 Claude Code CLI

任务：

```bash
agent_orb run -- claude
```

验收：

- 能显示 `active` / `silent` / `completed` / `failed`。
- terminal 原始交互体验尽量不变。

---

## 10. Phase 6：测试、文档与 Dev Build

### 目标

让 MVP 可交付、可复现、可验证。

---

### 任务 6.1 core 单元测试

覆盖：

```text
event parsing
status serde
visual mapping
state machine transitions
config loading
```

验收：

```bash
cargo test -p agent-orb-core
```

---

### 任务 6.2 daemon 集成测试

覆盖：

```text
/health
POST /v1/events
GET /v1/status
POST /v1/status/clear
invalid token
multiple session priority
```

验收：

```bash
cargo test -p agent-orb-daemon
```

---

### 任务 6.3 cli wrapper 测试

覆盖：

```text
参数透传
stdout 转发
stderr 转发
exit code 保持
idle timeout
prompt detection
```

验收：

```bash
cargo test -p agent-orb-cli
```

---

### 任务 6.4 手工验收脚本

任务：

创建脚本：

```text
scripts/manual-check-windows.ps1
scripts/manual-check-unix.sh
```

验收：

- 一键跑基础状态路径。

---

### 任务 6.5 README 补全

README 至少包含：

```text
项目定位
架构图
安装方式
运行 daemon
运行 UI
wrapper 用法
配置文件路径
安全与隐私说明
MVP 已知限制
```

验收：

- 新用户按 README 能跑通 `agent_orb run -- echo hello`。

---

### 任务 6.6 Dev Build 打包

任务：

- 生成 Windows dev build。
- 如条件允许，生成 macOS / Linux dev build。
- 不强求 notarization。

验收：

- 至少一个平台能安装并启动 Floating Orb。

---

## 11. 最小闭环版本

如果需要最快看到效果，建议先实现以下最小版本：

```text
1. core 状态机
2. daemon /health /v1/events /v1/status
3. cli wrapper 支持 echo + stdout + exit code
4. ui 轮询 status 并变色
```

第一版可以暂时不做：

```text
WebSocket
拖拽位置保存
复杂点击菜单
多平台打包
prompt detection 高精度
完整 PTY 交互
```

最小闭环验收命令：

```bash
agent_orb run -- echo hello
```

预期链路：

```text
wrapper emits events
        ↓
daemon updates status
        ↓
orb changes color
```

---

## 12. 第一周建议排期

### Day 1

- 建 Rust workspace。
- 建 `agent-orb-core` crate。
- 定义 event / status / visual / config 基础类型。

### Day 2

- 实现状态机。
- 编写 core 单元测试。

### Day 3

- 建 `agent-orb-daemon`。
- 实现 `/health`。
- 实现 token 生成与 auth middleware。

### Day 4

- 实现 `/v1/events`。
- 实现 `/v1/status`。
- 接入 `SessionStore`。

### Day 5

- 建 `agent-orb-cli`。
- 跑通 `agent_orb run -- echo hello`。

### Day 6

- 实现 stdout / stderr / exit code。
- 实现 idle timeout。

### Day 7

- 做最简 UI。
- UI 轮询 `/v1/status`。
- Orb 根据状态变色。

---

## 13. 风险与后置项

| 风险 | 影响 | 建议 |
|---|---|---|
| Windows 交互式 CLI 包装复杂 | Codex / Claude 交互体验受影响 | 先 pipe，必要时引入 PTY |
| prompt detection 误判 | Orb 错误显示等待输入 | 明确标注 heuristic，并允许配置规则 |
| Tauri 透明窗口跨平台差异 | UI 开发延期 | MVP 先保证 Windows 11 单显示器 |
| 多 session 状态冲突 | 用户看到的全局状态不符合预期 | 严格实现优先级和最近更新时间规则 |
| token 文件权限处理差异 | 本地 API 安全风险 | 平台分别处理，写测试覆盖 |
| stdout / stderr 泄漏敏感信息 | 隐私风险 | 默认不采集 sample，开启后截断和脱敏 |

---

## 14. 当前推荐下一步

建议从 Phase 0 开始：

```text
1. 检查当前仓库结构
2. 创建 Rust workspace
3. 创建 agent-orb-core
4. 先实现 Event / Status / State Machine
```

只要 `core` 状态机稳定，后续 Daemon、Wrapper、UI 都会变得清晰。

---
---

## 15. Phase 7：Lightweight Distribution & npx Bootstrapper

### 目标

实现轻量安装路径，让用户可以通过：

```bash
npx agent_orb
```

进入交互式设置流程，并选择为 Codex CLI、Claude Code CLI 或两者配置 Agent Orb。

Phase 7 不替代运行时核心架构，而是在运行时前面新增一层 installer / configurator。

---

### 任务 7.1 创建 npm bootstrapper package

任务：

- 创建 npm package：`agent_orb`。
- 暴露 bin command：`agent_orb`。
- `npx agent_orb` 默认执行 setup。
- 使用 TypeScript 或 JavaScript 实现 bootstrapper。

建议目录：

```text
packages/agent_orb-bootstrapper/
├── package.json
├── src/
│   ├── index.ts
│   ├── setup.ts
│   ├── platform.ts
│   ├── download.ts
│   ├── checksum.ts
│   ├── adapter.ts
│   └── doctor.ts
└── bin/
    └── agent_orb.js
```

验收：

```bash
npx agent_orb
```

可以启动 setup 流程。

---

### 任务 7.2 实现 platform detection

任务：

- 检测 OS：Windows / macOS / Linux。
- 检测 CPU 架构：x64 / arm64。
- 映射到对应 native bundle 名称。

验收：

- Windows x64 映射到 `agent-orb-windows-x64.zip`。
- macOS arm64 映射到 `agent-orb-macos-arm64.tar.gz`。
- Linux x64 映射到 `agent-orb-linux-x64.tar.gz`。

---

### 任务 7.3 实现 native bundle download

任务：

- 从 GitHub Releases 或配置的 release endpoint 下载 portable runtime。
- 下载到临时目录。
- 解压到 Agent Orb 本地 bin 目录。

本地目录：

```text
Windows: %LOCALAPPDATA%/agent-orb/bin/
macOS: ~/Library/Application Support/agent-orb/bin/
Linux: ~/.local/share/agent-orb/bin/
```

验收：

- 首次运行会下载对应平台 bundle。
- 第二次运行会复用已下载 runtime。

---

### 任务 7.4 实现 checksum 校验

任务：

- 下载 `checksums.txt`。
- 对 native bundle 计算 SHA256。
- 校验失败时中止安装。
- 不执行校验失败的 binary。

验收：

- 正确 bundle 可通过校验。
- 篡改 bundle 后安装失败。

---

### 任务 7.5 实现 Codex / Claude 检测

任务：

- 检测 `codex` 是否存在于 PATH。
- 检测 `claude` 是否存在于 PATH。
- Windows 同时检测 `.exe`。
- 输出检测结果与路径。

验收：

```text
✓ Codex CLI: found at ...
✓ Claude Code CLI: found at ...
```

如果未找到，提示安装目标 CLI 或选择跳过。

---

### 任务 7.6 实现交互式目标选择

任务：

让用户选择：

```text
Codex CLI
Claude Code CLI
Both
```

要求：

- 如果只检测到 Codex，默认选 Codex。
- 如果只检测到 Claude，默认选 Claude。
- 如果都检测到，默认可选 Both 或让用户明确选择。
- 如果都没检测到，进入 manual mode。

验收：

```bash
npx agent_orb
```

能通过交互选择配置目标。

---

### 任务 7.7 引入 Adapter Profile

任务：

新增 adapter profile 模型。

Codex profile 至少包含：

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

Claude profile 至少包含：

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

验收：

- setup 选择 Codex 后写入 Codex adapter config。
- setup 选择 Claude 后写入 Claude adapter config。
- setup 选择 Both 后同时写入两个 adapter config。

---

### 任务 7.8 写入安装配置

任务：

生成或更新 `config.toml`。

示例：

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

验收：

- 选择 Codex 后 `[adapters.codex] enabled = true`。
- 选择 Claude 后 `[adapters.claude] enabled = true`。
- 不覆盖用户已有无关配置。

---

### 任务 7.9 创建 wrapper command / shim

任务：

为已启用 adapter 创建 wrapper command 或 shim。

目标：

```text
codex-orb -> agent_orb run -- codex
claude-orb -> agent_orb run -- claude
```

要求：

- 不默认覆盖 `codex`。
- 不默认覆盖 `claude`。
- 如果需要写 shell profile，必须让用户确认。

验收：

```bash
codex-orb
claude-orb
```

能调用 Agent Orb wrapper。

---

### 任务 7.10 实现 daemon / orb 启动

任务：

- setup 完成后启动 `agent_orbd`。
- setup 完成后启动 `agent_orb_ui`。
- 如果已启动，避免重复启动。
- 端口冲突时给出明确提示。

验收：

- `npx agent_orb` 完成后 daemon 可访问 `/health`。
- Floating Orb 出现在桌面。

---

### 任务 7.11 实现 doctor 命令

任务：

支持：

```bash
npx agent_orb doctor
```

检查项：

```text
platform
native runtime exists
runtime version
checksum status
daemon health
orb process
config path
token exists
codex adapter
claude adapter
```

验收：

- doctor 能输出可读诊断报告。
- 缺少 Codex / Claude 时给出修复建议。

---

### 任务 7.12 实现 uninstall 命令

任务：

支持：

```bash
npx agent_orb uninstall
```

卸载范围：

- 停止 `agent_orbd`。
- 停止 `agent_orb_ui`。
- 删除 Agent Orb portable runtime。
- 删除 Agent Orb 创建的 shim。
- 可选删除 config 与 token。
- 不删除 Codex CLI。
- 不删除 Claude Code CLI。

验收：

- uninstall 后 Agent Orb runtime 被清理。
- Codex / Claude CLI 仍可正常使用。

---

### 任务 7.13 实现 upgrade 命令

任务：

支持：

```bash
npx agent_orb upgrade
```

升级流程：

```text
1. 获取 release manifest
2. 比较本地 runtime version
3. 下载新 bundle
4. 校验 checksum
5. 停止旧 daemon / UI
6. 替换 binary
7. 启动新 daemon / UI
8. 保留 config 和 token
```

验收：

- 本地 runtime 可升级。
- config 和 token 不丢失。

---

### 任务 7.14 发布 native portable bundles

任务：

CI 生成以下产物：

```text
agent-orb-windows-x64.zip
agent-orb-macos-arm64.tar.gz
agent-orb-macos-x64.tar.gz
agent-orb-linux-x64.tar.gz
checksums.txt
```

每个 bundle 包含：

```text
agent_orb
agent_orbd
agent_orb_ui
```

验收：

- bootstrapper 能下载并解压所有目标平台 bundle。
- checksum 与 release artifact 匹配。

---

### 任务 7.15 端到端安装验收

任务：

在干净环境执行：

```bash
npx agent_orb
```

选择 Codex CLI。

验收：

```bash
codex-orb
```

预期：

- daemon 自动启动；
- Floating Orb 自动启动；
- Codex CLI 正常运行；
- Orb 能显示 active / silent / completed / failed。

再执行：

```bash
npx agent_orb doctor
```

预期：

- 所有核心检查通过。

---

## 16. 更新后的优先级建议

轻量安装会影响 MVP 的交付顺序。

推荐顺序调整为：

```text
1. agent-orb-core: event/status/state machine
2. agent-orb-daemon: local API
3. agent-orb-cli: wrapper process monitor
4. agent-orb-ui: minimal floating orb
5. integration: wrapper -> daemon -> UI
6. bootstrapper: npx agent_orb setup
7. distribution: native portable bundle
```

注意：

Bootstrapper 不应早于最小 runtime 闭环。否则会出现“安装流程很好，但安装后没有稳定功能”的问题。

---

## 17. 更新后的最小可演示版本

新的最小可演示版本为：

```text
1. cargo build 生成 native runtime
2. bootstrapper 能检测平台
3. bootstrapper 能找到本地 runtime 或下载 mock bundle
4. npx agent_orb 可选择 Codex / Claude
5. 写入 adapter config
6. agent_orb run -- codex 可跑通 echo / codex
7. Orb 能显示状态
```

最终验收命令：

```bash
npx agent_orb
codex-orb
```

或者：

```bash
npx agent_orb
claude-orb
```

