# 真实 Codex ACP 会话生命周期探针（WP0）

状态：**已执行（2026-10-22，Windows 10.0.26300 x64，真实 codex-acp 2.2.2）**。§1–§7 定义探针的用途、运行方式、判据与报告字段；§8 是主检出真实运行的执行记录（PASS 42 / FAIL 0 / SKIP 2，退出码 0）。SKIP 项没有被写成 PASS。

关联材料：

- 变更任务：[`openspec/changes/session-activation-lifecycle/tasks.md`](../../openspec/changes/archive/2026-10-25-session-activation-lifecycle/tasks.md) 的 1.1–1.3 与 [`plan.md`](../../openspec/changes/archive/2026-10-25-session-activation-lifecycle/plan.md) 的 PV0。
- 会话规范：[`specs/local-session-lifecycle/spec.md`](../../openspec/changes/archive/2026-10-25-session-activation-lifecycle/specs/local-session-lifecycle/spec.md)。
- ACP 方法矩阵：[`compatibility/acp/v1/matrix.json`](../../compatibility/acp/v1/matrix.json)（`method.session_resume`、`method.session_close`）。
- 探针脚本：[`scripts/probe-session-lifecycle.mjs`](../../scripts/probe-session-lifecycle.mjs)。

## 1. 目的与门槛

本探针在**隔离的数据目录**中启动真实 `codex-acp` 适配器，用真实 ACP stdio 协议回答四个交付前必须确认的问题：

1. **recall**：同一会话内约定一个随机标记；`session/close` 之后对**同一 `sessionId`** 调用 `session/resume`，Agent 是否仍能回答该标记；恢复请求是否只含协议字段、没有重发旧 prompt，也没有重复执行工具副作用。
2. **restart**：终止适配器进程树后，用同一 `codex-home` 与 data-dir 启动**新进程**，再次 resume 原 `sessionId`，是否仍能回答标记；新进程是否没有出现 `session/new`。
3. **isolation**：同一进程内存在 A、B 两个会话；关闭 A 之后 B 是否仍正常应答；A 的关闭是否错误地终止共享进程。
4. **release**：会话建立后、A close 后、进程退出后的进程树进程数与 WorkingSet；进程树是否真正清理干净。

硬门槛（不满足即阻断 WP1–WP6，不能把 SKIP 写成 PASS）：

- Agent 未宣告 `agentCapabilities.sessionCapabilities.resume`：`recall.resume_advertised` 记为 **FAIL**，后续恢复检查不再执行。
- Agent 未宣告 `agentCapabilities.sessionCapabilities.close`：`recall.close_advertised` 记为 **FAIL**；`recall.close_succeeded` 记为 **SKIP**（未执行），因为交付门槛要求 close 后可恢复。
- `session/close` 返回错误、`session/resume` 返回错误、resume 返回了不同的 `sessionId`：对应检查记为 **FAIL**。
- close/resume 后回答不含随机标记，或 resume 后出现新的工具调用、副作用文件变化、旧 prompt/标记被重发：对应检查记为 **FAIL**。
- 重启后的新进程出现 `session/new`：`restart.no_session_new` 记为 **FAIL**。
- A close 后 B 不能正常返回，或共享进程消失：对应检查记为 **FAIL**。
- 进程退出后仍有进程树残留：`release.process_tree_terminated` / `release.no_orphan_children` 记为 **FAIL**。
- 未观察到工具副作用（`side-effect.txt` 不存在）：相关子检查记为 **SKIP**，并明确写“无法据此断言未重复执行工具”，不得补写 PASS。

结论只来自报告中的 `checks`，脚本不写死任何场景结论。

## 2. 安全与隔离

- **独立 data-dir**：`--data-dir` 缺省在系统临时目录下创建 `acpr-session-lifecycle-<random>`。脚本拒绝把仓库根、用户家目录、真实 `CODEX_HOME`、仓库工作树内的路径或已存在的 `.codex` 目录作为 data-dir。
- **独立 codex-home**：缺省为 `<data-dir>/codex-home`，并通过子进程环境变量 `CODEX_HOME` 注入。真实用户会话不参与本次运行；探针不会对用户 `CODEX_HOME` 写入任何文件。
- **凭据复制**：从真实 `CODEX_HOME`（缺省 `%USERPROFILE%/.codex` 或 `$HOME/.codex`）只复制 `auth.json` 与 `config.toml`（若存在）。脚本只记录“copied / present / missing”文件名，**从不读取、打印、写入报告或转录**这些文件的内容。`state.json` 只保存本次随机标记与会话身份，不含凭据。
- **权限应答**：客户端自动选择 `allow_once` 或第一个 `allow*` 选项；该应答正文不进入转录与报告。转录里只出现 `redacted: true` 与 `requestId`。
- **入站消息脱敏**：适配器在 `initialize` 之后会推送认证状态通知（`_auth/status_update`），其 `params.authStatus.account` 含登录账号（邮箱）。探针在写入 `transcript.ndjson` **之前**脱敏：
  - 认证类方法的 `account` 字段整体替换为 `"[redacted]"`，`authStatus` 的其余状态字段（如 `type`、`methodId`）保留，以便判断是否发生过状态变化；
  - 任何层级上键名为 `token`、`authorization`、`apiKey`/`api_key`、`email`、`password`、`secret`、`cookie` 等的字段整体替换为 `"[redacted]"`（大小写不敏感）；
  - 字符串值中的邮箱、JWT（`eyJ…`）与 `Bearer …` 凭据按值替换。
  脱敏只作用于**写入证据的副本**，协议处理仍使用原消息。报告同样不记录认证状态：认证类通知与认证类 `session/update` 只计入 `server.authNotifications` / `server.authUpdatesSkipped`，摘要与 `notes` 都不写其内容；`session/update` 摘要的 `title`/`text` 也做同样的值层脱敏。脱敏次数计入报告 `server.inboundRedactions`。可用 `--self-test` 复验这套规则。
- **客户端 fs/terminal 服务**：只允许访问隔离 workspace 内的路径；越界请求返回 `-32602` 并计数。未识别的客户端方法一律返回 `-32601` 并记录方法名。
- **适配器 stderr**：只写入 `<data-dir>/adapter-<runId>.stderr.log`，不回显、不入报告（报告只记录文件路径与字节数）。
- **清理**：默认在结束时清理隔离 `codex-home`（含本次复制的凭据副本与 Agent 会话状态），保留 `report.json`、`transcript.ndjson`、`state.json` 与 workspace。`--keep` 保留完整临时目录；若使用 `--keep`，请在取证后手动删除该目录，避免凭据副本长期留在临时目录。
- 随机标记与副作用标记只用于本次探针证明上下文承接，不是凭据；它们会出现在 `transcript.ndjson` 与 prompts 中。

**限制**：Agent 进程本身仍以当前用户权限运行；探针的 fs/terminal 服务边界不等于操作系统沙箱，Agent 自带的 shell 工具理论上仍可访问用户文件。本探针的隔离目标是“独立 data-dir/codex-home、不向用户会话写入、不把复制的凭据输出到证据”，不能替代针对不可信 Agent 的沙箱。`transcript.ndjson` 的入站消息已按上述规则脱敏，但仍可能包含 Agent 在隔离 workspace 内产生的内容；对外分享前请自行检查，并确认没有把账号/凭据粘贴进 prompt。

## 3. 前提条件

- Windows 10/11 x64；进程树采样依赖 PowerShell（`Get-CimInstance Win32_Process`）。
- Node.js ≥ 22.12（仓库 `package.json` 的 engines）；本机环境为 v22.20.0（环境前提，不是探针结论）。
- 真实适配器 `@agentclientprotocol/codex-acp`（探针会读取入口所在包的 `package.json` 记录版本）。
- Codex 已登录：真实 `CODEX_HOME` 中存在 `auth.json`，或 `config.toml` 配置了可用的认证方式。缺失时探针不会伪造登录，相关 prompt 会失败并记为 FAIL。
- 若适配器通过 PATH 解析，`codex-acp` 需可执行；更推荐显式传 `--adapter <dist/index.js>`，避免解析到不同版本。

## 4. 运行方式

静态检查与帮助（`--help` 只打印用法，不创建目录、不启动子进程，退出码 0）：

```powershell
node --check scripts/probe-session-lifecycle.mjs
node scripts/probe-session-lifecycle.mjs --help
```

脱敏自检（只断言脱敏逻辑，不读凭据、不建目录、不启动子进程；全部通过退出 0，否则退出 1）：

```powershell
node scripts/probe-session-lifecycle.mjs --self-test
```

它构造假消息（含 `_auth/status_update` + `account.email`、`token`、`authorization`、`apiKey`，以及普通消息）断言：写进 transcript 的那条记录不含邮箱/凭据、认证字段变为 `"[redacted]"`、非敏感字段原样保留。

完整运行（PowerShell；显式 data-dir 便于保留证据）：

```powershell
$adapter = "D:/tmp/wp7-agents/tools/node_modules/@agentclientprotocol/codex-acp/dist/index.js"
$data = Join-Path $env:TEMP ("acpr-pv0-" + (Get-Date -Format "yyyyMMdd-HHmmss"))
node scripts/probe-session-lifecycle.mjs --scenario all --adapter $adapter --data-dir $data --keep
```

参数：

| 参数 | 说明 |
| --- | --- |
| `--adapter <path>` | `dist/index.js` 或可执行的 `codex-acp`。缺省依次取 `ACPR_CODEX_ACP_ADAPTER`、PATH 中的 `codex-acp`、已知安装路径；都不存在时明确报错并退出 2。 |
| `--data-dir <path>` | 隔离数据目录；拒绝仓库根、用户家目录、真实 `CODEX_HOME`、仓库内路径与 `.codex` 目录。 |
| `--codex-home <path>` | 隔离 Codex home，缺省 `<data-dir>/codex-home`；拒绝真实 `CODEX_HOME` 及其子路径。 |
| `--scenario <name>` | `recall`、`restart`、`isolation`、`release` 或 `all`（默认）。 |
| `--json-out <path>` | 报告 JSON；缺省 `<data-dir>/report.json`。 |
| `--timeout-ms <n>` | 单个 ACP 请求超时，缺省 180000。超时会取消请求并终止进程树。**不作用于**副作用轮。 |
| `--side-effect-timeout-ms <n>` | 只作用于 recall 的副作用轮 `session/prompt`，缺省 300000。该轮要求 Agent 用编辑/写文件工具直接创建 `side-effect.txt` 且不运行 shell，实测慢于普通问答，因此单独放宽；其他轮次仍用 `--timeout-ms`。 |
| `--keep` | 保留完整临时目录（包括隔离 codex-home）。默认只清理隔离 codex-home。 |
| `--self-test` | 只跑脱敏内置断言，不建目录、不启动子进程；全通过退出 0，否则退出 1。 |
| `--help` / `-h` | 只打印用法并退出 0。 |

单独运行 `--scenario restart` 时，需要同一 `--data-dir` 中已有完成 close+resume 的 recall 状态；跨次运行还需要 recall 时使用 `--keep`，否则隔离 codex-home（含 Agent 会话状态）已在上一轮结束时清理。前置不满足时脚本以退出码 2 明确拒绝，不会伪造 restart 结果。

退出码：`0` 表示没有 FAIL（允许 SKIP）；`1` 表示至少一个 FAIL；`2` 表示参数、环境、前置条件或协议致命错误；`130` 表示中断。

产物：

- `<data-dir>/report.json`（或 `--json-out`）：结构化证据。
- `<data-dir>/transcript.ndjson`：ACP 协议消息；权限应答整体脱敏，入站消息在写盘前按 §2 脱敏（认证账号、`token`/`authorization`/`apiKey`/`email` 等替换为 `"[redacted]"`）。
- `<data-dir>/adapter-<runId>.stderr.log`：适配器 stderr，只落盘。
- `<data-dir>/state.json`：随机标记、sessionId、close/resume 结果与上次进程终止信息（跨阶段 restart 使用）。

## 5. 场景判据

### 5.0 环境与全局检查

| check id | 可观察判据 | 失败/跳过条件 |
| --- | --- | --- |
| `environment.adapter_resolved` | 适配器入口存在且可启动，记录解析来源与入口 SHA-256 | 解析不到时脚本在写报告前退出 2 |
| `environment.adapter_version` | 从适配器包 `package.json` 读取名称与版本 | 读不到记 SKIP（initialize 的 `agentInfo` 仍会记录） |
| `environment.isolated_data_dir` | data-dir 不在仓库、用户家目录或真实 `CODEX_HOME` 内 | 不安全路径直接拒绝，退出 2 |
| `environment.codex_home_isolated` | 隔离 codex-home 与真实 `CODEX_HOME` 分离 | 重叠路径直接拒绝，退出 2 |
| `environment.credentials_copied` | 真实 CODEX_HOME 中是否存在并复制了 `auth.json`/`config.toml` | 未找到 `auth.json` 记 SKIP（登录可能不可用，不伪造） |
| `<stage>.completed` | 阶段汇总：任一检查 FAIL → FAIL；否则存在 SKIP → SKIP；否则 PASS | 用于防止报告缺少阶段结论 |
| `protocol.no_invalid_messages` | 入站 ACP 消息无法解析的数量 | 0 记 PASS；大于 0 记 FAIL |
| `probe.fatal` | 参数/前置/内部致命错误 | 记 FAIL，退出码 2 |

`<stage>.completed` 的实际 id 为 `recall.completed`、`restart.completed`、`isolation.completed`、`release.completed`。

### 5.1 recall

prompt 顺序固定为：记忆标记（仅该轮出现）→ 只问标记 → 一次工具副作用请求 → close → resume → 再次只问标记。

副作用轮的 prompt 显式限定工具面：**用文件编辑/写文件工具（如 `apply_patch`、`write_file`）在当前 cwd 直接创建或覆盖 `side-effect.txt`，只写入唯一一行随机标记；不运行任何 shell 命令，也不调用终端/exec 类工具**，完成后只回 `done`。真实 PV0 运行（runId `20261022022916-692e9219`）曾因 prompt 只说“使用你的工具”，Codex 先尝试 `exec_command`（本机 `ShellExecuteExW` 失败）再退回编辑工具，耗时超过 180s 使该轮 `session/prompt` 超时；因此该轮使用独立的 `--side-effect-timeout-ms`（默认 300000），其他轮次仍用 `--timeout-ms`。该轮超时/失败仍按下方判据记 SKIP 或 FAIL，不伪造结果。

| check id | 可观察判据 | PASS | FAIL | SKIP |
| --- | --- | --- | --- | --- |
| `recall.initialize_capabilities_recorded` | initialize 返回的 `agentInfo` 与 `sessionCapabilities` 原文 | 已记录 | 无 `sessionCapabilities` | — |
| `recall.resume_advertised` | `sessionCapabilities.resume` 存在 | 宣告 | 未宣告（阻断） | — |
| `recall.close_advertised` | `sessionCapabilities.close` 存在 | 宣告 | 未宣告（阻断） | — |
| `recall.session_new` | 隔离 cwd 下 `session/new` 返回非空 `sessionId` | 成功 | 无 sessionId | — |
| `recall.first_prompt_completed` | 第一轮 `session/prompt` 的 `stopReason` | `end_turn` | 其他值 | — |
| `recall.side_effect_observed` | 副作用回合后 `side-effect.txt` 的行数、字节数、内容 SHA-256、预期标记出现次数、`tool_call` 计数 | 文件含预期标记 | 文件存在但不含标记 | 文件不存在（未观察到工具副作用） |
| `recall.marker_same_session` | 第二轮只问标记时回复包含随机标记 | 包含 | 不包含 | — |
| `recall.close_succeeded` | `session/close` 返回 | 成功 | 返回错误 | 未宣告 close，未执行 |
| `recall.resume_request_protocol_only` | resume 请求键集合限于 `sessionId`/`cwd`/`additionalDirectories`/`mcpServers`/`_meta`，且不含标记、副作用标记或旧 prompt 文本；`sessionId`/`cwd` 与原会话一致 | 全部满足 | 任一不满足 | — |
| `recall.resume_request_succeeded` | `session/resume` 返回且未返回不同 `sessionId` | 成功 | 请求错误或身份改变 | — |
| `recall.marker_after_resume` | close→resume 后再次只问标记，回复包含随机标记 | 包含 | 不包含 | — |
| `recall.no_replacement_session` | 本次 run 的 `session/new` 次数 | 恰好 1 次 | 大于 1 次 | — |
| `recall.no_replayed_side_effect` | resume 窗口（发送 resume 之前到恢复后追问结束）的 `tool_call` 增量为 0，且副作用文件行数/字节数/标记次数/内容 SHA-256 均未变化 | 两者成立 | 出现工具调用或文件变化 | 未观察到副作用，无法判断 |
| `recall.no_old_prompt_resent` | 请求账本中没有 `containsOldPrompt` 条目；标记与副作用标记各只在各自 prompt 中出现一次 | 成立 | 任一重复 | — |
| `recall.marker_sent_once` | 含记忆标记的请求数 | 1 | 不为 1 | — |

### 5.2 restart

先终止旧适配器进程树（记录退出码/信号并确认树清空），再用同一 codex-home/data-dir 启动新进程。

| check id | 可观察判据 | 说明 |
| --- | --- | --- |
| `restart.previous_process_tree_terminated` | 旧进程树采样为空 | 采样失败记 SKIP，不伪造终止成功 |
| `restart.initialize_capabilities_recorded` | 新进程 initialize 返回 `sessionCapabilities` | — |
| `restart.resume_request_protocol_only` | 同 recall 的协议字段/敏感文本检查 | — |
| `restart.resume_request_succeeded` | 新进程 resume 同一 `sessionId` 成功且身份未变 | — |
| `restart.no_session_new` | 新进程的请求账本中 `session/new` 次数为 0 | 这是“没有新建替代会话”的直接证据 |
| `restart.marker_after_restart` | 新进程 resume 后问标记，回复包含标记 | 上下文跨进程承接 |
| `restart.no_replayed_side_effect` | 重启 resume 窗口（resume 之前到重启后追问结束）的 `tool_call` 增量为 0 且副作用文件未变化 | recall 未观察到副作用时记 SKIP |
| `restart.no_old_prompt_resent` | 新进程请求不含旧 prompt、记忆标记或副作用标记 | 独立 restart 运行的旧 prompt 集合来自 `state.json` |

### 5.3 isolation

同一进程内创建 A、B 两个会话；A close 后对 B 发一次计算题（`17*23`，直接回答，避免必须权限）。

| check id | 可观察判据 | 说明 |
| --- | --- | --- |
| `isolation.sessions_distinct` | A、B 的 `sessionId` 均非空且不同 | — |
| `isolation.b_ready_before_close` | A close 前 B 正常回复 | — |
| `isolation.a_close_succeeded` | `session/close(A)` 返回成功 | 未宣告 close 记 SKIP，隔离结论无法验证 |
| `isolation.b_works_after_a_close` | A close 后 B 的回复含正确结果 `391` | 未成功 close 时记 SKIP |
| `isolation.shared_process_survived` | A close 后共享适配器根进程仍存活 | 采样失败时用根进程存活探测；不能确认则不为 PASS |
| `isolation.resource_samples_recorded` | 已记录 A close 前后与 B 交互后的进程树采样 | — |

报告字段 `isolation.conclusion` 只在 A close 成功、B 正常返回、共享进程存活三者同时成立时写“A 的关闭没有终止共享进程”；否则明确写未验证或失败。

### 5.4 release

采样时机：会话建立后 → A close 后（成功时等待 1 秒）→ 进程退出后。

| check id | 可观察判据 | 说明 |
| --- | --- | --- |
| `release.session_established` | `session/new` 返回 sessionId | — |
| `release.a_close_succeeded` | `session/close` 返回成功 | 未宣告 close 记 SKIP |
| `release.memory_after_close` | close 前后进程树 WorkingSet 与下降值 | 只有 close 成功且下降达到 `max(16 MB, 5%)` 才记 PASS；否则记 SKIP，不能据单次采样断言泄漏或释放成功 |
| `release.process_tree_terminated` | 进程退出后进程树采样为空 | 采样失败记 SKIP |
| `release.no_orphan_children` | 进程退出后无遗留子进程 | 采样失败记 SKIP |

## 6. 报告 JSON 字段

报告顶层字段：

| 字段 | 内容 |
| --- | --- |
| `schemaVersion` / `tool` / `runId` / `scenario` / `startedAt` / `finishedAt` / `durationMs` / `exitCode` | 运行标识与退出码。 |
| `summary` | `total`、`pass`、`fail`、`skip` 以及失败/跳过检查 id 列表。 |
| `environment` | 平台、架构、Node 版本、适配器路径/解析来源/入口 SHA-256、适配器包版本、Codex 包版本与 CLI 版本、data-dir、workspace、codex-home、真实 codex-home 路径、复制/已存在/缺失的凭据文件名、`timeoutMs` 与 `sideEffectTimeoutMs`。**不含凭据内容。** |
| `capabilities` | 首次 initialize 的 `protocolVersion`、`agentInfo`、`sessionCapabilities` 原文、`supportsResume`、`supportsClose`、完整 initialize result；后续进程的能力见 `runs[].capabilities`。 |
| `checks` | 检查数组：`id`、`stage`、`status`（`PASS`/`FAIL`/`SKIP`）、`summary`、`details`、`at`。 |
| `requestLedger` | 客户端请求数组：`runId`、`method`、`requestId`、`paramsSha256`、`paramsBytes`、`paramsKeys`、`containsSessionMarker`、`containsSideEffectToken`、`containsSensitiveToken`、`containsOldPrompt`、`sentAt`。不保存请求正文。 |
| `resourceSamples` | `stage`、`point`、`runId`、`sampledAt`、`pid`、`processCount`、`workingSetMb`、`rootPresent`、`method`（`win32-cim`/`ps`）、`error`。 |
| `sessionIds` | recall / isolationA / isolationB / release 的 sessionId。 |
| `timestamps` | 各阶段 start/end。 |
| `runs` | 每个适配器进程：命令、cwd、codex-home、启动/结束时间、退出码、信号、是否由探针终止、能力、出站方法计数、入站方法计数、进程计数器、终止后的进程树快照。 |
| `server` | 客户端服务计数：权限请求、fs 读/写/拒绝、terminal 创建/拒绝、未知客户端方法、无法解析的入站消息数、`inboundRedactions`（转录脱敏次数）、`authNotifications` / `authUpdatesSkipped`（认证类通知/更新只计数，内容不入报告）。 |
| `sideEffect` | 副作用文件路径、是否存在、行数、字节数、预期标记次数、内容 SHA-256；不保存任意 Agent 输出正文。 |
| `isolation` / `release` | 两个场景的原始数字与结论字段。 |
| `notes` | 运行时观察（脱敏、采样限制、意外退出等）。 |

`paramsSha256` 是对参数序列化文本的 SHA-256，用于证明“没有重发同一请求”，不暴露正文；`containsOldPrompt` 只把 `params.prompt[*].text` 与历史 prompt 文本做**整串相等**比较（各轮追问天然共享措辞，子串匹配会把正常追问误判为旧 prompt），它与标记布尔值直接回答“是否含旧 prompt/标记”。

## 7. 资源释放解释

- `resourceSamples.workingSetMb` 是**进程树内各进程 WorkingSet 之和**。Windows 的 WorkingSet 与 Unix 的 RSS 都包含共享库/共享内存，按进程累加会重复计算，且不等于独占物理内存；绝对值只能做同机、同版本、同场景的前后对比。
- Agent 运行时（Node/Codex）可能把已释放的内存保留在分配器堆中，或延迟归还给操作系统。**单次 WorkingSet 未下降不能断言泄漏，下降也不能断言全部堆已归还**。因此 `release.memory_after_close` 只在 close 请求成功且出现显著下降时记 PASS，并附带“分配器可能保留堆”的说明；否则记 SKIP。
- `release.process_tree_terminated` / `release.no_orphan_children` 是更硬的判据：它验证探针的终止路径是否真的清空了进程树，而不是只看根进程是否消失。
- 采样失败（PowerShell/ps 不可用）记为 SKIP，不写成“已释放”。

## 8. 执行记录

以下记录来自 2026-10-22 在主检出 `D:/Project/acp-remote` 的真实运行。**FAIL 或 SKIP 没有被写成 PASS；报告路径指向本次真实运行产生的 JSON。**

### 8.1 环境与命令

| 项目 | 填写内容 |
| --- | --- |
| 执行日期 / 执行人 | 2026-10-22 / 主检出 Agent（omp） |
| 操作系统 / 架构 | Windows 10.0.26300 / x64 |
| Node 版本 | v22.20.0 |
| codex-acp 适配器路径与版本 | `D:/tmp/wp7-agents/tools/node_modules/@agentclientprotocol/codex-acp/dist/index.js`，`@agentclientprotocol/codex-acp@2.2.2`（入口 sha256 `6b1bb4e7…902aaa`，1,728,337 字节） |
| Codex 包版本 / CLI 版本 | 适配器内置 `@openai/codex@0.160.1`；本机 `codex-cli 0.163.0`（已登录 ChatGPT） |
| 实际运行命令（含 `--scenario`、`--adapter`、`--data-dir`） | `node scripts/probe-session-lifecycle.mjs --scenario all --data-dir D:/tmp/acpr-session-lifecycle-pv0b --json-out D:/tmp/acpr-session-lifecycle-pv0b/report.json --timeout-ms 180000`（适配器经已知路径解析） |
| 实际使用的 `--timeout-ms` / `--side-effect-timeout-ms` | `180000` / 缺省 `300000` |
| data-dir | `D:/tmp/acpr-session-lifecycle-pv0b`（显式；不自动创建） |
| 原始报告路径（`--json-out`） | `D:/tmp/acpr-session-lifecycle-pv0b/report.json` |
| transcript / state 路径 | `D:/tmp/acpr-session-lifecycle-pv0b/transcript.ndjson` / `D:/tmp/acpr-session-lifecycle-pv0b/state.json` |
| `node --check` 退出码 | 0 |
| `--help` 退出码与是否创建目录/进程 | 0；未创建目录、未启动子进程 |
| `--self-test` 退出码（脱敏断言） | 0（19/19 通过） |
| transcript 中是否出现账号邮箱/凭据（`grep` 报告 `server.inboundRedactions`） | 否：用 `_auth/status_update` 实际到达的邮箱字面量对 `transcript.ndjson` 做 `grep`，计数为 0（本文件不记录该字面量）；报告 `server.inboundRedactions = 8`、`authNotifications = 4` |
| 探针总退出码 | 0（PASS 42 / FAIL 0 / SKIP 2） |

### 8.2 各场景 check 结果

| 场景 | check | 结果（PASS/FAIL/SKIP） | 证据位置或备注 |
| --- | --- | --- | --- |
| recall | `recall.resume_advertised` | PASS | `capabilities.sessionCapabilities.resume = {}` |
| recall | `recall.close_advertised` | PASS | `capabilities.sessionCapabilities.close = {}` |
| recall | `recall.session_new` | PASS | `sessionIds.recall = 01a16177-92d2-7881-9d22-677c8b206ae7` |
| recall | `recall.first_prompt_completed` | PASS | 报告 checks |
| recall | `recall.side_effect_observed` | PASS | `sideEffect.exists = true`、`lineCount = 1`、`tokenSha256` 命中；`server` 计数记录工具活动 |
| recall | `recall.marker_same_session` | PASS | 报告 checks |
| recall | `recall.close_succeeded` | PASS | `session/close` 返回 `{}` |
| recall | `recall.resume_request_protocol_only` | PASS | `requestLedger` 的 `session/resume` 仅含 `sessionId`/`cwd`/`mcpServers`；`containsSessionMarker=false`、`containsSideEffectToken=false`、`containsOldPrompt=false` |
| recall | `recall.resume_request_succeeded` | PASS | 报告 checks；未返回不同 `sessionId` |
| recall | `recall.marker_after_resume` | PASS | 回答包含随机标记 |
| recall | `recall.no_replacement_session` | PASS | `session/new` 全程 4 次（四个场景各一次），recall 内只创建 1 个会话 |
| recall | `recall.no_replayed_side_effect` | PASS | resume 窗口无新工具调用；`side-effect.txt` 行数保持 1 |
| recall | `recall.no_old_prompt_resent` | PASS | `requestLedger` 的 `containsOldPrompt` 全为 false；两个标记各只出现一次 |
| recall | `recall.marker_sent_once` | PASS | 含标记的 prompt 请求数为 1 |
| restart | `restart.previous_process_tree_terminated` | PASS | `before_terminate` 16 个进程 → 终止后无残留；`runs[].treeAfter` 为空 |
| restart | `restart.initialize_capabilities_recorded` | PASS | 新进程 `initialize` 记录 `resume`/`close` |
| restart | `restart.resume_request_protocol_only` | PASS | `paramsKeys` 仅 `sessionId`/`cwd`/`mcpServers`；不含标记/旧 prompt |
| restart | `restart.resume_request_succeeded` | PASS | 新进程 resume 原 `sessionId` 成功 |
| restart | `restart.no_session_new` | PASS | 重启进程的 `session/new` 计数为 0 |
| restart | `restart.marker_after_restart` | PASS | 回答包含随机标记 |
| restart | `restart.no_replayed_side_effect` | PASS | 无新工具调用；副作用文件未变化 |
| restart | `restart.no_old_prompt_resent` | PASS | 无旧 prompt、记忆标记或副作用标记重发 |
| isolation | `isolation.sessions_distinct` | PASS | A `01a1617a-3325-…`、B `01a1617a-3368-…` |
| isolation | `isolation.b_ready_before_close` | PASS | B 返回 `ready` |
| isolation | `isolation.a_close_succeeded` | PASS | `session/close(A)` 返回 `{}` |
| isolation | `isolation.b_works_after_a_close` | PASS | B 返回 `391`（17×23） |
| isolation | `isolation.shared_process_survived` | PASS | A close 后根进程仍存活；`after_b_turn` 根存在 |
| isolation | `isolation.resource_samples_recorded` | PASS | `after_establish` 10 进程/353.1 MB；`after_a_close` 17/757.7 MB；`after_b_turn` 19/863.1 MB |
| release | `release.session_established` | PASS | `sessionIds.release = 01a1617a-6154-7a83-be21-355c0c8637ee` |
| release | `release.a_close_succeeded` | PASS | `session/close` 返回 `{}` |
| release | `release.memory_after_close` | SKIP | `after_session` 12 进程/535.4 MB → `after_a_close` 14/670.3 MB；单次 WorkingSet 未显著下降，分配器可能保留堆，不能据此判断泄漏或释放成功 |
| release | `release.process_tree_terminated` | PASS | `after_exit`：0 进程、0 MB、根不存在 |
| release | `release.no_orphan_children` | PASS | `after_exit` 无残留进程 |
| environment | `environment.adapter_resolved` | PASS | 已知路径解析 |
| environment | `environment.adapter_version` | PASS | `@agentclientprotocol/codex-acp@2.2.2` |
| environment | `environment.isolated_data_dir` | PASS | data-dir 不在仓库/家目录/真实 `CODEX_HOME` 内 |
| environment | `environment.codex_home_isolated` | PASS | 隔离 `codex-home` |
| environment | `environment.credentials_copied` | PASS | 只记录 `auth.json`、`config.toml` 文件名 |
| 全局 | `protocol.no_invalid_messages` | PASS | `server.protocolErrors = 0` |
| 全局 | `probe.fatal` | —（无 fatal） | 报告未生成该 check；无 fatal 记录 |
| 汇总 | `recall.completed` | PASS | 全部 recall 检查通过 |
| 汇总 | `restart.completed` | PASS | 全部 restart 检查通过 |
| 汇总 | `isolation.completed` | PASS | 全部 isolation 检查通过 |
| 汇总 | `release.completed` | SKIP | 仅 `release.memory_after_close` 未验证（单次 RSS 不足以下结论） |

### 8.3 结论

| 项目 | 填写内容 |
| --- | --- |
| 是否支持 `session/resume` | 是。`initialize` 宣告 `sessionCapabilities.resume`，且实际 resume 成功 |
| 是否支持 `session/close` | 是。`initialize` 宣告 `sessionCapabilities.close`，实际返回成功 |
| close 后可恢复原上下文 | 是（同进程）。close 后对同一 `sessionId` resume，标记回答正确；resume 参数只含 `sessionId`/`cwd`/`mcpServers`，未重发旧 prompt，未新建会话，未重复工具副作用 |
| 重启后可恢复原上下文 | 是。终止旧进程树后起新进程，同一 `codex-home` 下 resume 原 `sessionId`；新进程没有 `session/new`，标记回答正确，无旧输入/副作用重放 |
| A 关闭不影响共享 B | 是。A close 返回成功，B 随后正确回答 17×23=391，共享根进程仍存活 |
| 进程树是否真实释放 | 进程级：是。进程退出后进程树 0 进程、0 MB、根不存在，无孤儿。单会话堆级：**未证明**（`release.memory_after_close = SKIP`；close 后 WorkingSet 从 535.4 升至 670.3 MB，不能区分分配器保留与未释放） |
| 是否达到 WP0 门槛 / 是否阻断 WP1–WP6 | 达到能力门槛，不阻断 WP1–WP6：真实 Codex ACP 宣告并成功执行 resume/close，两条恢复路径上下文连续，A/B 隔离成立，进程树可完整清理。单会话堆级释放仍为未验证项，WP6 不得把它写成已通过 |
| 未完成项与限制 | ① `release.memory_after_close` SKIP：单次 WorkingSet 采样无法证明单会话内存归还；② `release.completed` 因上项为 SKIP；③ 仅 Windows/单次运行，未覆盖 Linux、macOS 与长时间循环；④ 适配器内置 Codex 0.160.1（与本机 CLI 0.163.0 不同），结论只对应本次适配器版本；⑤ `release` 场景只建立一个会话，未做多会话反复回收的容量基线（属于 WP6 的 10/25/50 档） |

填写规则：FAIL 或 SKIP 不得写成 PASS；SKIP 必须保留原因；报告路径必须指向本次真实运行产生的 JSON，不能用其他运行的报告代替。
