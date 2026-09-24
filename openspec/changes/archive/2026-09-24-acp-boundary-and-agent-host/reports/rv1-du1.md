> 转录说明：本文件由主 Agent 从独立复核者（`oracle`，run `9e0ae308…`）的输出逐字转录；为通过文档引用门禁，仅对文档引用写法做规范化，不改结论。

# DU1 候选版本独立检视报告（6.4 + 6.8）

**task_id**：`6.4` + `6.8`（变更 `acp-boundary-and-agent-host` / 交付单元 DU1）
**role**：独立 reviewer（`openspec/schemas/agentic/roles/reviewer.md`），Review ID `RV-DU1-round1`，Review Type = integration + merge / post-merge，Review Stage = 候选已合入本地 `main` 后
**phase**：Merge Unit（6.x）候选与合并新增差异检视
**agent_context**：全新隔离上下文，未参与本变更任何实现或讨论；工具 = 只读文件工具 + `bash`（仅只读命令：`git --no-pager`、`node`、`grep`、`cat`、`diff`）。会话内未写任何仓库文件、未做任何 git 写操作、未运行 `cargo` 任何构建或测试、未运行 `npm run check*`（按调度约束）。
**target_revision**：`e4a4492`（`e4a44923ac5429e1976db8cc9d7f44969ba930a1`）；基线 `094009b`（`094009b31f32c42928c01f2943eb7cf6e15c6154`）。合并后主分支记录提交 `582133e`（`582133ebf86b96986f699a24db59bd969f24c8f1`）；会话中段父 Agent 把 `verification.md` 提交为 `ceac8af`（`ceac8af875937c9b2fb7a941c4a07d31fcd6304f`，纯文档，见 6.8）。
**scope**：`094009b..e4a4492` 全区间（58 文件 / +13371 −49，新增 `crates/acp-protocol`、`crates/agent-host` 两个 crate）；6.4 五问（接口一致性、`LaunchSpec` 边界、三处口径、测试是否被削弱、既有 crate 公开契约）+ 6.8 两问（fast-forward 判定、候选外新增代码差异）。

## 版本稳定性与方法说明

- 代码判据取自**候选版本本身**：`git --no-pager diff --stat e4a4492 -- crates/` 为空（工作树 `crates/**` 与 `e4a4492` 逐字节一致），因此直接读取磁盘上的 `crates/agent-host/**`、`crates/acp-protocol/**`、`crates/core/**` 等价于读取候选提交。
- 候选之后被改过的文档（`design.md` 等）改用 `git --no-pager show e4a4492:<path>` 取值（例：`design.md:77/98` 在候选版本仍是绝对 `cfg` 判据，见 F1 说明）。
- `HEAD` 会话开始时 = `582133e`，结束时 = `ceac8af`（父 Agent 提交 `verification.md` 的 2 行文档修正）。本报告所有 6.8 结论对 `582133e` 与 `ceac8af` 两个引用分别复算，结论相同。
- 会话开始时工作树曾有一处父侧 staged 改动（`openspec/changes/.../verification.md`），结束时不存在；`.pi/settings.json` 的修改在我第一次 `git status` 之前就已存在，非本会话所为。结束时工作树除该文件外干净（我误建的一个临时文件已删除，见 manualNotes）。

## checks（实际执行的命令与结果）

| 检查 | 命令 | 结果 |
| --- | --- | --- |
| 引用解析 | `git --no-pager rev-parse e4a4492 094009b 582133e main` | `e4a44923ac5429e1976db8cc9d7f44969ba930a1` / `094009b31f32c42928c01f2943eb7cf6e15c6154` / `582133ebf86b96986f699a24db59bd969f24c8f1` / `main = 582133e`（会话末 `HEAD = ceac8af`） |
| 祖先关系（6.8） | `git --no-pager merge-base --is-ancestor e4a4492 582133e; echo $?` | `exit=0`（对 `ceac8af` 亦 `exit=0`） |
| 合并提交 | `git --no-pager log --merges --oneline 094009b..582133e` | 空（0 个合并提交） |
| 线性性 | `git --no-pager log --format='%h %p %s' 094009b..582133e`；`rev-list --count` | 13 个提交、全部单父（`094009b→4b0145e→f2ca1f5→64a7582→e6b4dfa→df734e3→9d3bb63→6f1515a→4c24fbd→d0fa731→e4a4492→1035395→f7bcf5f→582133e`） |
| 区间规模 | `git --no-pager diff --stat 094009b..e4a4492` | 58 files, +13371 −49；`4b0145e` 的直接父为 `094009b` |
| 候选后差异（6.8） | `git --no-pager diff --stat/--name-only e4a4492..582133e`；`… -- crates/ Cargo.toml Cargo.lock scripts/` | 10 文件全在 `docs/`、`openspec/changes/acp-boundary-and-agent-host/` 内；代码路径 diff **为空**（对 `ceac8af` 同样为空） |
| 契约门禁 1 | `node scripts/check-crate-boundaries.mjs` | **exit 0**：`crate boundaries OK: 8 个 crate 的依赖方向与 §5 矩阵一致（8 个已在矩阵登记）` |
| 契约门禁 2 | `node scripts/check-doc-links.mjs`（按约束替代 `check:docs`） | **exit 0**：`doc links OK: 367 relative links, 2851 section refs across 144 markdown files`（1185 处归属未判定，属设计） |
| 门禁脚本是否被削弱 | `git --no-pager diff 094009b..e4a4492 -- scripts/check-crate-boundaries.mjs` | 仅注释块改写（“8 个 crate … 9 个「列」”口径），**逻辑零改动**（`columns.forEach` 比对与 `CORE_ALLOWED_CLOSURE` 校验未动） |
| 依赖清单增量 | 自行解析 `094009b:Cargo.lock` 与候选 `Cargo.lock` 的 `name/version` 对集合后 `comm -23` | 基线 212 对 → 候选 232 对；**仅增不减、无版本变更**（`comm -23` 输出为空） |
| 跳过标记 | `grep -rn "#\[ignore\]" crates/` | 本变更两 crate **零命中**；工作区仅 2 处既有 `#[ignore = "…"]`（`crates/storage-sqlite/tests/commit.rs:1092`、`tests/migration.rs:747`，均带理由） |
| 测试删除审计 | `git diff --numstat 094009b..e4a4492 -- crates/*/tests`；`git log -p … -- crates/agent-host/tests \| grep '^-[^-]'` | 相对基线**删除行数为 0**（全部 `+N 0`）；fix 轮内 75 行删除属重构 + 一处**恒真断言**移除（见 6.4 第 4 问） |
| 平台分支分布 | `grep -rn "cfg(\|cfg!" crates/agent-host/src/` | `platform.rs:21,62,121,177`（平台分支）；`launch.rs:159`（`#[cfg(test)]`）；`bin/` **零命中** |
| 日志面 | `grep -rn "tracing::" crates/agent-host/src` | 23 处，均只带方法名/字段名/计数/字节数；无一处格式化 `LaunchSpec`、env 值或 args |
| 未执行（按约束） | `cargo fmt/clippy/build/test`、`npm run check`、`npm run verify`、`check-acp-compatibility`、`check-schema-fixtures`、PV1/PV4/PV5 | **未执行**（约束禁止；结论不依赖我对这些命令的重跑）。相关既有日志仅作为**带出处引用**：`reports/du1-pv1.log`、`reports/du1-main-verify.log`、`wp2/wp3/wp4/wp5-*.log`、`pv5-windows-tree.log`、`alt-final-verification.md` — **均未由我复跑，不作为我本轮 PASS 的依据** |

## findings

| 编号 | 结论 | 级别 | 证据 |
| --- | --- | --- | --- |
| **6.4-1（接口一致性）** | **PASS** — `agent-host` 只经 `acp-protocol` 公开 API 使用 wire 层，未分叉判别子/上限/分帧语义；`AcpRaw` 与 `PortError` 映射符合 core 端口契约 | — | 见下方 F-detail「Q1」 |
| **6.4-2（`LaunchSpec` 边界）** | **PASS** — 单一产出方/消费方；凭据只以「变量名→值」进入 `env_clear()` 之后的注入；`Debug` 与全部 `tracing` 调用点均不暴露凭据值或参数正文 | — | 见 F-detail「Q2」 |
| **6.4-3（三处口径一致）** | **PASS（含 2 条 MINOR 口径残留，见 F1/F2）** | — | `Cargo.toml:3-12` members（8）/`:22-56` 依赖登记 ↔ `docs/MODULE_ARCHITECTURE.md` §5 矩阵 `agent-host` 行 = ✓core ✓acp-protocol、`acp-protocol` 行全空（`:421-437`，含缺列注记）↔ 变更声明 `proposal.md:9-11,24,70-71`、`design.md:29-50`、`plan.md:508-523`、`tasks.md` 2.1/2.2/2.4/2.5/2.12。实跑两门禁均 exit 0（见 checks）。门禁脚本改动为注释级，未放宽判据 |
| **6.4-4（是否只为让测试通过）** | **PASS** — 无断言削弱、无场景删除、无错误被吞、无新增 `#[ignore]` | — | 见 F-detail「Q4」 |
| **6.4-5（既有 crate 公开契约）** | **PASS** — `crates/core/**`、`crates/storage-sqlite/**`、`crates/acpr-*`、两个既有协议 crate **零文件改动** | — | 见 F-detail「Q5」 |
| **F1** | **FAIL（MINOR，文档口径自相矛盾）** — 权威文档仍写「平台差异只存在于 `agent-host::platform` **与它的 `bin/`**」，而同一变更的 `tasks.md` 2.17 / `plan.md:601` / `lib.rs:21` / `design.md:77` 均写「`bin/` 零命中」；实测量亦为零命中 | MINOR | `docs/MODULE_ARCHITECTURE.md:280`（该行由本变更新增，`094009b..e4a4492` 的 `+` 行）；`grep -rn "cfg(" crates/agent-host/src` → 仅 `platform.rs:62/121/177`、`platform.rs:21 (cfg!)`、`launch.rs:159 (#[cfg(test)])`，`bin/` 0 命中；`crates/agent-host/src/lib.rs:21`、`tasks.md`（2.17 完成条件）、`plan.md:601` 均为「`bin/` 零命中」。**无门禁覆盖此措辞**（`check:docs` 只统计小节引用；`check:contract-drift` 只比 §7 DDL 与 §5 端口签名）。可选修法：删去「与它的 `bin/`」 |
| **F2** | **FAIL（MINOR，变更声明 vs `Cargo.toml` 依赖面漂移）** — `tasks.md` 2.12 声明的 `agent-host` 依赖与实现不一致 | MINOR | `tasks.md:21` 写 `tokio` 的 `process`/`io-util`/`sync`/`time` + `serde_json`/`thiserror`/`sha2`/`tracing` + `win32job`/`nix`；实际 `crates/agent-host/Cargo.toml:14-29` = `tokio` features `["process","io-util","macros","rt"]`，且**另有** `async-trait`、`base64`。`Cargo.toml:17-18` 的注释「tokio 只提供进程/IO 类型与同步原语」与已启用的 `macros`/`rt`（`process.rs:182-192, 402` 用 `tokio::spawn`）不相称。功能无影响（`sync`/`time` 由 workspace 级 feature 提供；本 crate 不建 runtime），但声明与代码的两份事实无门禁抓（`check:boundaries` 只比工作区内依赖边） |
| **F3** | **FAIL（MINOR，耦合无断言）** — agent→client 方法分派是第二份字面量清单，未走 `acp_protocol::methods` | MINOR | `crates/agent-host/src/session.rs:296-312` 用字面量匹配 `"session/update"`/`"session/request_permission"`/`"elicitation/create"`，其余回 `CODE_METHOD_NOT_FOUND`；`crates/acp-protocol/src/methods.rs` 的 `status_of`/`direction_of` 未被 `Envelope::ensure_direction`（`envelope.rs:158-184`）使用。**实核当前一致**：registry 中 `AgentToClient` 且 `implemented = true` 的恰为这三条（`methods.rs:177,184,240`；`fs/*`、`terminal/*`、`elicitation/complete`、`$/cancel_request` 均 `false`）。风险：registry 未来变化时该集合静默漂移，无测试断言两集合相等 |
| **F4** | **FAIL（MINOR，终态事件可被静默丢弃）** — 事件构造失败时不发事件也不记日志 | MINOR | `session.rs:261-263`（turn 终态）、`session.rs:288-290`（进程退出终态）、`session.rs:786-793`（`session.mode.changed`）三处均为 `if let Ok(event) = … { self.emit(event) }`，`Err` 分支无 `tracing`。终态要求「恰好一次」。可达性≈0（`mapper::turn_event` 的 `EventType` 为固定字面量、view 极小），故不阻断；同类「静默丢弃」正是 RV3/RV4 反复点到的模式 |
| **F5** | **SUGGESTION** — 两处 `unwrap_or`/`.ok()` 把失败掩成合法值 | SUGGESTION | `session.rs:515-519`：`serde_json::from_str::<Value>(&values.to_json_text()).ok()` 失败时把用户作答静默降级成 `content: None`（而非报错/记录）；`mapper.rs:279-282`：`serde_json::to_value(other).unwrap_or(Value::Null)` 失败时把结构化 `block` 降级成 `null`。二者用本 crate 自带的编码/解码配对，实践不可达（自产 JSON 不会被自己拒绝），故只作建议：改为显式错误或至少 `tracing::warn` |
| **F6** | **已知缺口，已登记，不阻断（MINOR）** — `TurnAccepted.turn` 是适配器本地占位 id | MINOR | `session.rs:215-219` 返回 `self.ids.turn_id()`，注释自陈「不具权威性」；core 侧 `crates/core/src/broker.rs:1637-1638` 为 `Ok(_) => Ok(true)`，**不读取该值**，故今天不构成接口破坏。已作为 Check Plan Change 3 登记并获用户裁定走 core 侧收口（`reports/du1-integration.md`「未随合并完成的事项」） |
| **6.8-1（是否 fast-forward）** | **PASS** — 是 fast-forward，无合并提交 | — | `merge-base --is-ancestor e4a4492 582133e` → `exit=0`（对 `ceac8af` 亦 0）；`log --merges 094009b..582133e` 空；`%h %p` 全单父；与记录一致：`reports/du1-integration.md` 记 `094009b → 6f1515a`（7 提交 ff-only），随后 `4c24fbd`/`d0fa731`/`e4a4492` 在 `main` 上叠加（`094009b..e4a4492` = 10 提交、0 合并、58 文件 +13371/−49，与我的 `diff --stat` 一致） |
| **6.8-2（候选之外是否引入代码差异）** | **PASS** — 无新增代码差异 | — | `git --no-pager diff --stat e4a4492..582133e -- crates/ Cargo.toml Cargo.lock scripts/` = **空**；`--name-only e4a4492..582133e` = 10 文件，全部落在 `docs/`（仅 `docs/SECURITY_DESIGN.md` 一个既有文档，1 行括注）与 `openspec/changes/acp-boundary-and-agent-host/`（`design.md`/`plan.md`/`spec.md`/`tasks.md`/`verification.md`/`reports/*`）之内；对 `ceac8af` 复算同样为空。即候选之后只有文档/证据提交 |
| **6.8-3（已有 review ID 记录）** | **PASS（记录项）** | — | RV1：`reports/rv1-wp1.md`（`RV-WP1-round1` 检视 `4b0145e→64a7582`、`RV-WP1-recheck` 复核 `64a7582→e6b4dfa`）+ `rv1-wp2/3/4/5.md`；RV2：`rv1-wp*.md` 的复核轮（修复落 `df734e3`、`6f1515a`）；RV3：检视 `4c24fbd`（修复落 `d0fa731`，`rv1-wp1.md` RV3 段 / `rv1-wp5.md`）；RV4：检视 `d0fa731`（修复落 `e4a4492`，`rv1-wp1.md:107-…`、`rv1-wp5.md` RV4 段）；RV5：检视 `e4a4492`（`rv1-wp5.md` RV5 段，oracle run `3cbb9840…`）。汇编见 `verification.md:154-179`、`:210-212` |

### F-detail（PASS 行的可证伪依据）

**Q1（`acp-protocol` ↔ `agent-host` 接口一致性）**
- 全部 wire 使用点都在公开 API 上：`process.rs:19,253,284,294,305,561,564`（`Envelope`/`RawDocument`/`IdLiteral`/`MessageClass` + `message::{request_with_u64_id,notification,response,error_response}`）、`session.rs:24-26,316,420,511-520`、`mapper.rs:20-22,341,372,377,440`、`host.rs:27-29,285,469`、`config.rs:8`。`acp-protocol/src/lib.rs` 的 9 个 `pub mod` 与 4 个根导出覆盖了这些路径。
- 上限**是别名不是复制**：`crates/agent-host/src/limits.rs:29` `pub const MAX_MESSAGE_BYTES: usize = acp_protocol::limits::MAX_MESSAGE_BYTES;`；`read_loop`/`handle_line` 的超限判据（`process.rs:467,531`）与 `RawDocument::parse` 的 `check_size`（`raw.rs:47-54`）同源同值（1 MiB），且超限一律 `abort_agent` 结束该 Agent（`process.rs:496-518`），不是「忽略这一条」。
- stdio 分帧归 `agent-host`（`process.rs:446-493`，含跨读持久缓冲、`\n`/`\r` 剔除、EOF 残帧兜底），`acp-protocol` 明确不承担换行（`raw.rs:18-20`）——层次划分与 `design.md` D2 一致，未出现两处各写一套分帧。
- 判别子语义未分叉：`session/update` 经 `message::session_notification` 解码（`session.rs:316`），11 种已知判别子与 `SessionUpdate::Unknown` 的可见降级由 `acp-protocol::update` 决定（`mapper.rs:225-237`），结构化内容走 `content::{ContentBlock,ToolCallContent}`（`mapper.rs:279-286`、`is_structured_content`/`has_diff`）；未登记方法走「显式不支持」（`session.rs:300-310` 回 `CODE_METHOD_NOT_FOUND`，`host.rs:407-419` 同理），与 `AcpError::Unsupported`（`error.rs:69-79`）语义一致。`methods.rs` 是矩阵的手工镜像，由 `tests/matrix_tables.rs` 逐条比对（`methods_table_matches_matrix` 断言条数与每行字段），故不存在「只藏在代码里的第二份支持列表」。
- `AcpRaw` 三要素符合 core §3.4/§5.1 与 `SYNC_PROTOCOL.md` §10.3（第 49/925 行）：`mapper.rs:424-436` 用 `raw.as_str()` 的 UTF-8 字节算 SHA-256 后编码为**无填充 base64url**，交给 `AcpRaw::available("application/json", raw, digest)`（`crates/core/src/model/event.rs:92-105` 自行校验良构 JSON 并由 `raw_json.len()` 导出 `byte_length`）；`process.rs:458-461` 已剥掉 `\n` 与 `\r`，因此不含 stdio 分隔符。未知字段/扩展 payload 走原文承载，未先经 `Value` 往返（`raw.rs` 的保真机制 + `tests/raw_fidelity.rs` 的反例断言）。
- `PortError` 映射逐臂显式，无通配吞噬：`crates/agent-host/src/error.rs:108-146` 覆盖全部 20 个变体，使用 core 既有 `ConflictKind::{AlreadyResolved,AlreadyExists}`（`crates/core/src/model/error.rs:19,29`）与 `UnavailableKind::{Busy,IoError,KeystoreUnavailable}`（同文件 `:76,80,89`），未新增 core 枚举取值；`CredentialUnavailable → Unavailable(KeystoreUnavailable)` 与 `docs/CORE_PORTS_AND_STORAGE.md:610-620` 的失败关闭口径一致。唯一可讨论项是协议损坏映射为 `InvalidRequest("agent protocol error")` 而非 `Corrupt`，但那是**对端输入**分类，且 `PortError::Corrupt` 的既有语义是本地存储只读失败关闭（§5.2/§9），不构成契约违反。

**Q2（`LaunchSpec` 边界）**
- 产出方唯一：`launch::resolve_launch`（`crates/agent-host/src/launch.rs:60-141`），唯一调用点 `host.rs:233-235`；消费方唯一：`Supervisor::start`（`process.rs:113-122`），supervisor 不读 profile、不解析凭据、不知道白名单（`launch.rs:19` 的模块文档即此约定）。公开面为 `lib.rs:39` 的 `pub use launch::{LaunchSpec, resolve_launch}` 与 `process::Supervisor`。
- 凭据形态：`LaunchSpec.env: Vec<(String,String)>`（`launch.rs:20-31`），明文仅在 `launch.rs:110` `expose_secret().to_owned()` 一处从 `SecretValue` 取出；注入严格是「先 `env_clear()` 再逐项 `env(name,value)`」（`process.rs:119-122`），参数走数组（`process.rs:117`），全程不过 shell。
- 失败关闭（全部在 spawn 之前）：白名单是上限的纵深复核 `launch.rs:88-99`、重复变量名 `launch.rs:100-106`、期望集合（`env_allowlist ∩ env` 绑定）必须全在 `launch.rs:113-127`（且刻意早于 `NECESSARY_ENV` 注入，避免宿主变量冒充绑定）、`ACPR_`/`ACP_REMOTE_` 静态拒绝 `launch.rs:145-163`。配套用例：`launch.rs:159-195`（保留前缀）、`tests/catalog.rs:1087-1115`（拒绝发生在 spawn 之前，用 dump 文件不存在作证）。
- `Debug` 脱敏：手写实现只输出 `program`、`arg_count`、`arg_lengths`、`env_names`、`env_count`（`launch.rs:34-49`），并由 `launch.rs:196-230` 断言「凭据值」与「参数正文」两串都不出现在 `format!("{spec:?}")` 里。`expect`/`unwrap` 在 `crates/agent-host/src` 非测试路径零命中（`grep` 复核）。
- 日志面：23 处 `tracing::` 调用的字段集合仅为方法名、字段名、`id`、字节数/计数、`dropped_bytes`、`status` 摘要；无一处格式化 `LaunchSpec`、`spec.env` 或 `spec.args`。stderr 内容是否进日志由 `process.rs:609-612` 明确禁止（只记计数），与 `docs/SECURITY_DESIGN.md:365,378` 的新口径一致。

**Q4（是否只为让测试通过）**
- 无 `#[ignore]` 新增（本变更两 crate 零命中）；工作区既有的 2 处 `#[ignore = "…"]` 在 `storage-sqlite` 且早于本变更、均带理由。
- 相对基线**没有删除任何测试行**（`git diff --numstat 094009b..e4a4492 -- crates/*/tests` 全部 `+N 0`）；修复轮内部的 75 行删除中，唯一被删的**断言**是 4 条 `!names.contains("ACPR_*")`——它们在 `tests/catalog.rs:322-325` 被明确记为**恒真断言**（父进程环境里本就没有这些名字），并以更强的**静态拒绝 + spawn 前不启动子进程**用例（`tests/catalog.rs:1087-1115`）替代。属「删掉无效断言、补上可证伪断言」，方向与「让测试通过」相反。
- 断言密度：`acp-protocol/tests` 176 处、`agent-host/tests` 199 处 `assert*`；`acp-protocol` 侧还有反例断言（`tests/raw_fidelity.rs` 断言「`Value` 再序列化 ≠ 原文」）与矩阵逐行比对（`tests/matrix_tables.rs`），这两类断言一旦实现回退会先变红。
- 错误是否被吞：`error.rs`/`host.rs` 的所有端口映射臂显式、无 `_ =>` 兜底；`host.rs` 的失败路径都返回显式错误（`ensure_runtime` 的关闭中拒绝 `:208-217`、协商失败回收进程 `:249-253`、`open()` 拒绝死 runtime `:512-514`）。15 处 `let _ =` 全部落在「主错误已返回」或「接收端可能已消失」的尽力路径上（`process.rs:136,156,385,413,439,485,510,571`；`host.rs:409`；`session.rs:303,372,391,436,574`），没有一处吞掉主流程错误；唯二值得收紧的降级式静默是 F4/F5。

**Q5（既有 crate 公开契约）**
- `git --no-pager diff --name-only 094009b..e4a4492 | grep -v '^crates/agent-host\|^crates/acp-protocol\|^docs/\|^openspec/\|^AGENTS.md\|^scripts/\|^Cargo'` → 仅 `README.md`。逐条判断全部非新增 crate 的文件：`AGENTS.md`（§4 状态表两个 crate 移入「已落地」、§9 未落地名单同步，`AGENTS.md:88-97,256`）、`Cargo.toml`（members +2、`[workspace.dependencies]` +tracing/win32job/nix、修正「sqlite 是唯一 runtime 持有者」注释）、`Cargo.lock`（**纯增量**：212→232 对，`comm -23` 为空，既有依赖版本无变化）、`README.md`（「仓库当前状态」表两行 + 「尚未开始」名单）、`docs/DEVELOPMENT_PLAN.md` §2、`docs/INITIAL_DESIGN.md:139`（stderr 口径）、`docs/MODULE_ARCHITECTURE.md`（§3/§3.1/§4.2/§4.5/§5）、`docs/SECURITY_DESIGN.md:365,374-378`（stderr 口径 + `ACPR_` 静态拒绝决定）、`openspec/config.yaml:11`（6 → 8 成员）、`scripts/check-crate-boundaries.mjs`（仅注释）。**`crates/core/**`、`crates/storage-sqlite/**`、`crates/acpr-transcript/**`、`crates/acpr-wire/**`、`crates/sync-protocol/**`、`crates/node-link-protocol/**` 一个文件都没有改**（`--name-status` 中所有 `M`/`A` 均落在上述清单内）——因此不存在对既有 crate 公开契约的悄悄破坏；core 的行 129-146 的 `PortError` 定义与 `:76-135` 的 `AcpRaw` 定义与基线一致，agent-host 只是**消费**已有枚举取值。

## result

- **6.4**：**PASS**。Q1–Q5 五项均通过，未发现 CRITICAL/MAJOR；列出 4 条 MINOR（F1 权威文档「`bin/`」措辞自相矛盾、F2 `tasks.md` 依赖面与 `Cargo.toml` 漂移、F3 方法分派无相等断言、F4 终态事件可静默丢弃）与 1 条 SUGGESTION（F5）+ 1 条已登记缺口（F6）。四项 MINOR 均不影响行为/协议/安全/用例有效性，且都属「文档口径」或「实践不可达路径」，**不构成阻断**；建议随下一轮文档收口一并处理，F3 建议加一条「agent-host 处理集合 == registry `Implemented ∩ AgentToClient`」的断言。
- **6.8**：**PASS**。合并为 fast-forward（候选是 `582133e`/`ceac8af` 的祖先、全程无合并提交），候选之后**没有引入任何代码差异**（仅 `docs/` + `openspec/changes/...` 的文档/证据提交），因此本单元已有 review（RV1–RV5，见 6.8-3）对 `e4a4492` 的判定可据以复用，不需要为合并差异另开检视轮。
- **完成条件**：本报告即 `tasks.md` 6.4「报告写入 `reports/rv1-du1.md`，无未解决阻断项」与 6.8「结论记录在 `reports/rv1-du1.md` 的复核段」的输入。**我无写文件工具且被要求只读，故未写入 `openspec/changes/acp-boundary-and-agent-host/reports/rv1-du1.md`；请主 Agent 以本报告全文落盘（建议文件名 `reports/rv1-du1.md`，含「6.4 段」与「6.8 段」），并保留本报告原文供最终验收核对。**
- reviewer 不更新任务状态、不宣称变更可归档；`7.3`/`8.1` 与最终验收仍归主 Agent。

## evidence_paths

- 候选代码：`crates/agent-host/src/{lib,launch,process,host,session,mapper,platform,error,limits,config}.rs`、`crates/agent-host/src/bin/acpr-fake-acp-agent.rs`、`crates/agent-host/tests/{catalog,session,supervision,support/mod}.rs`
- wire 层：`crates/acp-protocol/src/{lib,raw,envelope,error,limits,methods,update,content,message,capability}.rs`、`crates/acp-protocol/tests/{fixtures,raw_fidelity,matrix_tables,envelope,updates,capabilities}.rs`
- core 契约对照：`crates/core/src/ports.rs:95-125`、`crates/core/src/model/error.rs:19-142`、`crates/core/src/model/event.rs:76-135`、`crates/core/src/broker.rs:1637-1644`、`docs/CORE_PORTS_AND_STORAGE.md:60-131,560-620,1248`、`docs/SYNC_PROTOCOL.md:49,925`
- 口径三处：`Cargo.toml:3-56`、`docs/MODULE_ARCHITECTURE.md:218-286,421-437`、`openspec/changes/acp-boundary-and-agent-host/{proposal.md,design.md,plan.md,tasks.md,verification.md,specs/*/spec.md}`
- 既有评审：`openspec/changes/acp-boundary-and-agent-host/reports/{rv1-wp1.md,rv1-wp2.md,rv1-wp3.md,rv1-wp4.md,rv1-wp5.md,du1-integration.md,alt-final-verification.md,du1-pv1.log,du1-main-verify.log}`
- 门禁：`scripts/check-crate-boundaries.mjs`、`scripts/check-doc-links.mjs`

## 未验证 / 待补（不影响本轮判断）

1. 我未执行 `cargo fmt/clippy/build/test`、`npm run check`、`npm run verify` 与 ACP 契约/夹具脚本；这些由主 Agent 的 PV1/PV4/PV5 与 7.1 替代验证承载，我只做了静态核对与两道只读门禁。审查者不应把 code review PASS 等同于 Project Verify PASS。
2. `tests/**` 的**可证伪性**（改坏实现会让用例变红）本轮未做变异自检；我依赖 RV5 已记录的 4 条变异自检（`rv1-wp5.md` RV5 段）——这是**二手证据**，未由我复现。
3. `verification.md` 登记的「仍未处理」项（`RV3-Q4-4` 跨 `await` 持锁、`RV3-Q4-6` 残余、`RV3-Q3-2`、`RV-WP2-F6`、§5 缺列、`RV-WP3-F4` pid 复用窗口）本轮未重新判定，仅确认仍被登记且被绑定到「接线 `app` 之前」。
4. 候选之后有 3+1 个文档提交（`1035395`/`f7bcf5f`/`582133e`/`ceac8af`），其中 `1035395` 是 RV5 所列 6 条 MINOR 的修复批次，`verification.md:160` 自陈**未经新一轮独立检视**；虽为零代码差异，但其文档口径正确性只能由门禁与主 Agent 自证。若最终验收要求「被验收版本的全部改动都有独立检视」，需为这批文档补一轮（或明确接受）。