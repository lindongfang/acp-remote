# DR1 — Dependency Declaration Review（session-resume / plan.md），Round 2（recheck）
## Shared Report
```
task_id: NOT_APPLICABLE（plan/tasks.md 未为「依赖声明审查」建任务行；该门禁由 plan.md 的 Dependency Declaration Review 段与 verification.md 的 ## Dependency Declaration Review 表承载）
role: reviewer
phase: plan
stage: plan
round: 2
agent_context: standalone task-level reviewer subagent（新实例，未继承 Round 1 或任何实现/规划对话；本轮只读，无 write 工具）
target_revision: sha256:17460283ef4a8b1dcf2f3c5699affc511d66e1c0607f7171ec603f30dd619d5c
scope: plan.md 的依赖类型声明、Execution Waves、Runtime Resources 的 Exclusive Scheduling、
       Shared File Ownership（含 Merge Owner / Merge Order / Re-verify）、Contract Freeze 路径与写范围，
       并交叉核对 gates 脚本/源码/schema 以判断「分支级检查是否可产出」；不读代码 diff、不跑测试
changes: 无（只读；未修改、未暂存、未切换分支、未提交任何文件）
checks: NOT_APPLICABLE（规划审查不执行 PV1/PV2，不冒称已执行）
issues: 2×MAJOR（DR1-F9、DR1-F10）+ 2×MINOR（DR1-F11、DR1-F12）；Round 1 的 4×MAJOR + 3×MINOR + 1×SUGGESTION 全部复核为已解决
result: FAIL
evidence_paths: 本报告（openspec/changes/session-resume/reports/dr1-dependency-review-round2.md，由 main 持久化）
resource_cleanup: 未创建/启动/停止任何资源；未写仓库文件
```
## Review Context
| 项 | 实际取值 |
| --- | --- |
| Review ID / Round | DR1 / 2（同一线程；Type 与 ID 沿用 Round 1，显式 round=2，当前结论以本轮为准） |
| Review Type / Stage | plan / plan |
| Work Package | NOT_APPLICABLE（不代表任何 WP，非 WP Owner） |
| Repository | `D:/Project/acp-remote`（只读；未切分支、未建检视 worktree） |
| Base Revision | NOT_APPLICABLE（规划审查不针对代码 diff） |
| Target Revision | `sha256:17460283ef4a8b1dcf2f3c5699affc511d66e1c0607f7171ec603f30dd619d5c`（派发给出的当前 `contractDigest`）。本会话无 OpenSpec 引擎调用能力，**无法重算该摘要**，按派发值绑定，不声称已复核其内容映射 |
| Previous Findings | `reports/dr1-dependency-review.md`（DR1 Round 1，result FAIL，4×MAJOR = DR1-F1/F2/F3/F4，3×MINOR = DR1-F5/F6/F7，1×SUGGESTION = DR1-F8） |
| Requirements（已读全文） | `proposal.md`；`specs/acp-wire-protocol/spec.md`、`specs/local-agent-host/spec.md`、`specs/node-link-owner-server/spec.md`、`specs/storage-schema-v2-migration/spec.md`、`specs/workspace-resolution/spec.md`；`design.md`（D1–D6，含新增 D6）；`plan.md`（Contract Changes / Coverage Index 37 行 / Work Packages / Execution Waves / Shared File Ownership / Dependency Handoffs / Runtime Resources / Target / Merge Strategy / Verification Strategy / Main E2E / Independent Validation / Completion Criteria）；`tasks.md`（1.1–9.1） |
| Project Rules（已读） | `AGENTS.md` §4 依赖规则、§8 工作流、§9 测试要求、§10 文档维护与门禁改动四处同步、§11 完成定义、§12 工具链；`roles/reviewer.md`、`roles/_shared/role-report.md`；`openspec/schemas/agentic/procedures/workflow-check.md:66-70`、`templates/plan.md:58-70`（Contract Freeze 版本号只是人读标识、不做机械校验） |
| 本轮交叉核对的机器资产（只读） | `scripts/check-command-catalog.mjs`（全文）、`scripts/check-acp-compatibility.mjs`（全文）、`scripts/check-features.mjs`、`package.json`、`Cargo.toml`（workspace members）；`compatibility/commands/v1/commands.json`、`compatibility/acp/v1/matrix.json`、`schemas/acp/compatibility-matrix.schema.json`、`schemas/node-link/v1`（经 WP2 写范围对应）；`docs/SYNC_PROTOCOL.md` §11.5、`docs/SECURITY_DESIGN.md` §10.2、`docs/NODE_LINK_PROTOCOL.md` §10/§11.3/§12.5、`docs/ACP_COMPATIBILITY_MATRIX.md` §3.3、`docs/CORE_PORTS_AND_STORAGE.md` §2/§7.2/§7.3/§11.8、`README.md`「合同检查」、`.github/workflows/ci.yml`、`AGENTS.md` §10；`crates/core/src/broker.rs`（`command_name`/`required_grant`/`node_allowed`/`port_error_public`）、`crates/core/src/model/error.rs`、`crates/core/src/model/tests.rs`、`crates/identity-auth/src/authorization.rs`、`crates/identity-auth/tests/authorization.rs`、`crates/node-link-protocol/src/command.rs`、`crates/server/src/node_link/command.rs`、`crates/server/src/local_admin/params.rs`、`crates/storage-sqlite/tests/migration.rs` |
| Verification Evidence | 无（规划审查不得冒称已执行测试；本会话无 shell/测试执行能力，未运行 PV1/PV2、未跑 E2E） |
| Check Plan | plan.md `## Verification Strategy` 的 PV1/PV2 定义（只作为「依赖/写范围是否可产出该检查」的判据，未执行）；本轮核对 Check ID：PV1、PV2 |
| Isolation / 限制 | 未继承任何实现或规划对话；无法重算 `contractDigest`；`openspec/changes/session-resume/` 在主工作区未跟踪（verification.md `## Target` 已记录），因此**没有可 diff 的基线**，`## Contract Changes` 声称的「proposal/specs 未变」无法逐字节证伪，只能做一致性核对（见下文独立判断 3）。实际 Agent ID 与隔离设置由调度者关联登记 |
## Round 1 findings 逐项复核（沿用原问题 ID）
| ID | Round 1 级别 | 本轮结论 | 复核依据（读修订后的 plan/tasks/design + 脚本/源码） |
| --- | --- | --- | --- |
| DR1-F1 | MAJOR | **已解决** | 封闭词表原子点已由 `plan.md:226` 单一 WP2 覆盖 gate 断言的全部五处：`compatibility/commands/v1/commands.json` + `schemas/node-link/v1/` + `docs/SYNC_PROTOCOL.md` §11.5 + `docs/SECURITY_DESIGN.md` §10.2 + `crates/core/src/broker.rs`（镜像，`scripts/check-command-catalog.mjs:254`）；另含 `fixtures/node-link/v1/`、`docs/NODE_LINK_PROTOCOL.md`、门禁脚本与三处门禁说明。WP3 只改 `CommandPayload` 变体、`command_name` 与分发/用例（`plan.md:227`、`tasks.md:21`），不动 `required_grant` 语义——源码两处确为独立函数（`crates/core/src/broker.rs:94-106` `command_name` vs `:119-136` `required_grant`），且新增一条字符串臂不依赖 WP3 的变体即编译通过。`crates/core/src/broker.rs` 双写者已登记（`plan.md:253`：Merge Owner=WP3、Merge Order=WP2 → WP3、Re-verify=WP3: PV1, PV2；两包分处 W1/W2，`plan.md:243` 入场条件要求 WP2 已入已验收集成基线），不存在「同一处却同批」。**但同一原子点仍漏一个强制写目标 → 见新发现 DR1-F9** |
| DR1-F2 | MAJOR | **已解决** | D6 的「按 transport」规则与现行判据逐条对齐：`scripts/check-command-catalog.mjs:194` 现为 `name !== "session.create"` 豁免；`compatibility/commands/v1/commands.json` 中 `session.create`（第 12 行）是唯一 `pack: null` 且 `transport = ["node_link"]`（不含 sync）的命令，其余 11 条都含 `sync` 且各有 pack（逐条核对 commands.json:5-16）。故「含 sync 才要求 pack」等价于放行 `session.create` 与 `session.resume`、不放宽其它命令，且去掉硬编码特例。修补路径的写目标已登记：`scripts/check-command-catalog.mjs`、`AGENTS.md`、`README.md`（「合同检查」②）、`.github/workflows/ci.yml` 均在 `plan.md:226` / `tasks.md:20`；`package.json` 的 `check` 脚本本身不含该判据（`package.json:4-19`），plan 已显式声明其为 no-op，AGENTS.md §10 的「四处同步」因此成立 |
| DR1-F3 | MAJOR | **已解决** | 两份文档均在 WP2 写范围（`plan.md:226`）且各有 Shared File Ownership 行（`plan.md:257`、`:258`：单写者 WP2、Merge Owner WP2、Merge Order WP2、Re-verify PV2）。计数文本确实落在被登记区域内：`docs/SYNC_PROTOCOL.md:1268`（「完整命令集合（12 条）…node-link 全部 12 条」）位于 §11.5（标题 `:1249`，下一 `###` 为 `:1279`），与「§11.5 命令表与相邻计数文本」一致；`docs/SECURITY_DESIGN.md:269-285` 为 §10.2 表格，gate 按集合相等断言（`scripts/check-command-catalog.mjs:245`）。未发现其它位置残留命令计数（全库检索「12 条 / 12 个命令 / 命令集合」仅命中 SYNC:1268 与 NODE_LINK:587） |
| DR1-F4 | MAJOR | **已解决** | TP 拆分成立且声明自洽：TP1 在 W1（`plan.md:231`、`:242`）Dependencies=none、写范围仅 `reports/tp1-*.md`、Verification=`NOT_APPLICABLE（设计阶段不跑 PV）`，`tasks.md:27` 明写「本阶段**不写可编译用例**」；TP2（`plan.md:232`、`:247`、`tasks.md:28`）声明 `code:WP1…WP6`，位于 W5 = max(1,1,2,3,3,4)+1 = 5，且写范围是 `crates/**/tests/` 的可执行用例 + `[PV1]` 基础检查，与 `tasks.md:44`（4.2）和 Coverage 行引用的 `4.2` 一致。不再存在「声明 none 却必须引用未实现 API」的不可产出检查 |
| DR1-F5 | MINOR | **已解决** | WP3 的 Contract Freeze 已含 `design.md@D1, D2, D3`（`plan.md:227`，D1 = 命令名与 `grant.remote-work` 取值来源）；写范围已改为 `docs/CORE_PORTS_AND_STORAGE.md（§2、§3.1、§5.1、§5.2）`（`plan.md:227`），与 Shared File Ownership 的 `plan.md:254` 区域说明逐项一致 |
| DR1-F6 | MINOR | **已解决** | WP4 写范围改为「§7 标题、§7.2（版本常量与升级步骤）、§7.3（DDL）」（`plan.md:228`），与 `plan.md:254` 的说明一致；版本常量实际位置 `docs/CORE_PORTS_AND_STORAGE.md:856`（§7.2）与 §7 标题 `:843` 均落在该区域内；与 WP3 的 §2/§3.1/§5.1/§5.2 实际不相交，Merge Order=WP3 → WP4 仍成立 |
| DR1-F7 | MINOR | **已解决** | WP5 已显式声明 `code:WP1, code:WP3`（`plan.md:229`），Dependency Handoffs 有 `WP5 | WP1, WP3` 行与失效条件（`plan.md:274`：「WP1 变更 `session/resume` 响应 DTO → WP5/WP6/TP2 重跑」），`tasks.md:25` 也写明「响应用 WP1 的类型化 DTO 解码」 |
| DR1-F8 | SUGGESTION | **已解决** | `plan.md:291` 与 `verification.md` 的 `## Target` 均已改为 `reports/scout-target.md`；目录实存该文件（`reports/` 仅 `dr1-dependency-review.md`、`provisioner-worktrees.md`、`scout-target.md`），不再引用不存在的 `.log` |
## 独立判断（派发要求的第 1–4 项，不复述修订说明）
**1. Execution Waves 层级与 Serialization Reason —— 合规。**
逐行验算（上游取该 WP 声明依赖中**最早**出现的层级）：WP1 = W1（none，D1 已冻结，`plan.md:240`）；WP2 = W1（none，D1+D6 已冻结，`:241`）；TP1 = W1（none，specs 冻结，`:242`）；WP3 = WP2(W1)+1 = W2（`:243`）；WP4 = WP3(W2)+1 = W3（`:244`）；WP5 = max(WP1 W1, WP3 W2)+1 = W3（`:245`）；WP6 = max(1,1,2,3,3)+1 = W4（`:246`）；TP2 = max(1,1,2,3,3,4)+1 = W5（`:247`）。每行 Enter Condition 与 Dependencies 类型一致；8 行 Serialization Reason 全为 `NOT_APPLICABLE`，即格式要求的合法取值之一（其余形式为「<码>: <证据路径>」），且本变更 Runtime Resources 三行 Exclusive Scheduling 均为 `NOT_APPLICABLE`（`:284-286`，端口 `127.0.0.1:0`、唯一 tempdir、脚本只读），无资源互斥需要额外串行化理由 —— 与 Round 1 的判定一致，无新问题。
**2. 是否出现新的「同批却互为前置」或「同一处却同批」—— 三对已登记的双写者均无问题；新缺口是写范围本身（DR1-F9/F10/F12）。**
- `crates/core/src/broker.rs`：WP2(W1) / WP3(W2)，不同批；Merge Owner=WP3、Order=WP2 → WP3、Re-verify=WP3: PV1+PV2 齐备（`plan.md:253`）；区域互斥已由源码证实（`required_grant` vs `command_name`/payload/分发）。
- `README.md`：WP2(W1) 改「合同检查」② / WP6(W4) 改「仓库当前状态」，不同批、区域不相交、Merge Owner=WP6、Order=WP2 → WP6、Re-verify=WP6: PV2 齐备（`plan.md:260`）。
- `docs/CORE_PORTS_AND_STORAGE.md`：WP3(W2) §2/§3.1/§5.1/§5.2 / WP4(W3) §7 标题/§7.2/§7.3，不同批、区域不相交、Merge Owner=WP4、Order=WP3 → WP4、Re-verify=WP4: PV2 齐备（`plan.md:254`）。
- 另外交叉核对了三类**未登记但可能强制的跨 crate 写目标**，其中两类排除、一类成立：①`UnavailableKind` 新增取值——`crates/server/src/local_admin/params.rs:914-931` 对 `Unavailable(kind)` 统一映射为 `local.unavailable`（测试按 `UnavailableKind::ALL` 遍历，`params.rs:1562-1568`），且 `crates/core/src/model/tests.rs:2208` 与 `crates/core/src/broker.rs:3078-3092` 均在 core 内 → WP3 单包自足，无 server 写入需求；②node-link-protocol 加第 13 个 `CommandName` —— `crates/server/src/node_link/command.rs:247-262`、`:2268-2274` 的 match 均带通配臂，server/app 不会因新增变体编译失败；③`crates/identity-auth/**` —— **成立且未登记**，见 DR1-F9。
**3. `## Contract Changes` 声称的「行为契约未变、仅执行安排重排」—— 无法证伪，但也无法确认；未发现与之一致的反证。**
限制：change 目录在 git 中未跟踪（`verification.md` `## Target` 已记录 `?? openspec/changes/session-resume/`），本会话无 diff 能力，因此**不能**证明 proposal/specs 未被编辑。一致性核对结果：(a) Coverage Index 37 行的 `source.heading` 与 5 份 specs 的 37 个标题**逐条实存且一一对应**（acp 4 + local-agent-host 8 + node-link 11 + storage 10 + workspace 4 = 37），未出现「引用不存在的需求块」；(b) 5 份 specs 与 proposal 中已写「命令集合 12 → 13」（`specs/node-link-owner-server/spec.md:7`、`proposal.md:66`）与门禁 `pack` 规则（`proposal.md:73`），与修订后计划一致，未见与重排冲突的需求；(c) D6 属门禁脚本判据（`design.md:142-152`），不是产品/wire 行为，`plan.md:14-24` 的 Contract Changes 表只在 F2 行声明「新增决策 D6」，与「行为契约未变」的口径不矛盾；(d) `verification.md` 的 `## Check Plan Changes`（2026-09-30）记录了同一口径。结论：**无法确认**（非「未解决」），但不足以构成 finding；若 main 需要该断言可被机械核对，应在交付说明中把 specs 的基线固定为提交或摘要。
**4. Coverage Index 的 37 行 tasks 引用 —— 全部指向修订后真实存在的任务编号。**
37 行出现的 task 引用集合为 {2.1, 2.2, 2.3, 2.4, 2.5, 2.6, 4.2}，逐条在 `tasks.md` 中存在且语义对齐：2.1=WP1、2.2=WP2、2.3=WP3、2.4=WP4、2.5=WP5、2.6=WP6（各自带 `[wp:WPn]` 与 `[PV…]` 派发行，`tasks.md:17-26`），4.2=TP2 的用例编写 + `[PV1]` 基础检查（`tasks.md:44`，对应 `tasks.md:28` 的 `[wp:TP2] [PV1]` 派发行）。R1–R37 连续无重复，与 `plan.md` Completion Criteria 的「37 行」一致；未发现指向已删除/重编号前任务号的悬空引用。
## Findings
| ID | Severity | Location | Trigger / Evidence | Impact | Recommendation | Recheck |
| --- | --- | --- | --- | --- | --- | --- |
| DR1-F9 | **MAJOR** | `plan.md:226`（WP2 写范围）、`plan.md:227-232`（其余 WP 写范围）、`plan.md:249-266`（Shared File Ownership 无该 crate 行）；`design.md:124-136`（D5 资产表未列）；`proposal.md` Impact（未列） | 封闭词表原子点还有**第七个强制写目标且不在任何写范围**：`crates/identity-auth/src/authorization.rs:71` 的 `GRANTS` 镜像含 `("grant.remote-work", &["session.create"])`，而 `crates/identity-auth/tests/authorization.rs` 通过 `include_str!("../../../compatibility/commands/v1/commands.json")`（`:15-19`）逐项断言该镜像：`grants_match_machine_catalog`（`:46-61`，会员逐项相等）、`command_names_cover_the_whole_catalog`（`:99-121`）、`pack_members_are_registered_commands_with_matching_grant`（`:306-317`，对目录里每条命令断言 `expand_grant_scopes(command.grant).contains(name)`）、`node_request_expands_grants_to_command_scopes`（`:258-270`，`assert_eq!(request.scopes.len(), 6)`）。同时 `scripts/check-command-catalog.mjs:204`（`packs ∪ grants` 的成员集合必须等于全部命令名）与 `:210`（`pack: null` 的命令不得挂进任何 pack）强制 `session.resume` 必须挂进 `grants["grant.remote-work"]`；D1 已冻结 `grant = grant.remote-work`。`crates/identity-auth` 是 workspace 成员（`Cargo.toml:19`），而 WP2 声明的 PV1 是 `cargo test --locked --workspace --all-features`（`plan.md:308`）→ WP2 分支上 PV1 必然变红，且该 crate 无任何 WP 可写（WP2 写范围见 `:226`，不含 `crates/identity-auth/`）；`tests/authorization.rs` 属**既有文件修改**，也不在 TP2「`crates/**/tests/` 的新增用例文件」范围内（`:232`） | WP2 的冻结契约在其自身声明基线上不可交付（PV2 可绿、PV1 必红）；若实现者「顺手」改了未登记文件，就是无归属的跨域写入（授权词表镜像与验收测试由非 Owner 改写），与 Round 1 判定 MAJOR 的 F1/F3 同类。另：`proposal.md` 声称「不改变…授权词表」，与 grant 会员实际变化不一致 | 把 `crates/identity-auth/`（`src/authorization.rs` 的 `GRANTS` 会员 + `tests/authorization.rs` 的 `scopes.len()` 计数）写入 WP2 的 Write Scope，并在 Shared File Ownership 登记一行（单写者 WP2、Merge Owner WP2、Merge Order WP2、Re-verify WP2: PV1+PV2）；在 `design.md` D5 资产表与 `proposal.md` Impact 补该 crate；同时按 AGENTS.md §10「授权展开变化 → `docs/IDENTITY_AND_AUTH_CONTRACT.md`」明确该文档是否需一句说明（该文档 §2 只提到镜像存在、未列会员，可声明为 no-op 并给出理由） | 不适用（本轮新发现） |
| DR1-F10 | **MAJOR** | `design.md:124`（D5 冻结的 `layers`）、`plan.md:225`（WP1 写范围）、`plan.md:308`（PV2 内含 `check:acp`） | D5 把 `method.session_resume` / `cap.agent.session_resume` 的 `layers` 冻结为 `{ acp: native, broker: native, sync: explicit_unsupported, pwa: explicit_unsupported, facade: not_advertised }`，但 `schemas/acp/compatibility-matrix.schema.json:52-62` 的 `$defs.layers` 规定 `broker` 枚举 = `project_and_preserve \| local_service \| pass_through \| explicit_unsupported \| not_applicable`（**没有 `native`**），且该对象 `additionalProperties: false`；`scripts/check-acp-compatibility.mjs:14,43-58` 用 ajv 编译该 schema 并 `validate(matrix)`，违反即 `exit 1`（`npm run check` 第 ⑥ 项，`README.md:64` ②–⑩ 中的 `check:acp`）。`docs/ACP_COMPATIBILITY_MATRIX.md:90` 记录同一枚举，也无 `broker: native`；全库矩阵中不存在该取值（`matrix.json:61` 用的是 `broker: pass_through`） | 冻结契约在现行门禁下不可实现（与 Round 1 的 DR1-F2 同类）：WP1 声明的 PV2 必然 FAIL，除非改 D5 取值或改 schema；而 `schemas/acp/` 不在任何 WP 写范围（WP1 = `crates/acp-protocol/`、`compatibility/acp/v1/matrix.json`、`docs/ACP_COMPATIBILITY_MATRIX.md`），修补路径未登记，也未按 AGENTS.md §10 声明门禁说明同步 | 二选一并在计划中登记：(a)（推荐，改动最小）把 D5 的 `broker` 改为枚举内的合法值，并在 spec/D5 写明理由（按 §3.3 定义，`local_service` 表示「Owner Daemon 作为 ACP Client 在本节点提供服务」，`project_and_preserve` 表示「建公共领域视图并保留 raw」——选哪一个属契约判断，需 main/用户确认）；(b) 若确实要新增 broker 层取值，则把 `schemas/acp/compatibility-matrix.schema.json` 与 `docs/ACP_COMPATIBILITY_MATRIX.md` §3.3 的枚举表写入 WP1 写范围，并按 AGENTS.md §10 同步门禁说明四处 | 不适用（本轮新发现） |
| DR1-F11 | MINOR | `tasks.md:20`、`design.md:132`（`docs/NODE_LINK_PROTOCOL.md` 的同步区域清单）；`docs/NODE_LINK_PROTOCOL.md:409` | WP2 对 `docs/NODE_LINK_PROTOCOL.md` 的同步清单只列 §12.5/§12.7/§15，但该文件 §10「Export Policy」（369–417 行）内有一张 grant → 覆盖命令表，`docs/NODE_LINK_PROTOCOL.md:409` 现为 `grant.remote-work \| session.create`；`session.resume` 加入 `grant.remote-work` 后该行会陈旧。`scripts/check-command-catalog.mjs` 只把 `docs/SYNC_PROTOCOL.md` §11.5（`:241`）与 `docs/SECURITY_DESIGN.md` §10.2（`:245`）断言为全量集合，不会发现 NODE_LINK §10 的漂移（§12.5 的「12 个命令名」计数在清单内，已覆盖） | 交付后 `docs/NODE_LINK_PROTOCOL.md` 内部自相矛盾（§10 的 grant 会员表与 §12.5 命令表不一致），且没有门禁能发现；文件归属存在（WP2 的文件级写范围覆盖该文件），仅区域清单不全 | 在 `tasks.md:20` 与 `design.md:132` 的该行补「§10 grant 表（`grant.remote-work` 会员）」；若认为不需要改，则显式说明 §10 表的语义（如「只列首切片命令」）以免读者误判 | 不适用（本轮新发现） |
| DR1-F12 | MINOR | `plan.md:266`（Shared File Ownership 的 `crates/**/tests/（TP2 用例）` 行）与 `plan.md:225-230`（WP1–WP6 的整 crate 写范围）、`tasks.md:22`（2.4 补迁移测试） | 该行声明「TP2 独占测试目录；WP1–WP6 只写与被写代码同目录的单元测试」，但 WP1–WP6 的写范围是整 crate（如 `crates/storage-sqlite/` 含 `tests/`），而任务 2.4 明写「补升级/幂等/字节稳定/旧行保留测试」——仓库中这类测试的既有落点正是 `crates/storage-sqlite/tests/migration.rs`（`:149` 即「连续两次打开字节不变」的幂等基准），该 crate 的 `src/` 内只有一个 `#[cfg(test)]`（`storage-sqlite/src/migrate.rs:1280`），且 Coverage R13–R22 同时挂 `2.4` 与 `4.2` 两行 | 两条声明不能同时为真：要么 WP4 实际会写 `tests/migration.rs`（则该行「TP2 独占」不实，且该文件的双写者 WP4/TP2 没有 Merge Owner / Merge Order / Re-verify 登记），要么 WP4 的迁移测试无处可写。因两包分处 W3 与 W5（不同批），**不构成「同一处却同批」**，故按 MINOR 报告 | 二选一并在 `plan.md:266` 写清：把该行改为登记 `crates/*/tests/**` 中会被 WP 触碰的文件（如 `crates/storage-sqlite/tests/migration.rs`：Writers=WP4,TP2；Merge Owner=TP2；Order=WP4 → TP2；Re-verify=TP2: PV1），或规定 WP1–WP6 的迁移/契约测试只写在各自 `src/**` 的 `#[cfg(test)]` 模块并给出示例路径 | 不适用（本轮新发现） |
SAFE-NOTE：除上表外，未发现其它与派发 Scope 相关的具体问题；`@design-rev-2` 这类 Contract Freeze 版本标识经 `openspec/schemas/agentic/templates/plan.md:69` 与 `procedures/workflow-check.md:68` 确认属「人读标识、不做机械校验」，不作为发现。
## Check Plan Reconciliation
- PV1/PV2 本轮**未执行**（规划审查 + 只读边界），只核对「在各 WP 声明的分支基线上能否产出」：**WP2 的 PV1 不可产出**（DR1-F9，workspace 测试被 `identity-auth` 镜像与既有计数断言拦住）；**WP1 的 PV2 不可产出**（DR1-F10，`check:acp` 的 ajv 校验被 `broker: native` 拦住）；PV2 的其余断言点在本轮修订后均可产出（封闭词表五处、漂移门禁、依赖方向、固定向量、文档引用、agentic 门禁的写目标都已在 WP1–WP6 范围内）；PV1 的其余 crate 依赖面已核对无跨 crate 编译阻断（`UnavailableKind` 与 `CommandName` 两个新枚举取值都不会打破 server/app 的 match 与映射）。
- 未执行项与门禁归属：PV1/PV2 应在 3.1（分支）、6.3（候选）、6.7 与 8.1/8.2（主分支）由对应执行者补齐；本轮结论**不依赖**这些未执行结果，但对 F9/F10 的修复必须由修复者在其分支上实跑 PV1/PV2 才能闭环。
- Main E2E 为 `not-applicable`（`plan.md` `### Main E2E`，用户 2026-09-30 批准原话已记录），本轮不涉及 E2E 用例与配置，不做用例判据表核对。
- 未核对项：`contractDigest` 的取值本身（无引擎调用能力）；proposal/specs 的字节级未改动（无可 diff 基线，见独立判断 3）；`verification.md` 的 `## Worktree Handoff` 中 TP2 行「待 provisioner 补发」（plan 本身已声明该前置，属 main/provisioner 的执行事务，不在本轮 finding）。
## Assessment
**结论：FAIL**（对应 Target Revision = `sha256:17460283ef4a8b1dcf2f3c5699affc511d66e1c0607f7171ec603f30dd619d5c`）。
依据 `roles/reviewer.md`：存在已确认且未解决的 MAJOR 即 FAIL。Round 1 的问题面已基本闭环——**DR1-F1/F2/F3/F4 复核为已解决**（封闭词表原子点合并为单一 WP2 且五处 gate 断言点全覆盖；D6 的「按 transport」规则经脚本与 `commands.json` 逐条验证确实同时放行 `session.create` 与 `session.resume`；SYNC §11.5 + SECURITY §10.2 已登记为单写者并含计数文本；TP1/TP2 拆分后每行检查都可产出），**DR1-F5/F6/F7/F8 亦全部解决**（D1 入冻结、文档区域改为 §3.1/§5.2 与 §7/§7.2/§7.3、WP5 显式 `code:WP1`、scout 路径改为 `.md`）。
本轮新确认的 MAJOR：
- **DR1-F9**：封闭词表原子点仍有第七个强制写目标 `crates/identity-auth/src/authorization.rs`（`GRANTS` 镜像）+ 其既有验收测试 `crates/identity-auth/tests/authorization.rs`，由 `cargo test --workspace`（WP2 的 PV1）逐项断言 `commands.json` 的 grant 会员与命令全集；该 crate 不在任何 WP 的 Write Scope，也不在 Shared File Ownership——与 Round 1 判定 MAJOR 的 F1/F3 属同一类「登记缺失/写范围不实」。
- **DR1-F10**：`design.md:124` 冻结的 `broker: native` 不在 `schemas/acp/compatibility-matrix.schema.json` 的枚举内（`check:acp` 用 ajv 强校验），而 `schemas/acp/` 不在任何 WP 写范围——与 Round 1 的 F2 同类「冻结契约在现行门禁下不可实现且修补路径未登记」。
非阻断项：DR1-F11（`docs/NODE_LINK_PROTOCOL.md` §10 grant 表未列入同步清单，门禁不覆盖）、DR1-F12（`plan.md:266` 的「TP2 独占测试目录」与 WP 整 crate 写范围/任务 2.4 冲突，两包不同批）。两者不单独构成 FAIL 理由，但建议与 F9/F10 在同一轮修订里一并处理，避免第三次返工。
已核实为**合规**、不构成反对意见的部分：Execution Waves 的层级算式（W1–W5）与 Enter Condition 逐行一致、Serialization Reason 取值合法；三对已登记双写者（`broker.rs` WP2/WP3、`README.md` WP2/WP6、`CORE_PORTS_AND_STORAGE.md` WP3/WP4）均分处不同批且 Merge Owner/Order/Re-verify 齐备、区域不相交；`UnavailableKind` 新取值与 `CommandName` 新变体不会打破其它 crate 的编译与映射；Runtime Resources 的 Exclusive Scheduling 有依据；Coverage Index 37 行的 source 标题与 tasks 引用全部实存。
本报告不更新任何任务状态，也不代表整个变更可归档；修复后必须由**新的独立 reviewer 在同一 Review ID（DR1）下以新的 round** 针对新的 `contractDigest` 复核 F9/F10（含 F11/F12 的处置）。`verification.md` 的 `## Dependency Declaration Review` 第 2 行（Round 2）与 `## Review Findings` 的复核列由 main 按本轮结果回填。
```yaml
handoff_index: [{task_id: NOT_APPLICABLE, work_package: NOT_APPLICABLE, role: reviewer, phase: plan, stage: plan, round: 2, target_revision: "sha256:17460283ef4a8b1dcf2f3c5699affc511d66e1c0607f7171ec603f30dd619d5c", evidence_type: REVIEW, evidence_id: DR1, report_path: openspec/changes/session-resume/reports/dr1-dependency-review-round2.md, result: FAIL, evidence_status: NEW, applicability_basis: "针对重排后 plan.md 的当前 contractDigest 做静态规划审查；未执行 PV1/PV2、未针对代码 diff，Base Revision 为 NOT_APPLICABLE", source_evidence: NOT_APPLICABLE}]
```
```acceptance-report
{
  "criteriaSatisfied": [
    {
      "id": "criterion-1",
      "status": "satisfied",
      "evidence": "完整报告已返回：Review Context（逐项 Inputs 实际取值）、DR1-F1…F8 逐项复核结论与依据、4 项独立判断（Waves 层级逐行算式、双写者/同批核查、Contract Changes 断言、Coverage Index 37 行 tasks 引用）、新发现 DR1-F9/F10（MAJOR）与 DR1-F11/F12（MINOR）、Check Plan Reconciliation、Assessment=FAIL、单行 handoff_index"
    }
  ],
  "changedFiles": [],
  "testsAddedOrUpdated": [],
  "commandsRun": [],
  "validationOutput": [
    "DR1-F1/F2/F3/F4 复核=已解决：check-command-catalog.mjs:194,204,210,241,245,254 与 commands.json/broker.rs/SYNC §11.5/SECURITY §10.2 逐条对齐；WP2 单包覆盖原子点，WP3 不动 required_grant（crates/core/src/broker.rs:94-106 vs :119-136），broker.rs 双写者已登记（plan.md:253）",
    "DR1-F4 复核=已解决：TP1(W1,none,NOT_APPLICABLE) 与 TP2(code:WP1…WP6,W5,max(4)+1) 与 tasks.md:27-28,44 及 Coverage 的 4.2 引用一致",
    "DR1-F5…F8 复核=已解决：WP3 冻结含 design@D1 且写范围 §2/§3.1/§5.1/§5.2（plan.md:227,254）；WP4 写范围 §7 标题/§7.2/§7.3（plan.md:228,254）；WP5 code:WP1+code:WP3 且有 Handoff 行（plan.md:229,274）；scout 路径为 reports/scout-target.md（plan.md:291，文件实存）",
    "新 DR1-F9（MAJOR）：crates/identity-auth/src/authorization.rs:71 的 GRANTS 镜像与 tests/authorization.rs:15-19,46-61,99-121,258-270,306-317 由 commands.json 驱动断言；Cargo.toml:19 为 workspace 成员 → WP2 的 PV1（plan.md:308 cargo test --workspace）必红；crates/identity-auth/ 不在 plan.md:226-232 任何写范围、无 Shared File Ownership 行",
    "新 DR1-F10（MAJOR）：design.md:124 冻结 broker: native，schemas/acp/compatibility-matrix.schema.json:52-62 的 broker 枚举无 native 且 additionalProperties:false，scripts/check-acp-compatibility.mjs:14,43-58 以 ajv 强校验 → WP1 的 PV2（含 check:acp）必红；schemas/acp/ 不在 plan.md:225 写范围",
    "37 行 Coverage 的 source 标题与 5 份 specs 的 37 个标题一一对应实存；tasks 引用集合 {2.1…2.6,4.2} 全部真实存在"
  ],
  "residualRisks": [
    "无法重算 contractDigest（无 OpenSpec 引擎调用能力），按派发值绑定",
    "change 目录在 git 中未跟踪，无法 diff 证明 proposal/specs 字节级未变；本报告只能给出一致性核对结论（无法完全确认）",
    "本轮未执行 PV1/PV2，也未运行任何命令（只读边界）；F9/F10 的修复必须由修复者在分支上实跑 PV1/PV2 才能闭环",
    "F9 的修复取值为契约判断（identity-auth 是否属于封闭词表原子点、IDENTITY_AND_AUTH_CONTRACT.md 是否需说明），F10 的修复需要在「改 D5 取值」与「扩 schema 枚举」之间做契约选择，均可能属需用户/主 Agent 决策的范围"
  ],
  "noStagedFiles": true,
  "diffSummary": "无 diff：本轮为只读规划审查，未修改、未暂存任何仓库文件，未切换分支、未提交",
  "reviewFindings": [
    "blocker: crates/identity-auth/src/authorization.rs:71 + crates/identity-auth/tests/authorization.rs:46-61,99-121,258-270,306-317 - GRANTS 镜像与其断言未在 WP2 写范围/Shared File Ownership，WP2 声明的 PV1 不可产出（DR1-F9, MAJOR）",
    "blocker: design.md:124 - 冻结的 broker: native 违反 schemas/acp/compatibility-matrix.schema.json:52-62 的枚举，WP1 声明的 PV2 不可产出且修补路径未登记（DR1-F10, MAJOR）",
    "non-blocking: tasks.md:20 / design.md:132 - docs/NODE_LINK_PROTOCOL.md §10 的 grant→命令表（:409）不在同步清单，交付后该文件自相矛盾且无门禁覆盖（DR1-F11, MINOR）",
    "non-blocking: plan.md:266 - 「TP2 独占 crates/**/tests/」与 WP1–WP6 整 crate 写范围及 tasks.md:22 冲突（DR1-F12, MINOR）"
  ],
  "manualNotes": "Round 1 的 4×MAJOR 与 3×MINOR+1×SUGGESTION 已全部复核为已解决；本轮 FAIL 完全来自新发现的 DR1-F9、DR1-F10（两者与 Round 1 的 F2 同类：「冻结契约 vs 门禁 + 修补写目标未登记」），修复后需以新 round 复核。main 需把本报告持久化到 openspec/changes/session-resume/reports/dr1-dependency-review-round2.md，并在 verification.md 的 ## Dependency Declaration Review 第 2 行填入 Plan Revision = sha256:17460283…d5c 与 result=FAIL，在 ## Review Findings 的 F1–F8 复核列写「Round 2 已解决」、新增 F9/F10/F11/F12 行。commandsRun 为空是本轮无 shell/测试执行能力所致（规划审查边界），非跳过检查。"
}
```
