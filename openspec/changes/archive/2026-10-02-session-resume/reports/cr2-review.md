> 由主 Agent 从 reviewer 子 Agent 的返回结果原样持久化（reviewer 工具集只读、无 write）；内容未改写。
> 检视线程：CR2 / WP2 / Round 1；run: 45bd6260-1376-4458-83f1-de11190fa9f5

## Review

### Review Context

| Inputs 项 | 本轮实际取值与核对方式 |
| --- | --- |
| Review ID / Round | CR2 / 1（首轮，Previous Findings = NOT_APPLICABLE） |
| Review Type | branch |
| Review Stage | work-package（分支交付前检视） |
| Work Package | WP2（封闭词表原子点 + node-link wire） |
| Repository | `D:/Project/acp-remote`（主仓库，`refs/heads/main`）；目标版本在 worktree `D:/Project/acp-remote-wt/session-resume-wp2` |
| Base Revision | `81e350ff340014265eb7c9251237c799d4357fee` —— 独立核对：`.git/refs/heads/main` 内容 = 该 SHA，主仓库工作区相对 HEAD 无未提交改动，因此主仓库文件即 base 内容 |
| Target Revision | `32f71f5d5f08145cfd3e3bddd7d6f6a46a0a5992` —— 独立核对：`.git/worktrees/session-resume-wp2/HEAD` → `refs/heads/agentic/session-resume-wp2` → 该 SHA |
| 读取的规则/需求 | `openspec/schemas/agentic/roles/reviewer.md`、`roles/_shared/role-report.md`、`AGENTS.md`（§3/§4/§5/§7/§10）、`proposal.md`、`design.md` D1–D6、`plan.md`（Work Packages / Execution Waves / Shared File Ownership / Dependency Handoffs / Verification Strategy）、`tasks.md` 1.5 与 2.2、`specs/node-link-owner-server/spec.md`（全文；grep 证实其余 4 份增量规范不含 `session.resume`/`commands.json`/`pack`，与 WP2 面无关） |
| 实际检查范围 | base↔target 逐文件内容比对：`compatibility/commands/v1/commands.json`、`schemas/node-link/v1/{command,common}.schema.json`、`fixtures/node-link/v1/{manifest.json,README.md,valid/×2,invalid/×1}`、`scripts/check-command-catalog.mjs`、`crates/core/src/broker.rs`、`crates/identity-auth/src/authorization.rs`、`crates/identity-auth/tests/authorization.rs`、`crates/node-link-protocol/src/{command,common}.rs`、`crates/node-link-protocol/tests/envelope_fixtures.rs`、`docs/{NODE_LINK_PROTOCOL,SYNC_PROTOCOL,SECURITY_DESIGN,IDENTITY_AND_AUTH_CONTRACT}.md`、`AGENTS.md`、`README.md`、`.github/workflows/ci.yml`；越界核查用全 `crates/` 作用域 grep + `crates/server/` 逐点比对 |
| 验证证据（只读，不重跑） | `reports/wp2-coder.md`、`wp2-coder-PV1.log`、`wp2-coder-PV2.log`、`wp2-coder-red-window.log`、`verification.md` 第 33/34 行、`dispatch-queue.jsonl`（WP2 → coder-B → reviewing） |
| 隔离说明 | 本轮为新建独立检视实例，未参与 WP2 或任何 WP 的实现/讨论；只收到 Inputs 模板与该模板中列出的绝对路径契约 |
| 本轮限制（不得据此推定已核对） | 本环境无 shell/git 工具，**无法执行** `git show <target>` / `git diff <base>..<target>`；因此「diff」是用 base 文件（主仓库，SHA 已独立核实）与 target 文件（worktree，分支 ref 已独立核实）逐一内容比对得到的，属文件级等价证据，不是提交级 diff。全树「哪些文件被改」无法机械枚举，只能用越界信号（全 `crates/` grep、被点名文件的行数比对）间接判断。未运行任何检查（角色禁止）。 |
| 未验证内容 | PV1 阶段 2（workspace 全量，候选/主分支门禁，本阶段不适用）；E2E（plan.md 记 not-applicable）；「目标 Agent 是否真宣告 `sessionCapabilities.resume`」（属 7.1 validation） |

### Findings

| ID | Severity | Location | Trigger / Evidence | Impact | Recommendation | Recheck |
| --- | --- | --- | --- | --- | --- | --- |
| CR2-F1 | MINOR | `plan.md` `## Shared File Ownership`（无 `crates/node-link-protocol/tests/` 行）vs target `crates/node-link-protocol/tests/envelope_fixtures.rs:33,35` | coder 自报的「未登记既有测试文件」属实：base 该文件为 `EXPECTED_VALID_MESSAGE_CASES = 38 / EXPECTED_BODY_REJECTED = 9`，target 为 `40 / 10`；`fixtures/node-link/v1/manifest.json` 的 `"valid": true` 条目 40→42、无效条目 10→11，新增的 3 条 fixture 文件名与 manifest 登记一一对应，**没有新增/删除用例、没有弱化断言**（同一测试的 `assert_eq!` 计数、`every_manifest_case_behaves_as_declared` 仍在 243–253 行） | 仅为登记口径歧义：该文件在本变更内**只有 WP2 一个写者**（`crates/**/tests/ 的新增用例文件` 行只覆盖 TP2 的**新增**文件），WP2 与 TP2 分处 W1/W5 不并发；且 WP2 的 `Write Scope` 列本身就是整个 `crates/node-link-protocol/`。**因此**不满足「同一处却同批/无主写入」的 MAJOR 条件 | 结论：不以 MAJOR 阻断。建议 main 二选一——(a) 按 DR1-F28(a)/F29 的先例在 `verification.md` 记「计划勘误」而不动 plan.md（避免为一行登记白开依赖声明轮次）；(b) 在 Shared File Ownership 补一行 `crates/node-link-protocol/tests/`（Writers=WP2[,TP2]；Merge Owner=TP2；Order=WP2→TP2；Re-verify=TP2: PV1）。若选 (b)，plan.md 的 `contractDigest` 会变化，**必须**重新派发 DR1（第 7 轮）；纯登记改动、无代码影响 | 首轮，待 main 决定 |
| CR2-F2 | MINOR | `schemas/node-link/v1/command.schema.json:840-871`（accepted.result 只有 `session.create` 的 `if/then`）与 `crates/node-link-protocol/src/command.rs:1044-1060`（`from_accepted` 只特判 `SessionCreate`） | 需求侧写死了该点：`specs/node-link-owner-server/spec.md:47`「Owner 先回 `command.accepted`（`result = null`）」，`docs/SYNC_PROTOCOL.md:1267` 同句；但 wire 层对 `session.resume` 只约束 `completed` 的 `terminal.result`，`accepted.result` 仍是通用 `oneOf[object,null]` | 当前无消费者受损（`node-link-client` 未落地，Owner 侧行为由 WP6 实现并按 spec 发 `null`）；不一致本身与 `session.create` 的同族既有约定不对称，会让 Access 侧无法用 schema/解码拒绝不合规帧 | 由 main 裁决冻结面：要么在 schema 加对称 `if/then` 分支并在 `from_accepted` 加同形守卫，要么按 coder 报告 §6 问题 2 在 `design.md@D1` 明确记录「本层不收紧 accepted.result，WP6 负责恒发 null」。二者都不阻断本 WP | 首轮，非阻断 |
| CR2-F3 | MINOR | `docs/NODE_LINK_PROTOCOL.md:662`（「本切片的 Owner 只为 `session.list` 与 `session.create` 投影 wire 结果」）vs 同文件 §12.7 新增的 `session.resume` 结果契约（656–660 行） | 同一文档现在既要求 `session.resume` 的 `completed` 回 `SessionResumeResult`，又保留「只为 `session.list`/`session.create` 投影」的收窄句；WP2 的授权区域是 §10/§12.5/§12.7/§15，而 WP6 的写范围（plan.md Work Packages）**不含** `docs/NODE_LINK_PROTOCOL.md` | WP6 落地路由后该句与实现不符，且没有工作包拥有该文件的后续修改权（门禁不覆盖此句，不会红） | 一句 doc 补丁即可：在 662 行收窄句里补「以及 `session.resume`（见下）」，或由 main 在 WP6 写范围临时放开该文件。属文档一致性，非阻断 | 首轮，非阻断 |
| CR2-F4 | SUGGESTION | `docs/SYNC_PROTOCOL.md:1267`（新行 `sessionId` 列） | 同表列约定与其它行只用「禁止/必须」（`session.create` 行 1266 为「禁止」），新行写「仅 Node Link（经 `sessionRef`）」这一第三种措辞 | 仅可读性/表格词表一致性；`check:command-catalog.mjs` 的 `tableCommands` 只取首列，不触发任何门禁 | 改为「禁止（该命令只经 Node Link，会话由 `sessionRef` 承载）」之类「禁止+括注」写法 | 首轮，非阻断 |
| CR2-F5 | SUGGESTION | `reports/wp2-coder-PV1.log:1-3`、`wp2-coder-PV2.log:1-2` vs `reports/wp2-coder.md` 的 `handoff_index` / `verification.md` 33/34 行 | 两份日志首行均写 `revision: 81e350ff… (dirty working tree, 交付前未提交)`，而证据行绑定 `32f71f5`；「提交树 = 被测树」只由 coder 自述（提交后 `git status --porcelain` 为空、worktree 未装钩子故无 `--no-verify` 干预）支持 | 证据绑定的严谨性，不影响本轮静态判断；风险低 | 候选阶段的 PV1 阶段 2（workspace 全量）会天然覆盖该绑定，无需重跑本分支证据；若要更严，只需在候选/主分支的证据行里绑定当次提交与日志 | 首轮，非阻断 |

**CRITICAL / MAJOR：未发现问题。**

### Assessment

**逐条回答本轮重点核查（每条给依据）**

1. **六处（+第七处）集合同批一致 —— 通过。** 名字逐处实存且拼写完全一致：`commands.json:17`（`session.resume`，mutation / `grant.remote-work` / `pack: null` / `transport:["node_link"]` / `conditional_mvp`）、`commands.json:34`（`grant.remote-work` 成员 = `["session.create","session.resume"]`，满足 `packs ∪ grants` = 13 = 命令总数）、`schemas/node-link/v1/command.schema.json:62` 与 `:776-809`（`submitSessionResume`：空 object payload、会话三字段非 null、`expectedVersion:null`）、`:1087-1111`（completed → `sessionResumeResult`）、`docs/SYNC_PROTOCOL.md:1267,1269`（13 条 + 计数文本）、`docs/SECURITY_DESIGN.md:287`（13 行表 + 授权说明 291）、`crates/core/src/broker.rs:130`（`"session.resume" => "grant.remote-work"`，位于 `_ => return None`(131) 之前）。第七处：`crates/identity-auth/src/authorization.rs:71` 的 `GRANTS` 会员；`crates/identity-auth/tests/authorization.rs:267`（`scopes.len()` 6→7 + `contains("session.resume")`）、`command_names_cover_the_whole_catalog`、`grants_match_machine_catalog` 用 `include_str!` 直读同一个 `commands.json`。PV2 日志实证 `command catalog OK: 13 commands`。
2. **D6 的 pack 判据 —— 通过，未发现放宽过度。** base `scripts/check-command-catalog.mjs:194` 为 `if (!command.pack && command.name !== "session.create")`，target 为 `if (!command.pack && (command.transport ?? []).includes("sync"))`，并附理由注释。谓词分析：11 个含 `sync` 的命令全部已声明 pack（`commands.json` 第 5–15 行），新谓词对它们仍是**必须**，判定与 base 逐条相同；唯一能新通过 `pack:null` 的命令必须 `transport` 不含 `sync`，当前目录里恰好只有 `session.create` 与 `session.resume` 两条，二者确实无设备包。`transport` 为空数组或缺失时仍会先按既有 `empty transport` 规则报错，不会因新谓词漏检。（旁注，非本 diff 引入、不计为发现：脚本仍未校验「命令声明的 pack 名必须存在于 `packs`」——若某命令声明了不存在的 pack 名但出现在某个 grant 里，`packs/grants` 双向比对不会发现；base 同样如此。）
3. **门禁说明四处同步 —— 通过且措辞与脚本一致。** `AGENTS.md:299` 补「命令目录侧还按 `transport` 判定 `pack`——`pack.*` 是设备/配对面概念，`transport` 不含 `sync` 的命令…允许 `pack: null`，含 `sync` 的命令必须有 pack，该判据不按命令名硬编码」；`README.md:61` ②补同规则；`.github/workflows/ci.yml:5-7` 头部注释补同规则（base 三处均无该句）。`package.json` 未改是**正确**的：D6 只改已有门禁内部判据，未新增/删除门禁，`check` 仍是十道。
4. **未越界 —— 通过。** 全 `crates/` grep `session.resume` 只命中 `core/src/broker.rs:130`、`identity-auth/{src,tests}`、`node-link-protocol/{src,tests}`；`crates/server/`、`crates/app/`、`crates/storage-sqlite/` 零命中。`crates/server/src/node_link/command.rs` 在 base 与 target 的行数一致（两次 read 均报同一总行数、同一 `on_dispatched` 上下文），只有既存注释 `:363` 提到「12 条命令表」。`broker.rs` 全文件只有 1 处 `session.resume`，且无 `SessionResume`/`AgentSessionId`/`resume_session` 任何出现 → 确认**只加了 `required_grant` 一条臂**，`CommandPayload` 变体/`command_name`/分发/用例（WP3）未被触碰。
5. **`envelope_fixtures.rs` 的登记判定 —— 不阻断（MINOR，见 CR2-F1）。** 判断依据：①该文件在本变更内只有一个写者，WP2 与唯一另一个测试目录写者 TP2 分处 W1/W5，不存在并发同处；②WP2 的 `Write Scope` 列字面就是整个 `crates/node-link-protocol/`，Registry 的 Shared File Ownership 表按用途是**多写者/需协调文件**的仲裁表（DR1-F13/F14 的收口也是靠扩 `Write Scope` 列完成的），不能反过来否定 WP 级写范围；③改动是两条硬编码计数常量随新增 fixture 同步，无用例增删、无断言弱化，正是 DR1-F22 已认定「应登记」的同一类涟漪但落在**无第二写者**的目录里；④coder 已在报告 §6 主动申报并请裁决，不属隐瞒。与 DR1-F22/F15 的差别在于那两处是**跨包共享**目录，本处不是。因此按 MAJOR（阻断）判定过重。**建议与后果**：若 main 选择补登并改 plan.md，`contractDigest` 变化 → 按 reviewer.md「规划审查绑定当前 contractDigest」须重新派发 DR1（第 7 轮，纯登记）；若不改 plan.md，则按 F28(a)/F29 先例在 `verification.md` 记计划勘误即可，**无需**重跑 DCR，也**不影响** WP2 的代码判定。
6. **不新增封闭词表条目 —— 通过。** `compatibility/errors/v1/errors.json` 与 `compatibility/features/v1/features.json` 均无 `resume` 相关新增；D1 用的三个错误码均已存在（`errors.json:79 nodelink.command.unsupported`、`:82 nodelink.command.unsupported_field`、`:84 nodelink.internal.unavailable`），未新增 feature ID（与 `docs/NODE_LINK_PROTOCOL.md:455` 的 feature 表不动一致）。`docs/IDENTITY_AND_AUTH_CONTRACT.md` 的 no-op 判定成立：base 与 target 的 grep 结果完全相同，全文件只有 `:51` 一处提到 `authorization::{PACKS, PRESETS, GRANTS, LOCAL_CAPABILITIES}`，不列举任何会员、不含命令计数，因此会员从 1 项变 2 项无需改它。
7. **红窗口 —— 通过，与 plan.md 声明一致，无未登记涟漪。** `wp2-coder-red-window.log:7-9` 显示 workspace clippy 只有一条 `error[E0004]`，位置 `crates/server/src/node_link/command.rs:2191:25`（`core_payload()` 对 `CommandPayload::SessionResume(_)` 非穷尽）、`exit=101`，正是 plan.md `## Dependency Handoffs`「跨 crate 编译涟漪与预期红窗口（DR1-F13/F14）」登记的窗口与收口责任人（WP6，写范围含整个 `crates/server/`）。该日志同时说明红窗口**不是** PV1 结果（文件头自述 `evidence=NOT_APPLICABLE`），coder 未把它当通过证据。**范围说明（非发现）**：`server` 编译失败会阻断依赖它的 `crates/app` 的类型检查，因此该日志**不能**证明「不存在其它下游涟漪」；但 WP2 侧的越界信号（见第 4 条）为零，且 plan.md 已把 `app`/`server` 的收口逐点归给 WP6，未发现未登记的 WP2 涟漪。

**检查证据核对（Check Plan ↔ 实际证据）**

- **PV1 阶段 1**（plan.md 的 crate 列表 `-p node-link-protocol -p core -p identity-auth`）：`wp2-coder-PV1.log` 逐条给出 `cargo fmt --all -- --check` → exit 0、`cargo clippy --locked -p node-link-protocol -p core -p identity-auth --all-targets --all-features -- -D warnings` → exit 0、`cargo test … --all-features` → exit 0（`acp_core` 121、`identity_auth` 集成 15/20/30/2/9/5、`node_link_protocol` lib 9 + 集成 6/7/5/7/3/3、doc-test 2；无「零测试/全跳过」），与 plan.md 的阶段 1 命令形态和 crate 集合一致。**已核对，通过**（绑定细节见 CR2-F5，非阻断）。
- **PV2**（`npm run check`）：`wp2-coder-PV2.log` 显示十道门禁全绿，关键行 `command catalog OK: 13 commands`、`schema fixtures OK: 120 valid, 26 invalid`、`contract assets OK: 17 schemas, 159 fixture files`、`crate boundaries OK`、`contract drift OK`、`doc links OK`、`check:agentic` PASS，exit 0。与 plan.md 的 PV2 阶段 1 定义一致。**已核对，通过**。
- **待返回/阶段外证据**：PV1 **阶段 2**（workspace 全量）按 plan.md 属「全部 WP 集成后的集成基线/候选/主分支」，本轮工作包分支不适用，**不影响**本轮静态判断；其必须在 WP6 收口后、候选合入前补齐全绿（Completion Criteria 已明写）。E2E 按 plan.md 记 not-applicable（用户批准），本轮不判其执行。
- 未执行的检查与未验证假设（`sessionCapabilities.resume` 的真实 Agent 行为）不属本工作包分支的判据，留 7.1 validation。

**结论**

- **Assessment：PASS。** 在 `32f71f5d5f08145cfd3e3bddd7d6f6a46a0a5992`（WP2 分支交付提交）上未发现 CRITICAL/MAJOR；5 项 MINOR/SUGGESTION 均非阻断，其中 CR2-F1 需要 main 做一次登记/裁决决定（若改 plan.md 则须重开 DR1 轮次，若只记勘误则不需要）。
- 本轮为分支阶段静态检视，**不**代表 PV1 阶段 2、独立 validation、E2E 或候选合入已通过。

```yaml
handoff_index:
  - task_id: "2.2"
    work_package: WP2
    role: reviewer
    phase: branch
    stage: work-package
    round: 1
    target_revision: "32f71f5d5f08145cfd3e3bddd7d6f6a46a0a5992"
    evidence_type: REVIEW
    evidence_id: CR2
    report_path: "openspec/changes/session-resume/reports/cr2-wp2-branch-review.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "base 81e350ff…（.git/refs/heads/main）+ target 32f71f5…（.git/worktrees/session-resume-wp2/HEAD → refs/heads/agentic/session-resume-wp2）；无 shell 故未执行 git diff，改用 base↔target 逐文件内容比对与全 crates/ grep 越界核查；证据为 reports/wp2-coder{,-PV1.log,-PV2.log,-red-window.log}"
    source_evidence: NOT_APPLICABLE
```