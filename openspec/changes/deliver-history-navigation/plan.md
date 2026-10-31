> Q-001（2026-10-31）当前修订：本change复用通用闭环当前八份specs。缺历史重放不再整体禁用；输入受理授权、共享队列、无期限/不跨重启正文、附加审计及正文/输出容量取舍以通用规格为准。下方与旧RP/R2冲突的规划或用例预期不再有效，原ID/标题和证据保留历史归属；本次关联修订须独立tester/reviewer重新检查，不表示已实现或已验收。

## ARCH-001 当前修订范围（2026-10-30）

本轮按用户授权补齐缓存外分页的可靠续读和独立重新定位，复用 structured-history 的内容安全边界；当前依据为本 change proposal/specs/design 及总体设计。共享文档由主执行者串行修订，本轮不改代码、独立 tests/review 或任务勾选，旧报告仍只属原快照，实现继续暂停。

恢复实施前由独立 tester 重基准场景：不变1000轮历史、20轮缓存、默认10轮且无跨加载稳定 ID 或原生分页时能定位访问全部保留轮次；来源删改、定位失败与迟到响应不得伪装连续续读；正文、元数据及在途响应有界，不能用全历史索引兜底。随后由独立 reviewer 检视，并更新详细合同、工作包和固定版本真实验证安排。本轮未取得新版独立结论，规格场景不算正式用例或产品已通过。

---

## RP-001 独立文档结论（2026-10-30）

独立 reviewer `/root/spec_reviewer` 对用户已确认的重放准入及兼容修订作限定文档复核：通过，本轮无新增阻断；本 change 增补用例为 HN-030–034。详见[本轮 review](review.md)顶部。本结论只覆盖本轮规则与用例定义，不代表产品、安全实测、全面架构或执行工作包通过。全部产品用例 not-run；旧合同、旧用例及工作包全面重基准和其他架构整改尚未完成，实现继续暂停。以下方案、规格、用例和计划的固定快照是本报告追加前值，不能冒充报告追加后的文件摘要。

---

## RP-001 独立文档结论（2026-10-30）

独立 reviewer `/root/spec_reviewer` 对用户已确认的重放准入及兼容修订作限定文档复核：通过，本轮无新增阻断；本 change 增补用例为 HN-030–034。详见[本轮 review](review.md)顶部。本结论只覆盖本轮规则与用例定义，不代表产品、安全实测、全面架构或执行工作包通过。全部产品用例 not-run；旧合同、旧用例及工作包全面重基准和其他架构整改尚未完成，实现继续暂停。以下方案、规格、用例和计划的固定快照是本报告追加前值，不能冒充报告追加后的文件摘要。

---

## RP-001 重放兼容当前范围（2026-10-30）

本轮独立 tester 新增定义 HN-030–034；当前只检查文档，不执行产品用例。恢复实施及固定集成摘要后，由独立 tester 在 Windows 专用原生根、节点及 Web 环境执行相应合同/故障和真实闭环，逐 ID 证据归 testing/runs/deliver-history-navigation/；真实 Harness、通用重放支持及故障入口缺失记受阻或未执行，不能以替身通过冒充实际能力。执行任务及完整旧用例仍须先按当前规格重基准。

Q-001当前引用：无重放也可按实际能力接入，历史与其他能力分别判断；当前行为统一遵循通用 harness-management、structured-history 和 acp-platform-constraints。下方原版本方案及独立报告仅作历史追踪，与新规格冲突的预期不再有效。

本轮只修订文档；独立 tester 增补本轮用例，独立 reviewer 复核本次范围，产品用例仍未执行。详细合同、全套旧用例与工作包全面重基准及其他架构底座整改未完成，不恢复实现、不修改 checkbox，不删除原生数据或改写旧证据。平台身份、审计及离线平台备份不因 Harness 未准入而失效。

---

## 本轮文档修订独立复核（2026-10-30）

本 change 的 SR-003/SR-004 文档修订和新增用例 HN-026–029 经独立 tester `/root/spec_tester` 编写、独立 reviewer `/root/spec_reviewer` 限定复核通过。对应的受审查快照、范围和问题关闭记录见 [本轮 review](review.md) 顶部。50 文件报告追加前聚合 SHA-256 为 `7e77ec113fd48b6796139cfb4465d2949dc3a788c7ee033e23ddc84136d77d2d`；追加本记录不改动规格、设计或用例。

本次只完成文档检查：全量 OpenSpec strict 11/11 与文档 `git diff --check` 通过；R-SR-DOC-001 和 R-SR-TEST-001 经独立复核关闭。新增产品用例全部未执行；旧合同、旧用例及执行工作包仍待全面重基准，不恢复实现、不勾产品任务、不把下方旧阶段记录当新版通过。下方 Project Verify 已登记本轮用例及未来执行安排，须在恢复实施前落实到重基准后的工作包。

---

> 五项审查修订补充（2026-10-30）：跨入口原生身份、非 Run 操作审批、节点共享连接视图、能力准入分层及按能力独立交付已写入当前 specs/design。下方旧用例与工作包尚未覆盖这些新场景和依赖调整，保持原 ID 供独立重基准，不据此开工或声明覆盖。

> 2026-10-30 规格重基准门禁：实现暂停。会话及历史以 Harness 为权威，见本 change 当前 specs 与 design。下方为原方案的plan.md内容及证据，凡依赖平台聊天持久化、入库版本、强刷、导入批次、平台删除或历史备份的部分均已失效；不可据旧任务勾选、旧检查或旧审查恢复实现。待独立 tester 修订用例、详细合同与工作包重排、reviewer 重新检视后再决定恢复。历史证据保留原范围，不改写旧结论。

## Execution Model

本轮文档修订范围为 SR-003/SR-004 的分页及实际加载引用。独立 tester 增补 tests.md 本轮用例，独立 reviewer 核对父子分别判断能力及主树归属；分页端口与有界轮次条件由本 change 提供，关系和父步骤展示由 agent-session-tree 提供。以下旧平台历史索引工作包、接口及依赖仍待全面重基准，不构成当前实现指令或产品通过依据。

状态：文档规划，未实现；依赖 `deliver-codex-session-loop` 的真实交付及本change独立方案/用例检视后才可开工。上游尚不存在可运行产品，不能以规划文件存在作为满足依赖。开工先读取其最终合同、真实检查入口与证据重基准，变化影响当前spec/design/tests时先协调修订。

主执行统筹；coder/tester/reviewer按openspec/role分工。tester/reviewer不继承coder对话。基线是proposal、specs、design、tests以及上游固定版本的完整文件摘要，未提交修改不以HEAD代替。保留用户改动；交付diff/可读报告，不自动提交发布。

## 本轮父步骤展开修订

WP2提供父轮次分页及共享预算的子展开块查询/授权合同；WP3提供展开代际、基线与缓存衔接及折叠资源释放；WP4提供原位展开/折叠与错误展示，不实现独立子页，展开区域支持前端传参正序分页。WP5新增HN-014–025，WP6重新检视当前文档，旧review仅对应旧快照。该阶段可靠子关联可以明确标注的fixture验证组件，真实Harness子身份及完整展开闭环由后续deliver-agent-session-tree交付，不让本change反向依赖它。通用读取和关系模块写范围不重叠；后续若调整共享网关/组件须串行集成并重跑HN与AT回归。

## 最新参数及展示修订

WP2提供前端pageSize默认10的通用逻辑分页、父DESC/子ASC、游标范围和页内块冻结校验；WP3保留内部提交事实但不向正常页面输出保存状态；WP4实现父子独立分页进度及异常才显示错误，正常保存/重连无徽标。WP5新增HN-020–025，WP6检视当前文本；当前产品用例未执行，旧固定10/子不分页/保存标识结论不覆盖本次修订。

## 测试流程修订

按当前config与模板整理为25条用例（HN-001–HN-025），保留全部编号、标题及行为断言；需求关联、执行环境与清理按组说明。HN-013 从LX改为L5，由独立reviewer使用工具检视，不新增人工门禁；工具/环境/权限或入口缺失记受阻/未执行，有效同版本同范围证据可复用。tests及当前执行安排须重新独立检视，旧review与历史结果不签本轮文本通过。

依据本轮用例复核，补实HN-003参数/顺序/版本预期与HN-004交接屏障、真实查询量、分块/队列预算及重同步断言；用例仍25条，参数与边界沿用design，未新增产品行为或常量。本次修订待独立复核，不沿用旧结论，产品未执行。

## Parallel Assessment

WP1先核对上游与检查环境；其他包共享会话历史和查询合同，有写范围交叉，按Merge Order串行集成。已确定的Web接口可让WP4在独立工作区并行准备，但必须等WP3稳定后合入，合入后重跑受影响测试。WP5与WP6对同一固定集成版本可并行独立工作，不能边改实现边签验收。Rust构建使用各自CARGO_TARGET_DIR。

## Work Packages

| ID | Goal / Scenarios | Dependencies | Agent | Write Scope | Deliverable | Verify |
| --- | --- | --- | --- | --- | --- | --- |
| WP1 | 固定上游轮次/条目/版本合同和查询测试入口 | 上游真实交付及方案检视 | coder | 本change基线记录、README.md、app/tests/history/ | 字段重基准与测量入口 | HN-001–004；证明上游有真实持久记录 |
| WP2 | 前端传参默认10轮的父倒序/子正序键集分页、大小预算与大条目展开 | WP1 | coder | app/src/history/、app/src/queries/、app/src/storage/、app/tests/history/ | 索引/查询实现与开发测试、合同文档 | HN-003–007、HN-014–025；查询量可观察，不先全量再隐藏 |
| WP3 | 原子订阅交接与旧位置更新 | WP2 | coder | app/src/history/、app/src/gateway/、app/tests/history/ | 基线/缓存覆盖/流代际及重同步实现 | HN-004、HN-008–011、HN-016–019真实竞态与崩溃窗口 |
| WP4 | Web分页、展开与版本合并 | WP2、WP3 | coder | clients/web/、README.md | 按需加载与重连界面、前端测试和操作文档 | HN-005–012、HN-014–025；父页前端传参默认10轮、子步骤原位展开且不重排 |
| WP5 | 独立验收及回归同步 | WP4 | tester | app/tests/acceptance/history/、clients/web/tests/acceptance/history/、testing/runs/deliver-history-navigation/、testing/suites/、本change/tests.md | 逐例报告及长期回归 | HN-001–025真实存储和Windows Web；HN-013由WP6独立reviewer执行 |
| WP6 | 独立方案/用例与交付检视 | 方案无产品依赖，交付WP5 | reviewer | 本change/review.md、plan.md结果节 | 逐场景检视与问题闭环 | 同版本无阻断，不以截图代替读取量/竞态证据 |

写范围均为仓库相对路径，表中“本change”指openspec/changes/deliver-history-navigation/。实际目录由上游建立，若上游更名须在开工时更新映射并独立复核，不静默拓宽范围。

## Merge Order

| Order | Work Package | Reason | Re-verify After Merge |
| --- | --- | --- | --- |
| 1 | WP1 | 上游门禁和检查入口先收敛 | HN-001/002以及相关能力探测/测量 |
| 2 | WP2 | 首次领域与持久化改动 | WP2所列开发与组件用例 |
| 3 | WP3 | 与WP1/2目录嵌套或共享，串行集成 | WP2/3全部受影响用例 |
| 4 | WP4 | Web可独立准备，依赖协议稳定后合入 | Rust/Web真实检查及HN-012完整链路 |
| 5 | WP5、WP6 | tester验收目录与开发测试目录嵌套，必须停止开发写入后验收 | 固定集成版本全部适用检查；修复后独立重测复查 |

## Project Verify

本轮新增验收定义为 HN-026–029，覆盖 SR-003/SR-004；当前只核对定义和来源。恢复实现并固定集成摘要后，由独立 tester 在 Windows 专用原生根及节点/Web 环境验证父子分别判断能力、禁止无界全文读取及加载归属，逐 ID 证据归 testing/runs/deliver-history-navigation/；执行安排须纳入重基准工作包，真实分页与有界资源需实际证据，不把文档检查或数组截页当通过。

| Check ID | Command / Working Directory | Scope / Pass Criteria |
| --- | --- | --- |
| PV1 | 根目录：openspec.cmd validate deliver-history-navigation --strict；git diff --check；tester核对HN-001/002 | 当前真实可执行，仅结构/差异检查 |
| PV2 | WP1从上游README和工程配置核对实际Rust/Web编译、格式、类型和测试入口，根目录及实际工程目录；HN-001–004 | 现有 npm run check、npm run build:rust 等入口按 README 执行；核对是否覆盖本 change，缺专用目标/合同入口记受阻/未执行，不虚构通过 |
| PV3 | 独立tester按tests.md共用执行安排及各例操作步骤执行HN-005–025；独立Windows与浏览器profile | 引用精确需求和断言，正常链路真实；故障fixture不能代替真实Harness验收 |

固定同一提交或完整文件SHA-256清单，报告存testing/runs/deliver-history-navigation/。记录OS、工具、协议/适配器版本、逐例结果与清理；最终检查覆盖实际集成版本。不得动用户真实数据库、密钥或原生会话；专用测试资源本包拥有并在失败/取消时清理。

## Code Review

| Work Package | Reviewer Agent | Review Focus | Pass Condition |
| --- | --- | --- | --- |
| 全部编码前 | 独立reviewer | proposal/spec/design/tests及当前计划与上游依赖 | 每个引用场景存在且覆盖，无未闭环设计阻断 |
| 全部交付时 | 独立reviewer | 固定集成diff与实际安全/平台/验收证据 | 必需检查实际通过，reviewer问题由其独立复查关闭 |

## Main E2E

```yaml
mode: required
```

对应tests.md的HN-012真实Windows/数据库/查询/Web链路；子关系可使用明确标注的可靠关联fixture，仅证明通用展开组件，不证明真实Harness关系能力。后者由后续AT-009完整验收，不能以本例代签。按角色使用真实浏览器/桌面工具；HN-013 由独立reviewer通过工具完成L5检视，当前无LX用例。缺工具、环境、权限或入口记受阻/not-run，不自动转为人工验收；同版本、同范围且环境符合要求的有效证据可复用。环境不可用记not-run，不以模拟替代真实数据库、查询或Web。

## Risks

上游合同尚未实施，当前完成的是待实现文档。真实依赖/版本探测、平台证据、安全与供应链在实施时逐项交付。现有上游编译入口须核对覆盖范围，不将上游构建或用例定义记成本change通过。设计发现违反已确认产品约束时先修文档，不能以门禁失败为由降级承诺。

## Handoff

只交接用户确认（Windows+Web、Codex/ZCode/DeepSeek、分页10轮、保存500ms/64KiB）、AGENTS/config/角色、当前proposal/spec/design/tests、上游固定版本及写范围。交接测试数据/端口/目录所有权，不继承开发对话或代签结论。

## Findings

| ID | Source | Severity | Target Version | Finding | Fix Version | Status | Retest / Re-review |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 文档方案检视 | 独立reviewer | 文档结论 | 当前review快照 | 旧快照方案/用例/计划/任务检视通过；本轮修订另行复核，问题及关闭依据见review.md | 旧快照 | 本轮待复核 | 独立复查；产品未执行 |

## Verification Results

- 受测版本：当前只有规划文本，正式检视固定各文件摘要；上游未交付，产品版本不存在。
- 摘要排除范围：plan结果、tasks状态和review报告自身；spec/design/tests及实现/配置变化不排除。
- 执行者与环境：主执行结构自检、独立tester用例规划、旧快照独立reviewer方案检视通过（见review.md，不覆盖本轮修订，非产品验收）。
- 验证命令及目录：当前仅根目录OpenSpec严格校验和Git差异；实际退出码在整体文档检查记录，非产品结果。

| Case ID | Executor | Target Version | Status | Evidence / Limitations |
| --- | --- | --- | --- | --- |
| HN-001–025产品部分 | 独立tester | 产品尚不存在 | not-run | 用例定义不是运行证据，未运行正常/异常/平台验收 |

- 未执行项与已知限制：本change所有产品、安全、平台及真实Harness验收；结构检查不替代它们。
- 问题关闭依据：待reviewer首次独立检视及修复复查，不代关。
- 新版本证据判断：受影响spec/design/tests或上游/实现改变后，tester/reviewer独立确认重验范围。
- 归档后复核：未归档；后续核对spec/长期用例同步、移动映射及摘要一致性。
