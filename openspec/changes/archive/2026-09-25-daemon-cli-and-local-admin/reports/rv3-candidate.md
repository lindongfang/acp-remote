# RV3-WP5 · 最终候选收尾复核报告（候选 c050c83）

> 由 reviewer 子 Agent 输出工件原样落盘（运行 dbda84f1），主 Agent 未改动结论。
> 本报告的 FAIL 由其 F1 决定：该轮候选验证为红（`crates/agent-host` 的既有抖动用例 `oversize_frame_ends_the_agent_and_fails_pending_requests`）。
> 主 Agent 在同一提交 c050c83 上重跑 [PV1] 已转绿（EXIT=0，82 targets / 718 passed），并已在 `verification.md` 的 Failures and Retests 登记该次失败与重试证据；
> RV3-WP5-F2/F3/F4 三条 P2 已就地修正。后续以 RV4 收尾复核报告作为 premerge 门的候选 review 证据。

## Review

**result: FAIL**（4 项复核对象全部「已解决」；但最终候选 `c050c83` 自身最新一轮候选验证是**红的**，且未登记 → 该状态阻断 `premerge` 门所需的「候选 Project Verify PASS」证据）

**Merge verdict: BLOCK**（唯一阻断项 = RV3-WP5-F1；其余 3 条为 P2 报告级）

---

### 1. 逐项复核结论（读 target 版本文件本身，不只看 diff）

| # | 复核对象 | 状态 | target 位置与判定依据 |
|---|---|---|---|
| 1 | **RV2-WP5-F1**（关闭顺序权威指针 `§7.1 第 4 条` → 应 `§7.5`） | **已解决** | `docs/MODULE_ARCHITECTURE.md:384` 现为「…→ 清理 endpoint/释放锁（与 `CORE_PORTS_AND_STORAGE.md` **§7.5** 第 4 条、`SECURITY_DESIGN.md` §12.1 一致）」。权威核对：`docs/CORE_PORTS_AND_STORAGE.md:1220` 是该节编号列表**第 4 条**＝关闭顺序；`:763` 是 §7.1 的**第 4 个 bullet**＝权限位（`0700`/`0600`）⇒ 旧指针确为笔误，新指针正确 |
| 2 | **RV2-WP5-F2**（§5 注记括注「没有任何依赖方」与同表事实相反） | **已解决** | `docs/MODULE_ARCHITECTURE.md:482` 现为「…**也有一行**（该行除自身格外全空白——**它自己不依赖任何 crate**）」。与矩阵逐字自洽：`:480` 的 `windows-local-ipc` 行除自格 `—` 全空白；`:477` 的 `server` 行在 `windows-local-ipc` 列上有 `✓`（列＝可被依赖对象、行＝发起方） |
| 3 | **主 Agent 同源修正**（`crates/app/src/daemon.rs:660` 注释） | **已解决** | target 注释现为「…先调 `OwnedTasks::cancel_all`（`CORE_PORTS_AND_STORAGE.md` §7.5 第 4 条）」。与 §7.5 第 4 条「取消周期任务与信号监听（必须先于停止 Agent）」、§12.1（`docs/SECURITY_DESIGN.md:358`「关闭时按顺序停止接入、取消任务、关闭 Agent、刷新存储并清理进程树」）以及实现 `daemon.rs:1013-1077`（`ingress_stopped` → `drain_connections` → `cancel_all`/`audit_writer.close` → `shutdown_all`+`drop(host)` → `composition.close()`(checkpoint) → `cleanup_endpoint`/`remove_record`/`drop(lock)`）四方一致 |
| 4 | **主 Agent 自我更正**（`verification.md` 730→717 + [PV5] 语义澄清） | **已解决**（我用 grep 独立重算） | 见下 §2 独立复算 |

### 2. 计数与 [PV5] 口径的独立复算（要求 4）

- **717 成立**：`reports/wp4wp5-final-verify.log` 共 **82** 条 `test result: ok.` 行，逐条求和 = **717**，含 2 条 `1 ignored`（`:1012`、`:1070`）⇒ 717 / 0 failed / 2 ignored。`reports/wp5-verify.log`（2.20 行的证据）82 条数值序列与之逐条相同 ⇒ 同样 717。两处更正都是对事实的更正。
- **730 的成因正如所述**：`wp4wp5-final-verify.log:59` 是 `Totals: 13 passed, 0 failed (13 items)`（`openspec validate`），717 + 13 = 730 —— 与「粗匹配 `grep -o "[0-9]* passed"` 把汇总行也算进去」的解释**完全吻合**，且 `grep passed` 在全日志只多出这一行。
- **718 成立**：`reports/candidate-verify.log` 第 1 轮 [PV1] 段（`:118-1336`）82 条 `test result: ok.`，求和 = **718**，2 ignored；`app` lib 目标 `50 passed`（`:417`）对比冻结轮 `49 passed`（`wp4wp5-final-verify.log:360`）⇒ +1 正是 2.27 新增用例，口径自洽。
- **[PV5] 澄清如实、且未把「0 命中」说成通过**：`:2990-3025` 的 `[PV5-app-windows]` 确为 `0 passed … 11 filtered out`/`10 filtered out`；`:2945-2951` 的 `[PV5-server-windows]` 为 `3 passed … 86 filtered out`。其声称的真实覆盖亦可核：[PV3] `local_endpoint_windows.rs` 4 passed、`local_endpoint_unix.rs` 0、lib 89 + 14 + 6 + 4 = **117 / 7 targets**（`:2630-2796`）；[PV4] `cli_commands` 11 + `daemon_lifecycle` 10 + audit_export 1 + lib 50 = **72 / 6 targets**；vendor `cargo test --all-features` **11 passed**（`:2916-2940`）。verification.md 6.2 行的 PV 数字全部与日志相符。
- **未删改既有结论**：`verification.md` 的 delta 是 2 处**同行**数字更正（2.20、3.7）、1 处注记行改写（① 的 `§7.1`→`§7.5`，并把「grep 确认」范围改成更弱的表述）、以及新增行/新增注记；没有任何证据行被删除，`reviewFindings` 中 RV2 的既有结论未被改写。

---

### 3. 新发现

#### RV3-WP5-F1 — **（P0，阻断合并）** 最终候选 `c050c83` 最新一轮候选验证是红的，且未登记
- **位置**：证据 `openspec/changes/daemon-cli-and-local-admin/reports/candidate-verify.log` 尾部「候选轮 2（最终候选）」段（`:3035` header「## 提交：c050c83…」→ `:3398` 结束）；登记面 `verification.md:65/75/133` 与「Failures and Retests」表。
- **事实**：该轮 `npm run verify` 输出 `EXIT(npm run verify)=101`，失败用例 `agent-host`：`crates/agent-host/tests/supervision.rs:422` `oversize_frame_ends_the_agent_and_fails_pending_requests` panic「超限后坏进程不得继续被当成『仍在运行』」，`test result: FAILED. 14 passed; 1 failed`。同轮 `[PV2]`=0、`[PV3]`=0，`[PV4]` 只写了段头、**日志在 `:3398` 处截断**（无 PV4/PV5 结果）。
- **预期**：预merge 门需要的「候选版本 Project Verify 全绿」；`verification.md` 的 `[3.7]`/`[6.2]`/5.2 行只记 `EXIT=0`（`[6.2]` 明确写「候选 = …`90c816a`」），而**最新的、针对 c050c83 的直接证据是 101**，且 `Failures and Retests` 表无该次失败/重试记录（delta 亦未新增）。
- **影响**：以「候选全绿」为前提的 premerge 证据链断裂；若直接合并，落地的是一份「最近一次全量验证为红且未处置」的候选。注意：失败用例在 `agent-host`，不在 `90c816a..c050c83` 的改动面内（delta 只动 docs + 一行注释），且该用例在冻结轮/`wp5-verify.log`/候选第 1 轮均通过（均 ~7.0 s）⇒ **最可能是既有的时序型 flaky（`!is_running()` 紧跟错误收敛后的杀进程时序）**，不是本变更引入的回归——但「红过且未登记」本身不能当绿用。
- **最小修复**（需调度者执行，我无 shell）：在干净树上对 `c050c83` 重跑并留证（命令见 §4 #1）；若转绿 → 在 `verification.md` 的 `Failures and Retests` 登记「候选轮 2 的 101 + 重试绿 + 失败用例为既有 flaky」并注明适用性；若仍红 → 按真实回归处理，不得合并。
- **另一处削弱证据强度的细节**：第 2 轮段头只记了提交号，**没有** `git status --porcelain` 干净性证据（第 1 轮有），故其「作用于 c050c83」的证明弱于第 1 轮。

#### RV3-WP5-F2 — **（P2，report-only）** `verification.md:121` 的「全仓零命中」叙述不成立
- **位置**：`verification.md:121`（本轮 delta 改写的 ① 行）：「…该条措辞在全仓只此一处副本（`grep -rn 停周期任务` 已确认为零命中）。」
- **事实**：target 版本 `grep 停周期任务` 仍有命中：`tasks.md:34`（2.25 任务定义「§7.1：**先停周期任务**」）、`reports/rv1-wp4.md:65/67`、`reports/rv1-wp5.md:77`、`reports/wp425-handoff.md:171-176`、`reports/wp427-handoff.md:38/70/139`。原文的「零命中」是**限定范围**结论（`wp427-handoff.md:70` 写的是 `grep -rn "停周期任务" docs README.md AGENTS.md`）。同一行里「只此一处副本」与「零命中」并列本身也互相拉扯。
- **影响**：读者按字面复核会得到相反结果，属登记准确性（不影响产品/判据）。
- **最小修复**：把括注改为与 `wp427-handoff.md` 同口径（「在 `docs/**`、`README.md`、`AGENTS.md` 内零命中」），或删掉该括注。若采纳，需重跑 `npm run check`。

#### RV3-WP5-F3 — **（P2，report-only）** 6.2 行记录的工件身份（行数 + sha256）已与磁盘文件不符
- **位置**：`verification.md:75`：「`reports/candidate-verify.log`（3033 行，`sha256=5215a54c…`）」。**事实**：该文件现含追加的「候选轮 2」段（`:3035` 起），可读内容已到 `:3398`（远超 3033 行）⇒ 记录的「3033 行/sha256」描述的是追加前的第 1 轮工件；第 2 轮那段**没有任何 hash/行数/登记**（我无法计算 sha256，只能证明行数已不符）。
- **影响**：按 hash 复核候选证据会失败；追加型日志使「同一路径两个工件」不可区分。
- **最小修复**：为第 2 轮单列 hash（或另存 `candidate-verify-round2.log`），并在 6.2 行注明该文件为**追加型、含两轮**。

#### RV3-WP5-F4 — **（P2，report-only）** 6.1/6.2 行把「集成运行 ID」写进了「Target Revision」列
- **位置**：`verification.md:74/75` 的 `Target Revision` 列 = `6d132632`（运行 ID），而 5.1/3.8/3.10 等同表行同列填的是提交（`90c816a`/`f39dda9`），运行 ID 放在适用性文字里。
- **影响**：该列存在的目的（证据对应的版本）在 6.1/6.2 两行失效；版本信息只在文本里（「候选 = … `90c816a`」），未被删除但需逐字阅读。另：PV1–PV5 证据记在 6.2 行、而拥有这些检查的 6.3 仍未勾选（`tasks.md` 6.3 `[ ]`），行标与内容略有错位。
- **最小修复**：把列填 `90c816a`（运行 ID 留在文本），或把 PV 证据行拆到 6.3。

---

### 4. 需调度者执行的命令（我无 shell，一律未执行）

| # | 命令 / 步骤 | 目的 |
|---:|---|---|
| 1 | `git -C D:\Project\acp-remote rev-parse HEAD`（应为 `c050c83`）+ `git status --porcelain`（须空）+ `npm run verify`（或最小集 `cargo test --locked --workspace --all-features`）@ `c050c83` | **清 RV3-WP5-F1**：给出候选本身的全量绿/红结论并写退出码；红则转真实回归处理 |
| 2 | 若 #1 仍红：`cargo test --locked -p agent-host --all-features --test supervision -- --nocapture` 连跑 5 次 | 刻画 `oversize_frame_ends_the_agent_and_fails_pending_requests` 的 flaky 率（是否为时序型） |
| 3 | 若 #1 转绿：把该轮（含失败轮）登入 `verification.md` 的 `Failures and Retests` + 6.2/3.7 行，并修 RV3-WP5-F3 的工件 hash/行数 | 让 premerge 门拿到的候选证据自洽、可复核 |
| 4 | 若采纳 RV3-WP5-F2（纯文本改动）：`npm run check` | `check:docs` 对 `§`/相对链接敏感，必须重跑重登 |
| 5 | CI-only（本地无等价物，不得写成已通过）：`deps`（`cargo-deny`：`rpassword 7.5.4`/`rtoolbox 0.0.6` 许可与来源）、`advisories`、`secrets`（gitleaks）、Linux `checks`（`#[cfg(unix)]` 路径） | 五个必需 job |
| 6 | 归档/合并前：`npx --quiet --no-install openspec-agentic workflow check --change daemon-cli-and-local-admin --stage premerge …` | 本报告只提供 REVIEW 证据，不代替该门 |

---

### 5. 覆盖范围与未覆盖声明（要求 5、6）

- **本轮 delta 只含修正与登记**：`docs/MODULE_ARCHITECTURE.md`（2 行）、`crates/app/src/daemon.rs`（1 行注释）、`tasks.md`（仅 3.10/5.1/5.2/6.1/6.2 勾选，无删除）、`verification.md`（2 处同行计数更正 + 1 处注记改写 + 6 行新增 + 3 段新增注记）、新增报告 `reports/{integrator-phaseA,rv2-wp4,rv2-wp5}.md`。`plan.md` 未动，判据未改（[PV5] 注记自称「不改判据」，核对确实只是补充留证）；新增文本中未见把未落地能力写成已存在（`server::sync`/`node::link`/`acp_facade`/`node-link-client` 仍标注为后续切片）。
- **契约门禁在最终候选上确为绿**（间接证据，来自第 2 轮输出）：`check:docs` 379 链接 / 4801 §引用 / 241 md、`check:boundaries` 12 crate、`check:drift` 36 DDL + 15 trait/87 方法、`check:agentic` PASS，且 `&&` 链走到了 `check:rust` ⇒ `npm run check` 通过（该轮未逐子项打印 `EXIT`）。
- **未覆盖 / 无法确认**：
  1. 我是结构只读子 Agent（无 shell/git/cargo/npm），**未运行任何命令**；上述计数是我用 `grep`/`read` 对日志逐行求和得到的，不是复跑。
  2. `reports/**/*.log` 受 `.gitignore` 忽略，**不在提交 delta 里**（`rv-input-*.log` 的「增补」无法作为 diff 检视）；`candidate-verify.log` 我是作为**证据工件**直接读磁盘的，其「第 2 轮作用于 c050c83」无干净性证明（见 F1）。
  3. `reports/rv2-wp4.md`/`rv2-wp5.md` 系子 Agent 报告经运行时落盘；`rv2-wp4.md` 顶部自述为过 `check:docs` 拆开了个别「文件名+章节号」表格单元（语义不变）。我**无法**独立比对它们与子 Agent 原始输出的逐字一致性，只能确认其中引用的 target 位置与 target 文件相符。
  4. `verification.md:168/172`（Review Findings 表，本轮未改）仍把关闭顺序权威写作「§7.1 第 4 条」；按「历史记录不回改」惯例（同 `reports/rv1-*.md`）可接受，但与本轮宣布「已修正…本文件」略有张力，仅登记不作发现。
  5. 候选第 2 轮的 `[PV4]`/`[PV5]` 结果缺失（日志止于段头），其 app 侧候选轮结果只能引用第 1 轮（90c816a）。
  6. 更早的未覆盖项不变：合并窗口端到端效果（本切片无 owned 会话）、真 TTY 凭据录入、真实设备 claim、平台 keystore 路径、Linux `#[cfg(unix)]` 路径。
- **关于「本轮 review 是否足以充当 premerge 门的候选 review 证据」**：就**文档/登记面**（本任务 scope）而言覆盖足够——两点笔误、注释同源、计数更正、[PV5] 澄清、delta 纯度均已逐条实证。**缺的是候选本身的绿证据**：`c050c83` 上没有一次可复核为「全绿」的 Project Verify（最新一次为 101，且未登记），以及该次运行的干净性与工件 hash 记录。补齐 §4 #1–#3 后，本报告可作为候选 review 证据，但 `result` 需按重跑结果更新。

---

```yaml
handoff_index:
  task_id: "6.4 候选独立检视"
  role: reviewer
  phase: candidate recheck
  evidence_type: REVIEW
  evidence_id: RV3
  report_path: openspec/changes/daemon-cli-and-local-admin/reports/rv3-candidate.md
  result: FAIL
  evidence_status: NEW
  base_revision: 90c816a6f88fb7925c84fbca269b1c946745e9e3
  target_revision: c050c83dbbe21a1b94bb09460e84810c5a157538
  applicability_basis: >
    结构只读 reviewer（fresh 上下文，仅 read/grep/find/ls/watchdog_diff；watchdog_diff 确认 worktree 相对
    c050c83 干净）逐项复核 RV2-WP5-F1/F2、daemon.rs:660 同源修正与 verification.md 的计数/语义更正：
    4 项全部「已解决」——§7.5 第 4 条经 CORE_PORTS_AND_STORAGE.md:1220/:763 确认为关闭顺序的权威条目，
    MODULE_ARCHITECTURE.md:384 与 daemon.rs:660 指向已一致并与 SECURITY_DESIGN.md §12.1、daemon.rs:1013-1077
    的实现序列四方一致；§5 注记括注改为「它自己不依赖任何 crate」后与矩阵行/列事实（server 行在
    windows-local-ipc 列有 ✓、该行全空白）逐字自洽；717/718/2 ignored 由 wp4wp5-final-verify.log、
    wp5-verify.log、candidate-verify.log 的 82 条 `test result: ok.` 行独立求和确认，730 = 717 + openspec
    validate 的 `Totals: 13 passed`，成因说明准确；[PV5] 语义澄清与候选日志的 0/3 命中、PV3 117/7、
    PV4 72/6、vendor 11 逐项相符且未把 0 命中说成通过；delta 只含修正与登记，未改判据、未写未落地能力为
    已存在、未删既有证据行。**但**目标候选 c050c83 的最新候选轮 `npm run verify` 记录为 EXIT=101（agent-host
    supervision.rs:422 失败，日志在 [PV4] 段头截断），而登记面只记 90c816a 的 EXIT=0 且未在 Failures and
    Retests 登记该红轮 ⇒ 阻断项 RV3-WP5-F1（P0）；另 3 条 P2（全仓零命中叙述、工件 hash/行数已过期、
    Target Revision 列填运行 ID）。清阻塞所需：在 c050c83 干净树上重跑并留证（绿则登记 flaky+重试，红则按
    回归处理）。
  source_evidence: NOT_APPLICABLE
```