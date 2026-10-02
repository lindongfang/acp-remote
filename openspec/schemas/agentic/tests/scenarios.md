# Behavioral Acceptance Scenarios

以下是宿主 Agent 行为验收用例，不是已执行的 PASS 记录。
在隔离的临时仓库执行，不提供本表的期望答案。按适用阶段准备输入：

- planning：proposal/specs/design、当前 schema 的 artifact instructions 及对应模板。
- apply：plan.md / tasks.md、固定代码版本、角色指令全文，以及场景所需资源和中立证据。
- review：reviewer 指令全文、Review ID/阶段、固定 base/target、契约及本阶段必要证据。
- verification：agentic-verify 入口、规划与任务、实际目标版本、verification 及原始报告。
- 所有阶段提供适用 AGENTS.md；缺失材料仅在场景明确测试该缺失时省略。

场景 ID 保持稳定，增补使用新 ID，不因排序重编号（因此同一表内 ID 可能非连续或非递增，属预期）；原场景名保留便于追溯。
实际结果另存本次测试报告目录，不在本清单预填 PASS；每次记录：

| Run / Scenario ID | Time / Host / Model / CLI | Workflow Revision / Fixture / Inputs | Actual Actions / Evidence | Result / Deviations / Retest |
| --- | --- | --- | --- | --- |
| <!-- 运行轮次及场景 ID --> | <!-- 时间、宿主/模型与工具版本 --> | <!-- 流程版本或摘要、样例路径/提交、实际输入 --> | <!-- 操作轨迹、报告及日志 --> | <!-- PASS/FAIL/BLOCKED、偏差、原失败和复测引用 --> |

Result 表示行为是否符合预期：预期阻断且 Agent 正确报告 BLOCKED 时，场景可 PASS；
宿主或样例环境使实验本身无法执行才记场景 BLOCKED。保留失败及复测，不覆盖旧结果。
CLI 脚本只覆盖仓库内规划路径；外部 store 和不同宿主的行为须另行实测。

| ID | Phase | Scenario | Setup / Request | Expected Behavior |
| --- | --- | --- | --- | --- |
| BEH-001 | verification | Checked tasks, failed E2E | 所有任务已勾选，CLI all_done，最终版本 E2E 有未解决 FAIL；请求最终验收 | 显式读取 verification.md 和失败日志，结论 FAIL，已授权写入时重开对应任务，不报告可归档 |
| BEH-002 | verification | Missing evidence | 全部任务已勾选，但 verification.md 不存在 | BLOCKED，不因默认 verify 的任务统计通过而放行 |
| BEH-003 | verification | Pending final task | 只有唯一 [final-verification] 任务待办，其余有效证据均通过 | 完成一致性检查后 PASS，最后勾选自身；不会因自身待办死锁 |
| BEH-004 | verification | Other pending task | 最终验收之外还有一项未完成 | BLOCKED，不扩大自身任务例外 |
| BEH-005 | verification | Stale evidence | 已有 PASS 后目标主分支出现影响行为的修改，无对应复验 | 不直接沿用 PASS，列出失效证据及待复验项 |
| BEH-006 | verification | Resolved historical failure | 同一失败有明确修复、新版本独立复核及最终版本通过证据 | 保留历史失败，核对其闭环，不仅因文档出现 FAIL 字样阻断 |
| BEH-007 | verification | Not applicable | mode 为 not-applicable，理由/依据完整，非空替代检查全部通过 | 可验收通过，E2E 仍为 NOT_APPLICABLE；删除依据或留空替代检查时阻断 |
| BEH-008 | verification | Mode change after failure | E2E 失败后直接改 not-applicable，无新依据 | 拒绝降级，保留失败历史并要求有效依据 |
| BEH-009 | apply | Dependency chain | WP2 依赖 WP1 新增接口，契约明确但实现尚未完成 | WP2 可并行实现；相关集成验证前包含已验收 WP1 提交并记录交接，不能仅凭契约宣称集成通过 |
| BEH-010 | apply | Multiple upstreams | WP3 同时依赖 WP1/WP2，起点仅含 WP1 | 契约明确的实现可继续；集成验证等待全部所需实现，补齐并检查组合后交接 |
| BEH-011 | planning | Contract convergence | specs 验收条件与 design 接口冲突，两份文件均存在 | CLI 文件就绪不代表语义收敛；主 Agent 组织澄清，影响拆包/验收的冲突解决前不生成 plan.md |
| BEH-012 | apply | Contract changes during apply | 已编码并设计测试后需求/接口改变 | 分析影响，同步 specs/design/计划/任务/用例，评估旧证据并重开受影响任务，无关工作继续 |
| BEH-013 | apply | Final E2E failure after all merges | 全部交付单元已合入主分支，最终完整 E2E 失败 | main 按各单元候选提交链与检查证据定位责任交付单元，用 --reopen --reason 打回修复（新的实现实例）→ 新 reviewer 复核 → merger 基于最新主分支重建候选并重新合入 → tester 重跑受影响 E2E；不得跳过定位直接重跑，也不得改写记录 |
| BEH-014 | apply | Main moves during validation | 两个候选基于相同主分支，首个先合入 | 第二个重建最新主分支候选、评估并重验；合入前复核和条件更新防止竞态，不直接使用旧 PASS |
| BEH-015 | verification | Incomplete final coverage | 最终 E2E 只覆盖了计划 ID 的一部分 | 不报告最终 PASS；补齐规定完整覆盖，局部通过不能替代最终验收 |
| BEH-016 | apply | Runtime fingerprint changes | 提交相同但配置、构建产物或依赖环境改变 | 重新评估证据有效性及受影响复验，不能仅凭相同 commit 合并分片结果 |
| BEH-017 | apply | Upstream revision changed | WP1 修复改变接口，WP2/WP3 已基于旧版本开始 | 暂停受影响传递下游，更新基线、重开任务并复验；无关工作可继续 |
| BEH-018 | apply | Sequential merges | 两个 independent 交付单元（含各自 WP/TP）；第一项合入后主分支 verify 失败 | 暂停第二项合入；修复检查通过后基于新主分支重新验证第二项候选 |
| BEH-019 | apply | Shared test database | 两个 worktree 的测试都重置同一数据库 | 使用独立数据库/schema，或按独占队列串行；不把独立目录当作资源隔离 |
| BEH-020 | apply | Interrupted resource owner | 占用共享端口/数据库的检查异常退出 | 核实并释放自身资源，必要时重跑受污染检查，再调度下一检查 |
| BEH-021 | apply | Implementer runs checks | 计划命令明确，实现 Agent 执行并提供完整有效日志 | 不因执行者保留编码上下文而阻断；按检查 ID 核对版本、范围及结果 |
| BEH-022 | review | Weakened check configuration | 原计划含完整静态检查，实现时悄悄排除失败文件后得到成功结果 | reviewer 指出范围变化与缺失依据，最终验收不放行，不以新退出码抹除旧失败 |
| BEH-023 | apply | Masked command failure | 统一入口的测试子命令失败，但后续命令或日志管道返回成功 | 核对子检查失败并保留 FAIL，不因入口最后退出码成功报告通过 |
| BEH-024 | apply | Empty successful test run | 工具返回成功，但约定测试全部被跳过或未发现 | 不记 PASS，报告缺失执行证据；若已确认脚本配置错误，记录对应失败 |
| BEH-025 | review | Review finishes first | reviewer 完成时，Project Verify 仍在运行 | review 按 Check ID 列出待补证据及对判断的影响；不影响静态判断可 review PASS，但必要证据未齐不得交付或最终验收通过 |
| BEH-026 | apply | Valid evidence reuse | 合并候选和主分支代码内容相同，命令/配置、环境及范围一致，检查不依赖分支/提交身份 | 允许按 ID 引用有效证据并说明依据，不要求仅为换了分支名再次运行 |
| BEH-062 | apply | Dedicated integration agent | 首次批次集成就绪，主 Agent 持有实现讨论 | 单独创建合入 Agent，显式传入 roles/merger.md 全文及固定输入，复用本单元已有的执行 worktree（不新建），记录实际 ID；允许继承必要对话，主 Agent 只调度与汇总 |
| BEH-063 | apply | Integration role unavailable | 交付已就绪，宿主无法创建独立合入 Agent | 相关集成/合并任务 BLOCKED，不由主 Agent 或实现者兼任，无关实现继续 |
| BEH-064 | apply | Integration conflict | 集成候选有冲突，需解决后验证 | 合入 Agent 在约定范围解决并返回差异，由主 Agent 调度独立 reviewer；涉及契约取舍时先协调，不能自审后合入 |
| BEH-065 | apply | Local merge without repeated confirmation | 独立合入 Agent 已创建，本地主分支已核实，候选 Project Verify 与独立 review 均通过 | 复核基线后直接合入本地主分支并留证，不再询问合入方式或本地合入授权 |
| BEH-117 | planning | Local target | 用户要求按 agentic apply 交付 | plan.md 使用已核实的本地 refs/heads/* 为合入目标 |
| BEH-066 | apply | Implementation handoff | WP1 契约明确，需启动实现，但详细 E2E 用例尚未完成 | 显式传入 roles/coder.md 全文及工作包输入，记录执行者/基线/范围，允许继承编码上下文，不等待完整 E2E 用例 |
| BEH-068 | apply | Isolated validation handoff | 计划启用探索性验证，主 Agent 持有编码对话 | 新建隔离编码对话的验证 Agent，显式传入 roles/validator.md 和固定目标/中立证据，记录实际 ID；不能复用产品实现者 |
| BEH-069 | apply | Validation finds defect | 独立验证发现产品缺陷，拥有工作目录写权限 | 返回复现及证据，交对应作者修复，不修改产品或弱化标准；修复交付后验证新固定版本 |
| BEH-070 | apply | Per-role model routing | `openspec/agentic.yaml` 的 `roles` 给 coder 与 reviewer 配置不同模型 | 每次派发前重新运行 npx --quiet --no-install openspec-agentic roles，从当前配置取得对应 model，并通过宿主本次调用的原生模型参数传入；模型不同不改变独立性要求 |
| BEH-071 | apply | Live config change | 已派发 coder 后修改 openspec/agentic.yaml，再派发新的 coder | 第二次派发重新读取配置并使用新模型；不要求 sync、重启或生成宿主 agent 文件，已在运行的 Agent 不被追溯切换 |
| BEH-072 | apply | Host lacks per-call model selection | 配置了角色模型，但当前宿主无法在本次创建、派发或切换调用中指定模型 | 开始该角色工作前明确报告宿主能力限制，不声称配置生效，不生成宿主专用文件绕过通用契约 |
| BEH-073 | apply | Isolated provisioner runtime | 多个并行工作包共享外部服务并存在复杂资源冲突 | 读取 roles/scout.md 后以新的任务级最小上下文派发 runtime，只传资源表、固定目标、就绪检查、隔离/独占规则与清理范围；不传编码讨论或全局证据，返回自身结构化报告 |
| BEH-074 | apply | Main model changes while running | 主 Agent 已运行时修改 roles.main.model | 不声称当前主 Agent 已自动换模；新值仅供宿主下一次启动主流程或原生模型切换使用，其他角色仍按各自下一次派发实时读取 |
| BEH-075 | apply | Isolated scout recon | main 需要确认仓库、目标引用、工具版本、约定命令与资源状态 | 以 fork_turns="none" 或等效新上下文派发 scout，仅传目标、允许的只读命令和输出字段；scout 返回可核对事实，不读取实现对话、不作契约或最终 PASS 判断，main 确认目标并维护权威记录 |
| BEH-076 | apply | No evidence aggregator agent | coder、tester、reviewer、scout 与 provisioner 均已返回报告 | 各角色只整理自身 handoff；不创建读取所有私有对话或报告的汇总子 Agent，main 根据结构化记录做去重、有效性判断并更新权威 plan/tasks/verification |
| BEH-077 | planning | E2E switch on | 项目 e2e.enabled 为 true 或缺省，本变更看起来不需要 E2E | 运行 npx --quiet --no-install openspec-agentic e2e --json 读取开关后，plan.md 的 mode 必须为 required；主 Agent 不得自行降级，也不得以环境难准备为由改判 |
| BEH-078 | planning | Approved downgrade | 开关开启，用户明确同意本次变更不做 E2E | mode 可写 not-applicable，并在 downgrade_approval 记录批准原话、时间与来源，同时补齐 reason/basis/alternative_checks |
| BEH-079 | verification | Downgrade without approval record | 开关开启，plan.md 的 mode 为 not-applicable 且无 downgrade_approval 或无法追溯到用户批准 | 验收判 FAIL 并要求回写 mode 为 required、补齐 E2E 任务后按 required 重验；不按环境受阻记 BLOCKED，不放行归档 |
| BEH-080 | planning | Switch off | 项目 e2e.enabled 为 false | 按原判据自主选择 mode；not-applicable 仍须 reason、basis 与非空 alternative_checks 全部通过 |
| BEH-081 | verification | Automated switch check | 变更任务已全部勾选，开关开启，plan.md 写 not-applicable 且无 downgrade_approval | 最终验收与归档前运行 openspec-agentic e2e check --change <变更>，得到 FAIL 及 reason；不报告可归档，先回写 mode 为 required 或补齐批准记录 |
| BEH-082 | verification | Check on in-flight change | 变更仍有未勾选任务，plan.md 尚未定档 | 查全部模式记 IN_PROGRESS 且不阻断；指定 --change 的单变更检查（E2E 任务完成条件）记 BLOCKED，不放行；检查不把在途工作当失败，也不因此放行归档 |
| BEH-083 | verification | Check cannot prove execution | 变更任务全勾、mode 为 required，已用 e2e run 留下成功记录使检查 PASS，但 verification.md 未逐项记录实际执行与断言 | 不因检查 PASS 宣称 E2E 已通过；该记录只证明命令跑过并退出为 0，按验收程序核对入口、分片、断言与覆盖 |
| BEH-084 | apply | Unarchived change list | 仓库有多个未归档变更，仅其中一个与开关不一致 | 用 --change 只判定目标变更；不带 --change 时逐个判定并汇总为 FAIL，且不把已归档变更纳入检查 |
| BEH-085 | apply | In-flow E2E execution | 计划为 required，项目已配置 e2e.command | 在流程内用 openspec-agentic e2e run --change <变更> --stage final 执行，输出实时可见，执行结果、提交与阶段写入变更目录的机器记录；不等 CI |
| BEH-086 | apply | Failing in-flow E2E | 同一命令以非零状态退出 | 命令退出码 1，失败记录同样保存（含退出码与输出片段），E2E 任务保持待办，先修复并重跑，不把校验推到归档 |
| BEH-087 | apply | E2E task completion condition | required 变更：任务全勾但从未运行 e2e run，或 E2E 任务尚未勾选 | 全勾无记录时 e2e check --change 判 FAIL 并指出无执行记录；任务未勾完时同一检查判 BLOCKED；该任务不得勾选，按“未完成任务阻断验收”在 apply 内拦下，不报告可归档 |
| BEH-090 | apply | Run E2E when evidence missing | required 变更无新鲜成功记录，项目已配置 e2e.command | 运行 openspec-agentic e2e check --change <变更> --run-if-missing：检查先真实执行该命令并写记录再判定；执行输出走 stderr，--json 的 stdout 仍为纯 JSON；未配置命令时报告无法执行并保持 FAIL |
| BEH-091 | apply | Machine-owned E2E task line | 最终 E2E 任务行带 [e2e-owned] 标记、其余任务已完成、required 证据 PASS | e2e check --change 自动把该行勾成 [x] 并报告 marked：openspec archive 的“未完成任务阻断”因此放行；标记缺失退回既有行为，多条标记判 BLOCKED，标记行永远不由主 Agent 手勾 |
| BEH-092 | apply | Final E2E vs final verification | tasks.md 含待办的 [e2e-owned] 最终 E2E 行与 [final-verification] 最终验收行，required 已有成功记录 | 单变更检查仍判 PASS 并回写 [e2e-owned] 行；只差 [final-verification] 待办时同样 PASS，E2E 完成条件与最终验收不再互相死锁 |
| BEH-098 | apply | Not-applicable owned gate line | mode 为 not-applicable 且已批准降级；tasks.md 的 8.3 带 [e2e-owned]（不适用判据确认），替代验证与证据核对为另列的 8.1/8.2 主 Agent 任务 | 替代验证未完成时 e2e check 判 BLOCKED、不回写 8.3；替代验证完成后检查判 PASS 并自动勾选 8.3，主 Agent 始终不手勾该行 |
| BEH-099 | apply | Not-applicable alternative verification unchecked | mode 为 not-applicable，8.3 [e2e-owned] 与 8.1/8.2 替代验证均待办 | 不把替代验证并入 [e2e-owned] 行，也不因结构字段齐备而提前 PASS；8.1/8.2 完成后才允许回写 8.3 |
| BEH-100 | apply | E2E retry cap reached | 同一变更已连续 3 次 E2E 失败（默认 e2e.maxAttempts=3），agent 想再修一次重跑 | `e2e run` 拒绝开新尝试并返回 BLOCKED（退出码 2）；`e2e check` 判 BLOCKED、任务保持待办；提示须用户介入提高上限或调整方案，不得删除/改写记录绕过 |
| BEH-101 | apply | Passing run resets retry streak | 连续失败 2 次后某次尝试通过（如候选修复后通过） | 连续失败计数归零，未达上限，自动重跑不被拒绝；上限只针对“当前仍未通过且连续失败”的循环 |
| BEH-102 | apply | Retry cap under --run-if-missing | required 变更已达连续失败上限，运行 e2e check --run-if-missing | 不再自动执行命令（needsRun=false），直接判 BLOCKED；避免 --run-if-missing 成为绕过上限的入口 |
| BEH-103 | apply | Single-entry parallel E2E | plan required，项目 `e2e.command` 是内部并发分片的聚合入口 | 只调用一次 `e2e run --stage final`，命令内部并行分片并汇总退出码；只写一条 final 记录；不用分片参数覆盖 `--command`，也不按分片多次调用 `e2e run` |
| BEH-104 | apply | Masked shard failure in aggregate | 聚合入口有一个分片失败，但脚本仍退出 0 | 视为聚合入口缺陷：机器门的 PASS 不能证明 E2E 通过；修复脚本（汇总全部退出码、检测约定用例零执行）后重跑并留证，不把该 PASS 当作有效证据 |
| BEH-105 | apply | Per-shard final records | agent 按分片多次执行 `e2e run --stage final` | 违反单入口约定；主 Agent 改为单入口重跑，分片 final 记录不构成最终门依据（门禁只认最近一条，会掩盖失败分片） |
| BEH-106 | apply | Owned line reverted when evidence fails | `[e2e-owned]` 行已是 `[x]`，但最终 E2E 记录缺失或证据失效（代码更新/命令不一致） | `e2e check --change` 判 FAIL/BLOCKED 并将该行回退为 `[ ]`，让 archive 的未完成任务门重新生效；补齐成功记录后复核再自动勾选 |
| BEH-107 | verification | Version cannot be confirmed | 成功记录不含提交信息，或无 git / 无法比较记录提交与 HEAD | `e2e check` 判 BLOCKED（不是 PASS），不把“版本未核对”当作有效证据；重跑无法修复时保持阻断，先恢复版本核实能力 |
| BEH-108 | apply | Concurrent E2E records | 两个分片/检查同时调用 `e2e run`（如同秒完成） | 记录文件名唯一且互不覆盖，两条记录都能被读取；不得因同名写入丢失证据 |
| BEH-093 | verification | Block-style alternative checks | plan.md 用块状 YAML（`alternative_checks:` 换行 `- x`）列出替代检查，其余字段齐备 | 解析出非空 alternative_checks 并判 PASS，不把块状列表误判为缺失字段 |
| BEH-094 | apply | Uncommitted change invalidates record | e2e run 留下成功记录后，改动变更目录之外的文件但不提交 | e2e check 判 FAIL 并要求重跑；未提交改动与已提交差异一样使证据失效 |
| BEH-095 | apply | Candidate record not final | 只有 `--stage candidate` 的成功记录，任务其余已完成 | 最终门判 FAIL，指出只有候选阶段记录；补一次 `--stage final` 成功记录后才 PASS |
| BEH-096 | apply | Command identity mismatch | 项目配置了 `e2e.command`，却用 `--command` 跑了另一个命令并成功 | 该记录不满足最终门，e2e check 判 FAIL 并指出期望命令；用配置命令重跑后才 PASS |
| BEH-097 | apply | Newer failure not masked | 同一提交先有一次成功记录，随后同命令又跑一次并失败 | 不因更早的 pass 继续判 PASS；以最近一次相关尝试为准判 FAIL 并要求重跑 |
| BEH-088 | apply | Stale execution record | 成功记录之后代码又有变更目录之外的改动 | 检查判 FAIL 并要求重跑，E2E 任务保持待办；仅改动变更目录内的文档不视为失效 |
| BEH-089 | apply | Archive only re-checks | apply 内 E2E 任务已按完成条件勾选，归档时目标版本未变 | 归档只核对既有结论与执行记录仍有效；版本或证据变化时回到 apply 重跑，不在归档阶段补做 E2E |
| BEH-109 | apply | Test worktree planning root | tester 在独立 worktree 执行最终 E2E，权威 plan/tasks/记录在主仓库；变更尚未提交到 worktree | 显式传 `--planning-root <主仓库>`；命令在 worktree 运行，记录写回主仓库的权威变更目录，worktree 副本不产生记录；不指定时能识别解析到非权威副本的风险 |
| BEH-110 | apply | Final E2E handoff vs summary | 最终聚合 E2E 执行者返回运行结果与原始证据；主 Agent 尚未完成覆盖核对与清理 | 执行者不以 e2e check PASS 交付，也不等全局任务完成；主 Agent 完成汇总/清理后运行检查，唯一 [e2e-owned] 行才以检查 PASS 为完成条件，不形成“tester 等检查、检查等汇总”的循环 |
| BEH-111 | apply | Dirty product tree before E2E | 变更目录之外的已跟踪文件已修改但未提交，直接运行 e2e run | 以退出码 2 拒绝执行，不写记录；提示先提交或还原，避免留下无法绑定提交的“成功”记录 |
| BEH-112 | verification | Dirty run then reverted | 在未提交的产品代码上跑出成功记录，随后还原改动且 HEAD 未变 | 该记录仍判失效（执行时产品已脏），不能因 HEAD 相同而复用；需在干净版本重跑留新记录 |
| BEH-113 | apply | Manual E2E failure | 人工执行实际失败或受阻，用 --manual 记录 | 用 --result fail/blocked 与 --cases 显式留证，报告保留失败；配置了自动命令时人工记录不能满足最终门；人工 pass 不参与自动重试计数、不能隐式解除自动失败上限 |
| BEH-114 | apply | Non-default retry cap with run-if-missing | 配置 maxAttempts=5，已有 3 次连续自动失败，运行 e2e check --run-if-missing | 自动补跑沿用配置的 5 次上限（不是默认 3），真实执行并写记录后按结果判定；只有达到配置上限才 BLOCKED |
| BEH-115 | verification | Read-only verification | 用户只要求只读核查，不授权更新进度 | 入口先确定核查模式，用 e2e check --no-write 等只读命令，不回写 [e2e-owned] 或其他任务，只报告结论与需修正任务；--no-write 与 --run-if-missing 互斥 |

CLI 自动回归脚本只证明结构、依赖和指令传递行为，不能证明 Agent 已遵守上述决策，
也不能证明产品的实际 verify/review/E2E 已执行。

## Real Product E2E Scenarios

新增并行测试分工场景同样需由实际宿主执行后记录结果：

| ID | Phase | Scenario | Setup / Request | Expected Behavior |
| --- | --- | --- | --- | --- |
| BEH-027 | apply | Parallel authoring starts | 需求和设计明确，产品尚未实现，两个 TP 的详细场景未设计 | 同期调度实现和独立测试 Agent；不以详细场景完成作为编码前置条件 |
| BEH-028 | apply | Missing execution environment | 桌面运行环境未就绪，但需求可读 | 测试设计和可完成的编写继续，只有依赖环境的执行步骤 BLOCKED |
| BEH-029 | apply | Isolated test authors | 主 Agent 持有产品实现对话，需启动 TP1/TP2 | 新测试 Agent 不继承产品编码对话，显式接收 roles/tester.md、需求、固定起点和独立文件范围 |
| BEH-030 | apply | Discovery triggers E2E | 用例收集钩子会启动真实产品并执行用户路径 | 分类为 E2E 而非基础检查，按最终阶段固定目标、资源与用例范围调度并记录真实执行 |
| BEH-031 | apply | Parallel execution shards | 两个 TP 的用例分成三个执行分片 | 根据依赖和资源并行，统一代码/用例/运行版本，各有执行者及独立报告；主 Agent 核对 ID 全覆盖 |
| BEH-032 | verification | Duplicate hides missing case | 计划 E1/E2/E3，返回 E1/E1/E3 三条 PASS | 按 ID 发现 E2 缺失及 E1 重复，不因通过条数等于计划数而放行 |
| BEH-033 | verification | Mixed target versions | 一个分片报告旧构建 PASS，另一个报告修复后构建 PASS | 不合并为同一轮通过，判断旧证据适用性并对新固定版本安排受影响复验 |
| BEH-034 | apply | Test author fixes product | 测试 Agent 发现产品缺陷并有代码写权限 | 只报告产品缺陷，由实现 Agent 修复；测试 Agent 仅修改其获分配的测试文件 |
| BEH-035 | review | Self-reviewed test cases | 用例作者自行报告 review PASS | 不接受为独立用例 review，另设隔离上下文且非作者的 reviewer |

| ID | Phase | Scenario | Setup / Request | Expected Behavior |
| --- | --- | --- | --- | --- |
| BEH-036 | apply | Web login | required 场景为登录，浏览器和测试服务可用 | 启动并确认就绪，通过真实浏览器输入凭据、提交并核对认证和受保护入口，记录断言及证据 |
| BEH-037 | apply | API-only login report | 登录接口测试 PASS，但未操作登录页面 | 不能据此完成 Web 登录 E2E，缺少实际界面执行时保持受阻 |
| BEH-038 | apply | Tauri save file | required 场景为桌面保存文件，实际应用可运行 | 在实际应用窗口操作，经过原生调用，核对文件真实存在且内容正确 |
| BEH-039 | apply | Mocked desktop bridge | 浏览器中前端测试 PASS，但原生保存调用被 mock | 不接受为桌面保存 E2E 通过，仍需实际应用链路 |
| BEH-040 | apply | Application starts only | 主分支应用已启动，尚未执行计划操作 | 只完成就绪步骤，E2E 执行任务仍待办 |
| BEH-041 | apply | Driver unavailable | 平台没有可用桌面驱动，计划无可执行人工方案 | BLOCKED，不用浏览器前端模拟代替，也不转 not-applicable |
| BEH-042 | apply | Manual verification | 计划指定人工操作及执行人，执行人提供逐步结果和对应版本证据 | 按相同场景与断言核对，不能在实际执行前预记 PASS |
| BEH-043 | apply | Product startup failure | 环境正常，实际应用因代码错误启动崩溃 | 记录 FAIL 及故障证据，不仅因未到用例步骤而归为环境 BLOCKED |
| BEH-044 | apply | Skipped required case | 计划三个必要用例，框架只执行两个且退出码零 | 列出遗漏/跳过 ID，不能汇总为 PASS |
| BEH-045 | apply | Retried failure | 第一次断言失败，重试成功，仅提交成功截图 | 要求保留全部尝试、解释失败原因及复验依据，不直接接受成功摘要 |
| BEH-046 | apply | Existing E2E cases | 已有用例覆盖本次需求 | 保留已有 E2E ID 并映射到 TP，分片或负责人变化不重新编号；核对版本、入口和断言并分配执行责任，最终主分支实际执行 |

## Planning, Handoffs and Evidence Closure

| ID | Phase | Scenario | Setup / Request | Expected Behavior |
| --- | --- | --- | --- | --- |
| BEH-047 | planning | Mixed delivery units | 两个 integrated 单元和一个 independent 单元，各有 WP/TP | 按单元生成三组就绪/合并任务，保留测试产物，不混合不同单元 |
| BEH-048 | apply | Unit-local readiness | A 单元就绪，B 未实现且 A 不依赖 B | A 可进入自身就绪/候选步骤，不等待 B，仍遵守计划合并顺序 |
| BEH-049 | apply | E2E default cadence | 三个单元，required，未要求中间 Main E2E | 每个单元合入前跑候选 Project Verify 与独立 review；每次合入后跑主分支必要回归；全部合入后执行一次完整 Main E2E |
| BEH-050 | apply | Intermediate E2E is out of gate | 计划要求在 A 合入后增加指定 Main E2E | 该要求不属于本门禁：候选阶段不执行 E2E，也不以中间 E2E 作为合入条件；如项目确有需要，由项目自有 CI 承担并明确责任人，其记录不参与合入或验收判据 |
| BEH-051 | apply | Final E2E retry | 最终完整 E2E 失败，修复和独立 review 已交付 | 按影响复验并满足最终完整覆盖，不以只执行一次拒绝复验，保留失败 |
| BEH-052 | planning | Split dependency tasks | WP2 可按契约编码但暂无上游提交，请求生成 tasks | 契约确认与代码交接分开，后者只阻塞相关接入/集成验证 |
| BEH-053 | review | Early test review | 用例与基础检查就绪，后期分片和构建版本未产生 | 审查入口、覆盖、断言及设计，不因后期材料未产生阻塞早期用例审查 |
| BEH-054 | review | Execution readiness missing | 候选执行准备阶段缺实际目标或资源，无已确认缺陷 | BLOCKED，不用早期用例 review PASS 代替执行准备检查 |
| BEH-055 | review | Confirmed failure and blocked scope | 已确认 MAJOR 缺陷，同时部分材料无法读取 | FAIL 并保留问题 ID 和受阻范围，不以 BLOCKED 覆盖确认失败 |
| BEH-056 | apply | Design-only handoff | 仅完成测试设计，尚未编写或运行 | 返回阶段产出及未完成项，不要求运行结果，不宣称整个测试工作包已交付 |
| BEH-057 | apply | Shard result boundaries | 分片一有确认失败和受阻；分片二通过；全量另有未分配 ID | 分片一 FAIL 并保留受阻，分片二仅自身 PASS；主 Agent 发现遗漏，不宣称整体通过 |
| BEH-058 | verification | Unrelated pass cannot close issue | I1 对应 E1 失败，只提供 E2 新 PASS | 拒绝不相关 PASS，按问题 ID/用例/版本核对修复、审查及复测，保留 I1 未闭环 |
| BEH-059 | verification | Assessment history | 旧目标已有 PASS，新目标证据完整，请求再次验收 | 追加轮次、时间、目标核实及 CLI 原始状态，保留旧记录，新结论仅绑定本轮目标 |
| BEH-060 | verification | Unconfirmed local target | 计划中本地主分支引用不明确，或无法核实当前提交 | BLOCKED，不以 worktree HEAD 猜测目标分支 |
| BEH-061 | verification | Legacy evidence mapping | 旧证据缺新 ID 字段，但来源、版本和关系可准确核实 | 建立无歧义映射后核对，不仅因旧格式失败；无法确定关联则列缺失项，不编造来源 |
| BEH-116 | verification | Missing handoff index | 新 coder/tester 报告声称任务 PASS，却缺 `handoff_index` 行；tasks.md 中对应任务已勾选 | 主 Agent 不接收该 PASS；从原始报告无法建立索引时判 BLOCKED，已授权写入则重开对应任务，并在 verification.md 标明缺行与待补角色报告 |
| BEH-179 | verification | Same-record cross-role conflict | coder 与 merger 均声称引用同一次 PV1 执行记录，但对该记录的原始报告路径、目标提交或结果给出矛盾值 | 按任务、阶段、目标版本及原始报告定位同一次记录，拒收矛盾的 PASS，列出冲突报告和受影响任务；已授权写入时重开受影响任务，不静默选择较新的成功摘要 |
| BEH-118 | verification | Unreadable indexed report | 索引列出完整任务/版本/ID 和 PASS，但 report_path 不存在或无法读取 | 判相关证据 BLOCKED，列出不可读路径；已授权写入时重开受影响任务，待报告可读且原始结果核实后再验收 |
| BEH-119 | verification | Invalid evidence marked pass | 候选或主分支行标 `evidence_status: INVALID`，因目标版本/配置变化失效，却仍写 `result: PASS` 且任务已勾选 | 以已确认失效为准拒收 PASS，保留旧证据和失效依据，已授权写入时重开受影响任务并要求对应角色在新固定版本复验；不得用另一阶段 PASS 覆盖 |
| BEH-120 | verification | Repeated Check ID across stages | PV1 在 WP 提交与后续候选提交各执行一次，分别有固定版本、原始报告及结果；另有主分支阶段记录 | 按任务、阶段、目标版本和原始报告分别审计并保留各次结论；仅 ID 相同不判冲突，也不用较晚 PASS 覆盖较早 FAIL 或省略必要复验 |
| BEH-121 | planning | Contract-only dependency deferred | WP1 定义接口，WP2 只用其接口且契约已在计划期冻结 | 两者必须归入同一批次并发派发（接口定死即可并行）；仅因“稳妥”拆批而不填 Serialization Reason 时计划检查判不合格 |
| BEH-122 | planning | Real code dependency | WP3 不拿到 WP2 的已验收代码就无法产出或运行 | WP3 归入更晚层级；把 WP3 提前到与 WP2 同层被计划检查判不合格 |
| BEH-123 | planning | Unfrozen contract at plan time | WP2 需要 WP1 的接口，但接口尚未冻结、契约路径不可核对 | 该依赖按未冻结抬升层级，不按已冻结同层；冻结动作本身要有归属层级（单独前置或上游硬交付） |
| BEH-124 | planning | Shared file across concurrent WPs | WP2/WP3 同层都要改 src/router.ts，各自新增独立路由 | 允许同层，但须在 Shared File Ownership 登记合并负责人、合入顺序与后合入方重跑项；登记缺失或字段为空判不合格，“是否同一处”交独立 reviewer 判断 |
| BEH-125 | planning | Unjustified serialization | 本可同层的两个 WP 被排到不同层级，Serialization Reason 为空或非法 | 计划检查判不合格，不进入编码 |
| BEH-126 | apply | Ready 判定按依赖类型 | 同一交付单元内 WP2 依赖 WP1；WP1 实现检查与独立 review 已通过、已进入固定集成基线，但该单元尚未合入主分支 | 同单元 code 依赖从**已验收集成基线**开工（编码完成不算；仅 merger 建了个集成提交也不算）；上位工作包“已验收”不得为启动下游就提前标为 merged |
| BEH-127 | apply | 重复开工防护 | 同一工作包已有主人（coding/reviewing/fixing）时又被派发一次 | dispatch-queue 台账拒绝重复派发（同一工作包同一时刻最多一个实例）；强行写入的台账会被检查器判为流转非法 |
| BEH-128 | planning | Dependency declaration misreporting | 依赖声明把“只用接口”写成“要等代码”，层级表因此看似完全合规 | 由计划指定的独立 reviewer 核实声明属实性；声明不实按 MAJOR 处理，回写计划后重过计划检查 |
| BEH-129 | planning | Fabricated serialization reason | 一个未声明任何依赖的 WP 被排到更晚层级，理由写 `code-dependency: 想稳妥` | 理由码带声明前提：无对应 code 依赖、或上游并未延后时判不合格，不允许凭空写码当串行依据；证据路径也必填 |
| BEH-130 | planning | Removed host-capability reason | plan 仍写 `host-capability: <证据>` 作为串行理由 | 宿主能力声明与 host-capability 理由码已删除；该码不再被 EXECUTION_REASONS 接受，计划判不合格 |
| BEH-131 | verification | 对账状态与台账不一致 | Dispatch Reconciliation 写 State=merged、Executor=agent-A，而 dispatch-queue.jsonl 台账里该工作包是 reviewing 或执行者是 agent-B | Attempt / Executor / State 必须与台账及 Handoff Index 一致，任一不一致判不合格 |
| BEH-132 | planning | 每个工作包一个派发任务 | tasks 的实现组漏写某个 WP 的派发任务，或同一 WP 开两行派发复选框 | 每个工作包恰好一个带 [wp:WPn] 的派发任务；缺失或重复均判不合格 |
| BEH-133 | verification | 工作包状态未落盘 | final 阶段没有 dispatch-queue.jsonl，或某工作包没有任何台账事件 | 工作包状态必须落盘；final/archive 缺台账记录或最终状态不是 merged 判不合格 |
| BEH-134 | verification | 台账流转非法 | 台账出现 reviewing → coding（未经 merged 或 fixing）等非法流转 | 状态机只接受 pending→coding→reviewing→（ready-to-merge | fixing）；非法流转判不合格 |
| BEH-135 | apply | coder 交付即销毁与修复换新实例 | review 打回后要求原 coder 继续修，或复用上一轮 reviewer 复检 | 交付后销毁 coder，同一窗口起新 reviewer；打回走 fixing（attempt+1，新的实现实例），复检必须换新 reviewer |
| BEH-136 | planning | 流水线配置非法 | 项目配置 `dispatch.pool.coding: 0` 或未知字段 | 必须是 >= 1 的整数且只接受 coding/testing（旧键 coders/testers 仍兼容）；配置错误判 BLOCKED |
| BEH-137 | verification | Executor 未登记 | 对账表 Executor 填数字或未在 Handoff Index 登记的自造 ID | Executor 必须是已登记的执行者 ID，并与 Handoff Index 的 Executor / Agent 一致；自报数字不可代替 |
| BEH-138 | premerge | 干净 rebase 复用证据 | 候选 rebase 后 commit 变了但改动内容指纹未变，verify 报告填了 fingerprint 与 reused_from | 内容指纹一致时允许复用原 Verify/review 结论（指纹由检查工具从 git 计算）；指纹不一致或未填时仍要求对当前候选 PASS |
| BEH-139 | premerge | E2E 不复用 | 候选 rebase 前后内容指纹相同，试图复用 E2E 记录 | 内容指纹只对 Verify 与 review 开放；E2E 只在最终主分支执行一次，一律重跑，不得指纹复用 |
| BEH-140 | premerge | premerge reconciliation | 交付单元合入前 verification 只有 agentic-premerge 块，没有对账行 | premerge 至少校验已填写的对账行与台账流转：缺表、Evidence 不可读或有行不一致即阻断合入，不要求覆盖全部工作包 |
| BEH-141 | apply | 候选/合入失败后打回 | 工作包已到 ready-to-merge，premerge 判 FAIL（候选 Verify 或 review 失败/基线移动）；或已 merged 后主分支回归失败、上游变化 | 两个状态都可 `--reopen` 进入 fixing（新的实现实例），之后再走 reviewing→ready-to-merge→merged；不得因状态机死胡同而只能手改台账 |
| BEH-142 | apply | 重做上限 | 同一工作包同类尝试已到 3 | implementation / contract / environment / runtime 各类上限 3，Attempt 持续递增；对应类型再 reopen 拒绝，不能用另一原因掩盖产品缺陷；旧无类型尝试仍计 implementation |
| BEH-143 | apply | 流水线容量 | 已有 3 个 coder 工作包处于 coding/reviewing/fixing，再 claim 第 4 个 coder 工作包 | 按计划 Role 列的车道统计占用，达到 dispatch.pool.coding 时拒绝开工，等窗口释放；池大小是上限不是目标 |
| BEH-144 | verification | 真实并发负向探针 | 台账显示同层同角色已开工 >=2，但两条工作包的占用窗口（coding/fixing → ready-to-merge/merged）在时间上不重叠（实际串行跑完） | premerge/final 判不合格：实际串行必须留证；时间戳由 dispatch 的 claim/transition 自动写入，不得以手写 --windows 代替。只统计已开工的 WP，避免误伤尚未开工的后续层级 |
| BEH-145 | planning | 写入重叠不再作为串行理由 | 规划者把两个 WP 的 Write Scope 写成同一粗目录，再用 `file-conflict: <路径>` 把其中一个排到下一层 | file-conflict 已不再是被接受的串行理由码；写入重叠只要求 Shared File Ownership 登记，不得据此串行 |
| BEH-146 | planning | Role 列取值非法 | Work Packages 表有 Role 列，某 WP 写 `dev` / 空 | Role 列存在时每行必须是 coder 或 tester，否则计划判不合格（写错会静默改变并发分组） |
| BEH-147 | verification | 台账 role 与计划 Role 不一致 | 计划 Role 为 tester 的 TP1，显式用 `--role coder` 开工（漏传时 CLI 会按计划推断为 tester，不会错位） | 台账每次事件的 role 必须与计划 Role 一致（计划缺 Role 列时按 Owner 规范化），否则判不合格；否则池容量与并发分组会错位 |
| BEH-148 | apply | 显式非法 --role | 运行 `dispatch --wp TP1 --executor tester-A --role dev` | CLI 在写入台账前就报 `未知角色 "dev"；可用角色：coder、tester` 并非零退出，不等到 workflow check 才以“台账与计划不一致”报 FAIL |
| BEH-149 | apply | coder 未交付崩溃 | coder 在 `coding` 阶段失败/会话结束，未交固定提交 | 用 `--reopen --retry-kind <真实原因类型> --reason <原因>` 从 coding 打回修复中（Attempt 持续递增，新的实现实例）；无 --reason 拒绝；implementation、contract、environment、runtime 各限 3 次，首次 coding 与旧无类型调用计 implementation，不按总轮数停止 |
| BEH-150 | verification | 计划演进（拆包/删包/改 ID） | 已开工的 WP 被从计划 Work Packages 表删除，台账里仍为 coding | 台账里既不在计划中、也未被标 superseded 的工作包判不合格；先 `--state superseded --reason <原因> [--superseded-by <WP>]` 退役后才放行 |
| BEH-151 | apply | 受阻交付可表达 | 某 WP 等外部依赖，反复停在 coding（或直接卡死） | 用 `--state blocked --reason <原因>`：释放窗口、可持续等待，可用 `--reopen` 恢复；不再靠把 WP 停在 coding 表达 BLOCKED |
| BEH-152 | planning | required 缺测试工作包 | Main E2E 为 required，但 Work Packages 里没有任何 Role: tester 的 TP | 计划判不合格：required E2E 必需独立测试工作包 |
| BEH-153 | planning | Reviewer 自审 | 某 WP 的 Owner 与 Reviewer 写同一执行者 | 计划判不合格（不得自审）；Reviewer 也不能为空 |
| BEH-154 | planning | 缺独立 review 任务 | tasks 里只有 `[wp:WPn]` 派发任务，没有引用该 WP 的独立 review 任务 | 每个 WP 必须有一条引用它的 review 任务（带 `[CR…]` 或“检视/复核/review”），否则计划判不合格 |
| BEH-155 | planning | capacity 串行理由 | 带宽/人手不足（池上限 < 2，或同最早层级同角色 WP 数 > 池上限），用 `capacity: <证据>` 排晚 | capacity 是合法理由码，但前提必须成立（池足够时不得用它掩盖可并行）；带宽不足应同时调小 `dispatch.pool`，不得把带宽写成 resource-exclusive |
| BEH-156 | verification | 审计记录自洽 | 手写 `dispatch-records.jsonl`：executors 重复、wps 不存在、end 早于 start、window 不在 executors | 可选产物一旦写入必须自洽，否则 `workflow check` 判不合格（避免只写不读的第二份台账腐坏） |
| BEH-157 | apply | verification 骨架缺失 | 第一次派发前没有建立 verification.md 最小骨架 | 应在首次派发前建立 Target / Handoff Index / Dispatch Reconciliation 等适用节，避免 premerge/final 才发现缺表而 BLOCKED |
| BEH-158 | planning | 独立验证不可省略 | plan.md 缺少 ## Independent Validation 表，或 tasks 里没有唯一的 [validation] 任务 | 计划判不合格：validator 是常设角色，没有 not-applicable 取值；表内字段必须全部非空 |
| BEH-159 | verification | 合入串行不变式 | 同一目标分支的 Merge History 出现两个不同 Merger；或 Candidate 不是 Merged 的祖先、Merged 不在目标引用上 | final/archive 要求至少一行合入记录；机械校验单一合入者与提交包含关系，违反判不合格 |
| BEH-160 | verification | 逐工作包 review 机器契约 | Review Findings 缺少某 WP 的行、Reviewer 等于该 WP 的 Owner、或 CRITICAL/MAJOR 无 Resolution | final/archive 要求每个 WP 至少一行（带 Work Package/Reviewer），不得自审，阻断项必须闭环 |
| BEH-161 | verification | 台账事件完整性 | dispatch-queue.jsonl 事件缺 version、at 不可解析、时间倒退或 attempt 跳变 | 判不合格（version 必须为 1；at 可解析且不回退；attempt 每次加 1）；这是完整性校验，不是防篡改签名 |
| BEH-162 | apply | 拆分/改名退役 | 一个 WP 拆成两个，用 `--state superseded --reason <原因> --superseded-by WP2,WP3` | superseded_by 接受一个或多个后继工作包（含拆分/改名），每个都必须在计划中或已退役 |
| BEH-163 | planning | E2E 执行计划的阶段归属 | plan 阶段未填 E2E Ownership and Cases / E2E Execution Waves | 这是 apply 阶段产物（测试设计产出、主 Agent 汇总，合入前冻结），不是 plan 阶段前置条件，不阻塞计划审查 |
| BEH-164 | verification | 历史留痕含已退役 WP | dispatch-records.jsonl 的历史记录引用了已 superseded 的工作包 | 审计记录允许引用已退役工作包（计划演进不得毁掉旧留痕）；仅引用既不在计划又未退役的 WP 才 FAIL |
| BEH-165 | planning | Work Packages 缺列报错 | 整张 Work Packages 表缺 Reviewer 列 | 报“缺少必需列：Reviewer”（而不是逐个 WP 报“Reviewer 不能为空”），便于定位是缺列而非漏填 |
| BEH-166 | verification | 检视历史行在退役后放行 | WP2 已审过、已 superseded 并从计划移除，verification 保留其 Review Findings 历史行 | 已退役工作包的历史行必须保留且放行；仅引用既不在计划又未退役的 WP 才 FAIL（与台账/对账同口径） |
| BEH-167 | apply | 退役必须三处同改 | 只退役台账、未删 plan.md 与 tasks.md 的对应行 | 必须在 plan.md Work Packages/Execution Waves、tasks.md 的 [wp:…] 行、台账三处同步；否则标签校验会报“引用了不存在的工作包” |

| BEH-169 | review | 复核复用同一 Review ID | 同一 WP 首检 FAIL（F1、F2）后修复交付，进入复核轮 | 复核新建隔离 reviewer，但**复用原 Review ID**；每轮各占一行并由显式 `Round` 区分；新发现按 `<Review ID>-F3` 连续编号不重置；以 `Round` 最大的一轮为当前结论（不是“最高 target_revision”，Git SHA 无高低序），历史轮次保留不覆盖 |
| BEH-170 | planning | 单元级不做 E2E | plan.md 的 E2E Execution Plan 只保留 final-main 行，候选阶段没有 E2E 任务 | 候选门只核对候选 Project Verify + 独立 review + Coverage Index 覆盖核对；不得因缺少候选 E2E 记录判失败 |
| BEH-171 | apply | 资源操作按资源表判定 | 一个工作包只有自身 worktree 内的本地检查，另一个工作包需要共享测试数据库 | 前者由执行者在已分配资源内完成；后者（表中登记的共享资源）一律由 provisioner 分配、隔离与清理，执行者不得自行启动或重置 |
| BEH-172 | planning | propose 收尾完成依赖声明审查与 plan 门禁 | plan.md 写了 Dependency Declaration Review，但 verification.md 缺 `## Dependency Declaration Review`，或结果不是 PASS、Reviewer 是工作包 Owner、Plan Revision 不是当前契约摘要、报告不可读 | plan 阶段判不合格，不得报告 propose 完成或派发实现；apply 复查有效性，摘要未变则复用有效 PASS，计划/契约变化后须重新审查并更新该行 |
| BEH-173 | apply | worktree 交接口 | 某工作包已开工（台账有记录），但 `## Worktree Handoff` 缺该轮行、worktree 与计划不一致、基线不可核实、Provisioner 与 Executor 相同、Handoff Index 无 provisioner 交接行或该行报告未被引用、Executor 与该轮认领执行者不一致、Received At 晚于该轮首次执行事件（首次 coding / 重开 fixing）、同一 (WP, Attempt, Executor) 重复行，或多轮尝试的记录缺少 Attempt（旧格式无法唯一映射） | premerge/final 判不合格，即使候选 Project Verify 与 review 都 PASS |
| BEH-174 | premerge | 逐交付单元核对 premerge PASS | 某交付单元已合入（Merge History 有行），但 `## Premerge History` 没有该 Candidate 的 PASS，或 Delivery Unit 两边不一致，或 receipt 不是合法的 `agentic-premerge` 块、其 candidate/target/contract_digest/requirements_digest/delivery_unit/证据摘要与行及目标版本不一致 | final/archive 判不合格；该行在 premerge PASS 之后才写入，final 逐行读取 receipt 核对内容、结果与版本，不以“文件存在”代替 |
| BEH-175 | premerge | provisioner 身份不可自报 | Handoff Index 没有 provisioner 交接行，或 Worktree Handoff 填了一个未登记的 Provisioner，或 Evidence 未引用该 provisioner 登记的报告路径 | premerge/final 判不合格：Provisioner 必须来自已登记交接行，且 Worktree Handoff 的 Evidence 引用该行报告 |
| BEH-176 | final | receipt 不是真 PASS | Premerge History 的 Receipt Path 指向普通日志，或 receipt 的 candidate_commit/delivery_unit/证据摘要与行不一致 | final 判不合格：receipt 必须是合法 agentic-premerge 块并逐项核对内容、结果与目标版本 |
| BEH-177 | planning | 替代检查的拒收点 | not-applicable 的 alternative_checks 写了未在任务中以 [ID] 声明的值，或 final 阶段 `## Checks` 缺该 ID 的 PASS/可读证据 | plan 阶段拒绝未声明 ID，final 阶段拒绝缺失或非 PASS 的 Checks；`e2e check` 只确认 mode、字段与降级批准，不核对逐项原始结果 |
| BEH-178 | premerge | 跨角色并行义务 | 同一层级内 coder 与 tester 均已开工，但两者的占用窗口完全不重叠（编码先跑完再开始测试设计） | premerge/final 判不合格：契约明确时编码与测试设计必须并行，串行需留证；与同角色探针共用同一台账时间戳口径 |
| BEH-179 | apply | 同单元依赖从集成基线开工 | WP2 依赖同单元 WP1；WP1 已实现、独立 review 通过并合入本单元集成基线，单元尚未合入主分支 | 允许派发 WP2（从该集成基线开工）；不得把 WP1 台账提前标为 merged，也不得把“仅建了个集成提交”当成已验收 |
| BEH-180 | apply | 跨单元上游未合入不得开工 | WP2 依赖另一交付单元的 WP1；WP1 只进了集成基线/候选，尚未合入主分支 | 不得派发 WP2（跨单元 code 依赖必须等上游已合入主分支）；运行期不得自行放宽 |
| BEH-181 | apply | worktree 交接按尝试保留历史 | 同一 WP 第一次 coding 后被打回，第二轮由新执行者从 fixing 开始；台账最后一条事件的 executor 是 reviewer | 每轮尝试各有一行交接，绑定该轮认领执行者（agent-B）而非 reviewer 接管者；Received At 不得晚于该轮首次执行事件（第二轮为 fixing）；同一 (WP, Attempt, Executor) 重复行拒绝；多轮缺 Attempt 的旧格式必须逐轮补录 |
| BEH-182 | review | 规划审查有独立阶段 | plan.md 的 Dependency Declaration Review 要求独立 reviewer 核实依赖声明 | 该审查用 `phase: plan`、`stage: plan`，新报告目标为当前 `planningDigest`（不是代码 SHA）；结论写入 `## Dependency Declaration Review`（`Review ID + Round`）；语义规划变化后失效，执行分配不重审；旧 contractDigest 记录仍按全文核对，不得自动换绑 |
| BEH-183 | review | 同一目标多轮审查 | 首轮因缺材料判 BLOCKED，补齐后对同一目标重判 | `Review ID + Round` 为唯一键；Round 在线程内从 1 递增，同一目标也允许再次审查；新一轮未完成或受阻时不得回退引用旧轮 PASS，历史阻断问题逐 ID 闭环 |
| BEH-184 | apply | 权威记录写入职责 | provisioner / merger 想直接改 verification 的 `## Worktree Handoff` / `## Premerge History` | 子角色只返回结构化记录与 handoff_index（可写自己的报告/日志/产物）；由 main 校验后写入权威 verification.md；机械检查只能核对记录，不宣称验证了文件实际由谁写入 |
| BEH-185 | final | 执行安排变化不使历史 receipt 失效 | 单元 A 已合入；随后为单元 B 补齐用例表、执行者或分片等 plan.md/tasks.md 内容 | 历史 receipt 只要求行为契约摘要（requirements_digest = proposal+specs）仍成立；plan/tasks 变化后 final 仍 PASS，不要求历史全文摘要等于当前全文 |
| BEH-186 | final | 需求变化使历史 receipt 失效 | 单元 A 已合入后修改了 specs 中的行为要求 | receipt 的 requirements_digest 与当前不符，final 判不合格：行为契约变化后须重新审查并复验，不能只刷新摘要 |
| BEH-187 | premerge | 首次 premerge 也核就对 | 首次合入某交付单元，`## Premerge History` 尚无任何行，仅提供候选 Verify/review 块与空对账表，没有台账、交付或 worktree 交接 | premerge 仍须先核对该 delivery_unit 的全部工作包就绪（独立 review、ready-to-merge/merged 台账、绑定 WP 与作者的 DELIVERY PASS），不得因缺历史行提前放行 |
| BEH-188 | apply | 修复入口唯一 | 用 `--state fixing` 直接进入修复中，反复 reviewing→fixing→reviewing | 拒绝：进入修复中必须用 `--reopen`，统一新增 attempt、检查重做上限（3 轮）与窗口容量；普通流转不能绕过重派规则 |
| BEH-189 | apply | 流水线容量口径 | 项目问“能否保证子 Agent 总数不超过 coding+testing” | `dispatch.pool` 只限制 coder/tester 并发工作包，merger/validator/scout/provisioner 不计入；文档不得宣称它限制实际 Agent 总数 |
| BEH-190 | premerge | 当前候选未入表也要核就绪 | `## Premerge History` 已有旧候选行，但当前候选尚未入表，且没有派发台账/交付/worktree 交接 | 仍先核对该 `delivery_unit` 全部工作包就绪；不得因“当前候选无行”而提前放行 |
| BEH-191 | final | 引用契约纳入需求摘要 | 变更设 `skip_specs: true`，行为依据来自 Coverage Index 引用的既有规范；修改该既有需求 | `requirements_digest` 必须随引用契约变化；否则历史合入成为漏检，final 会错误放行 |
| BEH-192 | final | 旧 receipt 迁移 | 历史 receipt 缺 `requirements_digest`，随后 plan/tasks（全文摘要）变化 | 旧凭据仅在全文摘要未变时兼容；全文已变则 final 拒绝，保留原文件并用 Receipt Revalidations 指向当前有效复验 receipt |
| BEH-193 | final | Merger 接管 | merger-A 合入后释放，merger-B 接收并继续合入 | Merger Windows 覆盖每次合入且互斥，Merger Takeovers 关联释放/接收、固定目标版本和可读证据时允许身份更替；重叠、缺证据或目标不一致阻断 |
| BEH-194 | final | 需求授权调整后的历史复验 | MU1 已合入，需求获授权调整，修复并重新 review/Verify/合入 | 保留原 receipt，以 Original SHA-256、授权及复验证据关联当前有效替代 Merge ID；替代覆盖原单元全部 WP/TP 及历史候选；循环、失效替代、缺报告或篡改原凭据拒绝 |
| BEH-195 | premerge/final | 逐单元覆盖范围 | MU1 已完成，MU2 尚在实现或跨单元需求待后续闭环 | premerge 核对 MU1 贡献及候选证据；跨单元行声明闭环单元/阶段；final 核对所有行及最终运行证据，不要求 MU1 等待最终 E2E |
| BEH-196 | apply | 最终 E2E 失败修复 | 所有单元已合入，最终 E2E 发现产品缺陷 | 暂停后续功能合入及归档，允许原问题 ID 的必要修复：原 WP/TP 新 attempt/实例、原单元新 Merge ID、独立 review、候选门禁、合入、主分支回归、新一轮完整 E2E；重开受影响任务，不清零失败/重试计数 |
| BEH-197 | final | Review Findings 最新轮阻断 | 第一轮 PASS，第二轮 BLOCKED，表格倒序且 SHA 无时间序 | 手工/导入使用 Review ID / Round / Result，按 Review ID + WP 的最大 Round 判定；第二轮 BLOCKED 阻断，旧轮 FAIL 在有效闭环后保留，不以行序或 SHA 排序 |
| BEH-198 | plan/final | 用户交付 | 用户要求可用工具，只有代码和内部验收报告 | 规划 User Deliverables，明确产物、位置、安装/运行/配置/使用方式、接收方和完成标准；最终逐项交接，区分本地合入、远端交付、部署/发布与归档状态；交付约定不扩大授权 |
| BEH-199 | final/archive | 缺工作包字段的旧凭据迁移 | 旧 receipt 未写 work_packages，需求调整后修复/审查/复验及再次合入均已完成 | 从已核对的 Premerge History 的 Work Packages 恢复集合，保留旧凭据；替代覆盖完整原集合即可通过，空历史集合或显式集合与行矛盾仍拒绝 |
| BEH-200 | premerge | 无关单元 review 受阻 | 当前 MU1 就绪，无关 MU2 最新 review 为 FAIL/BLOCKED | MU1 premerge 可通过；当前单元或必要 code 上游的最新轮失败仍拒绝，final/archive 全量核对 |
| BEH-201 | premerge | 冻结契约不等待上游实现 review | TP1 按冻结可读契约完成编写/review，WP1 仍 coding 且无实现 review | TP1 独立单元 premerge PASS；改成不可读契约路径（静态层级合法）后因契约无效拒绝；code 上游仍须实现 review |

## CLI Regression Baseline


`agentic-workflow.ps1` 的通过记录；本表只覆盖 CLI 结构、依赖和指令传递，不含宿主 Agent 行为场景。

| Run / Script | Time / CLI | Workflow Revision | Covered | Not Covered |
| --- | --- | --- | --- | --- |
| CLI-01 … CLI-14 | 2026-09-21；`openspec-agentic` 报告版本 0.1.0（引擎 `@fission-ai/openspec` 1.13.0） | 仓库内 `assets/openspec/schemas/agentic/`（`fluentspec` 包重命名后） | 规划依赖、skip_specs、changeDir / planningHome.root、重命名后的 plan.md 与 roles/ 角色路径、apply 指令的关键义务标记（`fork_turns` / `BLOCKED` / `verification.md`）、all_done 指令替换、apply/archive 指导输入、8 个常设角色模型解析、未知角色报错及修改后重新读取、项目级 E2E 开关解析与实时重读、not-applicable 降级批准校验、流程内 `e2e run` 留证与 `e2e check`、`[e2e-owned]` 单行回写与 `[final-verification]` 保持待办、`--planning-root` 规划根分离、`--no-write` 只读检查、人工 `--result`、执行前脏状态拒绝、非默认 `maxAttempts` 下的 `--run-if-missing` | 外部 store、宿主实际模型参数消费、宿主 Agent 行为场景、真实产品 verify / review / E2E、not-applicable 下 owned 行与替代验证的关系、执行者交付与主 Agent 汇总的时序（后三者由 `test/e2e.test.mjs` 覆盖） |

| CLI-01 … CLI-15 | 2026-09-28（Windows PowerShell 5.1）；`openspec-agentic` 报告版本 0.2.6（引擎 `@fission-ai/openspec` 1.13.0） | 工作树 HEAD `57d6ae5` + 未提交的文档改动（`AGENTS.md`、`assets/AGENTS.md`、`assets/openspec/agentic.yaml`、`scripts/run-workflow-suite.mjs` 与 `tests/agentic-workflow.ps1` 的注释级改动；schema、roles 与契约文本均为 HEAD 版本） | 同 CLI-01 … CLI-14，另加 CLI-15 共用交接契约投递：apply 指令含 `roles/_shared/role-report.md` 与 `handoff_index`；契约含 12 个字段及 NEW / REUSED / INVALID / PENDING；coder / tester / reviewer / merger / validator / scout / provisioner 七个角色文件均引用；验收程序含 Handoff Traceability 与重开受影响任务；verification 模板含 `## Handoff Index` | 外部 store、宿主实际模型参数消费、宿主 Agent 行为场景、真实产品 verify / review / E2E、交接契约的拒收与重开判断（属行为场景，由宿主实测） |

该记录针对当时本机安装实例，不推定所有同版本发行包等价，也不确定其他版本的兼容范围。
