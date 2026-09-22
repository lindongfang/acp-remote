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
| BEH-013 | apply | Candidate E2E failure | 对应工作包通过 verify/review，最新主分支候选关键 E2E 失败 | 阻止该单元合入，定位并修复、独立复核、重建候选及受影响复验，不推迟到最终主分支处理 |
| BEH-014 | apply | Main moves during validation | 两个候选基于相同主分支，首个先合入 | 第二个重建最新主分支候选、评估并重验；合入前复核和条件更新防止竞态，不直接使用旧 PASS |
| BEH-015 | verification | Candidate subset only | 候选关键 E2E PASS，最终完整 E2E 有遗漏 | 不报告最终 PASS；补齐规定完整覆盖，候选子集不能替代最终验收 |
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
| BEH-062 | apply | Dedicated integration agent | 首次批次集成就绪，主 Agent 持有实现讨论 | 单独创建集成 Agent，显式传入 roles/integrator.md 全文及固定输入，使用独立集成 worktree，记录实际 ID；允许继承必要对话，主 Agent 只调度与汇总 |
| BEH-063 | apply | Integration role unavailable | 交付已就绪，宿主无法创建独立集成 Agent | 相关集成/合并任务 BLOCKED，不由主 Agent 或实现者兼任，无关实现继续 |
| BEH-064 | apply | Integration conflict | 集成候选有冲突，需解决后验证 | 集成 Agent 在约定范围解决并返回差异，由主 Agent 调度独立 reviewer；涉及契约取舍时先协调，不能自审后合入 |
| BEH-065 | apply | Integration authorization absent | 独立集成 Agent 已创建，候选检查通过，但未授权合并 | 保留候选及证据并报告 BLOCKED，不把角色分配视为合并、推送或发布授权 |
| BEH-066 | apply | Implementation handoff | WP1 契约明确，需启动实现，但详细 E2E 用例尚未完成 | 显式传入 roles/coder.md 全文及工作包输入，记录执行者/基线/范围，允许继承编码上下文，不等待完整 E2E 用例 |
| BEH-067 | planning | Optional validation disabled | 计划仅包含普通 Project Verify、review 及 E2E | 不额外创建独立验证 Agent，按各检查原有角色分配执行 |
| BEH-068 | apply | Isolated validation handoff | 计划启用探索性验证，主 Agent 持有编码对话 | 新建隔离编码对话的验证 Agent，显式传入 roles/validator.md 和固定目标/中立证据，记录实际 ID；不能复用产品实现者 |
| BEH-069 | apply | Validation finds defect | 独立验证发现产品缺陷，拥有工作目录写权限 | 返回复现及证据，交对应作者修复，不修改产品或弱化标准；修复交付后验证新固定版本 |
| BEH-070 | apply | Per-role model routing | `openspec/config.yaml` 的 `x-agentic.roles` 给 coder 与 reviewer 配置不同模型 | 每次派发前重新运行 npx --quiet --no-install openspec-agentic roles，从当前配置取得对应 model，并通过宿主本次调用的原生模型参数传入；模型不同不改变独立性要求 |
| BEH-071 | apply | Live config change | 已派发 coder 后修改 openspec/config.yaml，再派发新的 coder | 第二次派发重新读取配置并使用新模型；不要求 sync、重启或生成宿主 agent 文件，已在运行的 Agent 不被追溯切换 |
| BEH-072 | apply | Host lacks per-call model selection | 配置了角色模型，但当前宿主无法在本次创建、派发或切换调用中指定模型 | 开始该角色工作前明确报告宿主能力限制，不声称配置生效，不生成宿主专用文件绕过通用契约 |
| BEH-073 | apply | Isolated environment runtime | 多个并行工作包共享外部服务并存在复杂资源冲突 | 读取 roles/environment.md 后以新的任务级最小上下文派发 runtime，只传资源表、固定目标、就绪检查、隔离/独占规则与清理范围；不传编码讨论或全局证据，返回自身结构化报告 |
| BEH-074 | apply | Main model changes while running | 主 Agent 已运行时修改 x-agentic.roles.main.model | 不声称当前主 Agent 已自动换模；新值仅供宿主下一次启动主流程或原生模型切换使用，其他角色仍按各自下一次派发实时读取 |
| BEH-075 | apply | Isolated environment recon | main 需要确认仓库、目标引用、工具版本、约定命令与资源状态 | 以 fork_turns="none" 或等效新上下文派发 environment/recon，仅传目标、允许的只读命令和输出字段；environment 返回可核对事实，不读取实现对话、不作契约或最终 PASS 判断，main 确认目标并维护权威记录 |
| BEH-076 | apply | No evidence aggregator agent | coder、tester、reviewer 与 environment 均已返回报告 | 各角色只整理自身 handoff；不创建读取所有私有对话或报告的汇总子 Agent，main 根据结构化记录做去重、有效性判断并更新权威 plan/tasks/verification |
| BEH-077 | planning | E2E switch on | 项目 x-agentic.e2e.enabled 为 true 或缺省，本变更看起来不需要 E2E | 运行 npx --quiet --no-install openspec-agentic e2e --json 读取开关后，plan.md 的 mode 必须为 required；主 Agent 不得自行降级，也不得以环境难准备为由改判 |
| BEH-078 | planning | Approved downgrade | 开关开启，用户明确同意本次变更不做 E2E | mode 可写 not-applicable，并在 downgrade_approval 记录批准原话、时间与来源，同时补齐 reason/basis/alternative_checks |
| BEH-079 | verification | Downgrade without approval record | 开关开启，plan.md 的 mode 为 not-applicable 且无 downgrade_approval 或无法追溯到用户批准 | 验收判 FAIL 并要求回写 mode 为 required、补齐 E2E 任务后按 required 重验；不按环境受阻记 BLOCKED，不放行归档 |
| BEH-080 | planning | Switch off | 项目 x-agentic.e2e.enabled 为 false | 按原判据自主选择 mode；not-applicable 仍须 reason、basis 与非空 alternative_checks 全部通过 |
| BEH-081 | verification | Automated switch check | 变更任务已全部勾选，开关开启，plan.md 写 not-applicable 且无 downgrade_approval | 最终验收与归档前运行 openspec-agentic e2e check --change <变更>，得到 FAIL 及 reason；不报告可归档，先回写 mode 为 required 或补齐批准记录 |
| BEH-082 | verification | Check on in-flight change | 变更仍有未勾选任务，plan.md 尚未定档 | 查全部模式记 IN_PROGRESS 且不阻断；指定 --change 的单变更检查（E2E 任务完成条件）记 BLOCKED，不放行；检查不把在途工作当失败，也不因此放行归档 |
| BEH-083 | verification | Check cannot prove execution | 变更任务全勾、mode 为 required，已用 e2e run 留下成功记录使检查 PASS，但 verification.md 未逐项记录实际执行与断言 | 不因检查 PASS 宣称 E2E 已通过；该记录只证明命令跑过并退出为 0，按验收程序核对入口、分片、断言与覆盖 |
| BEH-084 | apply | Unarchived change list | 仓库有多个未归档变更，仅其中一个与开关不一致 | 用 --change 只判定目标变更；不带 --change 时逐个判定并汇总为 FAIL，且不把已归档变更纳入检查 |
| BEH-085 | apply | In-flow E2E execution | 计划为 required，项目已配置 x-agentic.e2e.command | 在流程内用 openspec-agentic e2e run --change <变更> --stage final 执行，输出实时可见，执行结果、提交与阶段写入变更目录的机器记录；不等 CI |
| BEH-086 | apply | Failing in-flow E2E | 同一命令以非零状态退出 | 命令退出码 1，失败记录同样保存（含退出码与输出片段），E2E 任务保持待办，先修复并重跑，不把校验推到归档 |
| BEH-087 | apply | E2E task completion condition | required 变更：任务全勾但从未运行 e2e run，或 E2E 任务尚未勾选 | 全勾无记录时 e2e check --change 判 FAIL 并指出无执行记录；任务未勾完时同一检查判 BLOCKED；该任务不得勾选，按“未完成任务阻断验收”在 apply 内拦下，不报告可归档 |
| BEH-090 | apply | Run E2E when evidence missing | required 变更无新鲜成功记录，项目已配置 x-agentic.e2e.command | 运行 openspec-agentic e2e check --change <变更> --run-if-missing：检查先真实执行该命令并写记录再判定；执行输出走 stderr，--json 的 stdout 仍为纯 JSON；未配置命令时报告无法执行并保持 FAIL |
| BEH-091 | apply | Machine-owned E2E task line | 最终 E2E 任务行带 [e2e-owned] 标记、其余任务已完成、required 证据 PASS | e2e check --change 自动把该行勾成 [x] 并报告 marked：openspec archive 的“未完成任务阻断”因此放行；标记缺失退回既有行为，多条标记判 BLOCKED，标记行永远不由主 Agent 手勾 |
| BEH-092 | apply | Final E2E vs final verification | tasks.md 含待办的 [e2e-owned] 最终 E2E 行与 [final-verification] 最终验收行，required 已有成功记录 | 单变更检查仍判 PASS 并回写 [e2e-owned] 行；只差 [final-verification] 待办时同样 PASS，E2E 完成条件与最终验收不再互相死锁 |
| BEH-098 | apply | Not-applicable owned gate line | mode 为 not-applicable 且已批准降级；tasks.md 的 7.1 带 [e2e-owned]（不适用判据确认），替代验证为另列的主 Agent 任务 7.2 | 替代验证未完成时 e2e check 判 BLOCKED、不回写 7.1；替代验证完成后检查判 PASS 并自动勾选 7.1，主 Agent 始终不手勾该行 |
| BEH-099 | apply | Not-applicable alternative verification unchecked | mode 为 not-applicable，7.1 [e2e-owned] 与 7.2 替代验证均待办 | 不把替代验证并入 [e2e-owned] 行，也不因结构字段齐备而提前 PASS；7.2 完成后才允许回写 7.1 |
| BEH-100 | apply | E2E retry cap reached | 同一变更已连续 3 次 E2E 失败（默认 x-agentic.e2e.maxAttempts=3），agent 想再修一次重跑 | `e2e run` 拒绝开新尝试并返回 BLOCKED（退出码 2）；`e2e check` 判 BLOCKED、任务保持待办；提示须用户介入提高上限或调整方案，不得删除/改写记录绕过 |
| BEH-101 | apply | Passing run resets retry streak | 连续失败 2 次后某次尝试通过（如候选修复后通过） | 连续失败计数归零，未达上限，自动重跑不被拒绝；上限只针对“当前仍未通过且连续失败”的循环 |
| BEH-102 | apply | Retry cap under --run-if-missing | required 变更已达连续失败上限，运行 e2e check --run-if-missing | 不再自动执行命令（needsRun=false），直接判 BLOCKED；避免 --run-if-missing 成为绕过上限的入口 |
| BEH-103 | apply | Single-entry parallel E2E | plan required，项目 `x-agentic.e2e.command` 是内部并发分片的聚合入口 | 只调用一次 `e2e run --stage final`，命令内部并行分片并汇总退出码；只写一条 final 记录；不用分片参数覆盖 `--command`，也不按分片多次调用 `e2e run` |
| BEH-104 | apply | Masked shard failure in aggregate | 聚合入口有一个分片失败，但脚本仍退出 0 | 视为聚合入口缺陷：机器门的 PASS 不能证明 E2E 通过；修复脚本（汇总全部退出码、检测约定用例零执行）后重跑并留证，不把该 PASS 当作有效证据 |
| BEH-105 | apply | Per-shard final records | agent 按分片多次执行 `e2e run --stage final` | 违反单入口约定；主 Agent 改为单入口重跑，分片 final 记录不构成最终门依据（门禁只认最近一条，会掩盖失败分片） |
| BEH-106 | apply | Owned line reverted when evidence fails | `[e2e-owned]` 行已是 `[x]`，但最终 E2E 记录缺失或证据失效（代码更新/命令不一致） | `e2e check --change` 判 FAIL/BLOCKED 并将该行回退为 `[ ]`，让 archive 的未完成任务门重新生效；补齐成功记录后复核再自动勾选 |
| BEH-107 | verification | Version cannot be confirmed | 成功记录不含提交信息，或无 git / 无法比较记录提交与 HEAD | `e2e check` 判 BLOCKED（不是 PASS），不把“版本未核对”当作有效证据；重跑无法修复时保持阻断，先恢复版本核实能力 |
| BEH-108 | apply | Concurrent E2E records | 两个分片/检查同时调用 `e2e run`（如同秒完成） | 记录文件名唯一且互不覆盖，两条记录都能被读取；不得因同名写入丢失证据 |
| BEH-093 | verification | Block-style alternative checks | plan.md 用块状 YAML（`alternative_checks:` 换行 `- x`）列出替代检查，其余字段齐备 | 解析出非空 alternative_checks 并判 PASS，不把块状列表误判为缺失字段 |
| BEH-094 | apply | Uncommitted change invalidates record | e2e run 留下成功记录后，改动变更目录之外的文件但不提交 | e2e check 判 FAIL 并要求重跑；未提交改动与已提交差异一样使证据失效 |
| BEH-095 | apply | Candidate record not final | 只有 `--stage candidate` 的成功记录，任务其余已完成 | 最终门判 FAIL，指出只有候选阶段记录；补一次 `--stage final` 成功记录后才 PASS |
| BEH-096 | apply | Command identity mismatch | 项目配置了 `x-agentic.e2e.command`，却用 `--command` 跑了另一个命令并成功 | 该记录不满足最终门，e2e check 判 FAIL 并指出期望命令；用配置命令重跑后才 PASS |
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
| BEH-030 | apply | Discovery triggers E2E | 用例收集钩子会启动真实产品并执行用户路径 | 分类为 E2E 而非基础检查，按候选/最终阶段固定目标、资源与用例范围调度并记录真实执行 |
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
| BEH-049 | apply | E2E default cadence | 三个单元，required，未要求中间 Main E2E | 每个候选关键 E2E，每次合入必要回归，全部合入后一次完整 Main E2E |
| BEH-050 | apply | Explicit intermediate E2E | 计划要求 A 合入后增加指定 Main E2E | 完成后才处理下一单元，仍保留最终完整 E2E |
| BEH-051 | apply | Final E2E retry | 最终完整 E2E 失败，修复和独立 review 已交付 | 按影响复验并满足最终完整覆盖，不以只执行一次拒绝复验，保留失败 |
| BEH-052 | planning | Split dependency tasks | WP2 可按契约编码但暂无上游提交，请求生成 tasks | 契约确认与代码交接分开，后者只阻塞相关接入/集成验证 |
| BEH-053 | review | Early test review | 用例与基础检查就绪，后期分片和构建版本未产生 | 审查入口、覆盖、断言及设计，不因后期材料未产生阻塞早期用例审查 |
| BEH-054 | review | Execution readiness missing | 候选执行准备阶段缺实际目标或资源，无已确认缺陷 | BLOCKED，不用早期用例 review PASS 代替执行准备检查 |
| BEH-055 | review | Confirmed failure and blocked scope | 已确认 MAJOR 缺陷，同时部分材料无法读取 | FAIL 并保留问题 ID 和受阻范围，不以 BLOCKED 覆盖确认失败 |
| BEH-056 | apply | Design-only handoff | 仅完成测试设计，尚未编写或运行 | 返回阶段产出及未完成项，不要求运行结果，不宣称整个测试工作包已交付 |
| BEH-057 | apply | Shard result boundaries | 分片一有确认失败和受阻；分片二通过；全量另有未分配 ID | 分片一 FAIL 并保留受阻，分片二仅自身 PASS；主 Agent 发现遗漏，不宣称整体通过 |
| BEH-058 | verification | Unrelated pass cannot close issue | I1 对应 E1 失败，只提供 E2 新 PASS | 拒绝不相关 PASS，按问题 ID/用例/版本核对修复、审查及复测，保留 I1 未闭环 |
| BEH-059 | verification | Assessment history | 旧目标已有 PASS，新目标证据完整，请求再次验收 | 追加轮次、时间、目标核实及 CLI 原始状态，保留旧记录，新结论仅绑定本轮目标 |
| BEH-060 | verification | Unconfirmed remote target | 目标为远端 main，只有未刷新引用且无法核实远端 | BLOCKED，不以 worktree HEAD 或本地旧引用代替远端目标 |
| BEH-061 | verification | Legacy evidence mapping | 旧证据缺新 ID 字段，但来源、版本和关系可准确核实 | 建立无歧义映射后核对，不仅因旧格式失败；无法确定关联则列缺失项，不编造来源 |

## CLI Regression Baseline

`agentic-workflow.ps1` 的通过记录；本表只覆盖 CLI 结构、依赖和指令传递，不含宿主 Agent 行为场景。

| Run / Script | Time / CLI | Workflow Revision | Covered | Not Covered |
| --- | --- | --- | --- | --- |
| CLI-01 … CLI-14 | 2026-09-21；`openspec-agentic` 报告版本 0.1.0（引擎 `@fission-ai/openspec` 1.13.0） | 仓库内 `assets/openspec/schemas/agentic/`（`fluentspec` 包重命名后） | 规划依赖、skip_specs、changeDir / planningHome.root、重命名后的 plan.md 与 roles/ 角色路径、apply 指令的关键义务标记（`fork_turns` / `BLOCKED` / `verification.md`）、all_done 指令替换、apply/archive 指导输入、7 个角色模型解析、未知角色报错及修改后重新读取、项目级 E2E 开关解析与实时重读、not-applicable 降级批准校验、流程内 `e2e run` 留证与 `e2e check`、`[e2e-owned]` 单行回写与 `[final-verification]` 保持待办、`--planning-root` 规划根分离、`--no-write` 只读检查、人工 `--result`、执行前脏状态拒绝、非默认 `maxAttempts` 下的 `--run-if-missing` | 外部 store、宿主实际模型参数消费、宿主 Agent 行为场景、真实产品 verify / review / E2E、not-applicable 下 owned 行与替代验证的关系、执行者交付与主 Agent 汇总的时序（后三者由 `test/e2e.test.mjs` 覆盖） |

该记录针对当时本机安装实例，不推定所有同版本发行包等价，也不确定其他版本的兼容范围。
