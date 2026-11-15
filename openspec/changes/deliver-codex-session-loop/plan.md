# Execution Plan

## Execution Model

Q-001，2026-10-31。本轮只修订规划，不改产品实现或实际数据。已有schema4、旧子合同和旧验收保留原快照范围；当前八份specs和design取代冲突规则，准确新DDL/DTO与协议完成算法需另阶段独立冻结。

主执行维护规格、合同和计划。独立tester依据当前文件、AGENTS/config及角色读取，不继承主执行对话，在隔离候选目录编写tests；独立reviewer只读固定规划快照并输出报告，不代签产品功能。

## Parallel Assessment

主change核心规格及跨change引用存在共同授权、期限和历史依赖，由主执行串行改写，避免并发改同一文件。规格稳定后独立tester在隔离目录生成用例，可与主执行下游文档同步并行；合入后由另一reviewer独立核对同一快照。后续产品源码按WP2→WP3→WP4依赖串行；tester/reviewer正式验证采用独占数据根、端口和target，不并写产品目录。

## Risks

- 风险由当前独立验收、环境限制和未闭合的真实 Harness/Windows/供应链边界记录；未验证项保持 not-run，不用计划结构代替功能证据。

## Work Packages

| ID | Goal / Scenarios | Dependencies | Agent | Write Scope | Deliverable | Verify |
| --- | --- | --- | --- | --- | --- | --- |
| WP1 | Q-001决定、八份规格、限制核对及下游一致性 | 已归档节点接入、用户确认 | 主执行 | 本change规划/合同及相关docs/README/config/下游规划 | 规则单一来源、旧范围说明 | strict/diff及CL-087起限制核对 |
| WP2 | 节点登记、受理授权、共享队列、停止原因与严格审批 | WP1及独立用例/方案检视 | 后续coder | domain/application/session_storage、必要声明、开发测试和合同 | 未发送正文仅内存、去重及单次派发、附加审计分离、非成功结束暂停 | CL-003/004/014/017–021/075–084、CL-087起及CL-117–122 |
| WP3 | ACP真实来源、完成乱序、停止原因、有效配置、独立接收和资源 | WP2，版本合同冻结 | 后续coder | adapters/acp/platform/harnesses及node/lib资源装配、开发测试 | 无重放模式、可靠核验出口、共享实例、完整树及双版结束语义 | CL-032–041/047–074、CL-087起及CL-117–123 |
| WP4 | 同一认证通道共享队列及Web交互 | WP2/3，DTO冻结 | 后续coder | management_ws、clients/web、node/lib最终装配、README | A/B共享编辑取消、优先中断、停止原因显示、失败继续、历史提示 | CL-009/022/030/042/085/086、CL-087起及CL-120–123 |
| WP5 | 独立用例维护和正式验收 | 规划依spec/design；执行依冻结产品 | 独立tester | tests候选、acceptance/suites及自有runs | 稳定旧ID/标题、逐Scenario映射、新行为用例及真实边界 | 当前完整tests；不把本轮定义当执行 |
| WP6 | 独立方案及交付检视 | 方案依稳定文件；交付同WP5产品快照 | 独立reviewer | 只读产品及自有报告目录 | 问题ID和复查，固定范围 | CL-031、CL-117–123及限制/需求映射人工检查 |

## Merge Order

| Order | Work Package | Reason | Re-verify After Merge |
| --- | --- | --- | --- |
| 1 | WP1→WP5候选→WP6 | 核心规则先稳定，用例不受旧实现迁就 | strict、逐Scenario映射、差异 |
| 2 | WP2 | 准确队列状态/DDL/授权先冻结 | 内层及真实SQLite故障/并发 |
| 3 | WP3 | 协议事件不写业务SQL | 双版帧合同、嵌套审批及完整树 |
| 4 | WP4 | 最终认证和Web装配 | 完整工程检查、真实Web |
| 5 | WP5/WP6 | 同固定产品快照并行独立验证 | 真实必需项及问题复查 |

WP2→WP3→WP4共享node/lib时严格串行，后合入者重验集成；本轮仅WP1和规划用例/检视。不得因旧任务勾选或结构complete自动恢复代码接线。

Q-004 的停止原因、队列暂停和CL-122原项版本/无回退属于 WP2/WP3，已停止状态及继续交互属于 WP4；CL-117–123 的独立执行和同版检视属于 WP5/WP6。继续操作须用稳定请求身份去重；`Completed`等协议枚举只保留原始事实，业务停止分类不得以其名称代替负责规格；早终态与迟插入仍须分别关联。任一必需分支未执行、失败或没有同版证据时，任务保持待办。

6.8 最终开发证据已固定：[Q-004 6.8 evidence](../../../testing/runs/deliver-codex-session-loop/2026-11-10-q004-final-6-8-evidence.md)。v1/v2 × 四种非成功/取消原因 8 个真实 Windows 子进程场景、v2 早终态迟到插入与永不插入、v1/v2 × 三种立即发送结果、资源释放和重复释放均通过；队列 owner/SQLite 的编辑、取消、资源换代及中断失败均暂停保留项且零回退，shared queue 4/4 按显式继续恢复原顺序。旧 shared queue 用例中“失效后自动执行 X”的两条断言与 CL-122 冲突，已改为 `SelectionChanged`、全部暂停、零派发，显式继续后才按原顺序恢复；这不是放宽产品断言。`fmt/diff/OpenSpec strict` 同版通过。该证据仅覆盖 L3/L4 开发夹具及真实 Windows 进程，不代表 CL-117–122 的 L5 或 CL-123 真实 Harness/Web 验收；6.9与6.10保持待办。

6.9 最终开发证据已固定：[Q-004 6.9 evidence](../../../testing/runs/deliver-codex-session-loop/2026-11-10-q004-final-6-9-evidence.md)。生产 Web 已共用可测试的队列快照展示和显式继续身份协调；实际 Edge DOM 7/7 覆盖四种停止原因、选择失效、中断未确认、X/Z 暂停及失败重试同身份，Web 29 文件667例、check/build通过。节点 WSS 4/4、共享输入 L4 39/39（含普通继续不抢占活动项）、选择失败 owner 4/4；真实 Windows 子进程停止矩阵及 v1/v2×成功/失败/取消优先级矩阵通过，目标进程检查为空。该证据只关闭 6.9 开发门；6.10仍须独立 tester/reviewer 用真实 Harness/Web逐ID验收，不能以合成浏览器快照或受控原生对端代签 CL-123。

## 当前执行收敛（2026-11-13）

本轮按用户要求减少重复构建、复制、报告和串行等待；产品规则、验收预期、独立角色及完整任务完成判据不变。主执行按下面的完整功能批次组织工作，不再把逐文件安装或单份收据作为独立交付目标。

| 顺序 | 功能批次与退出条件 | 可复用输入与本轮动作 |
| --- | --- | --- |
| 1 | 当前认证 WSS 的上传、共享队列、正文读取与释放组合；逐场景有当前版本结果，失败形成具体修复项 | 使用[固定 PE 复用门](../../../testing/runs/deliver-codex-session-loop/2026-11-13-wp4-failure-review-v1/ACPI-PE-REUSE-GATE-20261113.md)允许的 library/child，先 list 核数，再有界串行实跑；当前源码不变时不重新编译。当前 native_release 7/7 只在原范围复用，不重复作新完成数。 |
| 2 | 新 Web 实际页面与真实 Harness 的最小闭环，再扩展 A/B 队列、审批和停止恢复场景 | 在批次1执行期间，由独立角色并行核实本机真实入口、来源、可用能力和已有浏览器资产；先暴露环境缺口，不能等全部局部回归后才检查。旧 Rust PE 内的旧 Web dist 不证明新页面，helper/FakeSocket 证据不能替代此批次。 |
| 3 | 剩余协议、存储、资源、安全及全工程检查缺口收口 | 按 tasks/tests 的未满足判据逐项补齐；只重验受改动影响的范围和必要跨层回归，版本稳定后执行完整适用入口。已有有效失败记录按真实原因处理，npm audit仍按用户决定未执行。 |
| 4 | 同一最终版本的独立验收、检视及规范/长期用例同步 | tester与reviewer并行读取同一固定版本；问题由原提出者闭环。只有完整判据满足才勾原任务，不把准备、局部绿或报告齐备当完成。 |

执行约定：

- 每批只维护一个当前产品身份和一个结果入口，原日志/失败记录保持不变；汇总引用既有证据，不再复制完整测试清单或为纯汇总另建源码副本。身份或写范围发生实质变化时才新增冻结版本。
- 同版本、同范围、环境相符的有效证据先做影响核对再复用；文档/报告更新不使产品测试自动失效。修复先跑 Red/受影响测试，稳定批次合入后做一次适用集成检查；不同角色不机械重复相同全量命令。
- 独立性靠不同执行者、固定输入与隔离副作用保证；reviewer只读已有候选和日志，不复制构建环境。tester需要运行时使用自有数据/端口/缓存，源码副本与target只在实际执行需要时建立，Rust并行构建仍各有target。
- 复用已审运行器；新增执行方式、资源所有权、安全边界或断言变化才作相应增量检视，不为纯路径/标签变更重开整套方案。执行前一次核工具链、owner、夹具与准确测试数量，结果保存机器可读状态和原日志，避免末尾手工汇总错误引发重跑。
- 同一故障已复现并定位后，下一次运行必须对应具体修正或不同诊断假设；重复环境阻断时明确记录，转做不依赖它的工作，不原样反复尝试，也不削弱ACL/断言。
- 每轮汇报只列完整功能状态、当前阻断和下一项退出条件；测试数字注明是否为复用或重复回归。不用9/28换算工时百分比，不在未核实真实环境前承诺整体完成时间。

## Project Verify

| Check ID | Command / Working Directory | Scope / Pass Criteria |
| --- | --- | --- |
| PV1 | 根：openspec.cmd validate --all --strict；git diff --check | 所有活动规格结构和格式；不替代功能 |
| PV2 | 冻结规划：检查全部Requirement/Scenario→tests映射及限制核对表 | CL-031规划子范围，独立reviewer核对具体断言 |
| PV3 | 后续根：npm.cmd run check；npm.cmd run build:rust；按仓库Windows owner前置 | CL-001/002；本轮未执行 |
| PV4 | 后续隔离SQLite、双版协议替身、节点认证入口 | CL-003/004/014/017–021/075–084、CL-087起，真实/模拟分列 |
| PV5 | 后续隔离Windows+可信Web+固定真实Harness | CL-030/046/054/057/060/085及新队列真实组合；缺分支not-run |

## Code Review

方案检查由独立reviewer读取当前用户确认、规格、design和完整tests，不继承主执行或tester对话。新状态机/DDL/协议算法须编码前另审；本轮文档通过不能授权未定义算法或迁移。交付检查同真实产品快照，旧报告不外推。

## Main E2E

```yaml
mode: required
```

真实节点+可信Web A/B+本地Harness：无重放/有重放实际能力→共享三项→编辑/取消→中断优先→失败暂停/继续→严格审批→释放恢复。验证A撤销不取消旧输入但阻止新读取/操作，节点重启不恢复正文、可能发送不重发，以及完整Windows树清理。

## Handoff

Q-001原始确认和边界在proposal/specs；传递文件及SHA256快照，不传开发对话。历史完成项与旧runs仅原范围复用；正文/视图限制、签名期限及actor-at-dispatch等冲突代码仍待改。tester生成候选后集成，独立reviewer核对引用及场景，不声明产品通过。

## Current Review Status

2026-11-13（当前WSS功能批次）：已复用同源固定library/child完成12组27/27开发回归，包括ACPI-R01–R09及codec13、R10三项、撤销/共享/登记/编辑取消优先/继续五项、正文读取四项及release路由/登记两项。真实Node/SQLite/TLS/WSS配合FakeProtector和受控原生子进程；每组及list的原进程自然exit0、自有Job归零、未触发强制清理，515既定源叶、PE和index前后未变，没有Cargo/重编或新增源码副本。证据入口为[本批RESULT](../../../testing/runs/deliver-codex-session-loop/2026-11-13-wss-combination-v1/execution/RESULT.json)，含准确枚举和各组原stdout/stderr/PROCESS；独立tester/reviewer正并行核对原断言、复用范围及剩余缺口，尚不代签逐R独立结论。真实Harness/新Web页面就绪检查并行进行，本批不证明新UI或CL-030/031/123。整体9/28不变，npm audit未执行。

2026-11-13（Web安装闭环及执行收敛）：固定Green精确九叶已安装主区，check/full917/917/build均exit0；独立tester的917/917+补72/72与原reviewer WEC-FR-002关闭结论已完成，安装后独立复核也通过，见[安装后检视](../../../testing/runs/deliver-codex-session-loop/2026-11-13-wp4-failure-review-v1/WEB-POST-INSTALL-20261113.md)及[主区收据](../../../testing/runs/deliver-codex-session-loop/2026-11-13-web-error-contract-fix-v1/root-integration-v1/POST-INSTALL-RESULT.json)。当前Rust515叶与既有library/child绑定一致，已获准复用做下一轮当前源WSS开发回归，正式运行器尚待固定；没有把此复用门当执行通过。以上“当前执行收敛”改为完整功能批次和增量证据复用，未修改产品规格、完整判据或任务勾选。整体仍9/28，真实main页面/WSS组合/Harness及完整交付未完成，npm audit未执行。以下段落保留各自历史时间点。

2026-11-13（Web交付预检补充）：原reviewer已逐叶核351叶候选、精确九叶差异和开发Red/Green原日志，交付预检通过；但WEC-FR-002保持open，缺少终止错误清除既有重连timer的完整矩阵，以及三个可恢复码驱动真实consumer连接恢复后按socket世代统计零mutation重发的证据。独立tester在自有源码和完整依赖副本补这些边界，并补新增commit_unconfirmed在两个认证阶段的拒绝用例；正式执行脚本仍须先经reviewer绑定。此时未安装Root、未形成独立执行通过结论。主执行已准备[精确安装与复验方案](../../../testing/runs/deliver-codex-session-loop/2026-11-13-web-error-contract-fix-v1/root-integration-v1/PLAN.md)，只读preflight核Root347/候选351及index通过，尚未执行install或主区test/check/build。报告见[交付预检](../../../testing/runs/deliver-codex-session-loop/2026-11-13-wp4-failure-review-v1/WEB-GREEN-PRECHECK-20261113.md)。整体9/28不变，npm audit未执行。

2026-11-13：Web顶层错误合同窄修复已在隔离候选完成开发Red/Green，尚未安装Root。编码门确认50码闭集（三产品叶+六测试叶），运行器将cache/config/token临时输出独占，并逐次核共享依赖1439叶前后无变化。原17码文案等价抽取17/17一致；真实Red共268项、116失败（parser33/client33/文案50），Green相同测试268/268通过；Web全量917/917、类型检查和构建exit0。Green351叶canonical为`0448cbe203f87f1b75077f07416e1d8a021633fc8871e874d6a8528b453297a0`，Root347叶仍与编码前基线一致、Git index不变。见[开发结果](../../../testing/runs/deliver-codex-session-loop/2026-11-13-web-error-contract-fix-v1/DEVELOPMENT.md)及[逐叶收据](../../../testing/runs/deliver-codex-session-loop/2026-11-13-web-error-contract-fix-v1/DEVELOPMENT-RESULT.json)。独立tester正在自有源码/完整依赖副本准备正式验收，原reviewer检视同版交付；尚不代签独立结果、Root合入、真实WSS/main页面/Harness或完整WP4。整体仍9/28，npm audit按用户决定未执行。以下记录保留历史状态。

2026-11-13：WP4-FR-002已完成服务端限域闭环：独立tester正式报告确认专项4/4、session_commands6/6、native_release7/7（13个不同顶层测试）；原reviewer关闭该问题并放行两叶，Root精确安装后重新编译6/6、7/7与fmt均exit0。515叶逐项bytes/SHA及Git index吻合，Root新测试PE身份另存[安装后验证收据](../../../testing/runs/deliver-codex-session-loop/2026-11-13-native-view-error-mapping-v1/ROOT-POST-INSTALL-VERIFICATION.json)，独立执行及包装脚本末尾汇总错误的补件见[正式报告](../../../testing/runs/deliver-codex-session-loop/2026-11-13-view-error-independent-v1/REPORT.md)，原reviewer交付结论见[FR002复核](../../../testing/runs/deliver-codex-session-loop/2026-11-13-wp4-failure-review-v1/FR002-DELIVERY-20261113.md)。以下候选未安装/open文字保留其历史时间点。下一窄包为Web顶层管理错误闭集与准确文案；只读初审明确排除close-only revoked及payload status.error，上传catch仍保留原顶层错误，不扩大上传流程；独立tester补用例、reviewer核方案后再编码。整体仍9/28，不代表完整WP4、WSS、真实Harness或全库strict通过；npm audit按用户要求未执行。

2026-11-13：WP4-FR-002服务端候选已完成开发Red/Green：旧实现2绿2红，配置错误实际被误报为protocol_failure；新映射专项4/4、session commands6/6、native release7/7及fmt通过（13个不同顶层测试，专项重复另计）。最终515叶canonical为`5f076a551d81c3b3ed28b18a0c6bc3d66772969170178962ec3b745c9aaf9942`，只改session_commands和新开发测试；包括29原生变体、17协议子项、30直接ViewError及两平台payload/SqlClose双态的固定JSON公开面。见[开发记录](../../../testing/runs/deliver-codex-session-loop/2026-11-13-native-view-error-mapping-v1/DEVELOPMENT.md)。独立tester正式实跑4/4、6/6、7/7均exit0，但其末尾PowerShell汇总脚本发生兼容错误，正在新增收据补件，暂不以未完成报告代签交付；原reviewer已完成同版静态及开发证据预检，FR002仍open。候选未安装Root，完整任务不勾，Web闭集缺口另行只读梳理。

2026-11-13：环境继承修复已通过独立限域验收（process owner 13/13、native release 7/7、platform owner 13/13，另重复关键真实继承例1/1）；原提出者关闭 WP4-FR-001 并放行三叶精确合入。主工作区安装后重新编译，三组33/33与fmt再次通过，514叶逐项匹配固定候选，Git index未改。见[独立报告](../../../testing/runs/deliver-codex-session-loop/2026-11-13-environment-independent-v1/REPORT.md)、[安装后复核](../../../testing/runs/deliver-codex-session-loop/2026-11-13-wp4-failure-review-v1/POST-INSTALL-20261113.md)和[主区验证收据](../../../testing/runs/deliver-codex-session-loop/2026-11-13-environment-inheritance-fix-v1/ROOT-POST-INSTALL-VERIFICATION.json)。旧清单聚合算法未能复现，保留原声明并以逐叶核验及明确规范的canonical摘要补件固定身份，不冒称旧算法已验证。WP4-FR-002仍open；下一步按[29分支映射方案](../../../testing/runs/deliver-codex-session-loop/2026-11-13-wp4-failure-review-v1/FR002-PLAN-REVIEW.md)在隔离候选中串行完成服务端Red/Green，再由独立tester和reviewer对同版验证。写集只含session_commands及新开发测试，不与只读角色并写；Web闭集错误合同另有缺口，当前服务端修复不代签Web或完整WP4。整体仍9/28，npm audit按用户要求未执行。

本轮规格、合同、用例及上下文修订不等于实现交付。独立用例与检视状态随最终报告记录；产品测试、真实Harness、浏览器、进程树与迁移未在本轮执行。

2026-11-10：ACPI 路由 v6 编码前审查包已固定，见[合同与写集](../../../testing/runs/deliver-codex-session-loop/2026-11-10-acpi-gate-v6/CONTRACT.md)和[状态记录](../../../testing/runs/deliver-codex-session-loop/2026-11-10-acpi-gate-v6/STATUS.md)。本版补齐实际 design 增量，并将当前 queue-management 纳入同一原服务集合的原子安装；保留完整 v4 合同及原测试定义。独立 reviewer 的直接任务和文件路径任务均被报告为不可读载荷，未实际审查；不以该工具失败代签通过。活动规格及产品接线未合入，4.1/6.10保持待办，整体9/28。该范围涉及共享管理入口和Node装配，先串行完成编码门；实现后WP5/WP6才按同一新产品摘要独立验证。

2026-11-10：ACPI 路由 `acpi-route-v10` 已由不继承开发对话的独立 tester 和 reviewer 分别判定 PASS，未关闭定义阻断项为零。清单 SHA-256 为 `8A7DE56D29B67542AEA9091DE96E8C3F6183B47DB399E6AF381AEA25D03A595A`；报告见[测试定义审查](../../../testing/runs/deliver-codex-session-loop/2026-11-10-acpi-gate-v10/TESTER-REVIEW.md)与[编码门审查](../../../testing/runs/deliver-codex-session-loop/2026-11-10-acpi-gate-v10/REVIEW.md)。该放行仅覆盖 v10 的 Rust 认证 WSS 上传路由编码门；Web UI、真实 Harness、最终验收和归档未放行。WP4 按 `node/input_upload.rs` → `node/session_services.rs` → `management_ws.rs` → `session_commands.rs` → `lifecycle.rs` 串行修改，并在 WP5/WP6 对实现后的同一源码摘要重跑 ACPI-R01–R10 和独立交付检视。规划合入及产品实现不改变当前 tracked 进度；4.1与6.10仍待办。


## Q-001本轮交付摘要

2026-10-31：独立tester候选按原字节集成到tests.md，SHA256 `7928246d0433783833a4b372c5cea11eba28cdde176525d1f424b831ea3614d6`，107例，保留旧86个ID/标题并新增21例；八份规格66个Requirement/121个Scenario均有原名映射。定义检查及限制说明见 [测试定义交接](../../../testing/runs/deliver-codex-session-loop/2026-10-31-q001-test-definition.md)。仅规划定义，全部产品用例未在本轮执行。

主执行与独立reviewer已分别运行OpenSpec全部严格结构检查11/11及diff-check通过；[独立规划检视及集成复查](../../../testing/runs/deliver-codex-session-loop/2026-10-31-q001-planning-review.md)通过，无新增规划阻断，实际tests与冻结候选字节相同。该结论仅覆盖报告固定规划快照及明确允许的交付记账，不放行尚未冻结的新DDL/DTO、协议关联算法及实际产品接线。产品功能、真实Harness、Web、Windows树与迁移均未在本轮运行。历史Findings及旧任务勾选不覆盖Q-001。

## Findings（历史原文，非Q-001通过依据）


| ID | Source | Severity | Target Version | Finding | Fix Version | Status | Retest / Re-review |
| --- | --- | --- | --- | --- | --- | --- | --- |
| BASE-001 | 主执行/独立只读reviewer | 实施差异 | f6d1f8df | 正文强刷/Run必需审批不符新spec | 待实施 | open | 逐阶段独立复核 |
| ENV-001 | 开发基线 | 环境 | f6d1f8df | sandbox DPAPI错误2，5项库测试失败 | 同版当前用户环境 | resolved | 加载仓库owner前置后check:rust exit0；未修改产品 |
| MGT-CODE-001 | 独立reviewer/tester | P2 | 外部管理a590e0e8清单 | 临时对象未按已验请求expiresAt收紧，过期后仍保留正文 | revision2 003850e2清单 | closed | 原失败保留；原29同预期通过，retained0/0，原提出者复查关闭；主区组合另验 |
| MGT-CODE-002 | 独立reviewer/tester | P1 | 外部管理a590e0e8清单 | 最后异步复核间共享管理故障后仍返回派发成功 | revision2 003850e2清单 | closed | 原失败writer1保留；新原断言writer0，原提出者复查关闭 |
| MGT-CODE-003 | 独立reviewer/tester | P2 | revision2 003850e2清单 | 操作预算/对象TTL耗尽误标产品签名过期，状态或错误失真 | revision3 0e116d1f清单 | closed | 新两失败保留；原31同字节断言通过，签名/对象/操作分别分类，原提出者关闭；未代签主4组合 |
| P1-CODE-001 | 独立reviewer/tester | P2 | raw模块1e38ef17 | context校验先于Ignored/匹配旧句柄检查，三类公开误用返回错误优先级不符合同 | revision2原断言11/11及root组合11/11，原reviewer复核 | closed（P1公开端口范围） | 原3/3失败保留，原10源精确备份.tmp/session-loop-p1-revision1；仅前置event分流/current检查，独立原断言不改；报告p1-revision2-acceptance及p1-raw-observations-revision2-review |
| VPROJ-PLAN-001 | 独立reviewer/主执行 | P2方案阻断 | ACP投影候选1d4046f6 | 首次淘汰后所有未知键进入DrainOnly使近端窗口永久停止更新 | ACP候选c978351e | closed（定义） | 原reviewer160e9a01确认逐帧滚动消除原缺口，未编码/验收；新002/003另列，不放行整体投影 |
| VPROJ-PLAN-002 | 独立reviewer/tester | P2方案阻断 | ACP候选c978351e | 256索引LRU遗忘仍保留工具后，可靠同ID晚更新被另开位置 | P2A已改原位anchor并独立验证；完整current仍待 | open（完整义务） | 257工具/长raw后晚完成原位已验，只有实际FIFO失基础才另建；不能用锚点小事实代签全字段current、首开交付或3.2 |
| VPROJ-PLAN-003 | 独立reviewer | P2方案阻断 | ACP候选c978351e | 同片段跨Replay到Live合并时单origin未定义准确来源表示 | P1逐原观察origin/seq已独立验证；P2须继承 | raw子范围closed，聚合待验 | P1不合并实体；不能用raw验证代签后续跨来源聚合，P2A/P2B需单独定义和验收 |

## Verification Results

- 受测版本：起始f6d1f8df，无用户未提交改动；本轮候选未签方案通过。
- 摘要排除：plan结果/tasks状态/报告；实现/配置/规格/用例纳入冻结。
- 执行者/环境：主执行Windows sandbox，实际Node/npm/Cargo；独立产品验收未执行。
- 命令：check:web exit0，tsc、7文件50测试、Vite构建通过。check:rust最初sandbox下库214过/5因DPAPI错误2失败；加载仓库owner前置的受限重试仍失败。随后同版在当前用户环境实际check:rust exit0：fmt/clippy、219库测试、4个JWS、12个平台测试全部通过，未跳测试或修改安全实现；平台fixture普通空跑不额外计功能证据。仅工程基线。

| Case ID | Executor | Target Version | Status | Evidence / Limitations |
| --- | --- | --- | --- | --- |
| CL-001/002基线 | coder | f6d1f8df | pass | Web50、Rust219+4+12，fmt/clippy/build；非新版验收 |
| 其余产品用例 | 独立tester/reviewer | 新版集成未建立 | not-run | 旧探测/模拟/规划不代签 |

- 关闭依据：ENV为同版真实环境重验通过；BASE待实施及独立复核。
- 新版本：独立角色判断重验/复用，旧报告不重写。
- 归档复核：未归档，必需项及长期规格/用例同步后执行。

### 当前增量记录（2026-10-31）

- 当前规划重排已独立检视：统一86例、50项Requirement/108个Scenario，引用无遗漏或错配，旧60例标题保持。报告及20输入摘要见2026-10-31-unified-planning-review.md及同名前缀.sha256；这是当前阶段依据，不放行未固定SQL/DTO/派发。
- 任务2.1纯模型完成：14个开发测试；独立公开API9/9，随后已作为app/tests/session_loop_acceptance.rs的activity_model入口实际9/9通过。源码与独立报告见2026-10-31-activity-model-source.json、activity-model-acceptance.md及activity-load-rereview.md。旧token跨目标、换连接、同连接新load被拒绝；真实资源及协议来源另验。
- 请求身份§3A纯子阶段：开发8/8、独立公共API12/12通过，仓库独立验收合计21/21；独立报告/源码及静态检视见2026-10-31-request-model-acceptance.md、request-model-source.json、request-code-review.md。固定期限和时钟模型不证明持久水位/重启/容量，任务2.2仍待办。
- 本轮一次完整npm run check已exit0：Web50；Rust241库、4JWS、12平台、当时9个已接独立活动用例；fmt/clippy及Web类型/构建通过。随后12请求用例接入后，独立tester在自身target执行其仓库12用例及该integration目标clippy exit0；主执行合计21/21通过。两处验收资产String::new lint修正不改预期，证据保留。
- 原始外部报告中的activity-probe/test.log对应仓库2026-10-31-activity-model-tests.txt；请求public-api-test.log对应2026-10-31-request-model-tests.txt。持续验收源码已纳入app/tests/acceptance，生成候选脚本/外部构建目录未入库。
- 未执行：新版SQL布局/迁移、管理受理/审计/无Run审批接线、协议重放及完整WSS/Web、真实Codex、安全/供应链最终验收；不将上述纯模型子范围的pass记为完整L5/L6通过。全change仍未交付，未提交/推送/发布/归档。

### 后续阶段结果（按实际执行追加）

- schema3 增量迁移子阶段已独立验收13/13并复查关闭SCHEMA-R001–003；保留schema1/2身份、撤销与旧正文。标准只读预检读取已提交WAL，拒绝未知版本；曾尝试的immutable方案会漏读WAL，已全部撤回，原失败证据保留。具体报告见本次schema3系列证据。任务2.3还须验证最终新入口完全隔离旧正文，暂不勾选。
- 修正两处trust测试夹具先建立合法合成node_meta后，完整 `npm run check` 在当前用户Windows owner环境exit0：Web50、Rust260库测试、JWS4、平台12、独立公开API21，fmt/clippy及Web类型/构建通过。此为新增ACP发现/重放代码之前的快照，不替代后续集成重验。
- 公共ACP发现/重放客户端新增9个开发测试，连同原19例合计28/28；后续all-targets clippy `-D warnings` exit0。已交独立tester/reviewer按§6A同版验收；未接实际节点、视图或真实Harness，不记任务3.1完整通过。
- §6A随后独立公开API执行17/17、0忽略、exit0，静态审查无阻断；源码/锁前后摘要一致。阶段报告归入本次acp-discovery-replay系列，完整CL及真实Harness仍未验。实际 `npm run build:rust` exit0，Web资源重新构建并成功嵌入调试exe。
- 共享视图纯容器：合同2d8b7362...及独立A–J用例9bf00176...经reviewer限定放行后编码。9个开发测试通过，all-targets clippy exit0；已将views.rs ee531f84...冻结交独立验收/检视。只证明内存容器，不解释ACP消息、可靠轮次、内容授权、实际慢订阅或Web。开发首轮有一处夹具在预算已满时仍想创建新空快照，已改为同时断言快照Capacity及内部Incomplete，未放宽预算。
- 该视图子阶段随后公开API独立19/19、0忽略，原reviewer读取报告/原日志及同版摘要后限定通过。不可从公开API触发的内部极值及结束后同Key等量新窗口预留失败仅静态核对，未冒领动态覆盖；具体证据见本次view-window系列。任务3.2完整接线仍待办。
- §8A公共权限投影按独立12组用例及reviewer预检实现，合法v2无subject/无tool command不再错误标unsupported，错误形状/未知主题仍分别拒绝/限制。开发ACP合计32/32，静态审查通过，独立AP执行待收取。新快照完整 `npm run check` 当前用户Windows owner环境exit0：Web50、Rust282库/JWS4/平台12、公开API57；fmt/clippy及类型/构建通过（权限AP新资产尚未接入）。该57包含新strategy上的原§6A17及视图19回归，不替代AP专门验收。
- §8A独立AP随后12/12、同cb91策略的§6A17/17回归及integration clippy exit0；原始日志/源码清单见本次acp-permission系列，等待reviewer读取原证据最终限定复核。相应L5/L6及实际审批owner仍未验。
- 原reviewer已亲读上述AP原证据、重新核对同版11文件摘要并追加最终限定通过，报告见2026-10-31-acp-permission-projection-code-review.md。该结论只为协议投影，不关闭管理001/002或实际审批owner任务。
- 管理候选首次冻结a590e0e8/c1ec6a1a未合入：开发27/27不代签，独立发现并实测复现MGT-CODE-001/002（两项目标断言失败）。原工作区、清单、patch、日志和原报告保留，初轮证据归入本次management-input-initial系列。原coder在新的management-revision2目录修复，tester继续其余独立断言；不得把候选开发通过记为正式管理阶段通过。
- 实施合同曾将“本change不实现清理”写成范围排除，与当前management-audit的独立清理场景及session-execution的清理后拒绝要求冲突，已纠正为仅当前事务子阶段保守不清理。完整2.2/2.4仍需补安全清理/水位/受保护依赖/策略说明及对应独立验收；统一CL-080–084义务不变，未由阶段通过或此文档修正签成功。
- 原管理候选独立扩展矩阵29项：27通过、仅原2项失败；两pool、必要写点/两种COMMIT、默认合法10000命令、缩限审计、原生自发活动、正文预算/取消期限、启动标记等实际子分支及负向类型编译证据分别记录，细分未运行分支和L5/L6保留。独立初始断言随后明确输出writer=1、retained=55字节/1对象；新revision2仍须同预期实际重验，不因其他27项通过关闭两阻断。
- 管理rev3外部31/31同原断言、21模型/11负向类型/clippy通过，原提出者关闭001–003并限定允许集成；root已检查完整patch SHA29bf3f49...及git apply --check，再--no-index/--unidiff-zero实际合入，不改index，保留views声明与Storage版本4。随后实际root4独立31/31、全部公开合同69/69及负向/clippy/fmt通过，证据见management-root4系列，不沿外部3绿代签。
- schema4纯布局/迁移：5新开发+6旧开发=11通过；独立K11+原3适配13=24/24、0忽略，Windows owner/WAL-only/字节/0字节侧文件/私有ACL及目录锁均实际覆盖，reviewer同版限定通过。保留原3源/报告，当前布局4新增attempt元数据不代表已实现审批owner/决定或安全响应。
- 实际root4完整 `npm run check` exit0：Web50、Rust库366、JWS4、平台12、公开合同69，fmt/clippy及Web类型/构建通过。原始日志2026-10-31-root4-full-check.txt；先前完整入口第一轮仅test-overlay模块声明排序FMT失败（Web50过、Rust未运行），第二轮PS日志包装受owner脚本ErrorActionStop把Cargo正常stderr当异常中断；均非产品功能通过。已修正测试声明及使用cmd原字节日志/显式真实退出码，第三轮全部通过。原包装日志在本次.tmp保留，未改仓库owner脚本。
- 供应链：根工程 `npm audit --audit-level=high` 返回0漏洞（125个开发依赖）；Web审计外发动作被自动审批判定需授权，用户明确选择“暂不外发，记录为未执行”，因此Web审计not-run，未重试或绕行。当前未找到gitleaks，密钥扫描not-run；不宣称安全/供应链最终验收完成。未新增依赖，Cargo.lock未变。

- 旧正文/强刷接口隔离完成：正常依赖编译13个旧入口逐项拒绝，15个当前正控制通过；旧业务22+34=56项仍实际运行，崩溃child准确匹配1项并验证exit93/VISIBLE_POSITION=1。旧实现主体15554字符原文搬移，未改变旧DDL/数据；schema24及管理31组合回归通过。独立报告legacy-isolation-acceptance（daf72174...）及code-review（bfc1124a...）已闭环，任务2.3据此完成，2.2/2.4/2.5及完整闭环仍待办。
- 隔离后完整npm run check与build:rust实际exit0：Web50、Rust库366/JWS4/平台12/公开合同69；fmt/clippy、Web类型/构建及最终exe构建均通过。原始日志见2026-10-31-legacy-full-check.txt及legacy-full-build.txt。独立legacy原名资产以2026-10-31前缀保存，.log扩展名原样保留；路径引用保留其外部受测版归属。
- 下一阶段为domain视图预算构建端口及ACP结构投影，分别先定义和检视再编码。首次淘汰后对全部未知键停止投影的候选限制正在修订，尚未作为产品规则接受或编码。审批合同新增固定SDK双版取消矩阵待该owner阶段独立复核；不声明实际节点已接线。

- Domain预算构建端口：合同d6ee1d36及独立A–I定义e0ae135e经原reviewer a8c039d5限定放行后实现。新11+原9开发20/20通过，all-target clippy最终exit0；布局实测workspace144/staged360/lease128/parts104/window160/status32字节。候选及旧lease共账、消费式move发布、旧对象CAS、零分配WindowStatus、Drop仅标原读取。当前10文件摘要见view-budget-port-source.json，已冻结交独立验收/检视；原19资产28f2df93未改。不签ACP投影或完整任务3.2。
- 同快照完整npm run check及build:rust实际exit0：Web50、Rust库377/JWS4/平台12/公开合同69，fmt/clippy及最终内嵌Web节点构建通过；原始日志见view-budget-port-full-check.txt/full-build.txt。开发首轮静态检查有新模块style lint失败，修正后通过；编译缓存硬链接失败自动复制仅为环境警告，无测试跳过，不以开发全绿代签独立结果。

- Domain预算端口正式独立45/45（新20+allocator6+原19）及14负向/16正向编译通过，原reviewer最终fdabf0a9限定交付通过；真实bulk null共6次，status零alloc1万次，静态内部极值/全局OOM/真实owner边界分列。已将新26项作为两个独立测试二进制入仓，避免allocator影响其他目标。根组合26/26、严格alltarget clippy/fmt通过；最初测试资产28条项目风格lint失败保留，仅样式修正，原reviewerfff9a5ea逐hunk确认未改assert/流程。domain README仅事实同步独立状态，原受测代码hash不变，原10项manifest保持为历史受测快照。
- P0消息ID窄修：833c1124合同与独立7组4a16aa62经reviewer fda9018a限定预审后实现，strategy72ad187e仅使协商v1的null投影None；v2粗投影原行为保留，不冒充P2完整结构校验。开发ACP36=原32+4通过、同版alltarget clippy通过，7源清单p0-v1-message-id-source.json已冻结交独立验收，未签完整任务3.1或P1/P2。

- P0正式独立35/35=新6+原发现17+权限12，根ACP开发36/36，原reviewer99539785限定交付通过；两次独立夹具前置/oracle错误保留原失败后按既有§6A纠正，未改产品或放宽raw/origin断言。新增单文件验收73580d59逐字入仓并实际6/6通过。随后完整npm check/build:rust实际exit0：Web50、Rust库381/JWS4/平台12/原公开69/新P0六/预算20+6，fmt/clippy及最终构建通过，日志p0-full-check.txt/full-build.txt。所有真实Harness/完整Node会话仍未验，不勾整体3.1。
- P1 raw观察合同a071335b与独立8组8f8e7d49已由reviewer3e728a77限定编码前通过；只新增raw模块/开发测试/mod，不改Domain/strategy/transport/namespace。003在逐原观察定义层关闭，P2实体来源仍须继承；002原位工具仍待具体anchor布局/解析方案检视。

- P1 revision2 raw端口已由原tester独立11/11、旧视图/预算95项及负编译同版重验，reviewer限定关闭P1-CODE-001；root入库11/11及严格all-target clippy/fmt通过。全部P1“Recorded”只证明观察写入时成功，不证明raw永久保留或真实owner来源/完整current已交付。
- P1 root全量检查首次Rust389/390：旧IPC-only测试先探测空闲TCP端口再释放，启动时10048 AddrInUse；原失败保存p1-full-check-initial.txt，不算产品通过。仅该测试改为127.0.0.1:0，让bind原子选端口，生产与断言不变；node.rs b959b31c...，独立reviewer已预审范围，单case1/1实际通过。
- 固定上述版本后完整npm run check及build:rust实际exit0：Web50、Rust库390/JWS4/平台12/原公开69/P0 6/P1 11/预算20+6，fmt/clippy与最终内嵌Web exe均通过。原日志p1-full-check.txt/full-build.txt；未执行Web audit，未新增依赖。最终独立root组合复核另附，不以全量自检代签整个change或3.2。
- 下一并行子包：P2A工具anchor在外部独占目录实施，仅新增ACP模块/声明，root窗口源码暂冻；Domain同步借用/revision仅合同r7 ce4c7b4e及独立定义/预审准备，root尚未编码。两包无写交叉；P2A合并后须重验Domain/P1组合，Domain后续变更也须重验原视图/预算/P1，不沿用外部绿灯。
- P1 root组合最终独立复核已读新全量日志并确认限定通过：p1-node-port-fixture-review最终0e125724、p1-raw-observations-integration-review最终2d5d9fae。两报告原文已归入root runs；ACP README仅将待验状态改为已验公开子范围并说明Recorded不保证未来仍保留，代码与历史manifest不回填。
- Domain借用/revision合同r8 e75e54d7及独立8组定义28319d55/inputs e3673a0f已固定：sealed保留旧诊断读取、拒绝新构建/一致读取；no-op不耗版本；测试极值仅私有闭合枚举单调设置，不unseal/降低。当前8组not-run，待原reviewer门禁；原views/staging及开发测试四源已精确备份摘要，旧测试预期不改。主coder只写Domain两源/新开发测试与必要API说明；P2A外部包单独ACP目录，不并写root。
- Domain借用/revision已按reviewer 88f1a491限定门槛实现。实施时仅stage_insert坏key/sealed优先级消歧，原reviewer书面确认并追加planning-review至3dce5d09；r8合同保留并追加该段，当前5ebc06dc，未改变用户产品行为或旧断言。
- Domain开发28/28=旧20+新8、strict all-target clippy actual0；ReadStatusCell实际48（原32）仍在WINDOW_BYTES1024内，窗口160及原构建元数据布局不变。首轮clippy要求显式缩短锁生命周期，已在实际内容提交后显式drop(meta)，不能按其早于提交的机械建议解锁；未增加allow或改规则。开发日志domain-borrow-revision-dev/clippy已归档，初轮lint反馈保留本次工具记录，不伪称其为产品断言失败。
- 冻结101公开文件manifest 0f09eafd：views01e8ad73、staging ad3bf864、新borrow_tests76389288；旧开发43035410/7533fa57及原独立19/预算26/P1十一均原摘要。原request_tester按固定8组与实施消歧在自有副本正式验收；当前不能用开发通过代签独立0alloc/负编译/死锁/MAX。root同版完整check/build启动，结果待实际退出。
- P2A隔离候选已冻结manifest9181f209/baseline5ddc610e/patch ea8bac49；tool_projection4cf2b6d3。开发15及其原回归通过仅为coder证据。原baseline_tester与reviewer正在同一外部固定版本分别验收/检视；其Domain仍旧a22/d2，不能混用root新Domain。通过后只合newmodule/tests及mod声明，实际新组合另验，不覆盖root文档/源码/既有修改。
- Domain冻结版完整npm run check/build:rust实际exit0：Web50、库398/JWS4/平台12/原公开69/P0 6/P1 11/预算20+6，fmt/clippy与最终构建通过；原字节日志domain-borrow-full-check/full-build已归档。此为工程自检，原独立8组与同版源码检视仍在执行，整体4/18不变。源码不再变，P2A尚未合入，后续实际组合必须另验。
- Domain正式独立20=公开9+allocator3+私有8、15负编译及106原回归完成，原reviewer最终3e86bf6c限定通过；acceptance16081ceee及28资产/101来源前后一致。子进程内部三时序不重复计数、真实73123-byte null/逻辑Capacity/未验全局OOM分列。报告与原日志已归root dbr系列，Domain README仅同步验收链接。
- 已按独立放行窄合P2A生产三路径及公共资产两路径；两次均先git apply --check再apply --no-index、不写index。Git在本机转CRLF，经逐文件证明只有换行差异后恢复外部原字节，证据p2a-patch-newlines.json；模块4cf2/开发f9cb/mod934d/P2A主a4a/supporte733。Domain两源01e8/ad3逐hash保持。
- Domain公共独立9+allocator3及support三文件已同样check/apply窄入仓；使用命令局部autocrlf=false仍因本机换行行为产生CRLF，已证明仅换行并恢复外部2538/3629/3ca0原LF，未改Git全局/仓库配置或index，证据dbr-patch-newlines.json。style/path逆向完整性见原作者69a5e5ce；没有改原断言/夹具/顺序/child watchdog或生产hook。
- 新实际Root组合107文件manifest1896b236，包含P2A+新Domain及独立公共资产，历史101/103清单不回填。两个原tester分别Root P2A19+旧112、Root DBR12+隔离私有8及适用106；reviewer另核本组合差异/新证据；主完整owner check/build实际启动，未出结果不记通过，4/18不变。
- P2B完整current/scanner候选在外部revision7 SHA569cc9ef，原reviewer只读预审87734a1b不放行，六项工程TG仍待精化：可观测来源子项/精确非工具字段表/独立scanner重复键政策/Extension raw依据/类型与可测入口/实际v1 compaction广告。没有新增用户产品决定，不以本阶段缺口缩减整个current义务，也不编码未获门槛的P2B。
- Root107组合最终独立验收/检视完成：baseline_tester dde3bb83，143=19+112+12；原DBR作者e0742afd，实际20=12公共+8私有及适用106回归。两位各核107源前后相同、实际strict clippy/fmt及资产原文；子进程exact1不额外充数。原15/6负编译本轮明确未重跑，不冒签新通过。
- 原reviewer最终bcb1fe24限定通过Root107：两源/新模块/测试逆向证明与8文件换行反算、34资产/107副本、私有owner范围收窄与pid回收、真实全量Web50/库413及构建均已亲核。apply先后仅主工具会话事实，不包装不存在的raw日志。正式报告已归入root；无本组合新增阻断。整体任务仍4/18，P2B/current、实际来源/审批/资源/输出授权/WSS/Web/Harness仍未交付。


## Q-002 本轮增量范围

用户已确认删除连接累计请求次数上限；WP3仅改transport累计阈值及定向开发测试，不扩展未冻结队列实现。先用现有双版协议夹具证明第4097请求失败，再删常量和对应检查并重跑；保留来源、重复ID与唯一响应。WP1同步两份spec/合同，WP5更新旧失败暂停用例并补长期连接及新输入优先/恢复用例，WP6独立复核窄差异。

失败后旧队列挂起，新输入在可派发时优先，旧项只在点击继续后重新参与调度；正常满额规则及前台串行不变。队列产品行为仍待WP2/WP4实现，6.7保持待办。Q-001规划报告不作为Q-002覆盖证据；新增检查结果单独记录。

Q-002独立tester候选已按原字节集成：116例、67Requirement/124Scenario、保留原107标题，SHA256 `4bbb4d67676bbe6bf7fe31cc520b1d357a99f54831a98fcdbc734eefd65c7bcb`。CL-108–116覆盖长连接和失败后新输入优先/显式恢复等新增边界；Q-001报告保持原文，不作为Q-002通过依据。


Q-002限定交付：task6.6已完成，实际红绿、ACP37/37、fmt/clippy/真实Rust构建及严格结构检查见[执行记录](../../../testing/runs/deliver-codex-session-loop/2026-10-31-q002-verification.md)；[独立限定检视](../../../testing/runs/deliver-codex-session-loop/2026-10-31-q002-review.md)通过，用例措辞问题Q002-PLAN-001由提出者关闭。独立reviewer复用固定二进制实际运行1/1和37/37，未独立重编译；主执行负责自身红绿和工程命令记录。task6.7完整队列产品仍待办，116例定义及完整CL-108/109/116矩阵未被本次窄开发回归整体代签。

## 当前增量日志（工作区原文保留）

[2026-11-09：WP2.2 管理 WSS 命令限域验收] [管理命令验收](../../../testing/runs/deliver-codex-session-loop/2026-11-09-wp2-2-management-command-wss.json)：新增真实 Node/TLS socket 用例验证 `input.register` 的稳定 requestId、同请求重试与冲突，以及 `input.consume` 结算、重复消费不重复入队；单例1/1、Node 队列管理组16/16通过（Rust 1.98.1、`--locked --offline`、独占 target）。这只覆盖 WP2.2 的管理命令接线路径，校时与完整队列操作竞态、独立验收、完整 Rust 与真实 Harness 仍未完成，任务不勾。

[2026-11-09：关闭启动时提前封闭连接 owner 的修复](../../../testing/runs/deliver-codex-session-loop/2026-11-09-wp1-1.4-followup-v3.json)：定位 `reg_close_06` 返回 `Cancelled` 的原因后，在 `Storage::begin_runtime_shutdown` 立即封闭 registry connection owner；已有 lease/acquisition 仍由原 owner 排空，未取消正在进行的 SQL。修复后 `independent_physical_close` **9/9** 通过，SQLite lifecycle development **14/14** 通过。完整 Rust、四个 SQLx floating-return probe、真实 Node/C1/DPAPI、poison 合同差异和用户拒绝的 `npm audit` 仍未闭合，1.4保持未勾。

[2026-11-09：WP1.4 当前源码复验与阻塞更新](../../../testing/runs/deliver-codex-session-loop/2026-11-09-wp1-1.4-followup-v3.json)：Rust 1.98.1、`--locked --offline`、Windows owner runner 和独占 target 下，原 SQLite worker thread + app owner join 物理文件关闭用例单例1/1通过。完整六例生命周期示例仍挂在真实 Node 启动失败后的原 owner join 场景，隔离复跑超过一分钟无进展并中断，不能记为通过。`reg_close_06` 当前源码复跑仍为 `Cancelled`、测试预期 `NodeStopping`，20秒后失败；未改断言或生产映射，须先核实 deadline/cancel 与 shutdown-seal 的规范优先级。poisoned-owner 场景中实现保守返回 `Unconfirmed`、开发测试却期望 `Complete`，没有找到足以授权弱化不确定状态的当前产品合同，作为合同差异待独立复核。全库当前有效串行日志为853通过/30失败，不能视为全绿；四个SQLx floating-return watchdog、当前身份 C1 IPC error 5、Node/DPAPI等仍阻断。`npm audit`按用户要求不执行；1.4保持未勾，未改Git索引或失败现场。

[2026-11-08：ACPI 上传真实路由仍阻断编码；Source SQL v6 冻结审查拒绝](../../../testing/runs/deliver-codex-session-loop/2026-11-08-q003-source-sql-v4-fresh-dynamic-attempt.json)：ACPI 真实 WSS 接线的独立编码前审查已完成，结论暂不放行，报告见 [ACPI 路由审查](../../../testing/runs/deliver-codex-session-loop/q3-p3b-acpi-route-pre-review-v1/ACPI-ROUTE-CODING-PRE-REVIEW.md)，SHA `B3245E8A…FF84EB`。审查确认现有 ACPI codec 和 Node owner 可复用，但尚无 ManagementService 安装入口、单一 writer 二进制 ACK 分支及 owner 持有/关闭合同；要求固定四命令与帧错误语义、actor gate/背压、SocketLifetime/取消顺序和 ACPI-R01–R08 真实 WSS 映射后再独立复审。未改产品、未运行 Cargo。Source SQL strict4R v6 独立静态复审拒绝 compile/list/test，报告 SHA `f4795a6b…f13ce7bc`：Register selector 在 manifest/verifier/report 之间未完全绑定，list 数量校验不正确解析多行 summary，REPORT-v6 未纳入冻结链且内容陈旧，config 指向 v5 拒绝报告而非 v6 有效授权。没有执行动态命令；P1 `SSF-SQL-RECONCILIATION001` 保持 Open。tracked 仍 6/25；npm audit 按用户要求未执行，未提交、推送或归档。

[2026-11-08：后续状态复核与Web检查](../../../testing/runs/deliver-codex-session-loop/2026-11-08-q003-continuation-status.json)：OpenSpec仍6/25。P3-A窄集成及47项Root回归证据保留，但拒绝关闭两秒真实I/O timeout、Node异常完整树/密钥及SQLite物理join未验证。Source v11收到两份独立静态拒绝，compile/list均未授权，reconciliation finding仍Open。ACPI R01–R08由独立tester有条件接收为未来真实WSS测试方案；缺确定错误注入的两个映射和design同步finding仍未解决，不能合入spec或开始生产编码。`npm run check:web` 本轮通过：TypeScript、28个测试文件/661测试、生产构建。ACP客户端独立17例既有报告仍是限定子范围；本轮Rust复跑受rustup临时目录拒绝及混用1.99产物阻断，未算通过、未改产品源码。`npm audit`按用户要求不执行。

[2026-11-08：Source SQL strict4R v11 冻结候选完成，等待独立静态复审；ACPI v2 合同待编码门复审](../../../testing/runs/deliver-codex-session-loop/2026-11-08-q003-source-sql-v11-acpi-v2-status.json)：strict4R v11 候选 revision `strict4R-v11-scopebound-20261108-r1` 的 config、manifest、freeze、runner、verifier 与报告已绑定，compile/list 是唯一允许模式、test 明确关闭；包 SHA/字节和交叉绑定记录见状态文件。fresh approval 尚无，未运行 verifier/Cargo/list/test，须等独立静态复审后再决定是否授权有限 compile/list。ACPI 实施合同 v2 已补齐 service 安装、attachment 唯一 owner、四命令监督、Binary ACK 单一 writer、错误/关闭和 R01–R08 真实 WSS 映射，SHA `30358D3F…0100F6`；它仍是编码门候选，等待独立 reviewer 明确放行，不改产品源码或运行 Cargo。完整变更仍为 6/25，P1 `SSF-SQL-RECONCILIATION001` 未关闭。

v11 随后被独立只读复审拒绝，报告 SHA `dafef9f0…f6808e4`：冻结的 runner 与活动文件不一致；活动入口读取未批准配置而批准配置未被使用；冻结 verifier 未绑定 v11 verifier，后者另有重复声明。没有运行 verifier、Cargo、list 或 test。保留 v11 原包并准备 v12 重新闭合唯一活动入口的冻结链，仍须 fresh 独立复审。

ACPI 路由合同 v2 也被独立编码前复审阻断，报告 SHA `83270D50…1369A5`：Binary `NodeStopping` 的 4400/1001 关闭码冲突；四命令和二进制错误语义尚未进入本 change 的权威 spec；双 service 安装原子性未收口；R03/R06/R07/R08 尚无可复现的 barrier、fixture、背压/permit 和撤销观察步骤。未修改产品、未运行 Cargo 或测试；需先修订合同/spec并再次独立复审。
[2026-11-08：第二轮 C 盘清理完成；P3-A 三叶后验准许窄集成，Source v4 正在恢复]
清理记录见 [session-cache-cleanup-followup](../../../testing/runs/deliver-codex-session-loop/2026-11-08-session-cache-cleanup-followup.json)：清理会话 Cargo target 中 7,019 个可重建编译文件、四个旧工作副本指定 Cargo 缓存 13,689 个文件，以及两份 SHA-256 与留存验收副本完全相同的可执行程序。C 实际可用空间由本轮开始时约 105.2 GiB 增至约 150.3 GiB；源码指纹、52 项既有 tracked 修改、Git 索引均保留，C 根和有权限拒绝的会话项未全盘扫描。唯一测试程序、报告、日志、数据库、聊天记录及旧 worktree 独有修改不清理。

P3-A：第六版三叶此前已经安装于 Root，且安装后独立限域后验已接受；更新记录见 [Root 集成验证](../../../testing/runs/deliver-codex-session-loop/2026-11-07-q003-connection-owner-root-integration.json) 与 `source-session-recovery-review-v1/P3-INSTALLATION-POSTERIOR.md`。36 项独立网络用例和 Root 同源47项回归实际通过，但不签完整 P3-A/NA015/NA016：真实 RejectedClose 两秒 I/O timeout、Node 异常关闭完整树/密钥和 SQLite worker 物理 join 仍未验证；原 finding 提出者不可用，其 Open 历史保留。strict 仍为原有708条、全库未通过。Root 最新复核发现三叶已于之前恢复工作中集成，未重新应用或覆盖。

Source native-session-facade：隔离恢复确认 Root 核心仍未跟踪/未合入；repair-v2 v4 是最高已固定候选，既有真实 discover/create/load 1例和4个 SQL 场景报告通过。完整六场景与同轮检查、固定源清单仍待核实；仅工厂4例不足以代表 Source 闭环完成。当前由原隔离 coder 继续核对既有 workspace，不新增副本/target、不改 Root。

ACPI P3-B 候选已由作者完成 v4 静态文档包，但独立复审未完成，未合入 spec、未编码、未运行测试。R01–R08、R06 可执行时序及写集/合同一致性正在复审；产品实现继续受独立编码门约束。

整体 OpenSpec 仍为6/25，用户要求不执行的 npm audit 仍未执行。Source、P3-B真实路由、完整Web/Harness、安全供应链、全局 strict 和最终集成验收继续待办；未提交、推送或归档。
[2026-11-08：WP1.3证据基线限定完成；1.4全库Rust检查仍阻断]
WP1.3 经独立只读review按规划/证据基线范围接受，并于Rust 1.98.1、独占 `.tmp/q3-wp1-13-rerun-20261108`、`--locked --offline` 下重跑公共ACP discovery/replay 17项：17通过、0失败/忽略，原始输出固定在 `2026-11-08-wp1-1.3-rerun.stdout.txt`，完整边界、schema摘要和当前源指纹见 `2026-11-08-wp1-1.3-evidence-result.json`。该结论只覆盖双版规范/schema身份、协议程序/场景追溯和公共客户端L3/L4子范围；CL-005/032–041/047–060完整产品用例、Node/SQL/Web、真实Harness及L5/L6均未因此通过。v1/v2 schema SHA分别为 `1c57095a…fdef5` / `28c7251e…e0778d`；不支持能力直接报错、无私有兜底/额外兼容裁决的要求以当前spec为准。

随后1.4适用入口 `npm run check` 当前失败：Web TypeScript、28文件661测试及构建通过；Rust fmt通过，但全targets strict Clippy 报707个既有诊断并停止，故完整Rust tests未到达。`npm run build:rust`通过（Cargo构建278警告）。具体记录见 `2026-11-08-wp1-1.4-checks.json`。依赖选择/锁文件已审阅；漏洞审计 `npm audit` 按用户明确要求不向registry发送依赖元数据，保持未执行，因此1.4待办不勾。

后续用独占 Rust 1.98.1 clean target 复核发现，旧串行Rust日志执行的是陈旧测试二进制：当前 product command independent acceptance 组干净编译后 **31/31通过**，旧日志中对应schema断言失败不作为当前源码结果。SQLite关闭复核中 max1/max2普通显式归还与Drop归还两组均通过，但四个依赖“held return尚未poll时必须观察到pool size=1/registry Complete”的独立时序用例仍真实20秒超时；失败现场显示pool closed、size0、registry Pending。另schema3真实deferred COMMIT例在预先安装node_meta trigger后调用schema preflight，与当前拒绝受管表额外trigger的规则冲突；未弱化验证或改用不能到达真实引擎COMMIT失败的测试替代。详细限域结果见 `2026-11-08-wp1-1.4-clean-target-followup.json`。完整Rust检查仍非全绿，1.4保持未完成。

[2026-11-08：WP1.4聚焦复核及schema3真实提交失败修复]
在Rust 1.98.1独占clean target中复核了旧全库日志：产品命令独立验收当前源码31/31通过。修正schema3迁移用例，使其先通过原schema preflight，再使用迁移器已有DeferredCommit路径在真实事务中插入延迟外键冲突；当前测试实际观察SQLite FK错误787，并通过回滚/旧schema保留断言（1/1）。没有放宽schema guard。普通SQLite max1/max2关闭各通过；RPC浮动连接的四个held-return时序仍未解决。另对DPAPI、C1 child和共享视图真实Node代表例进行聚焦复验：DPAPI写身份及C1子进程数据目录锁收到Win32拒绝访问；ACP真实Node仍ReadFailure，需在可用运行身份/环境继续定位，整组不能计绿。无效/中断的旧全库日志未计结果。逐项命令和边界见 [WP1.4聚焦复核](../../../testing/runs/deliver-codex-session-loop/2026-11-08-wp1-1.4-focused-followup-v2.json)；原始DPAPI、C1日志分别为 `2026-11-08-wp1-1.4-dpapi-focused.stdout.txt` 与 `2026-11-08-wp1-1.4-c1-focused.stdout.txt`。`npm audit`仍按用户要求不执行；1.4不勾，完整Rust验收未完成。
Schema3修正后完整 `schema3_migration_acceptance` 组13/13通过（包括旧schema升级、只读拒绝、WAL和回滚场景）；该结果仍是限域，不等同完整Rust库通过。

[2026-11-08：WP1.4 owner runner 聚焦复验](../../../testing/runs/deliver-codex-session-loop/2026-11-08-wp1-1.4-owner-rerun.json)：在 `testing/ci/windows-owner.ps1`、Rust 1.98.1、`--locked --offline` 和隔离 target/data root 下，真实 ACP shared-view Node 配对、Node owner IPC 启停、C1 真实 native child 所有权与停止等待、DPAPI 身份跨进程恢复各1/1通过。C1 先用 `native-dispatch-dev-fixtures` 构建真实子进程并设置 `C1_NATIVE_CHILD`；未放宽断言。此前默认身份的 OS error 5 与缺少 fixture 的失败保留为环境/准备问题。该结果只扩大有效聚焦证据，不能代替完整 Rust 库验收；四个 SQLx held-return watchdog、完整串行测试异常退出和全库 strict 仍待处理，1.4 不勾选。

[2026-11-08：WP1.4 当前源码干净target复验](../../../testing/runs/deliver-codex-session-loop/2026-11-08-wp1-1.4-clean-rerun-v3.json)：Rust 1.98.1、独占 `.tmp/q3-wp1-14-schema-20261108c`、`--locked --offline` 下，schema3真实deferred-FK COMMIT失败测试1/1通过；product-command independent acceptance组31/31通过。结果仅纠正这两个当前源码子范围的状态，不代表完整lib通过。完整串行测试仍无有效成功总结，四个held-return时序probe及strict gate仍阻断；`npm audit`按用户要求未执行，1.4不勾选。

[2026-11-08：WP1.4 关闭时序与真实 Node IPC 诊断](../../../testing/runs/deliver-codex-session-loop/2026-11-08-wp1-1.4-pool-return-semantics.json)：复核 SQLx 0.8.6 源码确认 `PoolConnection::return_to_pool()` 在构造 future 时即把连接移入 floating 队列，未 poll 的 future 不保证 `Pool::size()` 仍表示 checked-out 连接；因此 RPC floating 阶段的四个20秒 watchdog 失败是旧验收前置条件与当前 SQLx API 语义不相容，保留失败，不改生产关闭逻辑、不放宽门。真实 Node 复跑中 Node 已达到 READY，但 `ipc::begin_pairing` 返回 Win32 OS error 5（控制通道拒绝访问），故真实 Node/认证链路继续按环境受限记录，不能计为通过；诊断记录见 [node IPC 诊断](../../../testing/runs/deliver-codex-session-loop/2026-11-08-wp1-1.4-node-ipc-diagnostic.json)。WP1.4 仍未完成，npm audit 仍按用户要求未执行。

[2026-11-08：WP1.4 Windows poison 用例定位](../../../testing/runs/deliver-codex-session-loop/2026-11-08-wp1-1.4-poison-diagnostic.json)：同一 Rust 1.98.1 独占 target 下，poison 前置和原管道登记后两个实际句柄用例分别1/1通过。崩溃用例在真实 `CreateFileW` 打开 named-pipe client 时先收到 Win32 error 5（ACCESS_DENIED），尚未取得/登记 child handle；随后测试进程仍以 `0xc0000008 STATUS_INVALID_HANDLE` 异常终止，且启用 `windows-owner.ps1` 后可复现。故根因不归到 child 清理，异常退出仍未解释，未改生产代码或测试断言；完整 Rust lib 回归继续无效，WP1.4 保持未完成。

[2026-11-08：WP1.4 poison 修正停点复验](../../../testing/runs/deliver-codex-session-loop/2026-11-08-wp1-1.4-poison-diagnostic.json)：将该用例的测试专用停点移至真实 `CreateFileW` 成功取得 child handle 之后；生产 ownership 路径未改。授权的 `windows-owner.ps1` 隔离重试曾1/1通过，但随后四项分组为3/4，且同一完整限定测试连续三次均在 `after_child_create` 前超时；因此该用例和分组均不计为通过，当前问题定位为间歇性 named-pipe client acquisition，旧 ACCESS_DENIED/STATUS_INVALID_HANDLE 日志和新超时现场均保留。fmt只对两个改动源文件按 edition 2024 检查通过；全工程fmt受既有 `registration_acceptance/p/support.rs` 差异阻断。`npm audit`仍未执行，WP1.4未勾。

[2026-11-08：WP1.4 tracked named-pipe error 5 复现](../../../testing/runs/deliver-codex-session-loop/2026-11-08-wp1-1.4-poison-diagnostic.json)：更新测试 driver 完成诊断后，稳定用例失败具体落为 `open stdio pipe failed (Windows 5)`，发生在 child handle 获取之前。临时最小复现显示当前 raw `CreateNamedPipeW` owner-only helper 在 Tokio 注册前，用原生 `CreateFileW` 和 Tokio `ClientOptions::open` 均收到 ACCESS_DENIED；给客户端增加 `FILE_FLAG_OVERLAPPED` 无效。更关键的是，同一 `windows-owner.ps1` runner 下仓库原有 `ServerOptions` owner-only连接测试也在 tracked owner 代码之前收到相同 Windows 5，随后等待连接超时，因此本轮证据指向共享 owner-only pipe ACL/环境问题，不能据此归责tracked实现。探索用 `ServerOptions` 后复制句柄的做法以注册错误87结束，已撤回。探索代码均未保留，production ownership 未改；精确 ACL/API差异尚未查清。最新 owner-run日志为 `2026-11-08-wp1-1.4-poison-owner-diagnostic-latest.stdout.txt`；poison用例不计通过，WP1.4保持未完成。

[2026-11-08：WP1.4 pool-close owner-runner follow-up](../../../testing/runs/deliver-codex-session-loop/2026-11-08-wp1-1.4-pool-close-owner-followup.json)：在 Rust 1.98.1、`--locked --offline`、`windows-owner.ps1` 和独占 target 下，`storage_drop_seals_and_drains_retained_owner_cycle` 真实1/1通过；完整 pool-close 组中其余23项通过，6项RPC探针因首轮未注入 `RPC_FIXTURE_ROOT` 而作配置无效保留。补齐隔离夹具根后，RPC浮动连接探针仍在20秒阶段门失败，日志显示 `pool_closed=true`、`pool_size=0`、`registry_status=Pending`，并在进程门内没有最终汇总。结合已保存的 SQLx 0.8.6 源码语义，`return_to_pool()` 构造future时已发布连接，未poll future不能作为 `Pool::size()==1` 的关闭前置；不修改生产关闭逻辑、不放宽断言。WP1.4仍未完成：完整Rust库结果、strict gate、真实Harness/供应链及用户拒绝的 `npm audit` 仍待处理。

[2026-11-09：WP1.4 SQLite 生命周期剩余场景定位](../../../testing/runs/deliver-codex-session-loop/2026-11-09-wp1-1.4-followup-v4.json)：在同一 Rust 1.98.1、`--locked --offline`、Windows owner runner 和新数据目录下，原 SQLite worker/app owner join **1/1**、真实 Node 启动失败后的原 owner join **1/1**。真实 Node 正常停机例可完成 ready，但 owner named-pipe `status` 请求立即收到 Windows `拒绝访问 (OS error 5)`；此前无界 stop 等待因此挂起，记录为当前运行身份的 IPC ACL 环境阻断，不计产品通过。`storage.rs` 的提前 seal 修复仍通过既有9/9物理关闭与14/14生命周期组；WP1.4继续未勾，完整 Rust、SQLx floating-return、C1/DPAPI、strict 和 npm audit限制仍未闭合。

[2026-11-09：WP1.4 生命周期示例在 owner 身份下完整通过](../../../testing/runs/deliver-codex-session-loop/2026-11-09-wp1-1.4-followup-v5.json)：同一当前源码、Rust 1.98.1、`--locked --offline`、独占 target 和新数据根，在非沙箱 Windows owner 身份下串行运行六例 **6/6**，包括真实 Node IPC stop、原 Node owner join、SQLite worker join 与物理文件独占验证。受限令牌下 owner named pipe `OS error 5` 的失败仍保留；原因是执行上下文差异，不是产品代码变更。完整 Rust、strict、SQLx floating-return 时序及用户拒绝的 npm audit仍阻断 WP1.4，任务保持未勾。

[2026-11-09：WP1.4 数据根纠正与 C1 身份限制复核](../../../testing/runs/deliver-codex-session-loop/2026-11-09-wp1-1.4-followup-v6.json)：在 Rust 1.98.1、`--locked --offline` 和独占 target 下，补齐 `PA_DATA_ROOT` 后当前源码 prompt document actual-wire **5/5** 通过；此前全库日志中对应失败属于夹具准备问题。受限令牌下 C1 四例均在打开数据目录锁时收到 Windows error 5，未进入产品断言；切换 owner runner 的本轮四例重跑前两例完成、第三例真实 native child 停止场景长时间无输出后停止，不宣称结果。此前 owner runner 的 C1 真实 child **1/1** 有效证据仍保留。SQLx floating-return 四个时序探针仍因 SQLx 0.8.6 构造 future 时已浮动连接而不具备当前契约的阶段门，完整 Rust/strict/身份敏感覆盖和用户拒绝的 `npm audit` 仍阻断，WP1.4保持未勾。

[2026-11-09：WP2.2 校时与管理变更命令开发验证] [校时记录](../../../testing/runs/deliver-codex-session-loop/2026-11-09-wp2-2-clock-adjustment.json)显示节点壁钟前进一天后回拨，已受理队列不失效、不重排，容量事实仍为 Full；新例1/1、queue-owner组25/25通过。管理WSS的`edit/cancel/prioritize/resume`新例已编译，但在 owner runner 下创建Node数据目录锁即遇Windows error 5，未触达产品断言，失败记录见[WSS变更记录](../../../testing/runs/deliver-codex-session-loop/2026-11-09-wp2-2-mutation-wss.json)。已有`register/consume`稳定身份与单次派发仍为1/1、管理组16/16。WP2.2仍未完成，审批等待竞态、多客户端竞态和独立验收未覆盖。
[2026-11-09：WP2.2 管理 WSS 变更用例复验仍受 Node owner IPC 阻断]
校时用例及 queue-owner 组已通过；新增 WSS edit/cancel/prioritize/resume 用例已修正 body、bodyId 和 prioritize 快照断言。使用 Rust 1.98.1、`--locked --offline`、`windows-owner.ps1` 和独占 target 重编后，owner-run 与直接执行二进制均在 Node 配对控制通道收到 Windows error 5，未触达任何 WSS 业务断言；记录见 [WSS 复验](../../../testing/runs/deliver-codex-session-loop/2026-11-09-wp2-2-mutation-wss-rerun.json)。因此不把该用例计为通过，WP2.2 仍未完成。管理 register/consume 的既有 1/1 与组 16/16 证据保持原范围；审批等待、跨 socket 竞态、独立验收和真实 Harness 仍待。

[2026-11-09：WP2.4 管理审计保留与容量限域复验]
在 Rust 1.98.1、`--locked --offline`、Windows owner runner 和独占 target 下，管理审计默认保留/重复结算/到期清理用例 **1/1** 通过；真实 100000 条容量和双原始连接 owner 用例 **1/1** 通过。管理审计开发组共14例，独立进程复跑后有效 **14/14**：并行初跑唯一失败是 Windows 共享 fixture 目录 `AlreadyExists`，未进入产品断言，单例复跑通过。该组同时覆盖满额时未到期记录不删除、必要队列结算继续并产生附加审计告警、commit veto/回滚/回执丢失、附加审计不可用、布局降级和schema升级。证据见 [WP2.4 审计复验](../../../testing/runs/deliver-codex-session-loop/2026-11-09-wp2-4-management-audit-followup.json)。这仍是存储/开发限域结果，不覆盖真实多客户端 WSS 竞态、独立验收、真实 Harness 或完整 WP2.4 场景，任务不勾；`npm audit`按用户要求未执行。

[2026-11-09：WP2.2 登记与队列 owner SQLite 验收]
Rust 1.98.1、`--locked --offline`、串行测试和独占 target 下，`registration_owner_acceptance` **51/51** 通过，覆盖跨客户端编辑身份/revision、重复登记、授权与actor代际、编辑/取消/发送交错、活动唯一性、失败挂起、新输入优先、显式继续、SQLite提交不确定性责任保留及审计故障隔离。执行记录见 [WP2.2 登记验收](../../../testing/runs/deliver-codex-session-loop/2026-11-09-wp2-2-registration-owner-acceptance.json)。这是登记/应用层及真实SQLite范围，不经过Node WSS；`edit/cancel/prioritize/resume` 测试仍在配对控制通道 Windows error 5 处失败，未触达业务断言，因此WP2.2不勾，审批等待和跨socket竞态、独立验收与真实Harness仍待。

[2026-11-10：WP1.4 队列测试锁作用域与导入清理]
五处队列开发测试由显式drop改成局部锁作用域，全部原断言保留；六处未使用导入清理，无产品行为修改。最终Windows owner串行队列30/30、fmt/diff通过；strict Clippy仍exit101（lib422、lib-test406，含重复诊断），五条await_holding_lock不再报告。源码增量与原始日志见[证据](../../../testing/runs/deliver-codex-session-loop/2026-11-10-clippy-queue-scopes/EVIDENCE.md)。v11历史冻结保留原归属，未代签独立验收；1.4及完整Rust/Harness/Web门槛仍待办，整体9/28。
[2026-11-10：WP1.4 原生导入与全feature编译修复]
原生派发13文件清理45个未使用导入，导入区外代码保持一致。全feature检查暴露共享视图时间别名及审批测试类型引用/可见性缺口，三文件窄修后cargo check --all-targets --all-features --locked --offline实际exit0；没有改变审批行为或验收断言。最终strict仍exit101（lib377/lib-test361，含重复），fmt/diff及OpenSpec11/11通过。见[本轮证据](../../../testing/runs/deliver-codex-session-loop/2026-11-10-clippy-native-imports/EVIDENCE.md)。仅编译通过，完整测试/独立验收/Harness闭环未完成，1.4不勾，9/28。
2026-11-11：CL-009 开发子范围新增真实 WSS 用例 `queue_read_device_revoked_actor_keeps_accepted_items_and_other_actors_live`，通过真实 Node/TLS、双客户端和 owner IPC `device.revoke` 验证：撤销后 A socket 在下一次 `input.queue` 上关闭 4410/revoked；B 的共享队列仍可见 A 撤销前已受理项且五字段无 body/digest；B 可继续上传、登记并消费。证据见 [CL-009 复跑](../../../testing/runs/deliver-codex-session-loop/2026-11-11-cl009-device-revoked/queue_read_device_revoked.stdout.txt)，SHA-256 `256169E5…35C7`。这仍只是 CL-009 的开发/生产混合限域，不覆盖订阅、引用块、附件、审计与独立验收；4.1保持待办，整体9/28。
[2026-11-11：4.1 同版回归证据汇总]
CL-009、ACPI R01–R09、R10两例、CL-017、CL-024、registration owner、session loop、shared input queue和socket independent最新原始输出已汇总为 [v12回归证据](../../../testing/runs/deliver-codex-session-loop/2026-11-11-regression-evidence-v12.md)。该汇总记录有效运行、退出码、结果和SHA-256；旧v11 `SOURCE.json` 相对当前源码已漂移45个受控文件，不再作为这些新证据的source set。CL-017缺少feature的命令错误与full-rust 22失败的诊断均保留为失败证据。4.1仍缺完整同版Web/fmt/OpenSpec/diff/full-rust/strict和真实Harness验收，任务保持待办。

[2026-11-12：4.1 v20 同版开发回归已绿]
固定 v20 source set（904 个受控文件，SHA-256 `14A9BE497B966717DC1D72FFE989BB9BE46F7DB9F306B1018008C527BD3DE40D`）后，Windows owner 隔离运行中 ACPI codec/R01–R09 13/13、R10 三例 1/1、CL-009 1/1、CL-017 1/1、CL-024 2/2、registration owner 51/51、ACP view observations 11/11、ACP tool projection 19/19、session loop 57/57、shared input queue 39/39、socket-independent 15/15 全部通过；目标进程快照为空。首轮 socket-independent 因运行器漏传 `P1_DATA_ROOT` 为 1/14，原始 Red Check 保留，补齐环境后同一源码绿跑；未改产品断言或 source set。Web 同版 check/test/build 通过，31 个测试文件、680 项测试；fmt、OpenSpec strict 11/11、diff check 通过。证据与 40 文件清单见 [v20 回归](../../../testing/runs/deliver-codex-session-loop/2026-11-12-wp4-final-v20/EVIDENCE.md)，清单 SHA-256 为 `2CA2C7ED7D0DE3A52B084F6E32ADBD0E0592E37EEC6CC622BEBCEC2D07E4FDAB`。本轮只关闭 4.1 的同版开发回归缺口；CL-030/031、真实 Codex/DeepSeek Harness、可信 Web、完整 SQLite/Windows Job 组合、独立 WP5/WP6 和 strict 门仍未完成，4.1不勾，整体仍为 9/28。

[2026-11-12：WP4 原生释放持久化与回归]
原生释放由内存操作改为 schema 10 持久事实，固定 `actor_id`、认证代际、目标、`registered/releasing/released/failed` 阶段及有界错误码。actor 隔离 Red Check 为 1 failed（旧实现允许第二 actor 查询成功），持久化 Red Check 为 1 failed（`no such table: node_native_release_operations`）；最终 Green 中认证 WSS 路由 1/1、schema 10 故障回滚与重开 2/2、仓储重启/旧代际/终态失败重试 2/2。原生释放已成功但终态落库失败时返回 `necessary_storage_unavailable` 和 `result:null`，保持 `releasing`，恢复后可幂等重试为 `released`。同一固定 source set（512 文件，source set SHA-256 `f9693f4061118a7965cd2821244420322a668b0b7afcf689f94dceec99d971d3`）在真实 Windows 当前用户环境通过 registration owner 51/51、session loop 57/57、tool projection 19/19、view observations 11/11、shared input queue 39/39、socket-independent 15/15、真实 shared view 3/3，以及 ACPI codec/R01–R09 13/13 与 R10 三例各 1/1；两段目标进程快照为空。Web check、683 项测试与 build 通过，构建前后 `dist` 哈希一致；fmt、OpenSpec strict 11/11、diff check 通过。证据及 75 文件最终清单见 [WP4 原生释放回归](../../../testing/runs/deliver-codex-session-loop/2026-11-12-wp4-native-release-regression-v2/EVIDENCE.md)，`MANIFEST.json` SHA-256 为 `9D790AA41FF115121BF26779F0178DD701E3DFB843EEECDF1AC39506DB80B544`。该结果只关闭本条 WP4 原生释放开发范围；1.4 仍受完整 Rust/Clippy/供应链门阻断，CL-030/031、真实 Codex/DeepSeek、可信 Web、完整 SQLite/Windows Job、独立 tester/reviewer 和 6.10 仍未完成，因此 4.1 不勾，整体仍为 9/28。

[2026-11-13：WP4生产共享视图接线复验] Web focused 55/55、全量 37 files/972 tests、TypeScript check 与 production build 均 exit 0；独立增量复查确认 stream_rebound credit/detach、open失败candidate清理及正文迟到descriptor三处修正。精确结果见 [本轮复验](../../../testing/runs/deliver-codex-session-loop/2026-11-13-web-view-assembly-v1/ROOT-RESUME-RESULT.json)，代码复查见 [增量报告](../../../testing/runs/deliver-codex-session-loop/2026-11-13-web-view-assembly-v1/review/INCREMENTAL-20261113.md)。WV-13/14/15实际浏览器验收仍未执行：夹具要求可信专用HTTPS测试证书，本轮未找到可确认的可信证书；不跳过TLS校验、不改系统信任库。浏览器验收及WSS/Harness完整闭环仍待，4.2/6.4不勾，tracked进度9/28。npm audit按用户要求未执行。

[2026-11-13：当前验收恢复与下一来源会话批次差异]
WSS v4 的 library Cargo 编译 exit0，但根进程退出后原 Job 仍残留 PID12616；已强制清理至0，运行器记失败，child 和19项测试尚未执行。原失败留在 2026-11-13-wss-combination-v1/independent/build-execution-v4，不能将编译器成功写成完整构建监督通过。浏览器 WV-13/14/15 仍待运行器四项隔离/身份记录修正及原 reviewer 增量复核；合成 HTTPS 资源路由只验真实 main DOM，不证明 TLS/WSS/Harness。服务重启后原三个独立任务实际停在 pending_init，主执行核实后恢复同一任务，没有重建工作副本或重复编译。

下一来源会话批次已完成只读差异盘点：[实施依据](../../../testing/runs/deliver-codex-session-loop/2026-11-13-source-entry-rebase-v1/NEXT-BATCH.md)及[逐叶影响记录](../../../testing/runs/deliver-codex-session-loop/2026-11-13-source-entry-rebase-v1/IMPACT.json)。当前 Root 尚无 Source 会话服务装配及发现/创建/外部ID读取的认证 WSS/Web 入口；配置 catalog 不是会话发现。旧 repair-v2 v4 的44个变更文件中，10个当前仍是旧基线、21个已漂移、13个不存在；旧 Source schema10 与当前 native_release schema10冲突，SQL finding 仍Open，不能直接应用旧候选。先收敛当前两个验收，再按当前迁移链和独立七组定义恢复实现；本盘点未改产品、未关finding、未勾任务。tracked仍9/28，npm audit按用户决定未执行。

[2026-11-13：验收运行器故障定位与 Source 增量方案]
WSS v6 library/child 的 Cargo 均实际exit0，library Job自然0；child Job残留PID36716、CIM身份查询拒绝访问，强制清理1→0，故原构建监督结果仍failed，19项未由该运行器执行。后续单次handle诊断中rust-version与fresh child均自然0且无cleanup，但未复现残留，不能解释或关闭旧PID。为避免再次全量构建，已采用独立reviewer建议的证据分离：只读核验原compiler-artifact、现存PE摘要及源码身份，保留`buildSupervisionStatus=failed_open`，另审限定测试入口；不把诊断或产物存在升级为完整构建通过。原记录分别在[构建失败](../../../testing/runs/deliver-codex-session-loop/2026-11-13-wss-combination-v1/independent/build-execution-v6/RESULT.json)、[handle诊断](../../../testing/runs/deliver-codex-session-loop/2026-11-13-wss-combination-v1/independent/build-handle-diagnostic-v1/RESULT.json)及[产物核验](../../../testing/runs/deliver-codex-session-loop/2026-11-13-wss-combination-v1/independent/artifact-verification-v1/RECEIPT.json)。

WV13/14/15的两轮合成HTTPS main DOM运行均保留原失败：第一轮Vite HMR stub缺少`prune`而未初始化；修复后第二轮生产main拒绝fixture challenge的三个额外顶层字段，未进入三例业务判据。原签名payload应完整，challenge顶层须严格按协议字段集发送；修复仅限fixture，不改产品parser或等待时间。前后输入无漂移、没有外部请求或原生WebSocket、浏览器/Vite与私有目录已清理。当前待新唯一目录复跑，原[execution-v2结果](../../../testing/runs/deliver-codex-session-loop/2026-11-13-wv-main-dom-acceptance-v2/execution-v2/results.json)不是产品通过或产品缺陷结论。

Source后继[增量方案](../../../testing/runs/deliver-codex-session-loop/2026-11-13-source-entry-rebase-v1/REBASE-PLAN.md)已落盘：R1以schema11新增两表两索引并保留当前release schema10，固定旧DDL字节来源与迁移/回滚/旧fixture影响；R2沿原runtime/receiver、原SQL task/receipt与关闭责任移植discover/create；R3另接认证WSS/Web和无执行绑定的外部ID读取。独立TEST-DELTA及方案检视尚待，不授权旧候选直接安装或提前产品写入。主执行维护该方案，现有独立角色收尾后复用，不再建大副本。完整判据未满足，tracked仍9/28；npm audit仍未执行。

[2026-11-13：Source R1 schema11 精确复验]
修正 `native_release_schema` 测试夹具：结构异常的 trigger 与缺失索引分支此前从空内存库调用初始化，缺少 `node_meta` 身份，实际未到达拒绝断言；现在两分支均从已验证的 schema10/release fixture 升级到 schema11 后注入异常对象，未改产品逻辑或断言。Rust 1.98.1、`--locked --offline`、`windows-owner.ps1`（临时 ExecutionPolicy Bypass 子进程，owner probe 成功）和独占 target 下，真实 deferred-FK 787 用例 1/1、结构异常拒绝用例 1/1、整个 schema11 测试模块 8/8 通过，rustfmt check 通过。记录见 [Source R1 精确复验](../../../testing/runs/deliver-codex-session-loop/2026-11-13-source-r1-schema11-focused-rerun.json)。这只关闭该测试夹具阻塞；R1-07/R1-10 其余边界、独立复核、R2/R3 和安装仍未完成，整体任务不勾选。

[2026-11-13：Source R1 身份与布局边界扩展]
在上一组 8/8 通过的基础上，测试候选新增反向版本头不一致（pragma=10、metadata=11）、Source registration 意外列、有效但错误的复合外键目标，以及 fresh schema11 移除 `node_meta` 后空身份重开。当前测试文件 SHA-256 为 `C083E0D06878DE48454F9F8BE30EE5AC6821A9B4D76DEA280CF553C5F3C8C932`；rustfmt 和该文件 `git diff --check` 通过。针对新 SHA 的 Cargo 测试连续三次在执行器创建进程前被 `wait for runner spawn_ready` 拒绝，Cargo/rustc 均未启动；因此这四项尚未验证，旧 SHA 的 8/8 不复用为当前结果。精确记录见 [R1边界扩展记录](../../../testing/runs/deliver-codex-session-loop/2026-11-13-source-r1-schema11-focused-rerun.json)。R1 总体仍未完成；既有失败与数据现场保留。

修正后测试文件 SHA-256 为 `2435D97DD9E0261E1E2FF559A50DE4CDC94CC84DCE81B4C510E1D61BF38845EF`；Rust 1.98.1、`--locked --offline`、独占 target 下 schema11 测试模块实际 **9/9** 通过，覆盖上述新增场景。独立静态复核发现并促成移除空 identity fixture 中错误的 release 行断言；复核确认剩余 fixture 自洽，但反向版本头组合只能声称被拒绝，不能声称隔离命中双头比较分支。该次有效测试未加载 windows-owner runner，另一次 owner-run 启动被执行策略/runner readiness 阻断，因此只记开发局部通过，不记 Windows owner 验收。R1-07 其他边界和 R1-10 的各 Source 表非空/无身份矩阵仍待；多行 `node_meta` 在支持的 singleton 主键/check 布局中不可表达。运行明细见同一记录。

[2026-11-13：Source R1 身份缺失与非空表边界复验]
schema11 测试文件当前 SHA-256 为 `736B564621F7EAE78C6DBD1918CAA5027E6A07725C14F38634B6DAACDCFBC9F8`。在 Rust 1.98.1、`--locked --offline`、独占 `.tmp/q3-source-r1-20261113` target 下，schema11 测试模块 **10/10** 通过；覆盖 schema 10 夹带 Source 对象、unknown high version=12、schema11 缺少 operation 表、temp view shadow、Source 布局错误、孤立 Source operation 行，以及 registrations/operations 分别非空但无 `node_meta` 时的整体拒绝。后两项先断言外键恢复为启用、身份数为零、目标 Source 行仍在；独立 reviewer 提醒这两种损坏夹具会先被外键校验拒绝，不能声称单独命中 `require_empty`，记录仅表述为整体拒绝。未改产品代码。当前进程未加载 `windows-owner.ps1`，结果是局部开发验证，不是独立验收。运行记录为 [R1 schema11 复验](../../../testing/runs/deliver-codex-session-loop/2026-11-13-source-r1-schema11-focused-rerun.json)。R1-07 更广泛的 main/temp trigger/view、Source/profile/harness/workspace 外键矩阵仍待；整体任务不勾选。

[2026-11-13：Source R1 main/temp 结构矩阵]
在独占 Rust 1.98.1、`--locked --offline` target 下，schema11 测试模块 **11/11** 通过。新增 8 个场景：registrations 与 operations 各自的 main/temp trigger，以及四个必需 Source table/index 名称上的 temp table/view shadow；均断言拒绝并保留双版本头及 main schema，临时对象也保持原样。测试文件 SHA-256 `F7FFC7A1F3F2E2EE9DE71CDD6FECFBF123F513C2D244C54FED4467D6281E7982`。修正前夹具 SQL 语法错误的失败已从有效结果排除；首次权限不足和 rustup 错选 stable 的失败也分别记录。有效运行显式绑定 rustc/rustdoc 1.98.1，但未加载 Windows owner runner，属开发验证。详见 [R1 main/temp 矩阵复验](../../../testing/runs/deliver-codex-session-loop/2026-11-13-source-r1-schema11-matrix-rerun.json)。R1-07 的 Source/profile/source/harness/workspace 外键坏引用矩阵、其余组合及独立验证仍待；未改产品代码，未勾选任务。

[2026-11-13��Source R1 �����״������] ��ǰ tests.rs SHA-256 Ϊ 8D51AA6890D0F44605A0115F3F311A6F93B42AB934291EBBA3DCDACEBC41A23D���� Rust 1.98.1��--locked --offline����ռ .tmp/q3-source-r1-matrix-20261113 target �£�native_release_schema ����ģ�� 12/12 ͨ�������� registration �� clients/product_sources/harnesses/node_launch_profiles/workspaces �Ĵ������Ŀ�����˳��ܾ������������� schema ˫ͷ�������������𻵶��壻ͬʱ���Ǽ��� main/temp trigger��temp shadow��deferred SQLite FK 787 ��ع���������¼�� 2026-11-13-source-r1-schema11-fk-shape-rerun.json���ý����Ϊ�����ֲ���֤��R1-07 �Ļ��������ݾ���R1-10���������ա�R2/R3 �밲װ�Դ���npm audit ���û�����δִ�С�

[2026-11-13��Source R1 ���������ݾ�����] �� schema11 ��ʵ���� fixture ������ registration �� source_id/harness_id/workspace_id/profile_id �������ݣ��Լ� operation �� source_instance_id/connection_epoch �������ݣ�����ֻ�ڼо�ע���ڼ�ر����Ӽ� foreign_keys�����ָ������� PRAGMA=1��Rust 1.98.1��--locked --offline����ռ .tmp/q3-source-r1-matrix-20261113 target �£�native_release_schema ģ�� **13/13** ͨ�������� schema ˫ͷ������ sqlite_schema�����б����� UnrecognizedSchema �ܾ������� tests.rs SHA-256 291606C78652FC7AEAB4E44C7070BC45A2B8131234D6FED2102DAE75848D8727��ִ�м�¼�� 2026-11-13-source-r1-schema11-fk-shape-rerun.json���״�δ�ر� FK �ļо�ʧ�ܱ���Ϊʧ���ֳ���δ����ͨ�����ý�����ǿ����ֲ���֤������ Windows owner �������գ�R1-07/R1-10 ����߽硢�������ˡ�R2/R3 �Ͱ�װ�Դ���

[2026-11-13��Source R1 ������ֵ�������Բ�ǿ] ������̬������� registration �������� initialize �����ٴζ�ȡ��ֵ���Ѳ��� post-init SELECT��ȷ�� source_id/harness_id/workspace_id/profile_id �� malformed ֵδ���޸�����ͬ Rust 1.98.1��--locked --offline����ռ target �� native_release_schema **13/13** �ٴ�ͨ�������� tests.rs SHA-256 258077E2BD632450F8E07CC3F3AF20A54DB3C7BBB15859711EF07BA021079C3B��JSON ִ�м�¼�Ѹ��¡��ý�����޿����ֲ���֤��δת���� Windows owner ���ա�


2026-11-13 R1 schema11 development follow-up (independent review still open)
- Root developer source freeze: `testing/runs/deliver-codex-session-loop/2026-11-13-r1-complete-facts-development/SOURCE-FROZEN.json` (516 leaves, canonical `b9de84e397f31d3f7882c807b07ef28b47fe4774f93c6819cacce035498951aa`, Git index unchanged).
- The affected development suites now passed with Rust 1.98.1, `--locked --offline`, serial execution and an isolated target: native release schema 15/15, catalog schema 8/8, related schema migration 7/7; rustfmt and diff checks passed. The result and raw logs are in `testing/runs/deliver-codex-session-loop/2026-11-13-r1-complete-facts-development/RESULT.json`.
- The test-only changes add full SQLite row/column snapshots for legacy tables (NULL/BLOB-preserving), revoked actor/security audit/approval facts, exact Source fault error classification, and a real schema-8 metadata row with both heads asserted. This is development evidence only; it does not close independent R1 acceptance.
- Independent R1 file-database acceptance remains pending: fresh file open/close/reopen, committed-report-loss reopen, empty schema-10 without identity upgrade, and one controlled conflicting `node_meta` rejection, plus Red Check. The tester must bind the final source manifest and use an owned runner before dynamic execution.
- R2 Source session acceptance definition is complete but pending reviewer approval: `testing/runs/deliver-codex-session-loop/2026-11-13-source-r2-definition/` (V01 discover, V02 create, V03 WrittenUnknown de-duplication, then M01-M07). No R2 product write is authorized until the reviewer confirms the definition and R1 acceptance gate. `SSF-SQL-RECONCILIATION001` remains Open.
- `npm audit` remains intentionally unrun per user instruction. Overall tracked progress remains 9/28; no task checkbox is changed by this entry.

[2026-11-13：R2 Doc 窄切片安装后验完成] Root 已安装固定 517 叶 Doc 版本（canonical SHA `21137d04eb1cfcdce87342d363d29ccaa9734b2f75f716aaec760fe2cd8061f1`），与隔离源逐叶一致，Git index 保持 `ac76c7ad84c1d4333fc64a710647e60aeb96015eb4407f996a4bf6adc91fbff4`。独立 tester 固定 PE `7d8789785a2728360850daa858140b7beaeaa7948da66a0c2b806fcd0af57fc1`，7 个 Doc selector 各 exact 1 通过，所有 Job 自然退出、Stop=false、active=0；独立结果见 `2026-11-13-r2-doc-independent/RESULT.json`。Root 安装后 all-targets/all-features check、默认 build、Rustfmt 与 `git diff --check` 均 exit 0，机器记录见 `2026-11-13-r2-doc-slice-development/ROOT-POSTERIOR-RESULT.json`。该证据仅完成 Doc 窄切片，不能代签 Source producer、R2 V01–V03/M01–M07、WP4 或最终 task；`npm audit` 按用户决定未执行。

[2026-11-13：WP1.4 当前源码快照复跑] 为排除工作区受限历史临时目录和旧二进制影响，复制当前 `app`（排除 `app/.tmp`）到独立源码快照，显式使用 Rust 1.98.1、`--locked --offline`、独占 target。`session_storage::product_commands::independent_acceptance` 串行 **31/31** 通过；此前并行运行中唯一一次 `m_body_ttl_does_not_expire_still_valid_signed_request` 的 `Cancelled` 未复现，精确重跑 5/5 通过。真实 SQLite deferred COMMIT 用例 `cl004_real_engine_commit_failure_is_not_a_precommit_hook` 也 **1/1** 通过。记录见 `2026-11-13-wp1-1.4-product-command-rerun.json` 及 stdout。该证据仍属开发/源码快照范围，WP1.4 不能勾选：全库有效检查仍有既有 clippy 诊断、其他 Node/SQLite/DPAPI 环境范围未清、npm audit 按用户决定未执行。
