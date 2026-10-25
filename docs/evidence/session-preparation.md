# Session preparation — 分层证据

本文件是 `web-session-preparation` 任务 8.1/8.2/8.3 的**证据入口**。分层固定为：

| 层 | 入口 | 覆盖 | 结果 |
| --- | --- | --- | --- |
| fake 组合根 + 真实 SQLite/网络 | `cargo test --locked -p app --test session_preparation` | E1、E5、E6、E8、E9、相同文本第二条、附件 TTL | **11/11 通过**（2026-10-23 主检出；含 WP8Fix2 的真实启动顺序） |
| 真实 Codex 隔离跨进程探针 | `scripts/probe-session-preparation.mjs` | 真实 Daemon/TLS/Sync、真实 `codex-acp`、allocate → 真实模式/配置 → 强制终止/重启 → 同 ID restore → 首条 | **已执行两次**（见 §3、§4）；configure 修复后复跑时 allocate/restart/restore 通过，`session.start` 未在探测预算内返回，首条仍未写成 PASS |
| 真实浏览器 | `clients/app/e2e/session-preparation-live.mjs` + `crates/app/tests/session_preparation_browser.rs` | 分配 → 准备页 → 刷新恢复 → 首条提交 → 正式读取、固定 URL、无 `session.create`、>30 秒续租、陌生 ID 回目录 | 脚本与编排已交付；**未执行**（见 §5，本工作区没有可运行的 `cargo test` 入口与组合好的 Daemon+PWA） |

未执行的项保持「未完成」；下面每一行结果都对应一次真实运行或明确的可重跑命令。

> **2026-10-25 手动验收确认**：仓库所有者在本机手动执行了两层验收并确认通过，任务 8.2/8.3/8.4 据此标记完成。上方历史结果是 2026-10-23 当时的记录，保留原样、不追改为通过；本次确认的原始日志未入库，复核请按 §1 的命令重跑（详见 §7）。

## 1. 命令

静态检查与内置密码学自检（不建目录、不启动进程）：

```text
node --check scripts/probe-session-preparation.mjs
node --check clients/app/e2e/session-preparation-live.mjs
node scripts/probe-session-preparation.mjs --help
node scripts/probe-session-preparation.mjs --self-test
node clients/app/e2e/session-preparation-live.mjs --help
```

真实探针（主检出执行；隔离 data_dir/codex-home）：

```text
node scripts/probe-session-preparation.mjs \
  --server <当前构建的 acp-remote> \
  --adapter <真实 codex-acp 的 dist/index.js> \
  --data-dir <隔离目录> --keep
```

真实浏览器（由 `crates/app/tests/session_preparation_browser.rs` 编排；它设置下列环境变量并运行驱动）：

```text
cargo test -p app --test session_preparation_browser -- --ignored --nocapture
```

驱动本身需要的环境：`SESSION_PREPARATION_ORIGIN` / `_CODE` / `_WORKSPACE` / `_EVIDENCE`；可选
`BROWSER_PATH`、`SESSION_PREPARATION_RENEW_WINDOW_MS`（缺省 35000）。

## 2. 环境

| 项目 | 取值 |
| --- | --- |
| 日期 / 执行人 | 2026-10-23 / 隔离工作区 Agent（omp） |
| 操作系统 / 架构 | Windows 10.0.26300 / x64 |
| Node | v22.20.0 |
| 真实适配器 | `D:/tmp/wp7-agents/tools/node_modules/@agentclientprotocol/codex-acp/dist/index.js`，`@agentclientprotocol/codex-acp@2.2.2` |
| Daemon 二进制 | 本次用**当前工作树源码**在独立 `CARGO_TARGET_DIR=D:/tmp/acpr-wp8-target` 构建（`cargo build --locked -p app --bin acp-remote`）；主检出 `target/debug/acp-remote.exe` 是更早的构建（它协商不到 `core.session-preparation.v1`，不能用作本层证据） |
| 数据隔离 | 每次一个 `--data-dir`（真实 SQLite migration + 真实节点身份 `keystore = "platform"`，位于 data-dir 内）；codex-home 为其下副本，只复制 `auth.json`/`config.toml` |
| 凭据 | 只记录复制/缺失的文件名；从不读取、打印或写入报告 |
| 用户服务 | 未重启、未读取、未停止用户正在运行的 Daemon；探针在独立 loopback 端口启动自己的实例 |

## 3. 真实探针：可重跑结果

运行（最近一次，`--keep` 保留隔离目录以便复查）：

```text
node scripts/probe-session-preparation.mjs \
  --server D:/tmp/acpr-wp8-target/debug/acp-remote.exe \
  --adapter D:/tmp/wp7-agents/tools/node_modules/@agentclientprotocol/codex-acp/dist/index.js \
  --data-dir D:/tmp/acpr-prep-probe-8 --keep --timeout-ms 30000
```

| 项目 | 结果 |
| --- | --- |
| `--help` | 退出码 0；未创建目录、未启动子进程 |
| `--self-test` | 退出码 0，**25/25 PASS**（5 份固定 transcript 的编码/SHA-256/HMAC/P-1363 验签、篡改反例、准备凭据固定向量与签名回环） |
| 真实运行退出码 | 2（存在 FAIL，未冒充 PASS） |
| 汇总 | PASS 21 / FAIL 3 / SKIP 10 |
| 原始报告 | `D:/tmp/acpr-prep-probe-8/report.json` |
| Agent 边界证据 | `D:/tmp/acpr-prep-probe-8/agent-evidence.ndjson`（只有 pid、方法名、参数键、prompt 长度/SHA-256/是否与提交正文逐字相同） |

### 3.1 场景 ↔ 结果（第一次真实运行）

| 场景 | check | 结果 | 证据 / 备注 |
| --- | --- | --- | --- |
| 环境 | `environment.server_binary` / `adapter_resolved` | PASS | 路径与 SHA-256 在报告 `environment` |
| 环境 | `environment.isolated_data_dir` / `codex_home_isolated` / `credentials_copied` | PASS | 只记录文件名 |
| 环境 | `environment.tls_certificate` | PASS | 自签 `localhost` 证书（openssl 生成） |
| 环境 | `environment.daemon_ready` | PASS | 真实 TLS 监听 + `/pair` 200 |
| 环境 | `environment.feature_negotiated` | PASS | 业务连接协商到 `core.session-preparation.v1`，Host proof 验签通过 |
| E1 | `allocate.ready` | PASS | Owner 分配 sessionId 并签发凭据；运行期新资源代 |
| E1 | `allocate.real_modes` | PASS | 模式来自真实 `codex-acp` 的 `session/new`（`read-only`/`workspace-write`/`agent`/`agent-full-access`） |
| E1 | `allocate.real_config` | PASS | 配置项来自真实 Agent（`reasoning`，select） |
| 准备设置 | `allocate.configure_applied` | **FAIL** | `configure` 被服务端以 `protocol.schema_invalid` 拒绝（见 §4.1）；未把“捕获到真实设置”写成“可切换” |
| E1 | `allocate.no_formal_session` | PASS | 准备期间 `session.read` 回 `session.not_found`（没有正式行） |
| E4 | `restart.previous_process_tree_terminated` | PASS | 强制终止后 Daemon 与 Agent 进程树都消失 |
| E4 | `restart.same_id_restored` | PASS | 同一 receipt 的 `restore` 回到同一 sessionId 且 `ready` |
| E4 | `restart.new_runtime_epoch` | PASS | 运行期资源代改变 |
| E4 | `restart.new_endpoint_process` | PASS | 适配器由**新进程**重建（新旧 pid 都记录在报告） |
| E4 | `restart.no_prompt_before_first_message` / `no_formal_resume` | PASS | 重启+restore 后没有自动 prompt、没有 `session/resume` |
| E5 | `first_message.*` | **SKIP** | `session.start` 在 30 秒内没有任何结果/错误帧（见 §4.2），探针按超时结束；没有写成通过 |
| 附件 | `attachment.*` | 未驱动 | 见 §4.3 |

### 3.2 configure 修复后的复跑（2026-10-23）

第一次运行暴露 `configure` 被 `protocol.schema_invalid` 拒绝。根因有两层：

1. 真实 `codex-acp` 同时把 `mode` 暴露为 `modes` 与一个 `category: "mode"` 的配置项；探针把后者也塞进
   `configOptions`，服务端再走 `set_config_option` 时被 Agent 拒绝。
2. `agent_host::apply_selection` 把单个配置项的 `PortError::InvalidRequest` 当成整帧失败；按 D4 应保留为
   `unavailable` 并阻止派发。

修复后（探针跳过 `category: "mode"`；配置项拒绝映射为不可用），用当前源码构建的 daemon 复跑
`D:/tmp/acpr-prep-probe-9`：

| 场景 | 结果 |
| --- | --- |
| 环境 / 配对 / allocate（真实模式与配置） | PASS |
| 强制终止后 Daemon 与 Agent 进程树消失 | PASS |
| 同 ID `restore`、新 runtimeEpoch、新适配器进程 | PASS |
| 准备期间无 prompt / 无旧 `session/resume` | PASS |
| `session.start` 首条 | **未验证**：探测预算内没有收到结果；外部 600s 终止后未写出报告 |
| 浏览器层 | **未执行**（见 §5） |

这次复跑没有写成通过；`session.start` 的未完成状态保留为待查项。不要用第一次运行的 report 充当复跑结果。

## 4. 未通过项与未覆盖项（不写成成功）

### 4.1 `session.preparation` 的 `configure` 被 schema 拒绝

观察到的事实：`configure`（同一凭据、同一 `runtimeEpoch`/`generation`，带 `mode` 与
`configOptions: { "<id>": "<value>" }`）返回
`status = "failed"`、`error.code = "protocol.schema_invalid"`。`allocate`/`restore` 使用同一份
`receipt` 都能通过，因此**不是**凭据问题；候选原因只可能是 `mode`/`configOptions` 的线形状，或服务端
闭合分支的校验与 §11.9 的机器 schema 不一致。

- 本轮**没有**把它修掉：按分工产品代码属于 wp-coder；探针只报告可观察事实。
- 影响：证据中“真实设置可切换”一项保持 FAIL；`session.start` 携带同样的可选字段（`mode`/
  `configOptions`），它的超时可能与同一原因相关，但尚未证明（§4.2）。
- 未做的事：没有为了“让探针变绿”而删掉这些字段或把 FAIL 改标 SKIP。

### 4.2 首条提交没有收到任何响应帧

观察到的事实：`session.start`（`kind: "initial"`，含 `receipt`/`runtimeEpoch`/`generation`/`content`
与同样的可选 `mode`/`configOptions`）发出后，WebSocket 在 30 秒内没有收到 `command.result`、
`error` 或关闭帧（`expectResult` 因此超时，阶段整体 SKIP）。30 秒是人为缩短的诊断超时；默认 240 秒的
同款运行在 900 秒的命令预算内同样没有越过该阶段。

- 因此 `first_message.*` 的七项全部保持 SKIP；没有把“没观察到失败”写成“已检视”。
- 这个事实本身值得进主检出复查：若 `session.start` 的载荷被服务端整帧丢弃，客户端会挂起而不是得到
  `command.result(rejected)`；若只是本轮探针的帧形状问题，主检出可用 `cargo test --locked -p app --test session_preparation`
  的同一路径对照。

### 4.3 附件分段耗时

本层**没有**驱动准备附件（`attachment.begin`/`manifest_begin` 的 `target: preparation` 分支）：
真实附件上传/封存/认领与分段耗时由浏览器层观察，探针不伪造时间数字。因此报告里没有 upload/seal/claim
的分段值。浏览器层本轮未执行（§5）。

## 5. 真实浏览器层（脚本已交付，运行未执行）

交付：

- `clients/app/e2e/session-preparation-live.mjs`：配对 → 目录入口分配 → 准备页（真实模式/配置）→
  草稿（文字 + 非默认模式/配置 + 附件）→ 刷新恢复 → 35 秒续租观察 → 首条提交（单次上传/封存/清单
  认领 + 分段耗时）→ 正式读取刷新 → 陌生 ID 回目录；断言只看 DOM 钩子与真实出站/入站帧。
- `crates/app/tests/session_preparation_browser.rs`：真实 Daemon + TLS + 构建后的 PWA + 受控 stdio
  Agent；脚本结束后再断言 Agent 边界（恰好一次 `session/new`、零 `session/resume`、唯一 prompt 与
  提交正文逐字相同、非默认 mode/config 真的下发）与持久行（一个正式会话、一条用户消息、首条关联
  `terminal`、附件引用存在）。需要 `clients/app/dist` 已嵌入 Daemon。

本轮**未执行**该层，原因是环境而不是断言通过：

- 隔离工作区没有 `target/`，`cargo test` 的入口（`CARGO_BIN_EXE_acp-remote` 与集成测试编排）只在主
  检出存在；本工作区不跑仓库级验证。
- 主检出的 `target/debug/acp-remote.exe` 是更早的构建，协商不到准备 feature，不能充当浏览器层的对端。
  用当前源码 + 真实 dist 构建的 Daemon 由主检出 PV8 首次运行。
- 浏览器层要求 **当前的** `clients/app/dist`：Daemon 在编译期嵌入静态资源。本工作区核对到主检出
  `clients/app/dist` 的 `buildIdentifier`（`909bc4ac-dirty`）不等于当前 HEAD（`28c21260`），且已构建
  产物只含部分准备页 `data-*` 钩子（缺 `preparation-phase`/`preparation-input`/
  `preparation-attachment`/`preparation-lease-expired`）。因此运行前必须先
  `cd clients/app && npm.cmd run export:web`，再构建 Daemon；否则驱动会在等待准备页钩子时以可读超时失败，
  不会被当成通过。

已做的静态核验：两个脚本 `node --check` 通过；`--help` 退出 0；驱动使用的事件名/帧名与
`docs/SYNC_PROTOCOL.md` §11.9、`PreparationPanel` 的 `data-*` 钩子逐项对应。

因此浏览器层结果保持**未执行**；主检出的命令是：

```text
cargo test -p app --test session_preparation_browser -- --ignored --nocapture
```

它失败时会把脚本证据 JSON、Agent 证据与 Daemon 日志一起打出来，不会把“浏览器退出 0”当成通过。

## 6. 脱敏与限制

- 探针从不读取/打印 `auth.json` 与 `config.toml` 的内容；配对码、配对 secret、设备私钥只在内存里。
  报告写盘前还做一次文本自检：出现配对 secret 或设备私钥即判 FAIL（本次为 PASS）。
- Agent stderr 与 Daemon stdout/stderr 只落盘到隔离 data_dir；报告只记录路径与字节级别的采集计数。
- Agent 边界证据不含 prompt 正文：只记录长度、SHA-256、是否含本次标记、是否与提交正文逐字相同。
- 限制：只在 Windows x64 运行；真实 Codex 的模型输出天然非确定，证据只断言“标记被原样送达”，不断言
  回复措辞；`--keep` 保留的隔离 codex-home 含凭据副本，取证后应手动删除（本次为 `D:/tmp/acpr-prep-probe-8`）。
- 不声称覆盖：Linux/macOS、真实网络时延、多个真实设备并发、浏览器 BFCache/冻结态（后两者属前端套件与
  产品能力边界，不在本探针）。

## 7. 2026-10-25 手动验收确认

仓库所有者在本机手动执行了 §3.2 的真实 Codex 探针与 §5 的真实浏览器驱动，并确认两项均通过；任务
8.2（真实验收）、8.3（证据与分层记录）、8.4（最终门禁与检视收口）据此标记完成。

- 本次确认**没有**在仓库内留下原始日志、报告或分段耗时数值，因此本文件不补写具体计数；§3.1/§3.2 的
  2026-10-23 结果（含 FAIL 与 SKIP）保持原样，作为历史记录。
- 需要复核时使用 §1 的命令重跑；真实浏览器层要求当前源码构建的 Daemon 与 `clients/app` 当前
  `dist`（`npm run export:web` 后再编译 Daemon），否则准备页钩子不匹配。
- §6 的范围限制仍然有效：Linux/macOS、真实网络时延、多设备并发与浏览器 BFCache/冻结态不在本层覆盖内。
