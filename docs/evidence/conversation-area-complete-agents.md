# `conversation-area-complete` — E7（真实 Codex / Oh My Pi）与 E8（Windows 终端/进程树）证据

本文件只登记**实际执行并观察到**的结果。未执行、未观察到或证据无效的项目在 §4 明确列出，
不得计为通过。所有测试都使用独立临时数据目录、独立端口与独立 TLS 自签证书；
没有修改主检出，也没有 commit。

## 0. 环境与固定输入

| 项 | 值 |
| --- | --- |
| 平台 | Windows 10.0.26300 x64 |
| Daemon | 主检出 `D:/Project/acp-remote/target/debug/acp-remote.exe`（`0.0.0`，UI build `0.0.0+71badcd-dirty`） |
| Fake ACP Agent | `D:/Project/acp-remote/target/debug/acpr-fake-acp-agent.exe` |
| 浏览器 | Microsoft Edge headless（`C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe`） |
| Oh My Pi | `omp/18.12.2`（`C:/Users/jack/AppData/Local/omp/omp.exe acp`） |
| Codex CLI | `codex-cli 0.162.0` |
| Codex ACP 适配器 | `@agentclientprotocol/codex-acp 2.2.2`（临时安装到 `D:/tmp/wp7-agents/tools`，非仓库依赖） |
| TLS | `openssl req -x509 ... -addext "subjectAltName=IP:127.0.0.1,DNS:localhost"`，浏览器 `--ignore-certificate-errors` |
| E7 隔离目录 | `D:/tmp/wp7-agents/e7`（data、配置、截图、日志） |
| E8 隔离目录 | `D:/tmp/wp7-agents/e8`（data、探针、心跳、截图、日志） |

Daemon 配置使用 `identity.keystore = "platform"`（跨重启保持设备信任）与
`dev_mode.enabled = true`；E7 端口 `18974`，E8 端口 `18975`。浏览器设备身份由测试脚本通过
CDP 写入与产品同形的 IndexedDB 记录（不可导出 ECDSA P-256 私钥 + 65 字节 SEC1 公钥），
再用 `acp-remote device pair --fingerprint <sha256(raw public key)>` 完成真实配对。

## 1. E7 — 真实 Codex 与 Oh My Pi

### 1.1 直接 ACP 初始化（真实进程，非假 Agent）

命令（`D:/tmp/wp7-agents`）：

```text
node probe.mjs omp   D:/tmp/wp7-agents/e7 "Reply with the single word PONG ..."
node probe.mjs codex D:/tmp/wp7-agents/e7 "Reply with the single word PONG ..."
```

原始日志：`D:/tmp/wp7-agents/e7/omp-probe.log`、`codex-probe.log`。

两台 Agent 都返回 `protocolVersion = 1`，并各自真实宣告：

| 能力路径 | Oh My Pi 18.12.2 | Codex ACP 2.2.2 |
| --- | --- | --- |
| `agentInfo` | `omp` 18.12.2 | `@agentclientprotocol/codex-acp` 2.2.2（title `Codex`） |
| `loadSession` | true | true |
| `promptCapabilities.image` | true | true |
| `promptCapabilities.audio` | **未宣告** | **未宣告** |
| `promptCapabilities.embeddedContext` | true | true |
| `sessionCapabilities` | `list` / `fork` / `resume` / `close` | `resume` / `list` / `close` / `delete` / `fork` / `additionalDirectories` / `subagents` |
| `mcpCapabilities` | `http=true`、`sse=true` | `http=true`、`sse=false`、`acp=false` |
| `authMethods` | `agent`（沿用本机已有凭据） | `api-key`、`chat-gpt` |
| 其他 | — | `_meta.steering.supported = true`；`auth.logout` |

文本闭环（同一份日志）：两台 Agent 都流式产出 `agent_message_chunk`，最终 `session/prompt`
返回 `stopReason = "end_turn"`。Oh My Pi 约 1.5 s 返回 `PONG`；Codex 约 5.8 s 返回 `PONG`。
据此可核对「文本」在 Agent 侧真实通过。

### 1.2 工具与 ACP 终端

命令：

```text
node probe2.mjs omp   D:/tmp/wp7-agents/e7 ws tool
node probe2.mjs codex D:/tmp/wp7-agents/e7 ws tool
```

日志：`D:/tmp/wp7-agents/e7/omp-tool.log`、`codex-tool.log`。

- Oh My Pi：真实发出 `session/request_permission`、`session/update{tool_call,kind=execute}`，
  随后发出 **`terminal/create`**（`command = .../bash.exe`，`args = ["-l","-c","pwd; ls -la ..."]`，
  `cwd = D:\tmp\wp7-agents\e7`，`outputByteLimit = 51200`）。这证明 OMP 走 ACP 客户端终端，
  而不是跳过终端的私有通道。该探针没有实现 `terminal/output` 的内容回放，进程最终以退出码 1
  结束，因此**不能**用这条日志断言工具输出内容；只能断言「OMP 真实请求了 terminal/create」。
- Codex ACP：本次提示词下没有产生 `tool_call` 或 `terminal/*`，只返回文本
  `What would you like me to do with the tools?`。因此本次**未观察到** Codex 使用 ACP 终端，
  不能据此宣称 Codex 终端支持或不支持。

### 1.3 停止（`session/cancel`）— 未取得通过证据

命令：

```text
node probe2.mjs omp   D:/tmp/wp7-agents/e7 ws stop
node probe2.mjs codex D:/tmp/wp7-agents/e7 ws stop
```

日志：`omp-stop.log`、`omp-stop2.log`、`codex-stop.log`。

实际观察：脚本在 `session/prompt` 后 300 ms 发送 `session/cancel`，两台 Agent 的最终
`session/prompt` 结果仍然是 `stopReason = "end_turn"`（OMP 约 2.4 s / Codex 约 12.8 s），
没有出现 `cancelled`。OMP 的两次提示都被模型直接以文本结束（第二次回复「已停止。没有正在运行的任务……」），
Codex 回复 `Stopped.`。因此本次**没有**取得「真实 Agent 停止」的可核对证据，不能记为通过。

### 1.4 真实 Daemon + 真实 OMP 的会话（页面）

命令（脚本 `D:/tmp/wp7-agents/e7/fresh.mjs`）：

```text
node fresh.mjs      # 真实 Daemon :18974 + 真实 Chromium + 真实 omp acp
```

可观察结果（`fresh-ws.log` 为页面 WebSocket 原始帧，截断到前 1500 字节/行）：

- 真实配对成功；真实创建目录 `D:\tmp\wp7-agents\e7\ws` 与 `session.create` 成功后进入
  `/session/56548066-30e7-4578-a5b5-e355dfe7a369`（fresh2 运行）。
- `session.prompt` 被 `accepted`（`acceptedAt` 非空），随后 `turn.started` 与 4 条
  `agent.message.delta`（文本 `FR`/`ES`/`HO`/`MP`）到达同一连接，最终
  `command.result.status = "completed"` 且 `terminalEventId` 非空。
- SQLite `owned_message` 在同一 `sessionId` 下有 `channel=reply`、`state=completed`、
  `content_json = [{"text":"FRESHOMP","type":"text"}]`。

**真实缺陷（页面渲染/读取屏障）**：同一页面的 DOM 在 25 s 后仍满足
`document.querySelectorAll('[data-message-text]').length === 0`；轮次卡片为 `completed`。
刷新/重新打开
`https://127.0.0.1:18974/session/bab5fcda-7a1f-4ecb-b23b-0e6718906841`
后页面显示「已经是最早的消息」「已经是最早的结构化条目」，`cards = []`、`messages = []`。
`inspect-ws.log` 中该次打开**没有**发出任何 `session.read`，只完成 auth/subscribe。

原因可从帧与代码定位：`turn.delta_compacted` 到达后，客户端
（`clients/app/src/features/client-store.ts` 的 `#withMessageLane`）删除该轮次的
`volatile` 增量并把轮次加入 `discardedTurns`，但**不触发** `session.read` 回填；
服务端已持久化的 `owned_message` 因此不会出现在页面上。真实 Daemon 每个 turn 都会产生
`turn.delta_compacted`（实测 8 字回复也产生），所以真实 Agent 的正文在页面上会消失。
这不是本次测试的输入问题：同一会话的 `session.read` 结果在首读时是 `messages: []`，
回复只在增量与 `owned_message` 中；帧里没有任何失败码掩盖这一点。

复现步骤：
1. 用上述隔离配置启动 Daemon，真实配对；
2. 新建 `omp` 会话并发送「Reply with exactly the token FRESHOMP」；
3. 等 `command.result` 为 `completed`；
4. 观察页面 `data-message-text` 数量为 0；刷新后仍为 0，而 SQLite 中消息存在。

### 1.5 代码签名与表单/媒体结论

- 表单/审批：Daemon 组合根只宣告 `clientCapabilities.terminal = true`
  （`crates/app/src/compose.rs`），未宣告 `elicitation`/`fs`。因此本次真实 Agent 会话中
  **没有**可用的表单能力；真实 Codex/OMP 也没有收到可应答的 elicitation 请求。
  这是「明确不支持」而不是「未测试」。
- 媒体：两台 Agent 初始化都宣告 `image = true`、`embeddedContext = true`，
  音频**未宣告**。本次没有通过产品上传图片/音频做端到端断言；只记录了初始化能力。
- 历史：服务端持久化成立（见 §1.4 的 SQLite 与 `fresh-ws.log`），页面读取路径不成立（同上）。

**E7 结论（不得勾选 7.3）**：真实 Agent 的初始化能力与文本流通过；OMP 的 ACP 终端请求、
Codex 的文本流可核对；真实 Daemon 的 Sync 帧与 SQLite 持久化通过；但页面正文与刷新后历史
在真实 Daemon 下不可见（`turn.delta_compacted` 后无回填读取），且停止未取得 `cancelled`。
按任务判据「文本/工具/停止/历史真实通过」整体**不通过**。

## 2. E8 — Windows 终端任务与子进程树

### 2.1 真实终端输出（通过的部分）

配置（`D:/tmp/wp7-agents/e8/config.toml`）把 fake Agent 作为真实本地 Agent：

```toml
[agent]
fake-ok = [".../acpr-fake-acp-agent.exe", "--scenario", "terminal-ok", "--probe-file", "D:/tmp/wp7-agents/e8/ok.probe"]
```

命令：

```text
node D:/tmp/wp7-agents/e8/run.mjs
```

实际观察（`D:/tmp/wp7-agents/e8/evidence.json`、`ok.png`、`daemon.log`）：

- 通过真实页面创建 `fake-ok` 会话并发送提示；页面出现 4 张
  `[data-timeline-card="terminal-output"]`（`term-1` 第 0–3 块）与 1 张
  `[data-timeline-card="tool-call"]`，轮次 `completed`。
- `ok.probe` 实际内容为 4 组：
  `output_after_release_rejected` / `release_after_release_rejected`。
  这是 fake Agent 在 `terminal/release` 之后主动验证「句柄已撤销」的探针，说明
  create → output → wait_for_exit → release 全链路真实执行，且 release 后不可再读。
- Daemon 日志中对应的两次 `code=-32602 terminal 请求失败（已显式拒绝）` 正是 release 之后的
  预期拒绝，不是失败注入。
- SQLite（`D:/tmp/wp7-agents/e8/data/acp-remote.sqlite3`）中该会话
  `b2a44c2d-4328-41a2-baa9-1bc0ea0dce67` 持久化：
  `terminal.output = 16`、`tool.call.started = 4`、`turn.completed = 4`。
  这证明「release 后工具/终端历史仍可由后端读回」。

### 2.2 kill / Agent 崩溃 / Daemon 关闭 — 未取得有效证据

同一脚本本应按顺序为 `terminal-kill`、`terminal-orphan-crash`、`terminal-orphan`
新建会话，但脚本的 `createSession` 只用 `location.pathname.includes('/session/')` 判断成功；
在已经处于会话路由时该条件立即为真，后续「新建」实际停留在 `fake-ok` 会话，提示词被重复
发到同一会话。可核对的事实：

- `kill.probe`、`crash.probe`、`orphan.probe` 全部为空；
- `crash.heartbeat`、`orphan.heartbeat` 都是 0 字节（孙进程没有启动）；
- `evidence.json` 中 `crash.heartbeatStarted = false`、`orphan.started = false`；
- Daemon 关闭日志只有 `daemon.agents_stopped`，没有可归因到这三个场景的终端事件。

因此**不能**宣称 kill/release、Agent 崩溃清理、Daemon 关闭无孤儿进程通过。脚本缺陷本身不是
产品缺陷，但本次 E8 的这三项证据无效。

### 2.3 重启后历史

重启 Daemon（同一 `platform` 身份）后打开 `fake-kill` 会话
`4d866563-d358-4411-8b0c-f38c803c7578`：页面 `cards = []`，正文显示
「已经是最早的消息」「已经是最早的结构化条目」（`history.png`）。这与 §1.4 是同一类
读取/渲染缺陷。后端 SQLite 仍保留 `b2a44c2d-...` 的终端与工具事件（见 §2.1），
但 `fake-kill` 本身因 §2.2 未被派发，不能作为它的历史证据。

### 2.4 进程树观察方法（已执行但无有效对象）

脚本用 PowerShell `Get-CimInstance Win32_Process` 按命令行标记
`--terminal-child hang` / `--terminal-heartbeat <path>` 枚举进程，并在停止后再次枚举。
实际输出（`evidence.json`）：

- `kill.hangProcesses` 只匹配到 PowerShell 自身命令行（因为它包含搜索字符串），
  没有 `acpr-fake-acp-agent.exe --terminal-child hang` 实例；
- `orphan-before-stop.processes` 只有 fake Agent 本体，没有 `--terminal-child grandchild`；
- `orphan-after-stop.processes` 只有 PowerShell 自身；
- Daemon 退出码 0。

这些结果只能说明「枚举命令按标记工作」，**不能**说明终端孙进程被清理，因为孙进程从未启动。

## 3. 与任务判据的映射

| 判据 | 实测 | 结论 |
| --- | --- | --- |
| 7.3 真实 Codex 初始化能力 | `probe.mjs codex` | **通过（能力表见 §1.1）** |
| 7.3 真实 OMP 初始化能力 | `probe.mjs omp` | **通过（能力表见 §1.1）** |
| 7.3 文本闭环 | 两台 Agent 均 `end_turn` + 流式 chunk | **Agent 侧通过；页面侧失败（§1.4）** |
| 7.3 工具/终端 | OMP 真实发出 terminal/create；Codex 未观察到 | **部分证据；不能宣称两者都通过** |
| 7.3 停止 | cancel 后仍 `end_turn` | **未通过/未证明** |
| 7.3 历史 | 后端持久化通过；页面刷新为空 | **不通过** |
| 7.4 真实终端 output/release | fake-ok 真实子进程 + 4 块输出 + release 探针 | **通过** |
| 7.4 kill/release | 脚本未派发 | **未执行（证据无效）** |
| 7.4 Agent 崩溃清理 | 心跳未启动 | **未执行（证据无效）** |
| 7.4 Daemon 关闭无孤儿 | 同上 | **未执行（证据无效）** |
| 7.4 重启后历史 | 页面为空；DB 有 fake-ok 事件 | **页面不通过；后端通过** |

## 4. 未执行 / 明确不支持（不得计为通过）

1. 真实 Codex 经真实 Daemon 页面的文本/工具/停止/历史：**未执行**（E7 页面路径先被
   §1.4 的读取缺陷阻断；仅做了 OMP 的真实 Daemon 会话与两台 Agent 的直接 ACP 探针）。
2. 真实 OMP 页面的工具卡片/终端卡片：**未执行**（直接 ACP 探针有 terminal/create，
   但探针未回放输出；产品页面未跑 OMP 工具提示）。
3. 两台 Agent 的媒体端到端（图片/音频附件上传）：**未执行**。初始化能力已记录：
   image/embeddedContext 为 true，audio 未宣告。
4. 表单/elicitation：**明确不支持**（Daemon 未宣告 `clientCapabilities.elicitation`），
   不是「未测试」。
5. E8 的 kill、Agent 崩溃、Daemon 关闭无孤儿：**未取得有效执行**，原因见 §2.2。
6. CI 专属的许可证/advisory/密钥扫描：**未执行**，本文件不宣称等价通过。

## 5. 原始产物路径（临时目录，未入库）

```text
D:/tmp/wp7-agents/e7/omp-probe.log        ACP initialize/text 原始帧
D:/tmp/wp7-agents/e7/codex-probe.log
D:/tmp/wp7-agents/e7/omp-tool.log         OMP terminal/create 证据
D:/tmp/wp7-agents/e7/codex-tool.log
D:/tmp/wp7-agents/e7/omp-stop2.log        cancel 后 end_turn
D:/tmp/wp7-agents/e7/codex-stop.log
D:/tmp/wp7-agents/e7/fresh-ws.log         真实 Daemon + OMP 的页面 WS 原始帧
D:/tmp/wp7-agents/e7/inspect-ws.log       刷新/直开会话时的 WS 帧（无 session.read）
D:/tmp/wp7-agents/e7/fresh.png            创建会话后截图
D:/tmp/wp7-agents/e7/inspect.png          刷新后空历史截图
D:/tmp/wp7-agents/e8/evidence.json        E8 各步骤结构化观察
D:/tmp/wp7-agents/e8/ok.png               fake-ok 终端卡片截图
D:/tmp/wp7-agents/e8/history.png          重启后空历史截图
D:/tmp/wp7-agents/e8/daemon.log           E8 真实 Daemon 日志
```

截图与日志中不含 Provider 凭据、私钥或对话正文以外的敏感材料；Codex 初始化响应里的账户
显示名/邮箱未写入本文件，也未复制到证据目录。

## 6. 第 3 轮（修复后）：E7 与 E8 重跑

本节只登记本轮**实际执行并观察到**的结果；前两轮内容保留在上面。本轮使用新的隔离目录
`D:/tmp/wp7-agents/e7r3`、`D:/tmp/wp7-agents/e8r3` 与端口 `18977` / `18978`，没有复用上一轮
的数据目录或会话。所有页面断言来自真实 Edge + 真实 PWA 路由；原始 WebSocket 帧、SQLite
查询结果与进程枚举都保留在临时目录。

### 6.1 结论先行

- **D3 / D4 / D5 在真实 Agent 上成立**：OMP 与 Codex 的正文在 `turn.delta_compacted` 之后由
  `session.read include:["messages"]` 回填（各 1 次）；刷新和直接打开 `/session/<id>` 都会派发
  `session.read`（各 +1）并重新显示正文；`agent.message.completed.content` 与
  `owned_message.content_json` 含完整最终正文。
- **停止在 Agent 侧和产品侧都取得 `cancelled`**：直接 ACP 探针在流式前发 `session/cancel`，
  OMP 与 Codex 的 `session/prompt` 结果都是 `stopReason = "cancelled"`；产品页面点击「停止」后
  轮次也进入 `cancelled`。
- **E8 的 kill / Agent 崩溃 / Daemon 关闭无孤儿全部取得有效证据**：上一轮脚本的会话复用缺陷
  已修正（先回 `/` 并等待会话路径**变化**），四个场景各自新建了不同会话；kill 探针 6 项齐全，
  crash 心跳启动后停止且进程树为空，Daemon 关闭前后进程从 3 个变为 0 个、退出码 0。
- **仍有两个新缺陷阻断「完整闭环」**：
  1. **D8**：Daemon 重启后，页面的首次 `session.read`（`include` 含 `config_options`）被服务端
     以 `protocol.schema_invalid` 拒绝，历史与终端卡片无法加载。这是本轮新观察到的可复现失败，
     与 D4 的「不派发」不同：本轮**确实派发了**，但读取被拒。
  2. **D9**：真实 Agent 的 `tool.call.updated` / `.completed` 视图里 `title` 为空串，客户端
     `viewText` 把空串当作缺席，导致工具卡片降级为 `degraded`；Codex 工具轮次页面完全没有
     `tool-call` 卡片，OMP 仅保留了 `terminal-output` 卡片。服务端事件已持久化，不是丢事件。
- 因此 **7.3 仍不能整体勾选**（工具卡片与重启后历史未闭环）；**7.4 的终端/kill/崩溃/关闭
  清理可以勾选，重启后页面历史仍被 D8 阻断**（后端事件可读回）。

### 6.2 环境与构建

| 项 | 实际值 |
| --- | --- |
| 操作系统 | Windows `10.0.26300.0`（win32 x64） |
| 源码树 | 主检出 `D:/Project/acp-remote`，`HEAD = 71badcd`，工作区包含 D3–D7 修复 |
| 前端产物 | `clients/app/dist`，`build-version.json` 的 `builtAt = 2026-10-20T01:43:38.221Z`（本地 09:43:38），`version = 0.0.0+71badcd-dirty`；bundle 内 `refresh_messages` 3 处、`deferredReads` 1 处 |
| 被测二进制 | `D:/Project/acp-remote/target/debug/acp-remote.exe`，2026-10-20 09:43:49（+0800），37,792,768 字节，SHA-256 `c2f11985080778677cab114e38df7556ce5708ba6f4b898413d36b1efd367395`；服务端返回的 `/build-version.json` 与上表一致 |
| Fake ACP Agent | `D:/Project/acp-remote/target/debug/acpr-fake-acp-agent.exe` |
| 浏览器 | Microsoft Edge `155.0.4283.62`，headless，`--ignore-certificate-errors` |
| Oh My Pi | `omp/18.12.2`（`C:/Users/jack/AppData/Local/omp/omp.exe acp`） |
| Codex ACP | `@agentclientprotocol/codex-acp 2.2.2`（`D:/tmp/wp7-agents/tools`）；本机 `codex-cli 0.162.0` |
| Daemon 配置 | `identity.keystore = "platform"`、`dev_mode.enabled = true`、`tls.mode = "direct"`；E7 `18977`，E8 `18978` |

本轮第一次页面运行确实遇到了旧产物（`dist` builtAt `08:57`，bundle 无 `refresh_messages`）；
主 Agent 重新执行 `npm run export:web` + `cargo build -p app` 后，上表的产物与二进制才用于
下述全部页面断言。旧产物的失败没有计入本轮结论。

### 6.3 E7 能力矩阵（真实进程直接 ACP）

命令：

```text
node probe.mjs        omp   D:/tmp/wp7-agents/e7r3/ws "Reply with exactly ... E7OMPPAGE ..."
node probe.mjs        codex D:/tmp/wp7-agents/e7r3/ws "Reply with exactly ... E7CODEXPAGE ..."
node probe2.mjs       omp   D:/tmp/wp7-agents/e7r3/ws tool
node probe-stop3.mjs  omp   D:/tmp/wp7-agents/e7r3/ws
node probe-stop3.mjs  codex D:/tmp/wp7-agents/e7r3/ws
```

原始日志：`e7r3/omp-probe-r3.log`、`codex-probe-r3.log`、`omp-tool-r3.log`、
`omp-stop-probe.log`、`codex-stop-probe.log`。

| 能力路径 | Oh My Pi 18.12.2 | Codex ACP 2.2.2 |
| --- | --- | --- |
| `protocolVersion` | 1 | 1 |
| `agentInfo` | `omp` 18.12.2 | `@agentclientprotocol/codex-acp` 2.2.2（title `Codex`） |
| `loadSession` | true | true |
| `promptCapabilities.image` | true | true |
| `promptCapabilities.audio` | **未宣告** | **未宣告** |
| `promptCapabilities.embeddedContext` | true | true |
| `sessionCapabilities` | `list` / `fork` / `resume` / `close` | `resume` / `list` / `close` / `delete` / `fork` / `additionalDirectories` / `subagents` |
| `mcpCapabilities` | `http=true`、`sse=true` | `http=true`、`sse=false`、`acp=false` |
| `authMethods` | `agent`（沿用本机凭据） | `api-key`、`chat-gpt` |
| 实际认证状态 | 使用本机已有凭据 | `_auth/status_update` 报告 `ChatGPT Plus` 账户；账户信息未写入本文件 |
| 其他 | — | `_meta.steering.supported = true`、`auth.logout`、`additionalDirectories`、`subagents` |

文本闭环（真实 Agent）：

```text
OMP   : 首块 1.77 s，session/prompt -> {"stopReason":"end_turn"}，chunks 拼出 E7OMPPAGE
Codex : 首块 11.60 s，session/prompt -> {"stopReason":"end_turn"}，chunks 拼出 E7CODEXPAGE
```

OMP 的 `terminal/create` 请求（`omp-tool-r3.log`）：

```text
{"sessionId":"01a156fa-4f24-72d1-ae32-c7140d98a7b2",
 "command":"C:\\Users\\jack\\AppData\\Local\\Microsoft\\WindowsApps\\bash.exe",
 "args":["-l","-c","git log --oneline -5 2>&1; git status --short 2>&1 | head -20"],
 "cwd":"D:\\tmp\\wp7-agents\\e7","outputByteLimit":51200}
```

该直接探针没有实现输出回放，`terminal/wait_for_exit` 后退出码为 2，因此这里**只**断言
「OMP 真实请求并依赖 ACP 客户端终端」，不断言命令输出。

### 6.4 E7 文本：页面、D3、D4（通过）

脚本 `D:/tmp/wp7-agents/e7r3.mjs`（真实 Daemon + 真实 OMP/Codex + 真实 Edge）；原始帧
`e7r3/frames.log`，截图 `e7r3/omp-text.png`、`codex-text.png`。

```text
OMP   session 381e9d0d-cafc-49e7-840a-5618f13ad4df
      turns ["completed"]，messages ["E7OMPPAGE"]
      turn.delta_compacted 后 session.read +1
      reload:  beforeReads 2 -> readsAfter 3，正文仍为 E7OMPPAGE
      direct:  beforeReads 3 -> readsAfter 4，正文仍为 E7OMPPAGE
Codex session 85d187d1-382c-469d-a165-cf28197fc873
      turns ["completed"]，messages ["E7CODEXPAGE"]
      turn.delta_compacted 后 session.read +1
      reload:  beforeReads 10 -> readsAfter 11，正文仍为 E7CODEXPAGE
      direct:  beforeReads 11 -> readsAfter 12，正文仍为 E7CODEXPAGE
```

SQLite（`e7r3/db-copy-acp-remote.sqlite3`，Daemon 停止后复制）对照：

```text
OMP   agent.message.completed.content = [{"type":"text","text":"E7OMPPAGE"}]
      turn.delta_compacted.deltaCount = 6
      owned_message(reply, completed) 内容长度 36
Codex agent.message.completed.content = [{"type":"text","text":"E7CODEXPAGE"}]
      turn.delta_compacted.deltaCount = 5
      owned_message(reply, completed) 内容长度 38
```

即 D3（压缩后回填）、D4（刷新/直开派发首读）、D5（终态与本批尾段合并）在真实 Agent 的
真实 Daemon 会话上都成立。

### 6.5 E7 停止（Agent 侧 + 页面，通过）

直接 ACP 探针在**首个 `agent_message_chunk` 之前**（8 s 定时）发 `session/cancel`：

```text
OMP   [8.43s] CANCEL (no chunk yet)
      [8.44s] PROMPT_RESULT {"stopReason":"cancelled", ...} chunks=0
Codex [9.75s] CANCEL (no chunk yet)
      [9.76s] PROMPT_RESULT {"stopReason":"cancelled", ...} chunks=0
```

产品页面在长提示运行中点击 `[data-action="cancel-turn"]`：

```text
OMP   session 53c773ea-...：stopClicked=true，
      before turns ["running"]，after turns ["cancelled"]
Codex session c3709e89-...：stopClicked=true，
      after turns ["cancelled"]；SQLite turn.state = "cancelled"
```

两条路径都得到真实的 `cancelled`，不是「Agent 自己 end_turn + 页面假装停止」。本轮停止
发生在首块 delta 之前，因此没有可断言的部分正文；E2 的部分正文断言不在本节范围。

### 6.6 E7 工具 / 终端（部分通过，D9）

页面结果（真实 Agent，经真实 Daemon）：

```text
OMP tool session 5839b6ae-3e26-4cfb-8a95-c66d3dbe8922
  permission-requested 卡片出现，点击 allow_once（request 7088518f-...）后提交成功
  cards = ["turn","degraded","degraded","config-changed",
           "permission-requested","terminal-output","degraded","degraded"]
  terminal-output 文本 = "term-1 · 第 0 块"
  tool-call 卡片数量 = 0
  最终 reply 可见（owned_message.content_json 长度 355），其中包含 bash.exe 的 WSL 报错
Codex tool session 242ab512-7301-473a-ab64-d2577be05c24
  cards = ["turn","degraded","degraded","degraded","degraded","degraded"]
  tool-call 卡片数量 = 0
  messages = ["E7CODEXTOOL"]（模型最终回复）
```

后端事件（SQLite）证明不是「工具没执行」：

```text
Codex: tool.call.started  title="cmd /c echo E7CODEXTOOL"
       tool.call.updated  title=""  ×3
       tool.call.completed title=""
       agent.message.completed.content = [{"type":"text","text":"E7CODEXTOOL"}]
OMP  : tool.call.started  title="$ cmd.exe /c echo E7OMPTOOL"
       tool.call.updated  title=""（in_progress / failed）
       tool.call.completed title=""
       terminal.output chunkIndex=0 stream=stdout terminalId=term-1
```

OMP 的终端链路真实执行（Daemon 启动了 `bash.exe`，页面显示 `terminal-output`）；但本机
`WindowsApps\bash.exe` 是未安装 WSL 发行版的启动器，命令输出是 WSL 报错而非 `E7OMPTOOL`。
这是**环境限制**，不是产品绕过终端；不能据此宣称 OMP 工具输出内容成功。

**D9（新缺陷，页面渲染）**：`tool.call.updated` / `.completed` 的 `view.title` 是空串，
客户端 `clients/app/src/domain/timeline.ts::viewText` 只接受 `length > 0`，而
`buildTimelineCard` 取分组**最新**记录的 title；最新记录 title 为空 → 返回 `null` →
该分组降级为 `degraded`。所以两张真实 Agent 的工具卡片都不显示，尽管
`tool.call.*` 事件完整持久化。fake Agent 的 title 一直非空，因此 E8 的工具卡片正常。

### 6.7 E7 历史与 Daemon 重启（D8）

同一 Daemon 生命周期内，刷新和直接打开都通过（§6.4）。**Daemon 重启后**打开 OMP 会话
`381e9d0d-...`：

```text
SENT {"type":"command","body":{"command":"session.read",
      "payload":{"include":["messages","config_options","history","pending_interactions"]}}}
RECV {"type":"command.result","body":{"command":"session.read","status":"rejected",
      "error":{"code":"protocol.schema_invalid",
               "message":"the request does not match the v1 schema",
               "retryable":false,"details":{}}}}
页面 cards = []，messages = []，正文显示
「已经是最早的消息。…protocol.schema_invalid：the request does not match the v1 schema…」
```

**D8（新缺陷）**：这次**不是**没有派发 `session.read`（D4 已修），而是服务端拒绝。
可定位的执行路径：

1. `session.read` 请求带 `include:["config_options"]`；
2. `crates/core/src/use_cases.rs::read_session` 在 `include.config_options` 为真时调用
   `live_config`；
3. `live_config → broker.endpoint_for` 在 Daemon 重启后没有该会话的活动 endpoint；
   `crates/agent-host/src/host.rs::open` 返回
   `PortError::InvalidRequest("没有该会话的活动 endpoint")`；
4. `crates/server/src/sync/command/mod.rs::port_rejection` 把该 `InvalidRequest` 归入
   `Rejection::Schema`，最终对端看到 `protocol.schema_invalid`。

即「重启后读历史」被 `config_options` 的实时配置读取拖垮，`messages`/`history` 本身没有机会
返回。刷新页面会重复同一失败。此结论的**可观察部分**是上面的帧与页面；上面第 3 步是
只读代码定位（没有改产品代码），不是新增日志证据。

为排除「首读发得太早、赶在初始快照之前」这一替代解释，又做了同连接的第二次读取实验
（`e8secondread.mjs`，原始帧 `e8r3/second-ws.log`，观察 `e8r3/second-read-evidence.json`）：

```text
sync.caught_up 于 T 到达；
T + 10.9 s 在同一连接上点击另一个会话（fake-ok）触发第二次 session.read
  -> 仍然 rejected / protocol.schema_invalid
两次 session.read 都带 include ["messages","config_options","history","pending_interactions"]
```

第二次读取发生在初始同步完成之后，仍被同一错误拒绝，因此不是同步时序竞态；两个不同会话
（fake-kill、fake-ok）同时失败，也与「某个会话的数据损坏」不符。

### 6.8 E8：真实终端、kill、崩溃、Daemon 关闭

脚本 `D:/tmp/wp7-agents/e8r3.mjs`；配置使用四个 fake Agent 场景。上一轮 `createSession`
的缺陷已修正：

```js
await gotoDirs(cdp);                                  // 先导航到 `${origin}/`
const before = await cdp.evaluate('location.pathname');
await click submit;
await waitFor(cdp, `location.pathname.startsWith('/session/') &&
                    location.pathname !== ${JSON.stringify(before)}`, ...);
```

本轮实际创建了四个不同会话（`e8r3/evidence.json`）：

| 场景 | session | 探针 | 页面卡片 | 结论 |
| --- | --- | --- | --- | --- |
| `terminal-ok` | `98eb3f4c-b83d-4f66-b07f-26bf99484ff9` | `output_after_release_rejected`、`release_after_release_rejected` | `turn` + `tool-call` + 4×`terminal-output`（term-1 第 0–3 块） | 通过 |
| `terminal-kill` | `6fa996a9-9f36-405e-9609-20a50a83b1f0` | `kill_ok`、`kill_exit_reported`、`kill_twice_ok`、`wait_twice_stable`、`output_after_release_rejected`、`release_after_release_rejected` | `turn` + `tool-call` + `terminal-output`（term-2 第 0 块 = `started\n`） | 通过 |
| `terminal-orphan-crash` | `87a6f4ea-e93d-4674-b7be-ac7a3d9a1fa2` | `orphan_created` | `turn` + `tool-call` + `terminal-output`（term-3 第 0 块） | 通过 |
| `terminal-orphan` | `296f6e82-be75-4328-a661-1e442fa251b9` | `orphan_created` | `turn` + `tool-call` + `terminal-output`（term-4 第 0 块） | 通过 |

进程树观察（PowerShell `Get-CimInstance Win32_Process`，按 `Name -like
'acpr-fake-acp-agent*'` **并且** `CommandLine` 含标记过滤，避免匹配 PowerShell 自身）：

```text
kill（终端 kill 后）:
  hangProcesses 查询 `--terminal-child hang` -> []（gone=true）
crash（Agent 自行退出）:
  crash.heartbeat 启动 = true，随后停止 = true
  processesAfter（`--terminal-heartbeat .../crash.heartbeat`）= []（gone=true）
orphan-before-stop（Daemon 仍运行）:
  [37136] fake Agent --scenario terminal-orphan
  [34200] --terminal-child grandchild（父 37136 关系的终端子进程）
  [18316] heartbeat-child（父 34200，持续写 orphan.heartbeat，size=31）
orphan-after-stop:
  heartbeatStopped = true，`--terminal-heartbeat .../orphan.heartbeat` -> []（gone=true）
  remainingFakeAgents = []，daemonExit = 0
```

SQLite 对照（`e8r3/db-copy-acp-remote.sqlite3`）：

```text
ok     : tool.call.started ×1，terminal.output ×4，turn.completed
kill   : tool.call.started ×1，terminal.output chunk0 = "started\n"，turn.completed
crash  : turn.completed，terminal.output chunk0 = "started\n"（turn.completed 之后仍持久化）
orphan : turn.completed，terminal.output chunk0 = "started\n"
```

即 kill 后终端的输出仍可读、重复 kill/wait 收敛；Agent 崩溃与 Daemon 关闭都清理了终端
进程树，Daemon 退出码 0，没有残留 fake Agent 进程。

**重启后的页面历史**：与 §6.7 相同的 D8。`e8history.mjs` 打开 kill 会话：

```text
SENT session.read（1 条）
RECV status="rejected"，error.code="protocol.schema_invalid"
cards = []，terminalText = ""，页面显示同一失败说明
```

后端仍保留 kill 会话的 `tool.call.started` 与 `terminal.output "started\n"`（上表），
因此「历史部分输出与工具终端引用仍可恢复」在**后端**成立，在**页面**被 D8 阻断。

### 6.9 本轮新增缺陷与复现

**D8 — Daemon 重启后首次 `session.read` 被 `config_options` 拖垮**

1. 用真实 Daemon（`platform` keystore）创建并完成一条真实 Agent 会话；
2. `daemon stop` 后重新 `daemon start`（同一数据目录）；
3. 浏览器直接打开 `/session/<id>`（或刷新）；
4. 观察：页面派发了 1 条 `session.read`，服务端回
   `protocol.schema_invalid`，`cards=[]`；同一请求在重启前成功。
5. 代码路径见 §6.7；`session.read` 去掉 `config_options` 才会绕开实时配置读取（本报告
   未用自定义客户端验证这一点，只按调用链定位）。

**D9 — 真实 Agent 工具卡片因空 title 降级**

1. 用真实 Codex 或 OMP 发一个会触发工具的长提示；
2. 等轮次 `completed`；
3. 观察 `tool.call.started/updated/completed` 已持久化，但页面 `cards` 没有 `tool-call`，
   出现与工具事件数量对应的 `degraded`；
4. SQLite 中 `tool.call.updated.title` 与 `tool.call.completed.title` 均为 `""`；
   客户端 `viewText` 把空串视为缺席，分组最新记录构造卡片失败。
5. fake Agent 的工具卡片正常（title 非空），因此这是真实 Agent 的取值形状触发的前端缺口。
   修复可在服务端投影时保留上一非空 title，或客户端在最新记录 title 缺席时回退到组内最早的
   非空 title；本轮只登记现象与两处代码位置。

### 6.10 与 7.3 / 7.4 的映射（第 3 轮）

| 判据 | 本轮实测 | 是否可勾选 |
| --- | --- | --- |
| 7.3 Codex / OMP 初始化能力 | §6.3 能力矩阵，真实 `initialize` | **通过** |
| 7.3 文本闭环 | 两台 Agent 的流式 chunk + `end_turn`；页面正文可见、DB 完整 | **通过** |
| 7.3 停止 | 直接 ACP `stopReason=cancelled`；页面轮次 `cancelled` | **通过** |
| 7.3 工具/终端 | OMP 真实 `terminal/create` + 页面 `terminal-output`；Codex 工具执行并持久化；但两者 `tool-call` 卡片都因 D9 降级 | **不可整体勾选（D9）** |
| 7.3 历史 | 同一 Daemon 内刷新/直开通过（D3/D4）；Daemon 重启后 `session.read` 被 D8 拒绝 | **不可勾选（D8）** |
| 7.4 终端 output/release | `terminal-ok` 探针 + 4 块输出 + release 后拒绝 | **通过** |
| 7.4 kill/release | `terminal-kill` 6 项探针；无 `--terminal-child hang` 残留 | **通过** |
| 7.4 Agent 崩溃清理 | 心跳启动→停止；进程树为空；终端输出已持久化 | **通过** |
| 7.4 Daemon 关闭无孤儿 | 关闭前 3 个相关进程，关闭后 0 个；daemon exit 0 | **通过** |
| 7.4 重启后历史 | 后端事件可读回；页面被 D8 拒绝 | **页面不通过（D8）** |

### 6.11 未执行 / 明确不支持（不得计为通过）

1. 两台真实 Agent 的图片/音频端到端上传：**未执行**。只记录了初始化能力：
   `image=true`、`embeddedContext=true`、`audio` 未宣告。
2. Codex 的 ACP `terminal/create`：**未观察到**。本次 Codex 工具走其内部 exec（事件为
   `tool.call.*`，无 `terminal.*`），因此不能宣称 Codex 的 ACP 终端支持或不支持。
3. OMP 工具命令的成功输出：**未取得**。本机 `WindowsApps\bash.exe` 是没有 WSL 发行版的
   启动器，命令输出是 WSL 报错；只证明了「OMP 请求 ACP 终端 → Daemon 执行 → 页面显示
   terminal-output」这条链路。
4. 表单/elicitation：**明确不支持**（Daemon 未宣告 `clientCapabilities.elicitation`），
   不是「未测试」。
5. E7 的媒体消息渲染、附件上传与 E4/E6 的其余场景：不属于本节，未重跑。
6. 双真实节点 E5、PV7、`cargo test` / `npm run check` / `npm run verify`：**未执行**；
   本节只报告真实 Daemon + 真实 Agent + 真实浏览器的观察，不冒充测试套件结果。
7. 自定义 Sync 客户端只带 `include:["messages"]` 的重启后读取：**未执行**。D8 的调用链
   定位来自只读代码，未用第二个客户端做排除实验。

### 6.12 现场清理与原始产物

- 三个 Daemon（E7 主进程、E8 主进程、E8 重启进程）都由 `daemon stop` 停止；
  `orphan-after-stop.daemonExit = 0`。端口 `18977` / `18978` 没有 LISTENING 记录。
- 所有 Edge 实例由 CDP `Browser.close` / 进程句柄关闭；按
  `wp7-agents/e7r3` / `e8r3` 命令行枚举没有残留浏览器或 fake Agent 进程。
- 数据、日志、截图与原始证据都在临时目录；隔离工作区只改了这一份证据文件，没有改产品代码、
  `openspec` 任务或提交。

```text
D:/tmp/wp7-agents/e7r3/evidence.json        E7 页面各步骤结构化观察（文本/工具/停止/重启）
D:/tmp/wp7-agents/e7r3/frames.log           页面 WebSocket 原始帧（SENT/RECV）
D:/tmp/wp7-agents/e7r3/daemon.log
D:/tmp/wp7-agents/e7r3/omp-probe-r3.log     真实 OMP initialize/text
D:/tmp/wp7-agents/e7r3/codex-probe-r3.log   真实 Codex initialize/text
D:/tmp/wp7-agents/e7r3/omp-tool-r3.log      OMP terminal/create
D:/tmp/wp7-agents/e7r3/omp-stop-probe.log   cancel -> cancelled
D:/tmp/wp7-agents/e7r3/codex-stop-probe.log
D:/tmp/wp7-agents/e7r3/db-copy-acp-remote.sqlite3
D:/tmp/wp7-agents/e7r3/db-codex-tool.txt、db-omp-tool.txt、db-codex-stop.txt
D:/tmp/wp7-agents/e7r3/omp-text.png、omp-tool.png、omp-stop.png、
                            codex-text.png、codex-tool.png、codex-stop.png、history.png
D:/tmp/wp7-agents/e8r3/evidence.json        E8 各步骤结构化观察
D:/tmp/wp7-agents/e8r3/history-evidence.json、history-ws.log
D:/tmp/wp7-agents/e8r3/second-read-evidence.json、second-ws.log
D:/tmp/wp7-agents/e8r3/ok.probe、kill.probe、crash.probe、orphan.probe
D:/tmp/wp7-agents/e8r3/crash.heartbeat、orphan.heartbeat
D:/tmp/wp7-agents/e8r3/daemon.log
D:/tmp/wp7-agents/e8r3/ok.png、kill.png、crash.png、orphan.png、history-r3.png
D:/tmp/wp7-agents/e8r3/db-copy-acp-remote.sqlite3
```

## 7. 第 4 轮（最终，修复后）：E7 与 E8 复测

本节只登记这一轮**实际执行并观察到**的结果，第 1–3 轮内容全部保留在上面。本轮使用新的隔离
目录 `D:/tmp/wp7-agents/e7r4`、`e8r4` 与端口 `18979` / `18980`，没有复用上一轮的数据目录或
会话；所有页面断言来自真实 Edge + 真实 PWA 路由 + 真实 Daemon（E7 接真实 OMP/Codex）。

### 7.1 结论先行

- **D5 通过**：本轮四个真实 Agent 会话的 `agent.message.completed.content` 与
  `owned_message.content_json` 都逐字等于该轮全部 `agent.message.delta` 的拼接，
  `turn.delta_compacted.deltaCount` 等于 delta 事件数；其中 OMP 工具轮有 **56** 段 delta
  （`deltaIndex` 0–55），尾段 `Subsystem for Linux not being installed.` 完整进入终态正文。
  `daemon.log` 中 `daemon.merge_window_failed` 为 **0** 次，页面帧里 `session.read` 没有
  任何 `rejected` / `invalid_request`。
- **D8 通过**：Daemon 重启后直接打开 `/session/<id>`，`session.read`
  （`include` 含 `config_options`）返回 `status = "completed"`，正文 `E7OMPPAGE` 与 4 条
  history 回填，`config_options` 为空数组（best-effort），页面不再出现
  `protocol.schema_invalid`。重启前同一请求的 `config_options` 为 3 项，说明不是永久丢配置。
- **D9 通过**：真实 Codex 的 `tool.call.updated/completed.title` 仍是空串，但页面
  `tool-call` 卡片标题沿用 `started` 的 `cmd /c echo E7CODEXTOOL`，不再降级；OMP 的工具卡片
  沿用 `$ cmd.exe /c echo E7OMPTOOL`。`degraded` 卡片只剩会话级未支持视图
  （`session.commands.changed` / `session.info.changed`），与工具卡无关。
- **E8 通过**：terminal-ok / kill / crash / Daemon 关闭四个场景都取得有效证据；关闭前后
  相关进程 3 → 0，Daemon 退出码 0；重启后 kill 会话的 `session.read` 同样成功，终端输出与
  `tool-call` 卡片在页面恢复。
- **E7 的 OMP 命令输出仍是环境限制**（`WindowsApps\bash.exe` / `cmd.exe` 是未安装 WSL 的
  启动器），工具卡片按真实结果显示 `failed` 与 WSL 报错；这是**命令本身的失败**，不是卡片
  降级，也不是产品绕过终端。

### 7.2 环境与构建

| 项 | 实际值 |
| --- | --- |
| 源码树 | 主检出 `D:/Project/acp-remote`，`HEAD = 71badcd`，工作区含 D5/D6/D8/D9 修复 |
| 前端产物 | `clients/app/dist`，`build-version.json`：`builtAt = 2026-10-20T02:02:53.754Z`（本地 10:02:53）、`version = 0.0.0+71badcd-dirty` |
| 被测二进制 | `target/debug/acp-remote.exe`，2026-10-20 10:03:12（+0800），37,798,912 字节，SHA-256 `22C8A3E3179A0682E6F4616DEFFDBFB7FDF21B210DDF9F0AA6607D1A957D73E2`；页面 `/build-version.json` 返回同一 `builtAt` |
| Fake ACP Agent | `target/debug/acpr-fake-acp-agent.exe`（E8 four scenarios） |
| 浏览器 | Microsoft Edge `155.0.4283.62`，headless，`--ignore-certificate-errors` |
| Oh My Pi | `omp/18.12.2`（`omp.exe acp`） |
| Codex ACP | `@agentclientprotocol/codex-acp 2.2.2`，本机 `codex-cli 0.162.0` |
| Daemon 配置 | `identity.keystore = "platform"`、`dev_mode.enabled = true`、`tls.mode = "direct"`；E7 `18979`，E8 `18980` |

### 7.3 E7 文本 / D3 / D4（通过）

脚本 `D:/tmp/wp7-agents/e7r4.mjs`，原始帧 `e7r4/frames.log`，结构化观察
`e7r4/evidence.json`。

```text
OMP   session 635407ba-1a88-463f-9b50-95c948eb5744
      turns ["completed"]，messages ["E7OMPPAGE"]
      turn.delta_compacted 后 session.read +1（include ["messages"]，状态 completed）
      reload:  beforeReads 2 -> readsAfter 3，正文仍为 E7OMPPAGE
      direct:  beforeReads 3 -> readsAfter 4，正文仍为 E7OMPPAGE
Codex session 4ba62ded-8355-4f2e-ba05-27c660f1ce5d
      turns ["completed"]，messages ["E7CODEXPAGE"]
      turn.delta_compacted 后 session.read +1
      reload:  beforeReads 10 -> readsAfter 11，正文仍为 E7CODEXPAGE
      direct:  beforeReads 11 -> readsAfter 12，正文仍为 E7CODEXPAGE
```

两张卡片列表里的 `degraded` 都对应 `session.commands.changed` / `session.info.changed`
（客户端没有专用视图，按设计降级），不是消息或工具条目。

### 7.4 E7 工具 / 终端与 D9（通过）

```text
OMP tool session 074cfd8e-4279-4340-ab73-15c917ebb570
  cards = ["turn","degraded","degraded","config-changed","permission-requested",
           "tool-call","permission-resolved","terminal-output","degraded"]
  drive.permissions: 授权卡片出现并点击 allow_once，随后 permission.resolved 广播（D6 修复可见）
  tool-call: id=call_00_3eymjUt5yv4qkr4Qb5LH1026, state=failed,
             title="$ cmd.exe /c echo E7OMPTOOL"
  terminal-output: term-1 · 第 0 块
  degraded: 只剩 session.commands.changed / session.info.changed ×2
Codex tool session 93cb281b-c39e-40d0-a920-8dd182349f94
  cards = ["turn","degraded","degraded","tool-call","degraded","degraded"]
  tool-call: id=exec-d1e61fa5-e8e7-4f46-a79c-3ba4349a5edc, state=completed,
             title="cmd /c echo E7CODEXTOOL"
  messages = ["E7CODEXTOOL"]（模型最终回复）
```

后端事件（`e7r4/db-copy-acp-remote.sqlite3`）对照 D9 的取值形状：

```text
Codex: tool.call.started  title="cmd /c echo E7CODEXTOOL"（state in_progress）
       tool.call.updated  title=""  ×2（in_progress / completed）
       tool.call.completed title=""
       -> 页面 title 取组内最新非空值，等于 started 的标题
OMP  : tool.call.started  title="$ cmd.exe /c echo E7OMPTOOL"（state pending）
       tool.call.updated  title=""（in_progress，toolDetail.output.details.terminalId=term-1）
       tool.call.updated  title=""（failed，输出含 "Command exited with code 1"）
       tool.call.completed title=""
       terminal.output chunkIndex=0 stream=stdout terminalId=term-1
       -> 页面 title 同样回退到 started；state 如实显示 failed
```

本机 `cmd.exe` / `WindowsApps\bash.exe` 都是未安装 WSL 发行版的启动器，OMP 拿到的是 WSL
报错而不是 `E7OMPTOOL`；因此这里只断言「真实 `terminal` 链路执行 → 输出持久化 → 卡片显示
真实失败」，不宣称 OMP 命令输出成功。Codex 工具走其内部 exec，未观察到 ACP
`terminal/create`（见 §7.9）。

### 7.5 E7 停止（通过）

页面在长提示运行中点击 `[data-action="cancel-turn"]`：

```text
OMP   session 749a93db-21d3-4ddf-8865-c4c63e497f4c
      before turns ["running"] -> after ["cancelled"]，stopClicked=true
Codex session 32c1bc3c-a5e1-451f-a33c-70b4a353375c
      after turns ["cancelled"]，stopClicked=true
```

直接 ACP 探针（`e7r4/probes/omp-stop.log`、`codex-stop.log`）在首块 delta 之前取消：

```text
OMP   [8.49s] CANCEL (no chunk yet) -> PROMPT_RESULT {"stopReason":"cancelled"}
Codex [9.70s] CANCEL (no chunk yet) -> PROMPT_RESULT {"stopReason":"cancelled"}
```

### 7.6 E7 历史与 Daemon 重启（D8 通过）

同一 Daemon 生命周期内的刷新/直开见 §7.3。`daemon stop` 后重新 `daemon start`，直接打开
OMP 会话 `635407ba-…`（`e7r4/frames.log` 第 859 行起）：

```text
SENT session.read include ["messages","config_options","history","pending_interactions"]
RECV status="completed"，无 error
     resources.messages = 1（content "E7OMPPAGE"）
     resources.history  = 4
     resources.config_options = []   // 无活动 endpoint，best-effort 留空
页面 cards = ["turn","config-changed"]，messages = ["E7OMPPAGE"]，不再有失败文案
```

对照重启前的同会话读取（第 152 行）：`config_options` = 3 项、messages = 1、history = 4。
本轮全部 `session.read` 响应中没有 `rejected`，也没有 `protocol.schema_invalid`
（`frames.log` 内 `"status":"rejected"` 计数为 0）。

### 7.7 D5：终态正文与尾段（通过）

`check-d5.mjs` 直接比较 `owned_event` / `owned_message`：

```text
session                                    deltas  deltaIndex   completed==Σdelta  owned_message==completed  deltaCount
OMP  text   635407ba-…                        6     0..5              true                  true                  6
OMP  tool   074cfd8e-…                       56     0..55             true                  true                 56
Codex text  4ba62ded-…                        5     0..4              true                  true                  5
Codex tool  93cb281b-…                        6     0..5              true                  true                  6
```

OMP 工具轮最后一段正文为 `Subsystem for Linux not being installed.`（长度 357），
`agent.message.delta` 的 56 条与 `turn.completed` 同批/紧邻到达；`daemon.log` 中
`daemon.merge_window_failed` = 0。**本轮没有复现 delta 丢失**；四个会话都满足
「completed 正文 = 全部 delta 拼接、deltaCount = delta 条数」，因此这里只登记本轮结果，
不宣称此前定位的「delta_plan 下标错位」已被修复（本轮未构造能区分该路径的输入）。

### 7.8 E8：终端、kill、崩溃、Daemon 关闭、重启历史（通过）

脚本 `D:/tmp/wp7-agents/e8r4.mjs`，四个场景各自新建会话：

| 场景 | session | 探针 | 页面 | 结论 |
| --- | --- | --- | --- | --- |
| `terminal-ok` | `213014c5-3052-4319-bf43-0e751f5b0891` | `output_after_release_rejected`、`release_after_release_rejected` | `turn` + `tool-call` + 4×`terminal-output`（term-1 第 0–3 块） | 通过 |
| `terminal-kill` | `d0f0280c-9cd8-4fba-9da2-3f1bf41538b0` | `kill_ok`、`kill_exit_reported`、`kill_twice_ok`、`wait_twice_stable`、`output_after_release_rejected`、`release_after_release_rejected` | `turn` + `tool-call` + `terminal-output`（term-2 第 0 块） | 通过 |
| `terminal-orphan-crash` | `dfde2f4b-b710-42d7-afe4-e9af29d4725c` | `orphan_created` | `turn` + `tool-call` + `terminal-output`（term-3） | 通过 |
| `terminal-orphan` | `f31add38-4694-4aba-a341-810014b58e35` | `orphan_created` | `turn` + `tool-call` + `terminal-output`（term-4） | 通过 |

进程树（PowerShell `Get-CimInstance Win32_Process`，按 fake Agent 名称 + 命令行标记过滤）：

```text
kill（终端 kill 后）:
  `--terminal-child hang` 查询 -> []（gone=true）
crash（Agent 自行退出）:
  heartbeatStarted=true，heartbeatStopped=true；
  processesAfter（`--terminal-heartbeat .../crash.heartbeat`）-> []（gone=true）
orphan-before-stop（Daemon 仍运行）:
  [42820] fake Agent --scenario terminal-orphan（父 41784）
  [27816] --terminal-child grandchild（父 41784）
  [23144] heartbeat-child（父 27816，写 orphan.heartbeat，size=31）
orphan-after-stop:
  heartbeatStopped=true，`--terminal-heartbeat .../orphan.heartbeat` -> []（gone=true）
  remainingFakeAgents=[]，daemonExit=0
```

SQLite 对照（`e8r4/evidence.json` 内 `db` 字段，Daemon 停止后快照）：

```text
ok     : tool.call.started + terminal.output ×4（term-1，含 stdout/stderr）+ turn.completed
kill   : tool.call.started + terminal.output chunk0 = "started\n" + turn.completed
crash  : turn.completed 之后仍持久化 terminal.output chunk0 = "started\n"（term-3）
orphan : turn.completed 之后仍持久化 terminal.output chunk0 = "started\n"（term-4）
```

重启后的页面历史（D8 修复后的 E8 路径）：

```text
SENT session.read include ["messages","config_options","history","pending_interactions"]
RECV status="completed"（beforeReads 4 -> readsAfter 5）
cards = ["turn","tool-call","terminal-output"]，turns ["completed"]
terminalText = "term-2 · 第 0 块"，tool-call title = "运行命令"（state=in_progress，持久化时即未完成）
```

### 7.9 E7 能力矩阵（真实进程直接 ACP，第 4 轮复核）

命令（原始日志 `e7r4/probes/omp-probe.log`、`codex-probe.log`）：

```text
node probe.mjs omp   <ws> "Reply with exactly the token E7OMPPAGE ..."
node probe.mjs codex <ws> "Reply with exactly the token E7CODEXPAGE ..."
```

| 能力路径 | Oh My Pi 18.12.2 | Codex ACP 2.2.2 |
| --- | --- | --- |
| `protocolVersion` | 1 | 1 |
| `agentInfo` | `omp` 18.12.2 | `@agentclientprotocol/codex-acp` 2.2.2（title `Codex`） |
| `loadSession` | true | true |
| `promptCapabilities.image` | true | true |
| `promptCapabilities.audio` | **未宣告** | **未宣告** |
| `promptCapabilities.embeddedContext` | true | true |
| `sessionCapabilities` | `list` / `fork` / `resume` / `close` | `resume` / `list` / `close` / `delete` / `fork` / `additionalDirectories` / `subagents` |
| `mcpCapabilities` | `http=true`、`sse=true` | `http=true`、`sse=false`、`acp=false` |
| `authMethods` | `agent`（沿用本机凭据） | `api-key`、`chat-gpt` |
| 实际认证状态 | 使用本机已有凭据 | `_auth/status_update` 报告 ChatGPT Plus 账户；账户信息未写入本文件 |
| 其他 | — | `_meta.steering.supported = true`、`auth.logout` |

文本闭环（真实 Agent）：

```text
OMP   : session/prompt -> {"stopReason":"end_turn"}，chunks 拼出 E7OMPPAGE
Codex : session/prompt -> {"stopReason":"end_turn"}，chunks 拼出 E7CODEXPAGE
```

### 7.10 与 7.3 / 7.4 的映射（第 4 轮）

| 判据 | 本轮实测 | 是否可勾选 |
| --- | --- | --- |
| 7.3 Codex / OMP 初始化能力 | §7.9 真实 `initialize` | **通过** |
| 7.3 文本闭环 | 两台 Agent 流式 chunk + `end_turn`；页面正文可见、DB 完整、D5 尾段完整 | **通过** |
| 7.3 停止 | 直接 ACP `stopReason=cancelled`；页面轮次 `cancelled` | **通过** |
| 7.3 工具/终端 | OMP 真实 `terminal` 链路 + 页面 `terminal-output` + `tool-call`（title 沿用 started，state=failed 来自 WSL 环境）；Codex 工具执行并持久化，`tool-call` 标题正确 | **通过**（OMP 命令输出成功仍受环境限制，见 §7.11） |
| 7.3 历史 | 同一 Daemon 内刷新/直开通过；Daemon 重启后 `session.read` completed、正文/历史回填 | **通过** |
| 7.4 终端 output/release | `terminal-ok` 探针 + 4 块输出 + release 后拒绝 | **通过** |
| 7.4 kill/release | `terminal-kill` 6 项探针；无 `--terminal-child hang` 残留 | **通过** |
| 7.4 Agent 崩溃清理 | 心跳启动→停止；进程树为空；终端输出已持久化 | **通过** |
| 7.4 Daemon 关闭无孤儿 | 关闭前 3 个相关进程，关闭后 0 个；daemon exit 0 | **通过** |
| 7.4 重启后历史 | `session.read` completed；终端卡片与 `tool-call` 在页面恢复 | **通过** |

### 7.11 未执行 / 仍受环境限制（不得计为通过）

1. **OMP 工具命令的成功输出**：本机 `cmd.exe` / `WindowsApps\bash.exe` 是未安装 WSL 的启动器，
   命令以 WSL 报错退出；本轮只证明「真实 terminal 链路 + 真实失败输出 + 卡片正确」，不宣称
   命令输出成功。
2. **Codex 的 ACP `terminal/create`**：本轮 Codex 工具仍走内部 exec（事件仅 `tool.call.*`），
   没有观察到 `terminal.*`，不能据此判断 Codex 的 ACP 终端支持。
3. 两台真实 Agent 的图片/音频端到端上传、媒体消息渲染：**未执行**；只记录初始化能力
   （`image=true`、`embeddedContext=true`、`audio` 未宣告）。
4. 双真实节点 E5、PV7、`cargo test` / `npm run check` / `npm run verify`：**未执行**；
   本节不冒充测试套件结果。
5. 重启后配置项的具体 UI 内容未逐项断言：本轮只证明 `config_options` 为空时首读不再失败；
   无活动 endpoint 时按设计留空，未验证配置面板在重启后的展示文案。
6. 第 3 轮列出的其余未执行项（跨断点滚动位置、图片字节加载、历史 UI 翻页、主题像素证据、
   E4 失败注入）不属于本节，仍未执行。

### 7.12 现场清理与原始产物

- E7/E8 的 Daemon 均由 `daemon stop` 停止；端口 `18979` / `18980` 只剩 `TIME_WAIT`，
  没有 LISTENING；按命令行枚举 `e7r4` / `e8r4` 没有残留 fake Agent 或 Edge 进程。
- 数据、日志、截图与原始证据都在临时目录；隔离工作区只改了这一份证据文件，没有改产品代码、
  `openspec` 任务或提交。

```text
D:/tmp/wp7-agents/e7r4/evidence.json        E7 页面各步骤结构化观察
D:/tmp/wp7-agents/e7r4/frames.log           页面 WebSocket 原始帧（SENT/RECV）
D:/tmp/wp7-agents/e7r4/daemon.log           两次 Daemon 运行（重启前后）
D:/tmp/wp7-agents/e7r4/probes/omp-probe.log      真实 OMP initialize/text
D:/tmp/wp7-agents/e7r4/probes/codex-probe.log    真实 Codex initialize/text
D:/tmp/wp7-agents/e7r4/probes/omp-stop.log       cancel -> cancelled
D:/tmp/wp7-agents/e7r4/probes/codex-stop.log
D:/tmp/wp7-agents/e7r4/db-copy-acp-remote.sqlite3
D:/tmp/wp7-agents/e7r4/db-635407ba-…txt、db-074cfd8e-…txt、
                            db-4ba62ded-…txt、db-93cb281b-…txt
D:/tmp/wp7-agents/e7r4/omp-text.png、omp-tool.png、omp-stop.png、
                            codex-text.png、codex-tool.png、codex-stop.png、history.png
D:/tmp/wp7-agents/e8r4/evidence.json        E8 各步骤结构化观察（含四个会话的 DB 快照）
D:/tmp/wp7-agents/e8r4/ok.probe、kill.probe、crash.probe、orphan.probe
D:/tmp/wp7-agents/e8r4/crash.heartbeat、orphan.heartbeat
D:/tmp/wp7-agents/e8r4/daemon.log
D:/tmp/wp7-agents/e8r4/ok.png、kill.png、crash.png、orphan.png、history.png
D:/tmp/wp7-agents/e8r4/ok.sqlite3、kill.sqlite3、crash.sqlite3、orphan.sqlite3
```
