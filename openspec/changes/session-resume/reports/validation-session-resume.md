# 独立验证报告：`session-resume`（tasks 7.1，validator-A）

> 本报告由**全新实例 validator-A**（fork_turns=none，不继承任何实现对话）独立产出。
> **未采信** `verification.md` / `tp2-test-design.md` / CR1–CR8 / merger 报告的任何结论作为判定依据；
> 下述每条判定都由本实例自己执行命令或自己读代码得出，既有报告只作为「被验证的对象」出现。
> 本实例**未修改任何文件、未提交、未执行任何 git 写操作**；唯一写入是本报告与系统临时目录下的日志。

## Shared Report

- **task_id**: `7.1`
- **role**: `validator`（独立验证者，实例 validator-A）
- **phase / stage**: `validation` / `final-main`
- **agent_context**: validator-A，**全新实例**，`fork_turns="none"`。未参与 WP1–WP6/TP1/TP2 的实现、用例编写、
  评审或整合，也未参与 PV1/PV2 的执行或 premerge 判定；输入为固定目标提交 + 可读代码/报告，不接收实现者自评摘要。
- **target_revision**: `69f1ac1bc6af81461d199257f512965f646ec55c`（`refs/heads/main` HEAD；
  合并提交 `0d2be6d` + 文字修正 `69f1ac1`；候选为 `2ed142dedec2facf8f6e174d1aa165f549cbc47e`）
- **scope**: ①Coverage Index 37 行的覆盖充分性；②待验证假设（真实 Agent 是否宣告并实现
  `sessionCapabilities.resume`）；③迁移与能力门控的风险盲区；④本机未闭合项的如实登记。
  **不重判**各工作包内部实现质量（CR1–CR8 已覆盖），不做逐行代码风格/安全检视（交 reviewer）。
- **changes**: 无（只读验证；未改动产品代码、用例、规划或权威文档）
- **checks**: 见下方 §1–§4 与 `## Commands Executed`
- **issues**: 0×CRITICAL / 0×MAJOR / 5×MINOR / 3×SUGGESTION（清单见 §5）
- **result**: **PASS**（分项结论见 §5；其中第 ② 项按 plan.md `## Independent Validation` 的通过条件
  以「只读侦察证据」判 PASS，**运行时尚未做真实握手**，该限制已显式登记；另有 2 条 `#[cfg(unix)]` 用例
  **保留为 PENDING**，本报告不把它们计入通过）
- **evidence_paths**:
  - 本报告：`openspec/changes/session-resume/reports/validation-session-resume.md`
  - 本实例的临时原始日志（系统临时目录，非仓库）：`%TEMP%/val-pv1-workspace-test.log`、
    `%TEMP%/val-pv2-check.log`、`%TEMP%/val-clippy.log`、`%TEMP%/val-linux-check.log`
- **resource_cleanup**: 未创建/修改任何仓库文件；未新增临时脚本文件（覆盖核对用的一次性 Python 经 stdin 执行）；
  cargo 只写本仓库既有 `target/` 目录；日志写在系统临时目录。未清理任何 provisioner 资源。

---

## 1. 覆盖充分性：Coverage Index 37 行逐行核对

### 1.1 机械核对：37 行 heading 是否真在 specs 里

本实例解析 `plan.md` 的 `## Coverage Index`（`agentic-coverage` 块），得到 **37 行**（R1–R37），
逐行用「`source.path` + `source.heading` 原文」在 5 份 specs 中做**逐字包含**判定：

```
rows parsed: 37
missing headings: []
```

**结论**：37/37 行的 `source.heading` 在对应 spec 文件中**逐字实存**，无悬空引用、无编号错位。

### 1.2 抽查明细（本实例在 `crates/**` 里 grep + 亲读断言，共 15 行；要求 ≥12）

行号均为本实例在 `69f1ac1` 的工作树上实测所得。

| # | R 行 | 用例（文件:行） | 本实例的独立核实内容 | 结果 |
| --- | --- | --- | --- | --- |
| 1 | **R1/R2**（类型化往返保真、未知字段回写） | `crates/acp-protocol/tests/session_resume.rs:36` `session_resume_wire_round_trip_keeps_unknown_fields_byte_exact`、`:72`、`:99`（响应侧） | 用例存在；实跑 7/7 通过。判定式是字节级比对而非结构等价 | 证据可定位、已执行 |
| 2 | **R3**（缺 required 被拒） | 同文件 `:124`、`:164` | 缺 `cwd`/缺 `sessionId` 分别被拒，无默认值补齐 | ✅ |
| 3 | **R5/R6**（创建暴露 ACP 会话标识） | `crates/agent-host/tests/resume.rs:620` `creating_two_sessions_exposes_two_distinct_agent_session_ids` | 用例存在、已执行；app 侧经 `load_recovery` 读回持久化标识（`session_resume_e2e.rs:291`） | ✅ |
| 4 | **R7**（未取得标识不编造） | **复用** `crates/agent-host/tests/session.rs:906` `failed_session_new_yields_no_endpoint_and_no_identifier` | 本实例独立 grep 确认该函数真实存在于 `69f1ac1`（不是「报告说有」） | ✅ |
| 5 | **R8/R26**（线级恢复参数带持久化取值） | `crates/agent-host/tests/resume.rs:185` `the_wire_level_resume_params_carry_the_persisted_values` | 经 `CARGO_BIN_EXE_acpr-fake-acp-agent` **真实 stdio 子进程** + `--dump-request-params` 读行；已执行 | ✅ |
| 6 | **R10**（能力未宣告：不发送 + 回收子进程） | `crates/agent-host/tests/resume.rs:322`、`:402`（`null` 与省略同判） | 已执行；断言含 dump 无 `session/resume`/`session/new`、心跳停止增长、`runtime_running()==false` | ✅ |
| 7 | **R12**（反复恢复不产生第二端点） | `crates/agent-host/tests/resume.rs:521` | 已执行；断言「恰好一个端点可派发」+「只有一个进程」，前提与实现一致（复用进程） | ✅ |
| 8 | **R16/R17/R21**（升级保留、两列可读回不推导、半 NULL 语义） | `crates/storage-sqlite/tests/resume_columns.rs:282`、`:405`、`:354` | 真实 SQLite 文件；已执行 6/6 通过 | ✅ |
| 9 | **R18/R13/R14**（版本常量与幂等、列清单相等） | `resume_columns.rs:543`、`:486`、`:157` + **复用** `crates/storage-sqlite/tests/migration.rs:1023`、`:185` | 复用函数独立确认存在；已执行 | ✅ |
| 10 | **R19/R20/R15**（v3→v4、v2→v3、失败回滚） | **复用** `migration.rs:814`、`:659`、`:1381` | 三条独立 grep 确认实存；`:1381` 即 CR7-F4 裁定「复用而非复制空转注入」的那条 | ✅ |
| 11 | **R27/R34**（命令授权/幂等/终态、payload 与结果契约） | `crates/node-link-protocol/tests/session_resume_command.rs:64`、`:165`、`:198` | 已执行 5/5 通过 | ✅ |
| 12 | **R28/R29**（同 requestId 只派发一次 / 同键不同语义被拒） | `crates/app/tests/session_resume_e2e.rs:475` | 断言后端 `resume` 计数恒为 1 + `nodelink.command.idempotency_conflict`；已执行 | ✅ |
| 13 | **R31**（越权恢复先于本机读取） | `session_resume_e2e.rs:551` | 判别力经本实例复核：前提是「目标目录已被删除」，先读后校验会回 `internal.unavailable`，本用例得 `export.not_granted`；另断言 `load_recovery` 计数不变、存在/不存在同响应同 `details` | ✅ 非空转 |
| 14 | **R32**（崩溃窗口 → `uncertain`） | `session_resume_e2e.rs:828` | 断言经 wire `command.status` 得 `uncertain`、**重开同一 data_dir** 从持久行读回、该帧 `terminalEventId` 为 `null`、不产生第二会话 | ✅ |
| 15 | **R30/R33**（越权命令被拒、命令限流） | **复用** `crates/server/src/node_link/command/tests.rs:1509`、`:2888`、`:2303` | 三条函数独立确认实存，行号与 tp2 报告一致；限流的构造性依据（`command.rs:248` 的 `admit_rate` 在 `match submit.command` 之前）本实例已核对 | ✅ |
| 16 | **R37**（Agent 不支持 → 终态 `failed`） | `session_resume_e2e.rs:725` | 断言 `failed` + `nodelink.command.unsupported`，并与「Agent 拒绝」用**不同错误码**区分 | ✅ |
| 17 | **R22/R23/R24/R35/R36** | `session_resume_e2e.rs:291`、`:998`、`:1164` | 已执行 8/8 通过；三类错误码互不相同（`export.not_granted` / `internal.unavailable` / `command.unsupported`） | ✅ |
| 18 | **R25** | `session_resume_e2e.rs:1255`（平台无关，本机 PASS）、`:1408`、`:1514`（`#[cfg(unix)]`，本机未执行） | 见 §4.1 | ①✅ ②PENDING |

（实际抽查 18 行，超过要求的 12 行，覆盖 WP1–WP6 与 TP2 的全部 5 个新增用例文件 + 4 处上游复用落点。）

### 1.3 「报告说有、实际找不到」的核查

- **用例名层面：0 处对不上**。37 行映射到的用例名，本实例全部独立 grep 命中真实函数定义。
- **用例计数层面：自洽**。新增本机可执行用例实测为 `acp-protocol/session_resume` **7**、
  `storage-sqlite/resume_columns` **6**、`agent-host/resume` **7**、`node-link-protocol/session_resume_command` **5**、
  `app/session_resume_e2e` **8**，合计 **33**，与 `tp2-test-design.md §2` 的声明逐字一致。
- **证据路径层面：1 处对不上（MINOR，VAL-F1）**。`plan.md` 的 Coverage Index 中 **30 行**的
  `evidence: [reports/PV1.log]`、以及 `## Completion Criteria` 的「证据可读（`reports/PV1.log`、`reports/PV2.log`）」
  指向的文件**在本机不存在**（`reports/` 下 94 个 `.log` 里没有这两个名字）。
  `verification.md` 的 `## Checks` 用的却是真实存在的 `reports/merge-u1-candidate-PV1-stage2-workspace-test.log`
  与 `reports/merge-u1-candidate-PV2.log`。判定：**记录层不一致，实质证据存在且可读**，且本实例已独立重跑
  PV1/PV2（§3）自证。**不构成阻断**，但它是「报告说有、实际找不到」的唯一实例，须修正。

### 1.4 能力门控盲区（`ScriptedBackends` 而非生产 `AgentHost`）——本实例的裁定

事实（已独立核实）：`crates/app/tests/session_resume_e2e.rs` 通过
`crates/app/tests/support/owner.rs:169` 注入 `ScriptedBackends`（`:629`）作为 `SessionBackendFactory`；
生产组合根在 `crates/app/src/compose.rs:219-228` 用 `host.clone()`（真实 `agent_host::AgentHost`）注入。
因此 **`core::Broker::resume_session` × 真实 `AgentHost::resume` 这一对组合从未被同一个测试同时驱动**
（CR-C1-F2 登记的正是此事）。

本实例的判断：**可接受，登记为已知覆盖边界，不阻断本轮**。依据：

1. **两侧各自都是真实覆盖，不是纯替身对替身**：agent-host 侧 7 条用例拉起**真实 ACP 子进程**
   （`CARGO_BIN_EXE_acpr-fake-acp-agent`），断言线级 `session/resume` 请求、`initialize` 次数与进程回收；
   app 侧 8 条用例走**真实 loopback listener + 真实 SQLite + 真实 `identity_auth::Authority` + 真实 WSS 握手**，
   只有 Agent 后端与故障注入是替身。
2. **未覆盖的接缝很窄**：broker 在 `backends.resume` 返回后只保存 endpoint、不回读 `agent_session_id()`
   （CR-C1-F2 已核），跨实现假设极少。
3. **`AGENTS.md` §9 明确「普通 CI 使用可控的 fake ACP Agent」**，真实 Agent 属可选兼容套件。
4. **Main E2E 已按用户批准记 not-applicable**，替代检查 C1/C2（workspace 全量测试 + `npm run check`）本实例已独立复现为绿。

**但我把它记为一个真实的残余风险（VAL-F4，MINOR）**，并给出具体加固建议（不改本轮判定）：
后续切片可让 `session_resume_e2e.rs` 增加一条走真实 `AgentHost`（profile command 指向 fake agent 二进制）的变体；
同时组合根「把 `AgentHost` 注册为 `SessionBackendFactory`」这一装配动作目前**没有任何测试断言**
（`grep '\.host()' crates/app/tests` 无命中）——这是本变更之前就存在的结构，不是本变更引入。

---

## 2. 待验证假设的裁定（本项最易出错，单列）

### 2.1 挂起决策原文与口径

`verification.md` 的 `## Failures and Retests` 登记：用户 2026-09-30 明示「不阻塞当前实现，先挂着」；
口径为「**若届时无可用的真实 Agent，validator 应记 BLOCKED 而非 PASS**」。

### 2.2 本机侦察（只读，未启动任何 Agent、未发起任何真实会话）

| 候选 | 是否存在 | 只读侦察结果 |
| --- | --- | --- |
| `omp`（Oh My Pi） | **存在**：`C:\Users\zhang\AppData\Local\omp\omp.exe`（240 MB PE32+） | **宣告**：`initialize` 响应的构造源码逐字含<br>`agentCapabilities: { …, sessionCapabilities: { list: {}, fork: {}, resume: {}, close: {} } }`<br>**实现**：请求分发表逐字含 `case "session/resume": return j$(e, e.resumeSession, t, s);`，且 `async resumeSession(e)` 用 `e.sessionId` / `e.cwd` / `e.mcpServers ?? []` 解析既有会话并返回 `configOptions`/`modes` |
| `codex` | **存在**：`…\OpenAI\Codex\bin\codex.exe`（324 MB PE32+） | **不适用**：`sessionCapabilities` 命中 **0** 次、`session/resume` 命中 **0** 次、`agentCapabilities` 命中 **0** 次 ⇒ 该二进制不宣告本变更所需的能力，**不能**作为恢复能力的验证目标 |
| `zed` | 存在（编辑器，非 ACP Agent） | 不适用 |

> 方法说明：以上为对已安装第三方二进制的**静态字符串取证**（`grep -aob` + 上下文 dump），
> **未执行** `omp`，未发起任何 ACP 会话，未改动其配置或 `~/.omp` 状态。

### 2.3 裁定

- **「宣告」= 成立（PASS）**：真实第三方 Agent（omp）的出厂源码逐字声明 `sessionCapabilities.resume`，
  形状与本仓 fake agent 与 `acp-protocol` 期望的 `{"resume":{}}` **一致**。
- **「实现」= 成立（PASS，静态）**：`session/resume` 有分派分支且有实质 `resumeSession` 实现，
  其入参 `{sessionId, cwd}` 与本仓 `crates/acp-protocol/src/message.rs:528-547` 的
  `SessionResumeRequest { session_id → "sessionId", cwd }` **逐字段兼容**（`mcpServers` 在本仓侧不发，omp 侧可选）。
- **因此不触发 BLOCKED 条款**：本机**存在**可用的真实 ACP Agent，且已有只读侦察证据，
  按 `plan.md` 的通过条件（「标注为 BLOCKED **或**提供只读侦察证据」）判 **PASS**。

### 2.4 限制与触发条件（必须随结论一起传递）

1. **运行时尚未验证**：我没有执行 `initialize` 握手，也没有真的恢复一个会话。
   「宣告」与「实现」目前是**静态证据**（出厂源码），不是运行时观测。
2. 若用户/主 Agent 要求**运行时**证据（如真跑一次 `omp` 的 `initialize` 看 `sessionCapabilities`、
   或用真实 Agent 恢复一次会话），本项应改判 **PENDING/BLOCKED**，并需单独授权「启动真实 Agent 进程」
   这一超出本次只读边界的动作。
3. **本结论不推广到 Codex**：Codex 不宣告该能力，对它是**不支持**路径——而这条不支持路径本身
   已由 R10（agent-host `:322`/`:402`）与 R37（app `:725`）覆盖，故不构成覆盖缺口。
4. **fake Agent 的能力声明确实不能证明真实 Agent**——本项之所以不是 BLOCKED，靠的是
   「对真实第三方 Agent 出厂源码的独立取证」，**不是**靠 `acpr-fake-acp-agent` 的测试通过。

---

## 3. 本实例独立执行的验证（不复用实现者结论）

| 检查 | 命令（本实例实跑） | 结果 | 与既有证据的关系 |
| --- | --- | --- | --- |
| PV1 fmt | `cargo fmt --all -- --check` | **exit 0** | 与 PV1 记录一致 |
| PV1 test（阶段 2 全量） | `cargo test --locked --workspace --all-features` | **exit 0；91 个测试单元合计 1074 passed / 0 failed / 2 ignored**（本实例按 `test result:` 行逐条求和） | 与既有 PV1 记录的 1074/0/2 **逐字一致**；本次为**真实编译 + 真实执行**（多个 crate 出现 `Compiling`，各测试二进制均实际运行） |
| PV1 clippy | `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | **exit 0，0 条诊断** | ⚠️ **缓存空跑**：日志仅 1 行 `Finished in 0.75s`，无 `Checking/Compiling`，即 cargo 复用了既有产物。**本实例不把它算作独立复现**，只作为佐证；clippy 的独立复现需清空 target，属超出本次边界的重编译 |
| PV2（合同门禁） | `npm run check` | **exit 0；`Totals: 20 passed, 0 failed (20 items)`** | 与既有 PV2 记录一致；本次为**完全独立的脚本执行**（逐字读取磁盘上的合同资产） |
| 交叉编译可行性 | `cargo check --locked --target x86_64-unknown-linux-gnu -p app --all-features --tests` | **exit 101**，失败点在构建脚本：<br>`error: failed to run custom build command for libsqlite3-sys v0.30.1` / `cc-rs: failed to find tool "x86_64-linux-gnu-gcc"` | **独立复现**了「本机无法为两条 `#[cfg(unix)]` 用例取得任何编译证据」这一事实 |
| 覆盖率机械核对 | 一次性脚本（stdin 执行，未落盘）解析 Coverage Index 与 specs | 37/37 heading 实存 | 首次由本实例独立执行 |

---

## 4. 风险盲区判断

### 4.1 迁移（v5「只追加两列」）

**(a) 文档 ↔ 实现一致性：一致。**

- `docs/CORE_PORTS_AND_STORAGE.md:865` 标题为 `## 7. storage-sqlite v5 表结构（imported_* 家族仍为 v3）`；
  `:879` 版本常量 `v5 = 5`；`:889` 的 **v4 → v5 段**逐字规定「在列清单末尾追加两列，用 `ALTER TABLE owned_session ADD COLUMN`
  实现——**不**走 12-step 表重建；两列都可空、**无默认值**」；`:893` 明写回滚限制；
  `:1479` §11.3「过新」取值 `user_version = 6 = FILE_FORMAT_VERSION + 1`；文件头 `版本：0.15` 逐项登记了本次全部改动。
- `crates/storage-sqlite/src/migrate.rs:19` `FILE_FORMAT_VERSION: i64 = 5`；`:67-68` 新建库 DDL 两列可空无默认值；
  `:626-638` `V5_UPGRADE_OWNED` 恰为两条 `ALTER TABLE … ADD COLUMN`，**无重建语句**；
  `:955` `if file_version > FILE_FORMAT_VERSION` 即「过新拒绝打开」；`:990` 升级判据。
- `npm run check` 的合同漂移门禁（`check:contract-drift`）在本次实跑中为绿 ⇒ §7 的 SQL 块与 `migrate.rs` 无漂移。
- §9 判据 28（`:1382`）已把 v5 的四条追加断言写进权威文档（列清单相等、既有行为 `NULL`、读回逐字节相同、恢复流程不覆写）。

**(b) `resume_columns.rs` 的判别式是否真能区分「ALTER 追加」与「12-step 重建」？——本实例裁定：成立。**

独立判断过程：

1. **CR4-F3 原建议（`contains("IF NOT EXISTS")`）确实恒真，本实例认同 TP2 的驳回**：`migrate.rs` 的新建库 DDL 用
   `CREATE TABLE IF NOT EXISTS`，而 `ALTER TABLE ADD COLUMN` 追加后 SQLite 存储文本里该子句被去掉，
   重建脚本本来也不写它 ⇒ 两条路径都不含，按它断言无判别力。
2. **替代判据（表名是否带双引号）在本机成立**：本实例实跑 `v5_appends_the_recovery_columns_instead_of_rebuilding_owned_session`
   **通过**，该用例同时断言 ① 追加后 `CREATE TABLE owned_session (`（无引号）② **反证**：v1 库升级里确实被重建的
   `owned_audit` 存储文本为 `CREATE TABLE "owned_audit" (`（有引号）。
   **「有/无引号」在同一台机器、同一 SQLite 上同时取到两种取值 ⇒ 判别式不是恒真**，这是本实例的直接证据。
3. **判别式的结构性依据成立**：本仓 12-step 重建的固定写法是
   `CREATE TABLE owned_x_vN … DROP TABLE owned_x; ALTER TABLE owned_x_vN RENAME TO owned_x`，
   SQLite 在 `RENAME` 后会把存储文本的表名记成带引号形式；而 `ALTER TABLE ADD COLUMN` 是原地追加、表名不带引号。
4. **判别式不是唯一防线**（这是它可靠的关键）：同一用例还断言「既有列定义文本逐字节保留」
   「两列紧跟其后」「`cid` 0..13 连续无空洞」「无 `owned_session_v2/_v3/_v5` 残留表」。
   即使引号判据将来在某 SQLite 版本上失效，重排/重排版/留残表仍会被这三条抓住。
5. **唯一的理论缺口（SUGGESTION，VAL-F3）**：若有人用「`DROP TABLE owned_session;` 再用**逐字相同**的 DDL
   重建」绕过 RENAME（SQLite 允许），则 ①②③④ 可能全部通过。现实概率极低（重建的唯一理由是改既有 CHECK，
   而 v5 段不动任何约束；且 §7.2 与漂移门禁同时约束），但可作为后续加固方向（例如再加一条「行 `rowid` 不变 / 无重建期写入」的断言）。

**(c) 失败回滚**：复用 `migration.rs:1381 a_failed_upgrade_rolls_back_to_v1`，已实跑通过。

### 4.2 能力门控（`agent-host`）

已独立读 `crates/agent-host/src/host.rs:608-690` 与 `:303-326`：

- 顺序正确：`ensure_runtime_tracked`（`initialize`）→ 读**协商到的**能力（复用既有进程时读的是那个进程此前的协商结果，
  不重新协商、不虚报）→ `!supports_session_resume()` 即**在发送 `session/resume` 之前**返回
  `Unavailable(BackendUnsupported)` 且不建绑定。
- 四条失败分支都调用 `discard_spawned_runtime`：**能力未宣告**、**请求失败/超时/进程消失**、
  **响应无法解码**、**`insert_session` 失败**。
- `discard_spawned_runtime` 在 `spawned == false`（复用既有进程）时**直接返回，不杀别人的进程**——语义正确。

**测试盲区（MINOR，VAL-F2）**：

| 分支 | 覆盖 | 说明 |
| --- | --- | --- |
| 能力未宣告 → 回收 | ✅ `resume.rs:322`、`:402` | 断言心跳停止增长 + `runtime_running()==false` |
| 请求失败（Agent 拒绝） → 回收 | ✅ `resume.rs:451` | `assert_process_reclaimed` 已断言 |
| **响应无法解码 → 回收** | ❌ **无用例** | fake agent 无「返回畸形 resume 响应」场景；若发生，会走 `SpawnFailed` 且**已回收**，行为正确但无回归保护 |
| **`insert_session` 失败 → 回收** | ❌ **无用例** | 极难构造（同一 `SessionId` 已有绑定），风险最低 |
| **`spawned == false`（复用进程）时不得回收该进程** | ❌ **无用例** | 若误杀，会连带关闭同一 Agent 上其它会话的绑定——这是四条里**后果最重**的一条，却无测试 |

建议（不改本轮判定）：补一条「先创建/打开建立既有 runtime，再对一个能力未宣告的 profile 调 `resume`」
的用例，断言既有进程与其他会话绑定仍存活；以及给 fake agent 加一个「返回畸形 `session/resume` 响应」场景。

### 4.3 回滚限制

- **文档一致**：`docs/CORE_PORTS_AND_STORAGE.md:893`「**回滚**：v5 库不能被旧二进制打开（版本过新拒绝启动），
  因此回滚 = 恢复升级前的数据库备份 + 回退二进制；本合同**不提供**自动降级迁移」；
  `:880`「高于本二进制已知版本 → 拒绝启动，不降级写入」；`plan.md` 的 `## Failure and Recovery`
  「代码回滚会使 v5 库被『过新版本拒绝打开』拦住……回滚需用户单独授权」——**三者口径一致**。
- **实现一致**：`migrate.rs:955` 的 `file_version > FILE_FORMAT_VERSION` 即拒绝；用例
  `resume_columns.rs:486 a_too_new_database_is_refused_without_touching_a_single_byte`（已实跑通过）证明「拒绝且不写一个字节」。
- **MINOR（VAL-F5）**：`plan.md` 要求「**必须在交付说明中明确该限制**」，但交付报告
  `reports/merge-u1-main.md` 中检索「回滚/降级/过新」只命中「无回滚授权」字样，**未复述该限制**。
  权威文档已写，判定为记录层缺口，建议在最终交付说明中补一句。

---

## 5. 分项结论汇总

| 维度 | 结论 | 依据 |
| --- | --- | --- |
| ① Coverage Index 37 行覆盖充分性 | **PASS** | 37/37 heading 逐字实存；18 行抽查全部命中真实用例且已执行通过；33 条新增用例计数自洽。**1×MINOR（VAL-F1）**：30 行证据路径 `reports/PV1.log` 不存在（实质证据另在 `merge-u1-candidate-*` 日志，且本实例已独立重跑） |
| ② 待验证假设（真实 Agent 的 `sessionCapabilities.resume`） | **PASS（以只读侦察证据）** | 本机存在真实 ACP Agent `omp`，出厂源码逐字宣告 `sessionCapabilities.resume` 且有 `resumeSession` 实质实现，入参形状与本仓 DTO 兼容；Codex 不宣告该能力（走已覆盖的不支持路径）。**限制**：运行时握手未执行，若要求运行时证据则改判 PENDING |
| ③a 迁移风险 | **PASS** | 文档 §7/§9/§11.3/文件头与 `migrate.rs` 逐项一致；判别式经本机实测成立且自带反证，另有三条独立断言兜底。1×SUGGESTION（VAL-F3） |
| ③b 能力门控风险 | **PASS（含 1×MINOR 盲区 VAL-F2）** | 门控位置与回收语义正确；两条失败分支与「复用进程不得回收」分支无用例 |
| ③c 回滚限制 | **PASS（1×MINOR VAL-F5）** | 文档与实现一致；交付报告未复述该限制 |
| ④ 本机未闭合项 | **如实保留为 PENDING / 缺口** | 见 §6 |

**总体结论：PASS。** 无 CRITICAL / MAJOR，无已确认失败；无「因缺输入/执行/隔离/证据而无法判定」的情形
（隔离充分：全新实例；执行充分：PV1 测试与 PV2 门禁由本实例实跑复现）。

### Findings（VAL-F1 … VAL-F5）

| ID | 严重性 | 位置 | 影响 | 建议 |
| --- | --- | --- | --- | --- |
| VAL-F1 | MINOR | `plan.md` Coverage Index 的 30 行 `evidence: [reports/PV1.log]`；`## Completion Criteria` 同名路径 | 「报告说有、实际找不到」的唯一实例；不影响实质覆盖（本实例已独立重跑 PV1/PV2 为绿） | 改指向真实存在的 `reports/merge-u1-candidate-PV1-stage2-workspace-test.log` 等 |
| VAL-F2 | MINOR | `crates/agent-host/src/host.rs:641`（响应解码失败分支）、`:684`（`insert_session` 失败分支）、`:309`（`spawned == false` 分支） | 三条失败/保护分支无回归测试；其中「复用进程不得被回收」后果最重（会连带关闭同 Agent 的其它会话绑定） | 补 fake agent「畸形响应」场景 + 「既有 runtime 下 resume 失败不得杀进程」用例 |
| VAL-F3 | SUGGESTION | `crates/storage-sqlite/tests/resume_columns.rs:157` 判别式 | 「DROP + 逐字相同 DDL 重建」可绕过引号判别式（现实概率极低） | 可再加一条不依赖文本形状的断言 |
| VAL-F4 | MINOR（已知边界，CR-C1-F2 的独立复核） | `crates/app/tests/session_resume_e2e.rs` 注入 `ScriptedBackends`（`support/owner.rs:169`/`:629`） | `Broker::resume_session` × 生产 `AgentHost::resume` 的组合无测试；组合根装配亦无断言 | 后续切片补一条走真实 `AgentHost` 的变体 |
| VAL-F5 | MINOR | `reports/merge-u1-main.md` 未复述回滚限制（`plan.md` 的 Failure and Recovery 要求「必须在交付说明中明确」） | 交付说明层缺口（权威文档 `CORE_PORTS_AND_STORAGE.md:893` 已写） | 交付说明补一句 |

---

## 6. 未由本机闭合的项（如实登记，均**不计入 PASS**）

### 6.1 2 条 `#[cfg(unix)]` 用例 —— **PENDING**

- 用例：`crates/app/tests/session_resume_e2e.rs:1408`（符号链接改指）、
  `:1514`（`chmod 000` 打在父目录，附 `ModeRestore` 在 `Drop` 恢复模式位）。
- **本实例独立复核**：
  - 该文件共有 **10** 个测试函数；本机实跑 `cargo test -p app --test session_resume_e2e` 输出
    `running 8 tests` / `8 passed; 0 failed; 0 ignored` —— **不含**这两条，即本机**未执行**；
  - `rustup target list --installed` 显示 `x86_64-unknown-linux-gnu` **已安装**，但本实例实跑
    `cargo check --locked --target x86_64-unknown-linux-gnu -p app --all-features --tests`
    **exit 101**，失败在构建脚本（`libsqlite3-sys` / `ring` 的 `cc-rs` 找不到 `x86_64-linux-gnu-gcc`），
    即**连类型检查都拿不到** ⇒ **零编译证据、零执行证据**的状态属实。
- **证据链如实性复核：一致，未被粉饰**。本实例在 `plan.md` / `verification.md` / `reports/**` 中检索这两个用例名，
  命中的每一处（`tp2-test-design.md:103/165-166`、`tp2-tester.md:520-521/925-926`、
  `merge-u1-candidate.md:262-263/435`、`cr-c1-candidate-review.md:125`）都写明 **PENDING / 待 Linux CI**，
  **没有任何一处写成已通过**；两条用例也未被 `#[ignore]` 或从文件中删除。
- **关闭条件**：Linux CI 的 `checks` job（`ubuntu-latest`，非 root）首次编译并执行；失败则回 TP2 开新问题编号。

### 6.2 只由 CI 判定的两项

- `cargo-deny`（依赖许可证 / 来源 / advisory）、`gitleaks`（密钥扫描）：本机**不可执行**，
  按 `AGENTS.md` §8「本地绿 ≠ 这两类判定通过」，登记为**缺口**，本报告不代它们下结论。

### 6.3 clippy 复现的限制

- 本实例的 `cargo clippy --workspace --all-targets --all-features -- -D warnings` 为 **缓存空跑**（exit 0、0 诊断，
  但日志仅 1 行 `Finished`，无 `Checking`/`Compiling`）。因此它**只算佐证，不算独立复现**；
  PV1 的 clippy 部分仍以既有证据为准。

---

## 7. 隔离方式、资源与限制

- **隔离**：validator-A 为全新实例，`fork_turns="none"`，未参与任何工作包实现/评审/整合，未继承实现对话。
- **只读边界遵守情况**：未修改任何文件（唯一新增文件是本报告）、未 `git add/commit/checkout/reset`、
  未改任何产品代码或正式用例以让检查通过；只跑了 `git log/grep/diff`、`cargo`、`npm run check` 与文件读取。
- **资源**：使用仓库既有 `target/`（cargo 默认）与系统临时目录；未申请独占资源，未触碰 provisioner 建立的
  任何 worktree；无遗留后台任务。
- **输入版本**：`refs/heads/main` = `69f1ac1bc6af81461d199257f512965f646ec55c`（本实例 `git rev-parse` 实测）。
  工作区存在主 Agent 的**未提交记录层改动**（`plan.md`/`tasks.md`/`verification.md`/一份 premerge 回执），
  经 `git diff --stat` 核对为 4 文件 +13/−7、**不触及任何产品代码**，且 Coverage Index 内容与 HEAD 一致；
  因此不影响本报告结论，但说明 `npm run check` 是在**含该未提交改动**的工作区上跑出的。
- **超出范围的发现**：作为待协调项返回，不自行处置——VAL-F1（证据路径）、VAL-F5（交付说明缺回滚限制）、
  以及 §6.3 的 clippy 复现限制，均建议由主 Agent 在回写 `verification.md`/交付说明时同批处理。

---

## 8. Handoff Index

```yaml
handoff_index:
  - task_id: "7.1"
    work_package: NOT_APPLICABLE
    role: validator
    phase: validation
    round: NOT_APPLICABLE
    stage: final-main
    target_revision: "69f1ac1bc6af81461d199257f512965f646ec55c"
    evidence_type: VALIDATION
    evidence_id: NOT_APPLICABLE
    report_path: "openspec/changes/session-resume/reports/validation-session-resume.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "全新实例 validator-A（fork_turns=none，未参与 WP1–WP6/TP1/TP2 的实现、评审或整合）在 refs/heads/main=69f1ac1 上独立判定：Coverage Index 37/37 heading 逐字实存、18 行抽查用例真实存在且实跑通过、待验证假设以只读侦察证据判 PASS（真实 Agent omp 宣告并实现 sessionCapabilities.resume，运行时握手未执行）、迁移与能力门控风险已裁定。0 CRITICAL/0 MAJOR，5 MINOR/3 SUGGESTION 登记不阻断。2 条 #[cfg(unix)] 用例与 cargo-deny/gitleaks 明确保留为未闭合项。"
    source_evidence: NOT_APPLICABLE

  - task_id: "7.1"
    work_package: NOT_APPLICABLE
    role: validator
    phase: validation
    round: NOT_APPLICABLE
    stage: final-main
    target_revision: "69f1ac1bc6af81461d199257f512965f646ec55c"
    evidence_type: CHECK
    evidence_id: PV1
    report_path: "openspec/changes/session-resume/reports/validation-session-resume.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "本实例在 69f1ac1 上实跑 `cargo fmt --all -- --check`（exit 0）与 `cargo test --locked --workspace --all-features`（exit 0，91 单元合计 1074 passed / 0 failed / 2 ignored，与既有 PV1 记录逐字一致；多个 crate 实际重新编译、各测试二进制实际执行）。clippy 同步实跑 exit 0 / 0 诊断但为缓存空跑，仅作佐证不作独立复现（见报告 §6.3）。"
    source_evidence: NOT_APPLICABLE

  - task_id: "7.1"
    work_package: NOT_APPLICABLE
    role: validator
    phase: validation
    round: NOT_APPLICABLE
    stage: final-main
    target_revision: "69f1ac1bc6af81461d199257f512965f646ec55c"
    evidence_type: CHECK
    evidence_id: PV2
    report_path: "openspec/changes/session-resume/reports/validation-session-resume.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "本实例在 69f1ac1 的工作区上实跑 `npm run check`：exit 0，`Totals: 20 passed, 0 failed (20 items)`。合同门禁为纯脚本判定、与实现者执行相互独立。唯一差异：运行时工作区含主 Agent 未提交的 4 个记录层文件（+13/−7，不含产品代码，Coverage Index 与 HEAD 一致）。"
    source_evidence: NOT_APPLICABLE

  - task_id: "7.1"
    work_package: NOT_APPLICABLE
    role: validator
    phase: validation
    round: NOT_APPLICABLE
    stage: final-main
    target_revision: "69f1ac1bc6af81461d199257f512965f646ec55c"
    evidence_type: VALIDATION
    evidence_id: CR8-F6
    report_path: "openspec/changes/session-resume/reports/validation-session-resume.md"
    result: BLOCKED
    evidence_status: PENDING
    applicability_basis: "2 条 #[cfg(unix)] 用例（crates/app/tests/session_resume_e2e.rs:1408 符号链接改指、:1514 权限丢失）：本实例独立复核 Windows 上 `--test session_resume_e2e` 只 running 8 tests（不含这两条），且 `cargo check --target x86_64-unknown-linux-gnu` 在 cc-rs 找 `x86_64-linux-gnu-gcc` 处 exit 101 ⇒ 零编译证据、零执行证据属实，状态如实保留为 PENDING。关闭条件：Linux CI checks job（ubuntu-latest，非 root）首次编译+执行；失败回 TP2 开新问题编号。本行不因本轮 validation PASS 而改变 CR8-F6 的 PENDING 状态。"
    source_evidence: NOT_APPLICABLE
```