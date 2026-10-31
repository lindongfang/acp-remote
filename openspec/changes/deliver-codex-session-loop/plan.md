# Execution Plan

## Execution Model

Q-001，2026-10-31。本轮只修订规划，不改产品实现或实际数据。已有schema4、旧子合同和旧验收保留原快照范围；当前八份specs和design取代冲突规则，准确新DDL/DTO与协议完成算法需另阶段独立冻结。

主执行维护规格、合同和计划。独立tester依据当前文件、AGENTS/config及角色读取，不继承主执行对话，在隔离候选目录编写tests；独立reviewer只读固定规划快照并输出报告，不代签产品功能。

## Parallel Assessment

主change核心规格及跨change引用存在共同授权、期限和历史依赖，由主执行串行改写，避免并发改同一文件。规格稳定后独立tester在隔离目录生成用例，可与主执行下游文档同步并行；合入后由另一reviewer独立核对同一快照。后续产品源码按WP2→WP3→WP4依赖串行；tester/reviewer正式验证采用独占数据根、端口和target，不并写产品目录。

## Work Packages

| ID | Goal / Scenarios | Dependencies | Agent | Write Scope | Deliverable | Verify |
| --- | --- | --- | --- | --- | --- | --- |
| WP1 | Q-001决定、八份规格、限制核对及下游一致性 | 已归档节点接入、用户确认 | 主执行 | 本change规划/合同及相关docs/README/config/下游规划 | 规则单一来源、旧范围说明 | strict/diff及CL-087起限制核对 |
| WP2 | 节点登记、受理授权、共享队列、必要存储与严格审批 | WP1及独立用例/方案检视 | 后续coder | domain/application/session_storage、必要声明、开发测试和合同 | 未发送正文仅内存、去重及单次派发、附加审计分离 | CL-003/004/014/017–021/075–084及CL-087起 |
| WP3 | ACP真实来源、完成乱序、有效配置、独立接收和资源 | WP2，版本合同冻结 | 后续coder | adapters/acp/platform/harnesses及node/lib资源装配、开发测试 | 无重放模式、可靠核验出口、共享实例和完整树 | CL-032–041/047–074及CL-087起 |
| WP4 | 同一认证通道共享队列及Web交互 | WP2/3，DTO冻结 | 后续coder | management_ws、clients/web、node/lib最终装配、README | A/B共享编辑取消、优先中断、失败继续、历史提示 | CL-009/022/030/042/085/086及CL-087起 |
| WP5 | 独立用例维护和正式验收 | 规划依spec/design；执行依冻结产品 | 独立tester | tests候选、acceptance/suites及自有runs | 稳定旧ID/标题、逐Scenario映射、新行为用例及真实边界 | 当前完整tests；不把本轮定义当执行 |
| WP6 | 独立方案及交付检视 | 方案依稳定文件；交付同WP5产品快照 | 独立reviewer | 只读产品及自有报告目录 | 问题ID和复查，固定范围 | CL-031及限制/需求映射人工检查 |

## Merge Order

| Order | Work Package | Reason | Re-verify After Merge |
| --- | --- | --- | --- |
| 1 | WP1→WP5候选→WP6 | 核心规则先稳定，用例不受旧实现迁就 | strict、逐Scenario映射、差异 |
| 2 | WP2 | 准确队列状态/DDL/授权先冻结 | 内层及真实SQLite故障/并发 |
| 3 | WP3 | 协议事件不写业务SQL | 双版帧合同、嵌套审批及完整树 |
| 4 | WP4 | 最终认证和Web装配 | 完整工程检查、真实Web |
| 5 | WP5/WP6 | 同固定产品快照并行独立验证 | 真实必需项及问题复查 |

WP2→WP3→WP4共享node/lib时严格串行，后合入者重验集成；本轮仅WP1和规划用例/检视。不得因旧任务勾选或结构complete自动恢复代码接线。

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

本轮规格、合同、用例及上下文修订不等于实现交付。独立用例与检视状态随最终报告记录；产品测试、真实Harness、浏览器、进程树与迁移未在本轮执行。


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
