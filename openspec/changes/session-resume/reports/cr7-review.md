> 由主 Agent 从 reviewer 子 Agent 的返回结果原样持久化（reviewer 工具集只读、无 write）；内容未改写。
> 检视线程：CR7 / TP1 / Round 1；run: 594976c8-c893-48e6-b261-203c28631e66

# CR7 独立检视报告（Review Type: test-case / Review ID: CR7 / Round 1）

## Review Context

| Inputs 项 | 实际取值 |
| --- | --- |
| Review ID / Round | CR7 / 1（首轮，无 Previous Findings） |
| Review Type | test-case（按 `roles/reviewer.md` 的「test-case 判据表（编写阶段）」逐项核对） |
| Review Stage | work-package（TP1 **用例编写/设计**阶段；**不要求**执行材料） |
| Work Package | TP1（tasks 2.7 / 4.1） |
| Repository | `D:\Project\acp-remote`（只读） |
| Base Revision | NOT_APPLICABLE（本阶段无代码改动） |
| Target Revision | `sha256:449bd7b35…fbfd`（TP1 设计报告内容摘要）——与 `openspec/changes/session-resume/verification.md:35` 的 TP1 行、`## Dispatch Reconciliation` 的 TP1 行一致，可交叉核对 |
| 实际检视对象 | `openspec/changes/session-resume/reports/tp1-test-design.md`（614 行，全文读取） |
| 读取的规则 | `openspec/schemas/agentic/roles/reviewer.md`、`roles/_shared/role-report.md`、`AGENTS.md`（§3/§4/§5/§7/§10） |
| 读取的需求/契约（逐字） | `proposal.md`；`design.md` D1–D6（含 `## Risks / Trade-offs`）；`plan.md`（Coverage Index 37 行、Work Packages、Execution Waves、Shared File Ownership、Dependency Handoffs、Runtime Resources、Verification Strategy、Completion Criteria）；`tasks.md`（1.5–4.2）；5 份增量 specs 全文；`verification.md`（Target / Handoff Index / Checks / Check Plan Changes / Dispatch Reconciliation） |
| 实际检查范围 | ①58 个用例 ID 的计数/唯一性；②逐 R 行映射与 Coverage Index 的 `source.heading`/`tasks` 一致性；③入口真实性；④前置数据与资源可满足性（对基线代码逐个符号核实）；⑤断言可观察性；⑥每个 Requirement 的异常/边界路径；⑦有无 skip/todo；⑧资源登记；⑨设计自洽与引用真实存在 |
| 验证证据 | **无执行证据**（设计阶段，plan.md 的 TP1 行 = `NOT_APPLICABLE`；报告 §6 明确声明未编译、未执行）。本轮只读静态检视 + 对基线提交 `81e350f…` 的只读符号/夹具核实 |
| 限制（如实声明） | 我**没有 shell/哈希工具**：无法用 `git --no-pager show/diff` 读提交范围（本阶段 `Base Revision = NOT_APPLICABLE`，确实无 diff 可读），也无法自行复算 `sha256:449bd7b3…`；我用 `watchdog_diff` 确认工作区**无任何被跟踪文件的暂存/未暂存改动**，change 目录整体为 untracked（因此该报告的「目标版本」是磁盘内容摘要，不是提交）。以上限制不影响本轮的静态判据判定 |
| 本轮报告路径 | 本报告（由 main 原样持久化） |
| 核对的 Check ID | Check Plan = plan.md 的 `## Coverage Index`（37 行）与 `## Verification Strategy`；TP1 **不跑** PV1/PV2（plan.md TP1 行 NOT_APPLICABLE）→ 本报告不核对任何 PV 证据，也不把静态检视当作 PV/E2E 结果 |

**已确认通过（无问题）的核对项**

1. **ID 稳定唯一**：58 个 `SR-Rn-m` 无重复（逐行计数 2+1+2+1+2+1+2+2+1+2+1+2+2+1+1+1+2+1+1+1+2+1+3+2+2+2+3+1+2+1+2+1+1+2+1+2+1 = 58，与报告 §2 统计一致）；`SR-` 前缀与既有 ID 体系（`R1–R37`/`PV1/PV2`/`C1/C2`/`DR1`/`CR1–CR8`）不冲突；§2 映射表、§2.1 Ownership 表、§3.1–§3.5 详情标题三处的 ID 集合逐个吻合（含 `SR-R26-1`(app)/`SR-R26-2`(agent-host)、`SR-R7-1`(agent-host)/`SR-R7-2`(app) 的分层拆分），未出现「同一 ID 两处定义不同用例」。
2. **需求映射**：Coverage Index 的 37 行 `source.heading` 我逐条回读 5 份 specs，**全部实存**且与 `path` 对应（acp-wire-protocol R1–R4、local-agent-host R5–R12、storage-schema-v2-migration R13–R21、workspace-resolution R23–R26、node-link-owner-server R27–R37）；37/37 行都至少引用一个 ID，且 `tasks` 编号在 tasks.md 中实存。
3. **入口真实**：设计未用 API/mock 绕过被测入口——agent-host 走真实 stdio 子进程（`CARGO_BIN_EXE_acpr-fake-acp-agent`）、storage 走真实 SQLite 文件与真实 migration、Owner 侧走真实 loopback WSS/握手 + 真实 `identity_auth::Authority`。我核到 `crates/app/tests/support/owner.rs:7` 的注释与 `:187/:190` 显示既有 e2e 就是「用组合根的同一批装配点（`app::daemon::{net_config, register_node_link_paths}`）」——与报告 §5.1 的声明一致，不是新造的旁路。
4. **前置可满足（逐符号核实，均在基线 `81e350f…` 实存）**：storage `support::{temp_dir:92, raw_write_pool:144, table_snapshot:157, column_specs:238, copy_fixture_from:119}`、`fixtures/storage/v2/{empty,from-v1}.sqlite3`、`migration.rs:782 v3_database_upgrades_to_v4_by_appending_the_export_id_column_only`（其 v4 形状构造手法与报告 §3.3 描述逐字同源：`ALTER TABLE … DROP COLUMN` + `PRAGMA user_version` + `meta` 回写 + `wal_checkpoint(TRUNCATE)`）；agent-host `support::{FAKE_AGENT:23, TempFile:30, profile_with:298}`；app `support::{TempRoot:67, OwnerNode:56, data_dir:271, fail_commits_with:281, rejected_commits:65, ScriptedBackends:499}`、`support/nodelink.rs:{IO_TIMEOUT:32, MESSAGE_TIMEOUT:35, NodeLinkClient:415}`；acp `methods::{status_of:285, METHODS, wire_name/delivery/implemented}`、`tests/raw_fidelity.rs:14`、`tests/matrix_tables.rs`；core `use_cases.rs:512 recover_unsettled` + `broker.rs:2329` + `Actor::LocalCli`（`use_cases.rs:2048`）+ `ports.rs:38 ReplayLimit`；`core/src/model/tests.rs:2208` 的 `UnavailableKind::ALL.len() == 7`（= 报告 R-3 的断言对象，准确）、`server/src/local_admin/params.rs:1562` 的 `ALL` 遍历。拟新增的 5 个用例文件均无同名冲突（`crates/{acp-protocol,agent-host,storage-sqlite,app,node-link-protocol}/tests/` 现存文件已逐一比对）。
5. **断言可观察**：断言绑定错误码/返回类型/`quote()` 与 `sqlite_master` 字节/`--dump-requests` 文件行/心跳文件是否增长/后端派发计数，并在 §5.4 明确排除日志文本与私有字段；未见「仅截图」式断言（唯一的弱化点见 CR7-F4、CR7-F7）。
6. **正常 + 异常**：8 个 `### Requirement` 行（R1/R5/R8/R13/R21/R23/R27/R34）都各有 ≥1 条异常或边界用例（`SR-R1-2`、`SR-R5-2`、`SR-R8-2`、`SR-R13-2`、`SR-R21-2`、`SR-R23-3`、`SR-R27-2`、`SR-R34-2`）。
7. **无跳过**：全文无 TODO/FIXME/`#[ignore]`/skip 标记；平台受限用例（`SR-R24-2`、`SR-R25-2`）写的是 `#[cfg(unix)]` 而非 `#[ignore]`，因此 Linux CI runner 会真的跑到（AGENTS.md §10 已说明 Linux 额外执行 `#[cfg(unix)]` 路径）。**要求 TP2 保持 `#[cfg(unix)]` 而非 `#[ignore]`**。
8. **不因「未编译/未执行」判 FAIL**：plan.md 的 TP1 行写 `NOT_APPLICABLE`（设计阶段不跑 PV），报告 §6 也逐条声明未写 `crates/**`、无 PV1/PV2 证据、并列出「TP2 才可编译的未实现 API」清单——符合编写阶段判据。

---

## Findings

| ID | Severity | Location（target 版本） | Trigger / Evidence | Impact | Recommendation | Recheck |
| --- | --- | --- | --- | --- | --- | --- |
| CR7-F1 | **MAJOR** | `reports/tp1-test-design.md:223-231`（SR-R10-1）、`:509`（关键路径①）、`:569`（Q2）；冲突对端：`specs/local-agent-host/spec.md:23`（需求正文 `未宣告时 MUST 返回明确的不支持错误，MUST NOT 启动 Agent 进程`）与 `:33`（Scenario R10 THEN `不启动任何 Agent 子进程`）vs `design.md:118`（D4：`门控必须发生在 spawn 之后、session/resume 之前`）与 `design.md:166`（Risks 把口径降级为「不发送 + 清理进程」） | 能力宣告只能经 `initialize` 获得，而 `initialize` 必须已 spawn；因此 spec 的 `MUST NOT 启动 Agent 进程` 在 D4 下**不可满足**。TP1 按 D4 口径断言（无 `session/resume` 行 + 心跳停止 + 无新绑定）。判据「需求映射 / 断言可观察」在此失效：同一用例在两份冻结合同下要求相反的可观察结果 | TP2 将固化的验收基准与 spec 文字相反：一个按 D4 正确实现的版本会被「字面读法」判为违反 R10；R10 的证据无法被 validator 按 spec 判 PASS/FAIL。**Q2 裁决：这是规格与设计的真实冲突（不是措辞瑕疵）**——design 的风险注记不能替代 spec 修改 | 由 main 择一并同批同步（推荐改 spec，保留 D4）：把 `specs/local-agent-host/spec.md:23` 与该 Scenario 的 THEN 改为「未宣告时 MUST NOT 发送 `session/resume`，MUST 在返回前清理已拉起的子进程、不改会话状态、不降级为新建会话」，并同步 `design.md` D4/风险条目与 `docs/SESSION_CONTINUITY_DESIGN.md` 措辞；若改 D4，则 SR-R10-1/2 的断言语义需重写为「完全不 spawn」（当前不可实现）。改后需刷新 `verification.md` 的 `## Target` spec 摘要基线 | 未闭环（Fail） |
| CR7-F2 | **MAJOR** | `reports/tp1-test-design.md:202`（SR-R7-2）、`:328`（SR-R21-2）、`:512`（关键路径④）；冲突对端：`specs/workspace-resolution/spec.md:7`（`校验失败（…持久化取值为 NULL…）时 MUST 返回服务端不可用类错误`）vs `design.md:105`（`该会话没有持久化恢复数据（两列为 NULL）→ 与能力不支持同一条路径（nodelink.command.unsupported）`）、`design.md:94`（`任一恢复字段为 NULL → 显式不支持/不可用`） | 同一个输入（两列为 NULL）在两份冻结合同里被指派到两个互斥错误码：`nodelink.internal.unavailable`（R23 文字）vs `nodelink.command.unsupported`（D3 + TP1 三处断言）。两者都是既有登记码、可区分，因此不是「同名同义」，而是真实分歧。技术上两侧都可行：`crates/core/src/broker.rs:3050 port_error_public` 对 `PortError::Unavailable(_)` 已默认给 `internal.unavailable`（走 R23 侧无需新臂），而走 D3 侧需要为新 `UnavailableKind` 取值显式加 `command.unsupported` 臂（`crates/server/src/node_link/command.rs:2012` 已有 `"command.unsupported" → nodelink.command.unsupported` 的映射，链路可实现） | TP2 会把与 R23 文字相反的错误码写成断言（含 agent-host 侧 R7 的 NULL 场景）；且「R19 里 node-link 层表现为…」的依据编号写错（应为 R21），使该断言的合同来源不可追溯 | 由 main 二选一并同步全部合同：①（推荐）把 `specs/workspace-resolution/spec.md:7` 的「持久化取值为 `NULL`」从句改为「与能力不支持同一条路径（`nodelink.command.unsupported`）」，保留 D3；②或把 `design.md:105` 改回不可用类，并核对 R13/R16/R21/R37 的措辞。裁决后同步修订 TP1 的上述三处，再交新 reviewer 复核 | 未闭环（Fail） |
| CR7-F3 | MINOR | `reports/tp1-test-design.md:227`、`:569`（两处把口径来源写成 `plan.md` 的 `## Risks / Trade-offs` 第 2 条）；`:328`（把依据写成「R19」） | `plan.md` 全文没有 `## Risks / Trade-offs` 节（其节为 Scope and Contracts / Contract Changes / Coverage Index / Work Packages / Execution Waves / Shared File Ownership / Dependency Handoffs / Runtime Resources / Target / Merge Strategy / Verification Strategy / Main E2E / Independent Validation / E2E Execution Plan / Failure and Recovery / Completion Criteria）；该节实际在 `design.md:162`。R19 的标题是「v3 到 v4 升级给既有节点行写空清单」，与 NULL 恢复数据无关 | 引用不实会让后续读者按错误路径取证据；也弱化了 F1 的实质（缓解措施在 design 的风险注记里，不在 specs 里） | 改为 `design.md` 的 `## Risks / Trade-offs` 第 2 条与正确 R 编号（R21/R13） | 未闭环（非阻断） |
| CR7-F4 | MINOR | `reports/tp1-test-design.md:279-287`（SR-R15-1）、`:570`（Q3） | 第 2 条注入（对 v4 形状注入同名列 `ALTER TABLE owned_session ADD COLUMN agent_session_id INTEGER`）的断言是「不变量式」：`user_version != 5 ⇒ 两列都不存在或都不被 v5 段创建；user_version == 5 ⇒ 两列规格等于新建库`。若 WP4 按列存在性守卫实现，注入**不会导致失败**，该子断言恒真（空转），无法证明「事务中途失败整体回滚」 | 该用例对 v5 段的证据强度低于其名义（可能静默空转），但不会造成错误结论 | **Q3 裁决：不影响用例设计成立性**——`specs/storage-schema-v2-migration/spec.md:14` 的 Scenario 明确允许「（或更早版本连续升级）」，报告的第 1 条注入（`fixtures/storage/v2/from-v1.sqlite3` + 冲突对象）是确定性的。建议 TP2 二选一：把 v5 段注入改为必然失败（预建与 v5 段要创建的同名对象）或显式标注为 best-effort，并把回滚判定交给第 1 条注入 | 未闭环（非阻断） |
| CR7-F5 | MINOR | `reports/tp1-test-design.md:568`（Q1）；`plan.md:250`（WP5 写范围）、`tasks.md` 2.5；`crates/agent-host/src/bin/acpr-fake-acp-agent.rs`（现有场景表在 `:380/:595/:648`，`--heartbeat-file/--capabilities/--scenario` 已有，**无** `session/resume` 分支、**无** `--dump-requests`） | **Q1 裁决：不是阻断项，归属 WP5。** `crates/agent-host/src/bin/acpr-fake-acp-agent.rs` 落在 `plan.md:250` WP5 声明的整 crate 写范围 `crates/agent-host/` 内，不存在「无主文件」或「双写者」，TP2 也**不需要**写它（且 `TP2: code:WP1…WP6` 已声明依赖）。但设计依赖的 fake child 测试支撑面（`session/resume` 分支、`resume-ok`/`resume-error`、`SR-R7-1` 需要的 `session-new-error`、`--dump-requests` 的「每次收到请求追加一行 method」约定）在 WP5 行与 tasks 2.5 的完成条件里都**没有写成义务** | `SR-R10-1`（关键路径①）、`SR-R8-1`、`SR-R11-1`、`SR-R12-x` 的核心可观察证据依赖「发过/没发过 `session/resume`」的文件级痕迹；若 WP5 未按本设计的场景名与 `--dump-requests` 约定实现，TP2 会「按图施工却无据可写」 | main 在 tasks 2.5（或 plan.md WP5 行的完成条件）补一行显式义务：`acpr-fake-acp-agent` 必须支持 `session/resume` 分支、`resume-ok`/`resume-error`/`session-new-error` 场景与 `--dump-requests <path>` 的 method 逐行追加；可加一句「供 TP2 复用」 | 未闭环（非阻断） |
| CR7-F6 | SUGGESTION | `reports/tp1-test-design.md:438-445`（SR-R32-1）、`:572`（Q5）；`crates/app/src/daemon.rs:1178`、`crates/core/src/use_cases.rs:512` | **Q5 裁决：in-process 调用足够，无需真实进程重启。** 报告调用的 `recover_unsettled(&Actor::LocalCli, ReplayLimit::new(16))` 正是组合根启动时调用的同一入口（`daemon.rs:1178`），且终态经 `command.status` 从持久化行读回，因此「崩溃窗口 → uncertain」的可观察性成立 | 唯一未直接覆盖的维度是「不确定性只存在于内存」——但该用例的读回路径已经是持久化行，风险低 | 建议 TP2 用「重新打开同一 `data_dir` 的 store / 重组 `OwnerNode`（`support/owner.rs` 已有 `data_dir()`）后再 `command.status`」把该维度显式化；真实进程级重启记为可选强化（E2E 已记 not-applicable，不必需） | 不适用（建议） |
| CR7-F7 | SUGGESTION | `reports/tp1-test-design.md:242-252`（SR-R12-1 前置） | 该用例的备选口径写「同一文件 + 断言最终只有一个写入者存活」：同一心跳文件由两个子进程追加字节，**无法**区分写入者，该备选不可观察（主口径「每进程一个心跳文件路径」是可观察的） | 若 TP2 采用备选口径，会得到恒真的弱断言 | 固定为「每进程独立心跳文件路径」，或删除该备选；`--dump-requests` 的 `session/prompt` 行数上限断言可保留（共享文件的追加计数仍可观察） | 不适用（建议） |
| CR7-F8 | MINOR | `reports/tp1-test-design.md:530`（`127.0.0.1:0`）；`plan.md:313-319`（`## Runtime Resources` 只有 Rust 构建缓存/tempdir、SQLite 临时库、`npm run check` 三行） | 判据「资源登记」：app 层用例需要真实 loopback listener（内核分配临时端口）。报告在 §5.2 写了构造方式，但 plan.md 的资源表**没有端口/监听器一行**（也没有「为何不入表」的说明）；W1（WP1/WP2/TP1）与 W3（WP4/WP5）等窗口确实会并行跑各自 crate 的测试 | 登记缺失而非真实争用（临时端口 + 进程隔离，实际无互斥需求），因此不影响用例设计与执行，但违反「所需资源已在资源表登记」的判据表面要求 | main 在 plan.md 的 `## Runtime Resources` 补一行（Isolation = 内核分配临时端口、各 worktree 无共享；Exclusive Scheduling = NOT_APPLICABLE），或明确记录「无争用故不入表」的理由 | 未闭环（非阻断） |

---

## Assessment

**结论：FAIL**（对应 Target Revision `sha256:449bd7b3…`，绑 TP1 设计报告内容摘要在 `verification.md:35` 的记录）。

- 存在 **2 项已确认且未解决的 MAJOR**（CR7-F1、CR7-F2），均为**合同级冲突**（specs ↔ design），且冲突点正好落在 TP1 用例的断言口径上：`SR-R10-1/2`（能力未宣告）与 `SR-R7-2/SR-R21-2`（NULL 恢复数据）。按 `roles/reviewer.md` 的判定规则，这构成 FAIL，且**不是**用例作者可自行修复的问题——责任人分工为：**main 同步 specs/design（必要时刷新 `verification.md` 的契约 sha256 基线并标记受影响证据 INVALID）→ 再修订 TP1 受影响用例 → 由新的独立 reviewer 复核（CR7 Round 2）**。
- 除此之外，TP1 的设计产物**质量足够、可继续**：58 个稳定 ID 无重复、37/37 需求映射实存、入口真实（未用 API/mock 绕过被测入口）、前置数据全部落在计划资源与基线既有设施内（逐符号已核实）、8 个 Requirement 行都有异常/边界路径、无 skip/todo、断言以可观察结果为主。因此 F1/F2 的修复是**外科式对齐**（改一两句 spec 措辞 + 改 3–5 处断言期望值），不需要重做设计。
- **不因「未编译/未执行」判 FAIL**：本阶段判据表明确不要求执行材料，plan.md 的 TP1 行也是 `NOT_APPLICABLE`；报告 §6 逐条声明未写 `crates/**`、无 PV1/PV2 证据、未声称任何用例通过——声明与实际（`watchdog_diff` 显示工作区无被跟踪文件的改动）一致。
- **报告自报的覆盖盲区 R-1（新增 `UnavailableKind` 取值进入公开错误映射无独立 R 行）：不构成 TP1 缺陷，也不影响本轮判定。** 该映射的效果已在端到端层面可观察：`SR-R10-1` 断言 core 侧「后端不支持」与「服务端不可用」取值可区分，`SR-R37-1` 断言 node-link 终态码 `nodelink.command.unsupported`；§3.6 已把它登记为 WP3 内联项。是否需要独立 R 行属 Coverage Index 调整，由 main/validator 决定（我按职责不替代判定覆盖完整性）。附带核实：`crates/server/src/local_admin/params.rs:914 map_port_error` 对 `Unavailable(_)` 是通配臂，故报告 R-3「params.rs:1562 不破编译、自动通过」的判断**正确**；而 `crates/core/src/broker.rs:3050 port_error_public` 对 `UnavailableKind` 是**逐值穷尽**（无通配臂），因此新取值必然打破编译、必须由 WP3 显式加臂——报告 §3.6 把该点列入 WP3 写范围与 R-3 的涟漪提示也**正确**。
- **Check Plan 核对**：本轮计划内检查 = plan.md 的 `## Coverage Index`（37 行）与 `## Verification Strategy`；我逐行核对了 Coverage Index 的 `source.heading`/`tasks` 与 TP1 的映射表，未发现计划与产出之间的映射缺失（heading 37/37 实存；`tasks` 编号实存）。**不含任何待补执行证据**：TP1 计划内无 PV/E2E（`NOT_APPLICABLE`），因此不存在「待返回证据影响本轮判断」的情形；但请注意 **code review PASS 不等于任何 PV/E2E 通过**，本报告不涉及也不覆盖 PV1/PV2/C1/C2。
- **契约变更的下游影响（供 main 决策）**：若按 F1/F2 修改 specs，`verification.md` 的 `## Target` 6 行 sha256 基线（`proposal.md` + 5 份 specs）将失效，必须同批刷新；受影响的既有证据（DR1 的 spec 一致性核对、TP1 本轮的 37 行 heading 核对、以及本报告 F1/F2 相关的断言）需按 `roles/_shared/role-report.md` 的规则标记或重评。另附一条非阻断观察：`specs/acp-wire-protocol/spec.md` 的标题写作「圆形保真」（应为「往返保真」），TP1 按原文引用因此映射无误，若 main 本轮改 spec，可顺手订正。
- 未涉及/未验证（明确边界）：我没有也无法执行任何构建或测试；未做 E2E；未判定「覆盖是否完整/风险盲区」（交 main 的 Coverage Index 与 validator）；未核对其他 WP（WP1/WP2 的 PV1/PV2 证据、WP3–WP6 的实现）——这些不在 CR7 范围。

### handoff_index

```yaml
handoff_index:
  - task_id: "3.8"
    work_package: TP1
    role: reviewer
    phase: test-case
    round: 1
    stage: work-package
    target_revision: "sha256:449bd7b35015eaf6bcf8be6b7ca35d214aa1b002cb4535310d2d685a675ffbfd"
    evidence_type: REVIEW
    evidence_id: CR7
    report_path: openspec/changes/session-resume/reports/tp1-test-design.md
    result: FAIL
    evidence_status: NEW
    applicability_basis: "只读静态检视 TP1 设计报告（磁盘内容，change 目录未跟踪，watchdog_diff 确认工作区无被跟踪文件改动）；判据为 roles/reviewer.md 的 test-case 编写阶段判据表；2×MAJOR 为 specs↔design 合同冲突（CR7-F1：local-agent-host spec MUST NOT spawn vs design D4 门控在 spawn 之后；CR7-F2：workspace-resolution spec 把 NULL 归入不可用类 vs design D3 归入 nodelink.command.unsupported），其余判据（ID 唯一/需求映射/入口真实/前置可满足/断言可观察/异常路径/无跳过）已逐项核对通过"
    source_evidence: NOT_APPLICABLE
```

单行格式：`role: reviewer, phase: test-case, stage: work-package, round: 1, target_revision: sha256:449bd7b35015eaf6bcf8be6b7ca35d214aa1b002cb4535310d2d685a675ffbfd, evidence_type: REVIEW, evidence_id: CR7, result: FAIL, evidence_status: NEW`