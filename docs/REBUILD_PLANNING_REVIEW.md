# 重建规划独立检视

> 版本范围提示（2026-10-29）：下文为既有独立检视及固定摘要，保留原记录。随后用户确认了通用ACP/Codex首验、DeepSeek/ZCode配置与兼容验收、用户决策阻断、升级复验和v1/v2兼容目标；受影响三项proposal/spec/design/tests/plan/tasks及重设计/能力地图已修订。原结论不覆盖新文本，新增用例和多版本功能均不能视为已验收；本轮独立复查另记，不能使用旧摘要签新版本通过。

结论：全部10项当前文档快照的方案、用例、计划与任务设计检视通过，未发现未闭环设计阻断。仅表示待实现文档达到本轮审查目标；本轮不授权编码、提交、推送或发布，也没有产品、安全、供应链、兼容性或正式验收通过结论。

独立reviewer不继承coder对话，实际读取AGENTS、OpenSpec config与reviewer职责、重设计及能力地图，并逐项审读proposal/specs/design/tests/plan/tasks；首项另审access-contract。reviewer只写各change/review.md及本报告，没有修改产品设计或独立tester用例。独立tester承担用例定义及执行安排复核，reviewer不以其自评代签。

## 历史逐项结论与固定报告

各报告记录逐文件SHA-256。报告自身摘要如下，用于定位这次最终复查版本；后续plan结果/进度变化须辨别与行为设计变化的区别，spec/design/contract/tests变化要按实际影响重验。

| Change | 用例数 | 本轮文档结论 | 独立报告 | 报告SHA-256 |
| --- | --- | --- | --- | --- |
| deliver-agent-session-tree | 10 | 方案/用例/计划/任务设计检视通过 | [review.md](../openspec/changes/deliver-agent-session-tree/review.md) | `652805ECA0E54EB4C12FC590AC0460B8151D022268B091584F96F91BE4FC827C` |
| deliver-backup-recovery | 10 | 方案/用例/计划/任务设计检视通过 | [review.md](../openspec/changes/deliver-backup-recovery/review.md) | `C3B47D5903190EBA037E068AE1230F6432F5B5EC96DD76A65739E0BBEB585C30` |
| deliver-codex-session-loop | 31 | 方案/用例/计划/任务设计检视通过 | [review.md](../openspec/changes/deliver-codex-session-loop/review.md) | `A4E6F3064E1232C472CD936AA5A19DAB98AC1482A67CA354C9569546C52A9D39` |
| deliver-history-navigation | 13 | 方案/用例/计划/任务设计检视通过 | [review.md](../openspec/changes/deliver-history-navigation/review.md) | `D6581689B840B603394A4C507E797D59D3AF6FFD1C65E691EF041ECAA6F5D84C` |
| deliver-native-session-import | 10 | 方案/用例/计划/任务设计检视通过 | [review.md](../openspec/changes/deliver-native-session-import/review.md) | `87E17564D65A1D6023C9F284E60D3F8FAB7B2559F8410D43AE7B94D003D3C159` |
| deliver-session-capacity | 12 | 方案/用例/计划/任务设计检视通过 | [review.md](../openspec/changes/deliver-session-capacity/review.md) | `9A9A437B41BB2872350367274906B3B560E87E77EB507B18494EA18F650A9C66` |
| deliver-session-data-lifecycle | 11 | 方案/用例/计划/任务设计检视通过 | [review.md](../openspec/changes/deliver-session-data-lifecycle/review.md) | `77A126199DE91D54AB3602C93DD055DDEC3996F34F9CD709BC8407B3D24D68B1` |
| establish-node-access | 34 | 方案/用例/计划/任务设计检视通过 | [review.md](../openspec/changes/establish-node-access/review.md) | `D0B21DB495BC7A82919C92648BA06A1301F082DE4B9C85E372ABB7F4C4A6E37C` |
| integrate-deepseek-harness | 11 | 方案/用例/计划/任务设计检视通过 | [review.md](../openspec/changes/integrate-deepseek-harness/review.md) | `AF1187DDEF63756DB3849F9976E15814A95E5BB2E02C9DCE16CAE1221319B7C5` |
| integrate-zcode-harness | 10 | 方案/用例/计划/任务设计检视通过 | [review.md](../openspec/changes/integrate-zcode-harness/review.md) | `2D27BBAC266F594A87362249587857B72B796F2922AEC18DDBABE1CE4415B1D2` |

共152条用例。所有产品用例尚未执行，not-run被明确保留；用例定义、静态规划检查与真实运行结果严格分开。

## 问题闭环

- NA-SEC-01–08：节点引导、签名字段/挑战、Windows所有者、撤销发送线性化、Web身份、撤销key不可复活、资源预算与本机边界，设计修复实际复查关闭。
- NA-TEST-01：32条分页、游标身份/节点/revision/到期、通知合并，NA-029已补且实际复查关闭。
- HN-DES-01：单轮元数据无界，改同一10轮逻辑页内16KiB/64描述块与有寿命页句柄；HN-006含10000条/更新/取消/溢出/重同步，设计用例关闭。
- DL-DES-01：Session删除不得清空参与者parent/root，Agent tree与lifecycle均保留无正文身份墓碑；DL-006/AT重启用例含多层、重复委派、根可追溯，关闭。
- BR-DES-01：不备份密钥的恢复副本只能离线检验，不能启动daemon、静默换钥或继承旧信任；BR-008/009和最终Main E2E明确该边界，关闭。
- DL-DEC-01：初读疑似未授权分支，经重设计§8/§10明确允许详细化规则复核后撤回，不作为用户确认阻断；默认不级联等被记录为设计选择，不假称逐项用户确认。

## 一致性与实施门禁

所有spec场景在tests有精确引用及步骤/预期；正常、异常、重复、并发、取消与边界由逐场景及L3/L4用例覆盖，L6保留真实Windows/Web/Harness或数据库路径要求，LX人员检视不允许Agent代签。辅助检索仅核对结构、字段、场景名及WP标识，未冒称其可自动证明语义覆盖。

各plan包含并行评估、精确写范围、共享路径串行合入和重验；每个WP有真实checkbox任务引用及完成判据，全部实施任务未勾选，Main E2E每项只有一个required配置。共享注册按ZCode后DeepSeek串行合入；原生接管明确依赖容量，地图含G→H且提案/计划一致。首项协议与后续历史分块预算、身份/会话/参与者边界已交叉检查。

实施仍须按各change门禁依次完成：真实上游交付与最终合同重基准、实际Harness程序/版本/协议/能力探测、依赖维护/许可/漏洞与锁文件审查、真实编译与检查入口、平台/协议合同/进程树/保存故障/真实闭环及独立人工验收。ZCode入口及各款恢复/子身份能力未被虚构；缺必需能力应阻止adapter实现或交付，不降级产品语义。本轮文档检视通过不消除这些实施前条件。

reviewer实际执行git diff --check退出0（README行尾转换提示不构成差异错误）。未执行产品构建或功能测试；OpenSpec结构校验由主执行/作者另记录，本报告不代签其运行结果。

## 2026-10-29 通用 ACP 边界修订独立复查

结论：本轮规划修订检视通过，无开放规划阻断。此结论只适用于下列文件快照，不是产品实现、真实 ACP v1/v2 兼容或 Harness 验收通过，也不使上游交付、规范核实和真实对端门禁自动满足。旧报告及其摘要原样保留，旧通过结论不覆盖本轮改动。

独立 reviewer 读取 AGENTS.md、openspec/config.yaml、reviewer 角色和用户确认边界，只读检视重设计 §4.5、能力地图，以及 deliver-codex-session-loop、integrate-deepseek-harness、integrate-zcode-harness 的 proposal/specs/design/plan/tasks/tests；仅本节由 reviewer 追加，未修改受审规划。核对通用客户端和版本策略隔离、按版本与能力而非厂商分支、配置与兼容验收边界、不兼容报告与用户决策、升级复验和旧证据版本归属、v2 未核实门禁。核对新增 CL-032–041、DS-012–015、ZC-011–014 的实际步骤与预期，正常、异常、取消及边界均保留真实/模拟证据区别。

| 问题 ID | 位置及问题 | 独立复查结论 |
| --- | --- | --- |
| R-ACP-001 | 能力地图及三份 plan 把旧独立通过无条件沿用到本轮新规划 | 已关闭：明确旧快照归属、本轮另行复核，未覆盖旧报告历史 |
| R-ACP-002 | 通用闭环 plan 的 WP5/PV3 用例范围遗漏 CL-032–041 | 已关闭：WP3/WP5/PV3 扩展到新增用例；两接入的 Project Verify 明确新增用例的 WP2/3/4/5 安排 |
| R-ACP-003 | CL-033 的有效审批断言未区分取消前后，可能与 v1 待处理权限必须回复 cancelled 冲突 | 已关闭：独立 tester 修订后实际复查，取消前唯一裁决，取消后按版本规范处理；v1 回复 cancelled 并拒绝旧授权，权限取消不冒充前台完成 |

辅助只读 Node 检查精确 Requirement/Scenario 引用：三项分别覆盖 33、9、8 个规范场景，无不存在引用或未引用场景。该检查仅核对名称和归属，步骤与断言由 reviewer 人工复查，不宣称自动证明语义覆盖。工作包、任务引用、串行共享写范围和 Main E2E 人工核对。未运行产品编译、产品测试、真实 Harness、Windows 进程树、安全/供应链或人工 LX 验收；未核实官方 ACP v2 的发布状态和实际对端，不能据本报告宣称支持。

以下 SHA-256 为 reviewer 实际读取的完整文件字节摘要；包括三份 plan 全文，不作摘要排除。后续仅更新 plan 的 Findings/Verification Results 记录可另说明记录差异；其他受审内容变化仍需影响分析和复查。本报告自身不纳入摘要，以避免自引用。

| File | SHA-256 |
| --- | --- |


| docs/HARNESS_CONTROL_PLANE_REDESIGN.md | `6EBCD600C2B3896131BAC1923C5B333D544B1F50F773461D91E9F64D5A4AB3C1` |
| docs/REBUILD_CAPABILITY_MAP.md | `CC6B8D5D36E8304F48FFD1D8E55E3F10E050A45AF22884C1E860615D2BCFEBC9` |
| openspec/changes/deliver-codex-session-loop/design.md | `2C7EB6FEB1BA4150871C140C24809C085B78FA4545F875807AFF286D9D4A04E9` |
| openspec/changes/deliver-codex-session-loop/plan.md | `3855C19EF242898E9EABBE60753F96112E8BF37DB68AC0D4B1994D6497B85F15` |
| openspec/changes/deliver-codex-session-loop/proposal.md | `E803BC148733CB9FFA6195A50495DCBE9013AFE2F759484D9AD619C031BE1B28` |
| openspec/changes/deliver-codex-session-loop/tasks.md | `AEF3CC0EA9BE65A05B076784FEDB733F9DB9C0A99B18EFF8394FF11B676B4244` |
| openspec/changes/deliver-codex-session-loop/tests.md | `DB40BE35122D9159CEB61BFFF23452698ACFC18D48DD498AF2176025B5A11146` |
| openspec/changes/deliver-codex-session-loop/specs/harness-management/spec.md | `078D95A6831B267604C8E069D167CF40E5B91071DBEB68369C0F1FCD044CC847` |
| openspec/changes/deliver-codex-session-loop/specs/management-audit/spec.md | `66DD2DD8A304389F9A3B1C50FF80641A89AE85F1314BF882A2366981211D5715` |
| openspec/changes/deliver-codex-session-loop/specs/native-session-recovery/spec.md | `8FEFC8E4CCD951644A16FA56429F0C62CA0821BBBF0FAF07D986BA6B77363C2B` |
| openspec/changes/deliver-codex-session-loop/specs/permission-approvals/spec.md | `E61FB389DFBAD686F883FEA77E2D679AFEEADBE540D029E91F6632FACCFC7601` |
| openspec/changes/deliver-codex-session-loop/specs/session-execution/spec.md | `61067A30D797F0AD3789226CAA3B1491299AA4F45400F9230221298823ED00A6` |
| openspec/changes/deliver-codex-session-loop/specs/structured-history/spec.md | `67F05D151AAFE3D069BE1E962529D991F45B71940BBA2378CBDF1589067FD410` |
| openspec/changes/deliver-codex-session-loop/specs/workspace-registration/spec.md | `4CF4E9E297EFEB4A5C56FC89B706B3FD46FD866849708FE032EFBCD78C1E9DE9` |
| openspec/changes/integrate-deepseek-harness/design.md | `169E222A7C8F1462E5761994DBA59FACF18665BD7924A1E9F19836D4AE8F1322` |
| openspec/changes/integrate-deepseek-harness/plan.md | `4617C77181155E2F02EC03C92D9A43C51644723380BB5B1FBADB2DCDECA0F4A0` |
| openspec/changes/integrate-deepseek-harness/proposal.md | `B6C00E3D937B4715EB15100DAE797B44653450EA6680E72F1D564AFFEC00E216` |
| openspec/changes/integrate-deepseek-harness/tasks.md | `40E242CD102834620B99F598B01FBBE0B09B4ABA930F407072C8378CD9D107B0` |
| openspec/changes/integrate-deepseek-harness/tests.md | `7CD66BE88BD0CB6F31AC100561C43BE6484EF4AF44A418D809A7D3BC0E377462` |
| openspec/changes/integrate-deepseek-harness/specs/deepseek-harness-compatibility/spec.md | `E971170F75B70D0E107C60EE1EEE2233E94B4FF9B5C1381625E831668FA6A7BD` |
| openspec/changes/integrate-zcode-harness/design.md | `825E44D2C9F211FD25F4F8300372A8B322AD42FAB57EA3C9DE0A1E31AA069D0F` |
| openspec/changes/integrate-zcode-harness/plan.md | `DAE499344111BAC701BE9A3005BCC1674329551A1D70C97BD1A434D2B0928E14` |
| openspec/changes/integrate-zcode-harness/proposal.md | `08F88BFA46BDAC29D0DC24BA203503B69018A04919947180473D564A979F7A1B` |
| openspec/changes/integrate-zcode-harness/tasks.md | `77C67EC94E1AB81533A6B60AF168DE1683771B6444E28F47A9234163061C009A` |
| openspec/changes/integrate-zcode-harness/tests.md | `CB91044AE83B6A52A1F0A014E21B7FB723D6F539E335E477939D5E62309D03D6` |
| openspec/changes/integrate-zcode-harness/specs/zcode-harness-compatibility/spec.md | `361D7B9686866C850B8D6CEDE33C29340424401961C86F6590FD275E6B9A6C89` |

## 2026-10-29 父步骤展开修订独立复查

结论：本轮规划检视通过，未发现开放规划阻断。范围为重设计、能力地图，以及 deliver-history-navigation 与 deliver-agent-session-tree 的 proposal、spec、design、plan、tasks、tests，共14份文件；以下完整文件字节 SHA-256 固定本次结论，旧报告及摘要保持原样。此次结论取代这两项旧快照对父子展示方式的判断，不追认任何产品运行结果。

独立 reviewer 重新读取 AGENTS.md、openspec/config.yaml 与 reviewer 职责，依用户“子会话作为父会话的一步，点击可以展开”的纠正核对以下边界：父每页10轮，子原位展开/折叠；子身份、独立存储及内部顺序保留；无父子全局混排；重复委派引用同一可靠子身份，无法证明委派内容范围时明确未区分；嵌套逐层按需，父页和分支共享预算；折叠仅释放所属读取快照/订阅，不取消执行、停止保存、触发原生恢复或刷新保活；重连、旧代际/版本、缺失尾部和目标授权均不以展开便利性绕过。未发现展示修订隐式授权删除子历史或改变既有删除/墓碑边界。

人工核对新增 HN-014–019、AT-011–014 的步骤与预期，以及旧用例 HN-012、AT-005–009 的修订。HN-012 使用真实数据库查询、缓存/订阅和 Web，可靠关联可显式 fixture；其证据只证明通用读取/投影，不冒充真实 Harness 子身份。AT-009 承担真实关系、三款实际暴露范围及完整原位展开验收，不反向成为03的实施门禁。程序入口尚待实现的事实保留，缺实现/能力记 not-run，不以文字或模拟结果替代真实证据。

| 复核项 | 结论与证据范围 |
| --- | --- |
| 精确规范引用 | reviewer 实际运行只读 Node 辅助检查：HN 19例引用11个规范场景，AT 14例引用7个规范场景；不存在引用、未引用场景均为0。步骤与断言另由人工检视，名称匹配本身不证明语义覆盖 |
| 工作包与任务 | 每个声明WP均有真实checkbox正文引用，未发现无WP或未声明WP任务；人工复查共享写范围串行合入、HN/AT受影响重验、同版本独立角色与 Main E2E 边界 |
| 旧证据归属 | 两份旧review保持历史快照，不当作本轮新文本通过；能力地图及plan明确本轮另复查，用例数为19/14，任务仍9/12项待实施 |
| 新问题 | 本轮未新增需修复的问题，不虚构 HN-TREE 问题ID或关闭记录 |
| 差异检查 | reviewer 实际执行 git diff --check，退出0；已有LF/CRLF提示不构成差异错误。主执行者所报OpenSpec检查不在本结论中代签为reviewer运行 |

未验证范围：全部产品编译/类型/协议/功能测试、真实数据库性能和共享预算实测、真实浏览器展开体验、真实 Harness 子身份/嵌套/重复委派支持、Windows完整进程树、安全/供应链及人工LX验收均未执行。规划通过不解除真实上游交付、合同重基准、实际能力探测与实施前独立门禁，也不表示三款均有完整子会话能力。

以下摘要包含两份plan全文，不排除结果节；后续若只更新记录节，须明确记录差异，其他内容变化仍需影响分析和复查。报告自身不参与摘要。

| File | SHA-256 |
| --- | --- |
| docs/HARNESS_CONTROL_PLANE_REDESIGN.md | `E8BA4E1302A8B76599457A85720EDBBCC070FDB7C8D3A59185D9D1582D3A4AE2` |
| docs/REBUILD_CAPABILITY_MAP.md | `89E2EE2E2875B366A461A5D2D55A25C5EE583A27CA461311B1ED1C5B13BA1A38` |
| openspec/changes/deliver-history-navigation/design.md | `66F81F4D3DB7487762180504E0785089B33FEFE8F63008BF31B461DAE29E14DB` |
| openspec/changes/deliver-history-navigation/plan.md | `DFA74F15A709A9802AE1FE51C24C1A3116AD3EB2A261A13AF4155A38F46AA4B9` |
| openspec/changes/deliver-history-navigation/proposal.md | `30819C81C0981D7402406E69A40109D6A8042C72174951459D8AFB93B0A2F667` |
| openspec/changes/deliver-history-navigation/tasks.md | `43B3B84B3B9C510E9C0132104DAA7676DF28EF6124318D0EB4AA9120E2856AB1` |
| openspec/changes/deliver-history-navigation/tests.md | `4A8289ECD39F70063B99EA83B1CEB054A76B01EE3ACDA761EF96E0818D7DAEA1` |
| openspec/changes/deliver-history-navigation/specs/history-navigation/spec.md | `BAB48F8B44FD8E54F0C9C56B0CFC22FAE9E23B288386B8D4EBC414075B6B22D1` |
| openspec/changes/deliver-agent-session-tree/design.md | `CF088D51FE754DC467910CC6034ABC37F92D3D3A989A213EF7DBB8FD3670EF0B` |
| openspec/changes/deliver-agent-session-tree/plan.md | `702779A0AF1840FD7E6C1FA07A3B298C2C9F4F0A6B1E660B8862DFD39DDB8D86` |
| openspec/changes/deliver-agent-session-tree/proposal.md | `7F4D36FC8C0BFF0035E77B8375F9345ED2DC0E5921D4B1A70E2CD7345C816849` |
| openspec/changes/deliver-agent-session-tree/tasks.md | `F75DC9B543EE69524956B1C3C5B5EC7F3B6C20598A90319C3F2264058C8EE4AB` |
| openspec/changes/deliver-agent-session-tree/tests.md | `A467C6CF73B30A674907681595D586B53C01D23CC66DA8FB32265CFF4AD49228` |
| openspec/changes/deliver-agent-session-tree/specs/agent-session-tree/spec.md | `C3B47BA0BD8291F9AD568D6406E4128A501CAFE38535D3161342E6B5C8BDCE0F` |

## 2026-10-29 回收资格澄清独立复查

结论：本轮资源回收资格规划检视通过，无开放规划阻断。审查范围为重设计§7.4、能力地图及 deliver-session-capacity 的 proposal/spec/design/plan/tasks/tests，共8份文件；下表为实际完整文件字节SHA-256。保留所有旧报告及摘要，旧review只适用于其原快照；本结论不把旧产品证据迁移到新规划。

独立reviewer依据AGENTS、OpenSpec配置及reviewer职责，核对用户纠正：有效待授权同时豁免30分钟超时和满额淘汰；取消授权只解除该请求保护、重新评估，不代表prompt终态或资源已释放。其他有效审批、父/子前台或需保留后台工作及未知安全事实继续保护。全部保护解除后，超时从进入安全可回收状态与最近有效活动较晚者计时，不抵扣此前等待授权时长；容量满可从安全非保护主会话中选择静默最久者，无需等待30分钟。名额仅计主会话；子生命周期由父/Harness管理，不独立扫描超时或淘汰，子活动仍保护父。关闭失败、资源未知和无候选均不以强制取消、超限或自动排队解决。

人工逐项复核新增CP-013–018及更新CP-003/004/011/012：多个审批逐请求解除与并发取消、prompt未终态、父/子前后台与深层子活动、未知释放证据、候选选定后新增审批/工作、重新进入保护再解除后的时钟，以及受保护A静默40分钟、安全B12分钟/C3分钟的满额选择均有具体步骤与预期。CP-017覆盖未满池取消后不立即关闭、29:59/30:00及扫描边界和满额提前回收两个分支；CP-011保留真实50主会话、实际30分钟、真实资源与Harness证据要求，不把局部可控时钟/故障注入代签为真实链路。

reviewer实际运行只读Node辅助核对：18个用例、12个规范场景，不存在引用及未引用场景均为0；12个真实checkbox任务覆盖所有声明WP，无无WP或未声明WP任务。辅助检查仅证明名称/关联，步骤、断言、单一Main E2E、共享写范围串行合入及上游重基准门禁另由人工核对。能力地图18例/12项待实施任务与实际文本一致；未新增待关闭问题，不虚构问题ID。

reviewer实际执行git diff --check退出0，已有LF/CRLF提示不构成差异错误。主执行者和tester各自所报校验不冒充reviewer运行证据。全部真实产品编译、协议/功能、50会话负载和实际30分钟、Windows完整进程树、安全/供应链及人工LX验收均未执行（not-run）；无产品运行结论。本次规划通过不解除上游真实交付、能力探测、可靠子活动事实和独立实施验收门禁。

以下摘要包含plan全文，不排除结果节；后续若只更新记录节须说明记录差异，其他受审内容变更须重新分析影响与复查。报告自身不纳入摘要。

| File | SHA-256 |
| --- | --- |
| docs/HARNESS_CONTROL_PLANE_REDESIGN.md | `AC47980C0A586CA35D3F6D60F8F6BD01C0BE12FAC11B801B18EFFB37F6D5B5D9` |
| docs/REBUILD_CAPABILITY_MAP.md | `EEA6B2B7FBEADC3ADD41CFF991DFB823481F4BC980B77BB80D0E6CD63BB3415D` |
| openspec/changes/deliver-session-capacity/design.md | `EE5306031583B4372E91370E9B250645ADBCAD1E81850883157C1F364A4A7CCF` |
| openspec/changes/deliver-session-capacity/plan.md | `B930266A0B7E6DF93E250749F04935D9D1AA963E9260AF6954CF476F4AB9A5EC` |
| openspec/changes/deliver-session-capacity/proposal.md | `8C81C11C1E20735AF40860872593DA14BA307D4B2E997A970F04C0EC555A8E60` |
| openspec/changes/deliver-session-capacity/tasks.md | `3BC0F8A8EB2FE6D6B0C86E6E0671A734F9689F5E3D60BDE35704CBE29719EE88` |
| openspec/changes/deliver-session-capacity/tests.md | `3C447AF2FBB21DDD718E3CC6DE954887C23221B90B0E7F471E517DB0E39CDA13` |
| openspec/changes/deliver-session-capacity/specs/session-resource-policy/spec.md | `7E19216CB04DB2001E55597EC25ACAD7754365E8A14B80E77E15FBE379E76C44` |

## 2026-10-29 分页参数、方向与保存透明独立复查

结论：本轮规划修订检视通过，PAG-001已复查关闭，无开放规划阻断。范围为重设计、能力地图，以及 deliver-history-navigation、deliver-agent-session-tree、deliver-codex-session-loop 的proposal/design/plan/tasks/specs/tests，共26份文件，实际完整文件字节摘要见下表。原父步骤展开、ACP及其他独立报告和摘要保持历史原样；此前固定10轮、子不分页或正常保存标识的规划结论不覆盖本轮新快照。

reviewer独立核对用户最新三项要求及其跨文档语义：前端指定pageSize、默认10而非固定10；父轮次DESC（25→16）、展开子轮次ASC（1→10），轮内文字/思考/工具/子步骤仍按首次位置ASC；子逻辑分页留在原父步骤且不改变父页。共用参数/校验/游标合同，非法类型、非正整数或超过实施时公开有界上限明确拒绝，不静默改成10。下一逻辑页可调整大小并从原排他边界继续，同页字节块沿冻结轮次集合读取；身份、目标、方向及父步骤作用域不可串用，即使两个步骤引用同一子Session也不共享越界游标。页大小和展开分支不放大既定字节/发送队列预算，逻辑分页不由字节分块代替。

正常实时输出、后台提交、分页及可完整衔接的正常重连均不展示保存中/未保存/已保存徽标、提交版本或保存/同步成功提示；内部当前/提交版本与缓存事实继续用于合并，旧版本不盖新。只有保存失败、异常缺失或完整性无法确认才显示错误和必要范围。500ms/64KiB保存、强制刷新、持久化权威、有界缓存及故障停止规则未被展示调整削弱。HN真实数据库/查询/同步/Web与显式关联fixture边界保留，真实Harness子身份及完整步骤内分页由AT-009承担，不产生03反向依赖06。

| 问题ID | 位置、依据及修复 | 独立结论 |
| --- | --- | --- |
| PAG-001 | CL structured-history规范“有界缓存和失败停止”原要求任何未保存尾部都标记，与正常保存透明冲突；现限定截断、缺失或故障导致尾部无法保证等异常才错误，正常待提交尾部不展示保存状态。HN丢失尾部规范/design同步区分正常可完整重同步和真正异常 | 已实际复查关闭；HN-011/019/025和CL-042的步骤及预期保留对应正常/异常对照，不用常驻徽标代替错误 |

人工逐条读取新增HN-020–025、AT-015、CL-042及相关旧用例修订，核对自定义大小/逐页变更、非法值与公开上限、方向及轮内顺序、新轮与旧工具更新、游标越权/跨步骤、页内冻结、共享预算、正常保存/完整重连无提示和异常错误对照。reviewer实际执行只读Node辅助检查：HN25例/17规范场景、AT15例/8场景、CL42例/34场景，不存在引用及未引用场景均0；真实checkbox任务分别9/12/14项，全部声明WP有正文引用，无无WP或未声明WP任务。名称与WP辅助匹配不自动证明语义覆盖，执行安排、写范围与串行重验另人工核对。

reviewer实际运行git diff --check退出0（已有LF/CRLF提示非差异错误）。主执行和tester各自报告的其他校验不由本报告代签。全部产品编译、数据库查询性能、真实浏览器交互、真实Harness兼容、Windows进程树、安全/供应链及人工LX验收均not-run；没有将用例定义、规划通过或fixture当作真实功能通过。pageSize具体上限尚须实施时依据预算固定、公开并实测，不能从本规划推导某个虚构常量。

以下SHA-256包含三份plan全文，不排除结果节；记录性变化须另说明，其他受审文本变化仍须影响分析与复查。报告自身不纳入摘要。

| File | SHA-256 |
| --- | --- |
| docs/HARNESS_CONTROL_PLANE_REDESIGN.md | `38332BEDB4707D9D752413A3D3FD38C7246D5FAAB7C00C0A7957E7256423E451` |
| docs/REBUILD_CAPABILITY_MAP.md | `421C7DA4CB3F80469F31A2B501F55415579F6AA1C5B091199CF2BB09E725772D` |
| openspec/changes/deliver-history-navigation/design.md | `2957EF4826B4052C026E9FCBC8546402D0486F87DEB5F7E1256C5044B615194C` |
| openspec/changes/deliver-history-navigation/plan.md | `C4298C0877085D916ECC0059D59CE8319724DE7ACBE29B515ADEACAFC9F0347D` |
| openspec/changes/deliver-history-navigation/proposal.md | `EF75F79FD1E8E832203697F44919EFECF5114605D06E242C73F5F563810E1A31` |
| openspec/changes/deliver-history-navigation/tasks.md | `BC2ACF776152273FCE87E7CC57570DBF9CAC00E8C68E63A8B14FC27EBC3925C5` |
| openspec/changes/deliver-history-navigation/tests.md | `35A420584093DB08C05B03B0DCB9A10BAF80925DB532252358014831DB7767EA` |
| openspec/changes/deliver-history-navigation/specs/history-navigation/spec.md | `DDFFBE493A564A2EA9B208D8AA3468C077687A6F1B3FB8ADCA4537F3918F7583` |
| openspec/changes/deliver-agent-session-tree/design.md | `1B9C05CB744FFEBAD04D8013584F6FD28E00BD40B61E3009B15AB76C3F06DE6A` |
| openspec/changes/deliver-agent-session-tree/plan.md | `607DFE1F78E87CAF1ABA09F0C383AE357E623AFC6098692A24FD173CD6FC59C4` |
| openspec/changes/deliver-agent-session-tree/proposal.md | `315E4075F3422032B68E06248217BAFFD7BF8F7F57EAF9C810515415FF323BE8` |
| openspec/changes/deliver-agent-session-tree/tasks.md | `FF9D0D62F9EE78BD79161316C6E28EB746139A21C61000E57D8E99EEC6C6F13C` |
| openspec/changes/deliver-agent-session-tree/tests.md | `2DEE4273BE6DA9019B16659B4BFF82AEDC5D4E53B4788B6BBCFF88E15391D16A` |
| openspec/changes/deliver-agent-session-tree/specs/agent-session-tree/spec.md | `F47A3DF0A402D9B972F9306A9999B93D9976F56B949EFEFEF35572FD042B80E3` |
| openspec/changes/deliver-codex-session-loop/design.md | `9483A38B391CF3831779D7F9203569C43C68E8F0AFA597008923874405EF81AC` |
| openspec/changes/deliver-codex-session-loop/plan.md | `AAD9F2E6058491505A4ABFAE68A8B898A36B3A62F13484EF7BBF7199BF2D9715` |
| openspec/changes/deliver-codex-session-loop/proposal.md | `68FC1FF1F37A00ED939EF9457C4D718DA28719840EE45F8AA87CFDEAB6B7712E` |
| openspec/changes/deliver-codex-session-loop/tasks.md | `0EFA97499DE213EAF07EB4AFAB55ED86D0C72CEB9A8798183DB50FA20CE6CA89` |
| openspec/changes/deliver-codex-session-loop/tests.md | `D158554EBE67406DB195BFEE3EEF92B85795A3BE44C4588E15EDFE539ABE5E06` |
| openspec/changes/deliver-codex-session-loop/specs/harness-management/spec.md | `078D95A6831B267604C8E069D167CF40E5B91071DBEB68369C0F1FCD044CC847` |
| openspec/changes/deliver-codex-session-loop/specs/management-audit/spec.md | `66DD2DD8A304389F9A3B1C50FF80641A89AE85F1314BF882A2366981211D5715` |
| openspec/changes/deliver-codex-session-loop/specs/native-session-recovery/spec.md | `8FEFC8E4CCD951644A16FA56429F0C62CA0821BBBF0FAF07D986BA6B77363C2B` |
| openspec/changes/deliver-codex-session-loop/specs/permission-approvals/spec.md | `E61FB389DFBAD686F883FEA77E2D679AFEEADBE540D029E91F6632FACCFC7601` |
| openspec/changes/deliver-codex-session-loop/specs/session-execution/spec.md | `61067A30D797F0AD3789226CAA3B1491299AA4F45400F9230221298823ED00A6` |
| openspec/changes/deliver-codex-session-loop/specs/structured-history/spec.md | `A90A8CDBCC285EC8CDAB2AEFB1D7AF589DFAEF8CA5009354FBCDB1AF43D99FBB` |
| openspec/changes/deliver-codex-session-loop/specs/workspace-registration/spec.md | `4CF4E9E297EFEB4A5C56FC89B706B3FD46FD866849708FE032EFBCD78C1E9DE9` |

## 当前测试流程修订与计数（2026-10-29）

本节为主执行者的文档调整与数量记录，不是独立检视结论。按能力地图的待实现文档索引顺序，前两项已调整，本轮依次调整03–10的tests与plan，共118例；相关tasks同步明确安全用例由reviewer负责，实施任务数量和勾选状态不变。用例正文精简为级别、步骤和预期，共用环境及需求关联集中说明；8项原LX独立安全检视改为L5，真实依赖与安全断言保留。缺工具/环境/权限/入口记受阻或未执行，不转交用户重复手测；已有同版本、同范围、符合环境要求的有效证据可复用。

| Change | 当前用例数 | 其中LX |
| --- | --- | --- |
| establish-node-access | 33 | 1 |
| deliver-codex-session-loop | 42 | 0 |
| deliver-history-navigation | 25 | 0 |
| integrate-zcode-harness | 14 | 0 |
| integrate-deepseek-harness | 15 | 0 |
| deliver-agent-session-tree | 15 | 0 |
| deliver-session-capacity | 18 | 0 |
| deliver-native-session-import | 10 | 0 |
| deliver-session-data-lifecycle | 11 | 0 |
| deliver-backup-recovery | 10 | 0 |

当前合计 193 条（L1–L6 共 192 条，LX 共 1 条）。首项已由34例调整为33例；原NA-034与新NA-033的对应关系以该change/tests.md为准，不据编号迁移历史结果。上文152例及其后各轮计数、SHA-256与通过结论仅属于各自历史快照，不改写、不签本轮新文本通过。当前数量的维护入口为[能力地图索引](REBUILD_CAPABILITY_MAP.md#待实现文档索引)。

主执行者实际依序运行8项 openspec.cmd validate <change> --strict，均通过；另执行只读辅助自检：118个用例均含三个必填项、无重复编号，67组Requirement/Scenario引用均存在且所属关系匹配，无未引用规格场景，95项任务与声明WP关联完整。辅助匹配不证明语义或功能覆盖。git diff --check退出0，另核对本轮未跟踪文档的行尾空白与末尾换行；未执行本轮产品验收，也未取得修订后的独立tester/reviewer结论。各plan继续保留not-run并标记新文本待复核；缺失的新用例结果占位已补齐，未填入通过状态。
