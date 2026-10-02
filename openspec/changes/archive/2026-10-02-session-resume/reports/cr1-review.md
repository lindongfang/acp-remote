> 由主 Agent 从 reviewer 子 Agent 的返回结果原样持久化（reviewer 工具集只读、无 write）；内容未改写。
> 检视线程：CR1 / WP1 / Round 1；run: 51afad9d-0579-412b-a7e1-35ba628c1ea7

# CR1 — session-resume / WP1 分支检视报告（Round 1）

## Review Context

- **Review ID / Round**：CR1 / 1（首轮，无历史轮次；Previous Findings = NOT_APPLICABLE）
- **Review Type**：branch（分支交付前检视；非 recheck）
- **Review Stage**：work-package
- **Work Package**：WP1（`acp-protocol` 的 `session/resume` 类型化 DTO + 矩阵/文档提升；R1–R4）
- **Repository**：`D:/Project/acp-remote`；目标工作树 `D:/Project/acp-remote-wt/session-resume-wp1`（branch `agentic/session-resume-wp1`）
- **Base Revision**：`81e350ff340014265eb7c9251237c799d4357fee`（读 `.git/refs/heads/main` 实核 = 该值；基线内容取主工作区 `D:/Project/acp-remote`，其工作区相对 HEAD 无被跟踪改动，仅 `?? openspec/changes/session-resume/`）
- **Target Revision**：`248d9b9fa69a1e6375a3b9708fdaa2a127dc7ccb`（读 `.git/refs/heads/agentic/session-resume-wp1` 实核 = 该值；`.git/worktrees/session-resume-wp1/logs/HEAD` 末条为 `97a8f5b… → 248d9b9 commit (amend): feat(acp): 新增 session/resume 类型化 DTO 并提升矩阵交付节奏`，无更晚动作；`HEAD` 文件 `ref: refs/heads/agentic/session-resume-wp1`）
- **读取的规则与需求**：`AGENTS.md`（§3 不变量、§4 依赖、§5 协议规则、§7 Rust 约定、§10 文档/门禁同步）、本变更 `proposal.md`、`design.md`（D1–D6）、`plan.md`（Work Packages / Execution Waves / Shared File Ownership / Dependency Handoffs / Verification Strategy / Coverage Index R1–R4）、`tasks.md`（2.1、3.1、3.2）、`specs/acp-wire-protocol/spec.md`（R1–R4 的唯一增量需求）；角色契约 `roles/reviewer.md`、`roles/_shared/role-report.md`
- **实际检查范围**：WP1 四文件改动（`crates/acp-protocol/src/message.rs`、`crates/acp-protocol/src/methods.rs`、`compatibility/acp/v1/matrix.json`、`docs/ACP_COMPATIBILITY_MATRIX.md`）的全部内容、其承载机制（`raw.rs`、`envelope.rs`、`lib.rs`、`methods.rs`）、矩阵 schema、上游固定快照的 `ResumeSessionRequest/Response`、既有契约测试（`tests/matrix_tables.rs`、`tests/raw_fidelity.rs`）、`scripts/check-acp-compatibility.mjs`、全仓 `status_of`/`ensure_direction` 调用面、`fixtures/acp/v1/` 与 `crates/acp-protocol/` 文件清单比对
- **未采用/限制**：本子 Agent **无 git CLI 与 shell**，无法执行任务模板建议的 `git --no-pager show/diff`，也无法运行任何构建或测试。基线/目标内容以「主工作区（=base）与 WP1 工作树（=target，分支 ref 已核）的文件读取 + 行号对齐 + 全仓 grep 爆炸半径」重建；提交身份由 refs 与 reflog 核实，未核 `git status`（工作树是否有未提交改动无法机械确认，改动内容与 coder 报告逐条一致，风险低）。`watchdog_diff` 只覆盖当前工作区（=base，无被跟踪改动），不含已提交范围。
- **使用的验证证据**：`reports/wp1-coder.md`（PV1 阶段 1 与 PV2 的原始命令/版本/退出码/输出）、`verification.md` 的 `## Handoff Index` 与 `## Target` 契约摘要；**核对到的 Check ID：PV1（阶段 1）、PV2；E2E：本变更 Main E2E = not-applicable（plan.md），无 E2E ID 需核对**
- **本轮报告路径**：`openspec/changes/session-resume/reports/cr1-review.md`（由 main 原样持久化；本角色无 write 工具）

### 已经正确的部分（Correct，带证据）

1. **保真走原文、不经通用 JSON 回写**（AGENTS §3 最高优先级不变量）：`raw.rs:22-70` 的 `RawDocument::encode()` 直接返回持有的原文（注释即写明「再编码就是回写原文」），`envelope.rs:32-95` 的 `Envelope::classify` 只把 `document` 原样收进结构，`Envelope::document()` 暴露它；新代码（`message.rs:528-583`）只从 `params`（`envelope.params()` 的**只读** Value）取值，没有任何把 DTO 序列化后回写 wire 的路径。`lib.rs:8-19` 明确「任何先转 Value 再序列化的写法都会改变字节，所以本 crate 不提供这样的 API」。
2. **解码边界可区分拒绝、无默认补齐、无取值泄漏**：`message.rs:556-583` 的 `required_string` 对缺失返回 `AcpError::MissingField{field}`、对存在但非字符串返回 `AcpError::InvalidField{field, detail:"必须是字符串，收到 <类型名>"}`（`value_kind` 只给类型名，不带取值）；`SessionResumeRequest` 的两个 required 字段没有 `#[serde(default)]`，`from_params` 不做任何补全。测试 `message.rs:749-783` 覆盖 6 组输入并逐条精确断言两类错误（含 `cwd: null` 与 `cwd: [..]`）。
3. **`session/load` 未被连带改动**：`methods.rs:113-119`（base 与 target 同位置同内容，`Delivery::PostMvp/false`）、`matrix.json` 第 41 行未动、`message.rs` 中不存在 `session/load` 的处理分支；新增用例 `message.rs:804-826` 同时断言 `session/load` 仍 `NotImplemented` 且 `ensure_direction` 仍返回 `Unsupported`，并断言 `session/resume` 为 `Implemented`（防两行互相漂移）。
4. **矩阵取值合法且与 schema 相容**：`matrix.json:44/105` 的 `layers.broker = project_and_preserve`（`schemas/acp/compatibility-matrix.schema.json` 的 `$defs.layers.properties.broker.enum = [project_and_preserve, local_service, pass_through, explicit_unsupported, not_applicable]`，**无 `native`**；`additionalProperties:false`，由 `check:acp` 的 ajv 强校验）；`delivery=conditional_mvp` 下 `requirement:"optional"` 仍满足「optional ⇒ capability 为非空字符串」；非 baseline 方法要求 `facade ∈ {advertise_if_end_to_end, not_advertised, not_applicable}`，实取 `not_advertised` 合法。改动是**恰好两行**（base 第 44/105 行 → target 同序号行）。
5. **文档同步到位且措辞合规**：`docs/ACP_COMPATIBILITY_MATRIX.md` 仅新增 1 行修订记录（第 6 行）与 8 行 §5 说明（第 188–195 行），行号算术完全自洽（其后所有内容整体 +9）；§5 已把 `session/resume` 从 `post_mvp` 集合移出（base 第 185 行含它、target 第 186 行不含），并显式声明 `facade=not_advertised`、未宣告能力「不得发送、不得降级为新建会话」；对尚未落地部分用「本次变更的计划交付范围」措辞，符合 AGENTS §10「未实现的能力不得写成已存在」。
6. **不改动文件清单与写范围**：`fixtures/acp/v1/` 文件清单与 base 完全一致（未新增 fixture，走 tasks 2.1 允许的「只用内联字节」分支）；`crates/acp-protocol/` 文件清单与 base 一致（无新增文件）；全仓 grep `SessionResume|session/resume|session\.resume` 在该工作树只命中 `crates/acp-protocol/{src/message.rs,src/methods.rs,src/capability.rs,tests/capabilities.rs,tests/fixtures.rs}`、`compatibility/acp/v1/matrix.json`、`docs/ACP_COMPATIBILITY_MATRIX.md`、`scripts/check-acp-compatibility.mjs:63`（base 同行同内容，未改）与既有上游快照/设计文档；`compatibility/commands/v1/commands.json`、`crates/core`、`crates/identity-auth`、`crates/server`、`crates/agent-host`、`crates/app`、`schemas/**`、`scripts/**`、`fixtures/**` 无本次改动痕迹。

## 重点核查逐条结论

1. **未知字段与 `_meta` 逐字节保真**：**通过**。机制是「原文承载」而非「解析后回写」（证据见 Correct-1）；类型化 DTO 只读值。`message.rs:721-735`（请求）与 `786-803`（响应）分别以固定原始字节断言「解码成功 + 再编码逐字节相等 + 未知字段/`_meta` 仍在文本中」。`_meta` 在 DTO 上是 `Option<Value>` 视图，但**回写不经过它**（`lib.rs:15-19` 明文约定），故不构成「经通用 JSON 值改写」。
2. **`session/load` 未被连带改动**：**通过**（证据见 Correct-3）。
3. **矩阵取值合法性**：**通过**。`broker` 不是 `native`，是枚举内的 `project_and_preserve`；`delivery` 提升与 `requirement: optional` + 非空 `capability` 相容（证据见 Correct-4）。核对 coder 自报的「未改 `revision`/`checkedAt`/`schemaSha256`」也属实（base 与 target 第 4/10/11 行取值相同，`2026-09-18`）。
4. **`implemented: true` 是否虚报能力**：**结论——不判为虚报（非 CRITICAL/MAJOR）；但需要 main 显式裁定其语义边界，且不需要代码回改**。依据：
   - `MethodStatus::Implemented` 的定义是 crate 级「本 crate 能编码/解码并按语义处理」（`methods.rs:43-44`），`methods.rs:1-6` 明文该表是 `matrix.json` 的手工镜像；WP1 确实在本 crate 内新增了类型化编解码，定义成立。
   - **`implemented` 今天不改变任何对外行为**：全仓 grep `status_of`/`MethodStatus::Implemented`/`ensure_direction` 显示消费者只有 `crates/acp-protocol/src/envelope.rs:148-175` 与 acp-protocol 自己的测试；**没有任何其它 crate 引用 `acp_protocol::methods`**（`agent-host` 对 agent→client 方法使用自己的字面量分派，见归档评审 `2026-09-24-acp-boundary-and-agent-host/reports/rv1-du1.md` 的 RV-DU1-F3 记录）。
   - **宣告面由矩阵决定且未被放大**：`matrix.json:44` 的 `capability=agentCapabilities.sessionCapabilities.resume`、`sync/pwa=explicit_unsupported`、`facade=not_advertised` 未变，「能力协商如实反映端到端能力」的对外面没有被虚报；端到端门控按 design D4 属 WP5。
   - **`delivery` 的提升是契约强制，`implemented` 是自由选择**：`tests/matrix_tables.rs:82-93` 强制 Rust 侧 `delivery` 等于矩阵；`matrix_tables.rs:95-110` 只约束 `post_mvp ⇒ !implemented`。现状中两种先例并存（`authenticate`、`$/cancel_request` 为 `conditional_mvp + false`；`session/set_mode`、`session/set_config_option`、`elicitation/create` 为 `conditional_mvp + true`），故不存在「conditional 必须置真/置假」的既定规则。
   - **反向后果**：若置 `false`，WP1 交付的入口 `session_resume_request()`（`message.rs:567`）将恒返回 `AcpError::Unsupported`，与 R1/R3 要求的「类型化解码入口」自相矛盾。置真使 WP1 内部自洽。
   - **残余风险与建议裁决**：`implemented` 是 crate 级语义、不等于端到端已落地。建议 main 把该口径作为本轮结论登记（例如 verification 的 Review Findings 记「`implemented` = 本 crate 编解码能力；端到端门控在 `agent-host` + `facade=not_advertised`」），无需改代码；若 main 采用「整链路落地才置真」的口径，则必须回改 `methods.rs:132-142` 与 `message.rs:822-826` 的断言，并决定该入口去留——那将带来新一轮（Round 2）检视。**我建议维持现状**。
5. **证据核对**：PV1 阶段 1 与 PV2 的原始输出（命令、工具链版本、退出码、逐测试二进制计数）完整内嵌在 `reports/wp1-coder.md`，且与改动自洽（5 条新增单测 + 35 条既有 = 报告中的 `-p acp-protocol` 合计；`check:commands` 仍报 12 commands、`check:acp` 报 25 methods，符合「WP2 尚未落地」的事实）。**额外 workspace clippy exit=0 的结论我独立佐证但不重复执行**：WP1 只在叶子 crate 追加 pub 项、未改任何既有签名、未改依赖（`Cargo.toml` 无变化），且全仓无其它 crate 消费被改动的 `status_of`/`implemented`，因此跨 crate 编译涟漪在静态上不可能；这与报告的 exit=0 一致。**尚待**：PV1 阶段 2（`npm run check:rust`，workspace 全量）与候选/主分支 PV2——按 plan.md 属集成基线/候选/主分支门禁，本 WP 分支不跑是**计划规定**，不是遗漏。
6. **写范围**：**通过**。全部改动落在 WP1 Write Scope（`crates/acp-protocol/`、`compatibility/acp/v1/matrix.json`、`fixtures/acp/v1/`、`docs/ACP_COMPATIBILITY_MATRIX.md`）内，无越界；`fixtures/acp/v1/`、`crates/acp-protocol/tests/`（TP2 独占新增用例）均未动，`crates/acp-protocol/src/message.rs` 内联 `#[cfg(test)]` 单测属 WP1 允许的 `src/**` 范围（plan.md Shared File Ownership 明文）。方法学限制：跨 crate 的越界改动无法用文件清单比对完全排除，我以全仓 grep 该改名符号 + 关键合同文件无痕迹 + PV2 门禁（边界、漂移、封闭词表、契约资产）自洽作为交叉佐证。

## Findings

| ID | Severity | Location | Trigger / Evidence | Impact | Recommendation | Recheck |
| --- | --- | --- | --- | --- | --- | --- |
| CR1-F1 | MINOR | plan.md 的 PV1/PV2 `Evidence` 列（`reports/PV1-<WP>.log`、`reports/PV2.log`）；verification.md `## Checks` 同名行 | `find` 实测 `openspec/changes/session-resume/reports/` 只有 `wp1-coder.md`（无 `PV1-WP1.log`/`PV2.log`；对照 WP2 的行同样引用了不存在的 `wp2-coder-PV1.log`）；WP1 的原始输出只在 coder 报告内。main 的 `## Handoff Index` 已把 `report_path` 指向 `reports/wp1-coder.md`（可读、含命令/版本/退出码） | 非阻断：契约门禁结论不受影响；但 premerge/final 需要「命名可读的 PV1/PV2 证据」，当前命名与落盘不一致，且两个元数据（plan 的 Evidence 列 vs verification 的 Handoff Index）并存 | 二选一：把阶段 1 原始输出落盘为 `reports/PV1-WP1.log`/`reports/PV2.log`（或改用 `reports/wp1-coder.md` 作为规范路径并在 plan/verification 中统一），或按模板在 verification 的 `## Check Plan Changes` 登记该偏离 | 不适用（记录层，非代码） |
| CR1-F2 | SUGGESTION | `docs/SESSION_CONTINUITY_DESIGN.md:250`（「`crates/acp-protocol`：`session/resume` 的 DTO 与状态（**当前为 `NotImplemented`**）」） | WP1 已把该状态改为 `Implemented`（`methods.rs:137-142`），该行现为陈旧陈述；该文档由 design D5 / tasks 2.6 归 WP6 同步，**不在 WP1 写范围** | 非阻断：文档内部会短暂自相矛盾（WP1 分支上），若 WP6 漏掉该行，交付后仍余留 | 提醒 WP6 的状态注记把 §8 该行一并改为实施后状态（WP1 无需动作） | 不适用 |
| CR1-F3 | SUGGESTION | `crates/acp-protocol/src/methods.rs:132-142`（`implemented: true`） | 见「重点核查 4」：`implemented` 无机器判据约束（`matrix_tables.rs:82-93` 只强制 `delivery`），其语义是 crate 级编解码能力而非端到端 | 非阻断：今天无任何对外宣告受影响（无外部消费者 + `facade=not_advertised`）；但若后续有人把它读成「已可宣告」，或 WP5/WP6 未落地，则可能演化为虚报 | 由 main 明确裁定并落记录（建议：维持 `true`，并登记「端到端门控在 `agent-host`、facade 不宣告」的口径）；**不建议**回改代码 | 不适用（本轮非 recheck） |

**未发现 CRITICAL / MAJOR 问题。**

## Assessment

- **本轮结论：PASS**（目标版本 `248d9b9fa69a1e6375a3b9708fdaa2a127dc7ccb`）。R1–R4 均有代码与测试证据支撑，未发现已确认且未解决的 CRITICAL/MAJOR；CR1-F1（MINOR）与 CR1-F2/F3（SUGGESTION）均不影响本轮代码判断。
- **Check ID 核对**：
  - **PV1（阶段 1，`-p acp-protocol`）**：证据 = `reports/wp1-coder.md`（`cargo fmt --all -- --check` exit 0；`cargo clippy --locked -p acp-protocol --all-targets --all-features -- -D warnings` exit 0；`cargo test --locked -p acp-protocol --all-features` exit 0，含新增 5 条单测与 `tests/matrix_tables.rs::methods_table_matches_matrix`/`post_mvp_methods_are_not_claimed_as_implemented` 绿）。**未发现弱化规则、被吞掉的失败或未执行检查**；我按角色边界不重复执行。
  - **PV2**：证据 = 同一报告（`npm run check` exit 0，十道门禁串行全绿，含 `check:acp` 的 ajv 矩阵强校验、`check:assets`、`check:docs`、`check:boundaries`、`check:drift`、`check:agentic`）。**限制（需 main 知悉）**：WP1 工作树内不存在未跟踪的 `openspec/changes/session-resume/`（provisioner 已声明、`find` 实测只有 `archive/`），因此该次 `openspec validate --all --strict`（`scripts/agentic-gate.mjs:15`）的条目为 19 而主工作区基线为 20——差异来自「活动变更目录是否存在」，不影响本次涉及的契约资产门禁（矩阵/schema/fixture/文档/边界/漂移/封闭词表全部针对被跟踪文件）。
  - **待补证据（不改变本轮代码判断）**：① PV1 阶段 2（`npm run check:rust`，workspace 全量）——门禁：全部 WP 集成后的集成基线/候选/主分支（plan.md 红窗口说明）；② 候选/主分支 PV2——同门禁；③ 目标 Agent 是否真的宣告 `sessionCapabilities.resume`——属 tasks 7.1 validation 的待验证假设，无真实 Agent 时须记 BLOCKED（不得写成已验证）。
  - **建议由 supervisor 执行的命令**（我无执行权）：`cargo test --locked -p acp-protocol --all-features` 与 `npm run check`（如要独立复算本轮结论）；`npm run check:rust`（候选/主分支阶段）。
- **记录层小差异（不影响结论）**：WP1 报告称 `methods.rs` 加了「3 行注释」，实测为 2 行注释（`methods.rs:134-135`）+ `spec(...)` 两处取值改动；total 行数变化与行号对齐证明 `methods.rs` 的改动仅此一处。
- **Merge 判定：OK with notes**（存在 1×MINOR + 2×SUGGESTION；无阻断项）。

### handoff_index

```yaml
handoff_index:
  - task_id: "3.2"
    work_package: WP1
    role: reviewer
    phase: branch
    round: 1
    stage: work-package
    target_revision: "248d9b9fa69a1e6375a3b9708fdaa2a127dc7ccb"
    evidence_type: REVIEW
    evidence_id: CR1
    report_path: "openspec/changes/session-resume/reports/cr1-review.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "在固定目标提交 248d9b9（分支 agentic/session-resume-wp1，ref 与 reflog 已核）上只读检视 WP1 四文件改动的完整内容与承载机制；规则取 AGENTS.md §3/§5/§7/§10 + 本变更 proposal/design D1–D6/plan WP1/specs R1–R4；证据为 reports/wp1-coder.md 的 PV1 阶段 1 与 PV2 原始输出（未由本角色重复执行）；1×MINOR（证据落盘命名）+2×SUGGESTION，无 CRITICAL/MAJOR"
    source_evidence: NOT_APPLICABLE
```