> Q-001（2026-10-31）当前修订：本change复用通用闭环当前八份specs。缺历史重放不再整体禁用；输入受理授权、共享队列、无期限/不跨重启正文、附加审计及正文/输出容量取舍以通用规格为准。下方与旧RP/R2冲突的规划或用例预期不再有效，原ID/标题和证据保留历史归属；本次关联修订须独立tester/reviewer重新检查，不表示已实现或已验收。

## R2-002/003 当前修订范围（2026-10-31）

用户同意修复管理存储故障及请求过期去重边界，本 change 定义安全管理元数据清理并复用通用期限、审计保留和容量政策。主执行者串行修订共享文档；本轮不执行清理、代码或迁移，不改独立 tests/review、历史快照或任务勾选，实现继续暂停。

恢复实施前由独立 tester 覆盖：删除派发前后写入故障、过期删除结果清理后旧请求重放、清理与重试并发、未知核验和资源责任保护；独立 reviewer 检视后固定清理事务、过期拒绝水位和工作包。原 tests 的旧 ID 保留待映射，新增规格 Scenario 不等于正式独立用例或已验证产品能力，旧报告不覆盖本轮。

---

## RP-001 独立文档结论（2026-10-30）

独立 reviewer `/root/spec_reviewer` 对用户已确认的重放准入及兼容修订作限定文档复核：通过，本轮无新增阻断；本 change 仅补充影响范围，不机械新增重复用例。详见[本轮 review](review.md)顶部。本结论只覆盖本轮规则与用例定义，不代表产品、安全实测、全面架构或执行工作包通过。全部产品用例 not-run；旧合同、旧用例及工作包全面重基准和其他架构整改尚未完成，实现继续暂停。以下方案、规格、用例和计划的固定快照是本报告追加前值，不能冒充报告追加后的文件摘要。

---

## RP-001 重放兼容当前范围（2026-10-30）

本轮未机械新增本 change 的产品用例，仅标明重放政策影响范围。主接入门禁由通用闭环验证；资源、删除或备份本身仍按对应规格独立检查，已有相关用例只有确认未受影响后才能复用。尤其平台-only离线恢复不因原生重放不可用而失败。旧计划及产品结果仍须全面重基准，不在本轮执行。

Q-001当前引用：无重放也可按实际能力接入，历史与其他能力分别判断；当前行为统一遵循通用 harness-management、structured-history 和 acp-platform-constraints。下方原版本方案及独立报告仅作历史追踪，与新规格冲突的预期不再有效。

本轮只修订文档；独立 tester 增补本轮用例，独立 reviewer 复核本次范围，产品用例仍未执行。详细合同、全套旧用例与工作包全面重基准及其他架构底座整改未完成，不恢复实现、不修改 checkbox，不删除原生数据或改写旧证据。平台身份、审计及离线平台备份不因 Harness 未准入而失效。

---

## 本轮文档修订独立复核（2026-10-30）

本 change 的 SR-002 文档修订和新增用例 DL-012–016 经独立 tester `/root/spec_tester` 编写、独立 reviewer `/root/spec_reviewer` 限定复核通过。对应的受审查快照、范围和问题关闭记录见 [本轮 review](review.md) 顶部。50 文件报告追加前聚合 SHA-256 为 `7e77ec113fd48b6796139cfb4465d2949dc3a788c7ee033e23ddc84136d77d2d`；追加本记录不改动规格、设计或用例。

本次只完成文档检查：全量 OpenSpec strict 11/11 与文档 `git diff --check` 通过；R-SR-DOC-001 和 R-SR-TEST-001 经独立复核关闭。新增产品用例全部未执行；旧合同、旧用例及执行工作包仍待全面重基准，不恢复实现、不勾产品任务、不把下方旧阶段记录当新版通过。下方 Project Verify 已登记本轮用例及未来执行安排，须在恢复实施前落实到重基准后的工作包。

---

> 2026-10-30 规格重基准门禁：实现暂停。会话及历史以 Harness 为权威，见本 change 当前 specs 与 design。下方为原方案的plan.md内容及证据，凡依赖平台聊天持久化、入库版本、强刷、导入批次、平台删除或历史备份的部分均已失效；不可据旧任务勾选、旧检查或旧审查恢复实现。待独立 tester 修订用例、详细合同与工作包重排、reviewer 重新检视后再决定恢复。历史证据保留原范围，不改写旧结论。

## Execution Model

本轮文档修订范围为 SR-002（固定删除授权范围及并发变化），以当前规格为准。独立 tester 在 tests.md 增补本轮用例，独立 reviewer 核对范围约束及用例；以下旧平台删除和附件回收工作包仍待全面重基准，不因本轮检查通过而开工。真实原生范围保证及实际删除验收尚未执行。

本轮仅补齐文档到待实现状态，不执行以下产品任务。依赖 deliver-native-session-import、deliver-session-capacity 的真实交付；先读取其最终规格、实现与验收证据重基准，不能因上游文档齐备而视为已可运行。角色按 openspec/role/coder.md、tester.md、reviewer.md：coder实现，独立tester验收，另一独立reviewer检视，不继承开发对话。保护当前Git未提交改动，不自动提交/发布。

开始实现先记录基线、未提交文件及删除项SHA-256、真实工具版本和隔离数据根。目录沿上游单Rust应用app/与TypeScript clients/web/；以下为未来具体写范围，WP1须在真实上游树核对路径，不存在的检查入口先建立再执行。DL-DES-01的无正文participant墓碑和原parent/root链不能被Session删除清空；默认不级联属于设计细化，不称用户逐项确认。

## 测试流程修订

按当前config与模板整理为11条用例（DL-001–DL-011），保留全部编号、标题及行为断言；需求关联、执行环境与清理按组说明。DL-011 从LX改为L5，由独立reviewer使用工具检视，不新增人工门禁；工具/环境/权限或入口缺失记受阻/未执行，有效同版本同范围证据可复用。tests及当前执行安排须重新独立检视，旧review与历史结果不签本轮文本通过。

## Parallel Assessment

WP1串行：上游未交付及合同未固定前不能开始产品实现。WP2节点与WP3 Web在WP1冻结接口后可在独立工作区并行，禁止同工作区并行写入，各自独立端口、测试数据和CARGO_TARGET_DIR。WP4在两者合入后独立编写验收程序并测试；正式同版本tester验收与reviewer只读检视可并行，但最终结论须等全部必需用例结果。多个change涉及平台注册、存储模式、网关与Web注册文件时按能力地图依赖串行集成；无依赖的ZCode/DeepSeek适配器可隔离并行，harness/mod.rs和registry.ts统一按ZCode后DeepSeek顺序合入并重跑双方合同及上游回归。

## Work Packages

| ID | Goal / Scenarios | Dependencies | Agent | Write Scope | Deliverable | Verify |
| --- | --- | --- | --- | --- | --- | --- |
| WP1 | 上游重基准、版本/能力及真实检查入口门禁 | deliver-native-session-import、deliver-session-capacity真实交付 | 主执行者与独立reviewer | README.md检查入口段；本change design.md及proposal.md受影响依据；实际新增依赖对应锁文件与CI检查配置（仅确有需要时） | 上游版本与合同映射、入口命令清单、依赖审查；阻断未知未解不进入实现 | DL-001、DL-002；独立方案/用例检视通过 |
| WP2 | 显式删除范围、执行协调、参与者墓碑、共享附件回收与原生删除独立结果的节点实现及开发单元/合同测试 | WP1 | coder | app/src/policy/session_deletion/；app/src/storage/session_deletion/；app/src/storage/migrations/session_deletion/；app/src/platform/attachment_gc/；app/src/gateway/session_deletion/；app/tests/session_deletion_contract.rs；docs/implementation/session-lifecycle.md | 节点diff、对应单元测试与运行限制文档，不改tester预期 | DL-003、DL-004及DL-005、DL-006、DL-007、DL-008、DL-009的节点断言 |
| WP3 | Web可观察入口、限制及错误反馈 | WP1冻结接口 | coder | clients/web/src/features/session-deletion/；clients/web/src/features/agent-tree/unavailable-link.ts；clients/web/tests/session-lifecycle/；docs/implementation/session-lifecycle-web.md | Web diff、交互测试及使用说明 | DL-005、DL-006、DL-007、DL-008、DL-009；构建/类型/静态检查由WP1核定真实入口 |
| WP4 | 独立验收程序、故障注入与真实闭环 | WP2、WP3合入 | 独立tester | app/tests/acceptance/session-lifecycle/；clients/web/tests/acceptance/session-lifecycle/；testing/runs/deliver-session-data-lifecycle/tester/；本change tests.md（仅tester有依据修订） | 真实验收程序与逐ID结果、清理/限制证据，不能以mock代真实Harness | DL-001、DL-002、DL-003、DL-004、DL-005、DL-006、DL-007、DL-008、DL-009、DL-010 |
| WP5 | 最终集成与独立交付检视 | WP4负责的必需用例通过（不含WP5负责的DL-011） | 独立reviewer与主执行者 | testing/runs/deliver-session-data-lifecycle/reviewer/；本change plan.md Findings/Verification Results | 同版本独立检视、问题修复重测复查及结果摘要 | DL-011；全量必需用例通过且无未闭环阻断 |

## Merge Order

| Order | Work Package | Reason | Re-verify After Merge |
| --- | --- | --- | --- |
| 1 | WP1 | 上游合同/真实入口/独立方案门禁先确定 | DL-001、DL-002，核对差异不覆盖用户改动 |
| 2 | WP2 | 节点合同供集成调用；与其他change共享注册按依赖串行 | DL-003、DL-004及上游执行/历史/认证回归 |
| 3 | WP3 | 在WP2最终接口上集成；如接口变化必须回改本包 | DL-005、DL-006、DL-007、DL-008、DL-009，节点合同及真实Web构建/类型检查 |
| 4 | WP4 | 独立验收只针对完整集成版本 | DL-001、DL-002、DL-003、DL-004、DL-005、DL-006、DL-007、DL-008、DL-009、DL-010，记录真实/模拟边界与逐例断言 |
| 5 | WP5 | 结果摘要仅本包写，避免与tester报告冲突 | 重核同一受测文件清单、全部检查及问题关闭证据 |

共享README、锁文件、CI由WP1集中写；若WP2/WP3发现需改变，先串行回到WP1修订并重新检视及重跑DL-001、DL-002，不可静默扩范围。其他模块注册/迁移若上游路径调整，由WP1列精确映射及合入重验，不在本轮改动产品树。

## Project Verify

本轮新增验收定义为 DL-012–016，覆盖 SR-002；当前只核对定义和来源，不执行原生删除。恢复实现并固定集成摘要后，由独立 tester 在 Windows 专用原生根及节点/Web 环境执行并发范围变化、原生约束和未知结果断言，逐 ID 证据归 testing/runs/deliver-session-data-lifecycle/；真实机制无法保证固定集合时记录不支持或受阻，不用模拟报告冒充原生保证，执行安排须纳入重基准工作包。

工作目录统一D:/Project/acp-remote，实际构建/测试子目录与完整命令由WP1从README和工程配置确认并写入报告；现有上游工程命令按README核对，本change专用入口缺失记not-run，不以其他目标成功替代。环境为Windows、可信HTTPS、Chromium、隔离数据库/附件/原生根及所属进程；Rust工具链只读实际配置，Harness/适配器/协议版本及二进制摘要固定。证据写testing/runs/deliver-session-data-lifecycle/，报告需含断言数量与具体结果，不能只有截图。

| Check ID | Command / Working Directory | Scope / Pass Criteria |
| --- | --- | --- |
| PV1 | 独立tester：DL-001、DL-002；根目录 npx.cmd openspec validate deliver-session-data-lifecycle --strict 与 git diff --check，按WP1核定工程真实入口执行L1/L2 | 规划校验只证明文档；真实Rust/Web构建、合同、格式/静态与供应链检查实际执行，零目标/缺入口不通过 |
| PV2 | 独立tester：DL-003、DL-004；app/与clients/web/内WP1确定的领域/合同/故障测试入口 | 验证事务、并发、取消、失败与资源所有权；fixture不冒充真实兼容性 |
| PV3 | 独立tester：DL-005、DL-006、DL-007、DL-008、DL-009；实际Web与节点管理入口按tests.md逐项操作 | 对应规格场景真实可观察结果；不复述用例，不因能力缺失改预期 |
| PV4 | 独立tester：DL-010；Windows+Web Main E2E实际产品入口 | 真实父子与共享附件，默认仅删父后子历史及parent/root仍可追溯，运行中释放失败不隐藏执行，幂等结果和附件引用竞态，原生删除逐款真实支持才执行；真实版本结果与限制可读，必需失败/未执行阻断 |
| PV5 | 独立reviewer（工具验证L5）：DL-011；同版本源码/配置/证据 | 安全、交互、协议语义、依赖与敏感数据独立结论，不以主执行自查代签独立结论 |
| PV6 | 主执行者核对完整待提交集成快照，独立reviewer复核 | 新修改触发影响分析与相关重验；不能以旧HEAD证据覆盖未提交实现变化，归档路径复核另行执行 |

## Code Review

编码前reviewer独立读取用户原话、AGENTS、config、proposal/specs/design/tests/plan/tasks，人工逐条核对真实Requirement/Scenario、WP关联、Main E2E及可执行入口；阻断未知和独立问题未关闭不可开工。文档齐备、方案检视通过、产品验收通过是三种不同结果。

| Work Package | Reviewer Agent | Review Focus | Pass Condition |
| --- | --- | --- | --- |
| WP1 | 独立reviewer（不同于coder/tester） | 上游证据、能力门禁、方案和用例、写范围 | 必需决策已解决且问题ID有独立关闭依据 |
| WP2、WP3 | 独立reviewer | 真实diff、协议/事务/资源、安全和页面反馈 | 对同一集成版本完成检视，相关测试实际通过 |
| WP4、WP5 | 独立reviewer（含L5安全及交互检视） | 全用例结果、证据范围及复用、问题重测复查 | 全部必需测试通过，无未验证项被写成通过；未满足保持未完成 |

## Main E2E

```yaml
mode: required
```

由独立tester按DL-010在真实Windows节点与Web执行，真实Harness和存储参与；真实父子与共享附件，默认仅删父后子历史及parent/root仍可追溯，运行中释放失败不隐藏执行，幂等结果和附件引用竞态，原生删除逐款真实支持才执行。允许fixture的范围仅限tests的局部边界。必须记录退出/取消后的完整进程树清理及隔离数据保护，L6不能被启动成功或截图代替。

## Risks

上游尚未实现，未来版本、命令和接口可能变化；WP1必须重基准。真实Harness缺能力则记录阻断，不虚报支持。当前所有产品检查均未执行；不得从OpenSpec validate通过推导编译、功能或安全验收通过。新增依赖逐项审查并纳入锁文件，不删除用户实际数据/密钥/原生会话。无授权不提交、推送或发布。

## Handoff

交接只传用户确认要求（待实现文档、Windows+Web、10轮、三款首批及适用确认参数）、规则与文件、固定快照、精确读写范围、真实工具和完成判据；不传开发自评或coder对话。tester使用独立数据根、端口与进程；reviewer只读固定版本或隔离执行。修复保留稳定问题ID和原独立上下文，提出者重测/复查关闭。

## Findings

| ID | Source | Severity | Target Version | Finding | Fix Version | Status | Retest / Re-review |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 方案/用例/计划/任务检视 | 独立reviewer | 文档结论 | 当前review快照 | 全量文档已实际检视，无开放设计阻断；不代表产品通过 | 当前规划修订 | 设计检视通过 | 逐文件摘要及独立依据见review.md |

独立检视已完成，实际问题ID与关闭依据保存在review.md；后续实现或行为文件变化须按影响重新检视，不能沿用旧结论。

## Verification Results

- 受测版本：尚无产品实现；后续必须记录提交或完整文件/删除项SHA-256快照，不能仅用HEAD代替未提交集合。
- 摘要排除范围：只允许plan结果段、tasks勾选和报告自身；实现、配置、测试及用例变化不得排除。
- 执行者与环境：产品tester/reviewer、真实版本与工具待执行时记录；文档编写不是执行者验收。
- 验证命令及目录：目前仅可运行规划/差异检查，逐项结果以主执行者统一检查为准；产品完整命令由WP1核对，尚未执行。

| Case ID | Executor | Target Version | Status | Evidence / Limitations |
| --- | --- | --- | --- | --- |
| DL-001 | 独立tester待安排 | 产品快照未建立 | not-run | 当前无产品入口；定义不等于执行证据 |
| DL-002 | 独立tester待安排 | 产品快照未建立 | not-run | 当前无产品入口；定义不等于执行证据 |
| DL-003 | 独立tester待安排 | 产品快照未建立 | not-run | 当前无产品入口；定义不等于执行证据 |
| DL-004 | 独立tester待安排 | 产品快照未建立 | not-run | 当前无产品入口；定义不等于执行证据 |
| DL-005 | 独立tester待安排 | 产品快照未建立 | not-run | 当前无产品入口；定义不等于执行证据 |
| DL-006 | 独立tester待安排 | 产品快照未建立 | not-run | 当前无产品入口；定义不等于执行证据 |
| DL-007 | 独立tester待安排 | 产品快照未建立 | not-run | 当前无产品入口；定义不等于执行证据 |
| DL-008 | 独立tester待安排 | 产品快照未建立 | not-run | 当前无产品入口；定义不等于执行证据 |
| DL-009 | 独立tester待安排 | 产品快照未建立 | not-run | 当前无产品入口；定义不等于执行证据 |
| DL-010 | 独立tester待安排 | 产品快照未建立 | not-run | 当前无产品入口；定义不等于执行证据 |
| DL-011 | 独立reviewer待安排 | 产品快照未建立 | not-run | 当前无产品入口；定义不等于执行证据 |

- 未执行项与已知限制：全部产品构建、合同、真实Harness、Main E2E及独立交付检视未执行；规划校验结果另记。
- 问题关闭依据：待独立提出者提供固定版本复验，不由coder关闭。
- 新版本证据判断：后续改动逐项分析影响，由独立tester/reviewer确认重验或同版本证据复用。
- 归档后复核：尚未归档，未来检查移动映射、规范/长期suite及摘要保真；不能提前签通过。

## Workflow follow-up

产品实现、全部tracked任务证据与独立结论齐备后再同步长期规格和用例并归档，归档后复核真实路径映射及受影响检查。本轮不归档、不提交或发布。
