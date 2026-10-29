## Execution Model

状态：文档规划，未实现；依赖 `establish-node-access` 的真实交付及本change独立方案/用例检视后才可开工。上游已有节点与 Web 开发实现，但独立验收尚未完成，不能以开发实现或规划文件存在作为满足依赖。开工先读取其最终合同、真实检查入口与证据重基准，变化影响当前spec/design/tests时先协调修订。

主执行统筹；coder/tester/reviewer按openspec/role分工。tester/reviewer不继承coder对话。基线是proposal、specs、design、tests以及上游固定版本的完整文件摘要，未提交修改不以HEAD代替。保留用户改动；交付diff/可读报告，不自动提交发布。

## 本轮 ACP 规划修订

通用协议与版本策略写范围归 WP1/WP3，标准能力禁止按厂商分支；WP1核实 v1/v2 官方规范、实际版本标识及发布状态，WP3实现公共客户端和各版本策略，WP4使用通用能力/错误视图。WP5补充 CL-032–041，WP6重新检视本轮规划；全部新用例及真实多版本验收待执行，不沿用旧review对新文本签通过。升级复验和不兼容用户决策遵循 design 的通用 ACP 边界。无可核验v2规范或真实对端时记录阻断和影响并交用户决定，不静默只交v1。

## 最新保存展示修订

WP2持久化及提交事实内部保持，WP3继续正确同步版本，WP4正常页面不显示保存进度/状态/版本，仅异常显示错误。WP5新增CL-042并更新保存失败预期，WP6重新检视当前展示合同；不以旧review覆盖此次规格和用例修订，不修改保存周期、缓存限额或故障停止规则。

## 测试流程修订

按当前config和测试模板精简CL-001–042，需求与共用信息集中说明，保留ID、标题和行为断言；CL-031由LX调整为独立reviewer可工具验证的L5，不新增人工门禁。此次修订的独立用例/方案检视待完成，旧review不覆盖新文本；既有执行结果不改写。

依据本轮用例复核，补实CL-003保存/容量边界、CL-004事务及派发计数、CL-011强制刷新失败操作与资源责任断言；区分L3/L4整组件替身和L5真实组件局部故障，L6必需依赖保持真实。CL-031归WP6执行，WP5及PV3仅负责tester用例；用例仍42条，产品未执行，本次修订待独立复核。

## Parallel Assessment

WP1先核对上游与检查环境；其他包共享会话历史和查询合同，有写范围交叉，按Merge Order串行集成。已确定的Web接口可让WP4在独立工作区并行准备，但必须等WP3稳定后合入，合入后重跑受影响测试。WP5与WP6对同一固定集成版本可并行独立工作，不能边改实现边签验收。Rust构建使用各自CARGO_TARGET_DIR。

## Work Packages

| ID | Goal / Scenarios | Dependencies | Agent | Write Scope | Deliverable | Verify |
| --- | --- | --- | --- | --- | --- | --- |
| WP1 | 核对上游身份/撤销合同，核实v1/v2规范、固定通用客户端与Codex版本及实际能力 | 上游真实交付及方案检视 | coder | 本change兼容证据、app/src/adapters/acp/、app/src/harnesses/codex/、README.md | 固定版本探测报告与入口登记 | CL-001、CL-002、CL-005；不能用调研替代真实探测 |
| WP2 | 会话、命令、审批、历史事务及故障收敛 | WP1 | coder | app/src/domain/、app/src/application/、app/src/storage/、app/tests/unit/ | 原子受理、源位置/版本、审批唯一裁决及开发测试 | CL-003、CL-004、CL-008–027；检查受理输入与Run同事务 |
| WP3 | ACP接入、进程所有权与原生恢复 | WP2 | coder | app/src/adapters/acp/、app/src/harnesses/codex/、app/src/platform/、app/tests/integration/ | 通用ACP版本策略、Codex首验接线、Windows进程树及恢复文档 | CL-005–016、CL-020–024、CL-028/029、CL-032–041 |
| WP4 | Web闭环与组合装配 | WP2、WP3 | coder | clients/web/、app/src/gateway/、app/src/main.rs、README.md、.github/workflows/ci.yml | 可操作会话/审批界面、集成构建与文档 | CL-001、CL-002、CL-022、CL-030 |
| WP5 | 独立测试资产、验收和长期回归 | WP4 | tester | app/tests/acceptance/、clients/web/tests/acceptance/、testing/runs/deliver-codex-session-loop/、testing/suites/、本change/tests.md | 逐例报告和稳定回归资产 | CL-001–030、CL-032–042由tester执行，CL-031由WP6 reviewer执行；记录真实Codex版本和清理 |
| WP6 | 独立方案及最终交付检视 | 方案阶段无产品依赖，交付阶段WP5 | reviewer | 本change/review.md、plan.md结果节 | 方案/用例检视及同版本交付问题闭环 | 执行CL-031并核对完整场景、真实进程树/审批/恢复证据；无阻断 |

写范围均为仓库相对路径，表中“本change”指openspec/changes/deliver-codex-session-loop/。实际目录由上游建立，若上游更名须在开工时更新映射并独立复核，不静默拓宽范围。

## Merge Order

| Order | Work Package | Reason | Re-verify After Merge |
| --- | --- | --- | --- |
| 1 | WP1 | 上游门禁和检查入口先收敛 | CL-001/002以及相关能力探测/测量 |
| 2 | WP2 | 首次领域与持久化改动 | WP2所列开发与组件用例 |
| 3 | WP3 | 与WP1/2目录嵌套或共享，串行集成 | WP2/3全部受影响用例 |
| 4 | WP4 | Web可独立准备，依赖协议稳定后合入 | Rust/Web真实检查及CL-030完整链路 |
| 5 | WP5、WP6 | tester验收目录与开发测试目录嵌套，必须停止开发写入后验收 | 固定集成版本全部适用检查；修复后独立重测复查 |

## Project Verify

| Check ID | Command / Working Directory | Scope / Pass Criteria |
| --- | --- | --- |
| PV1 | 根目录：openspec.cmd validate deliver-codex-session-loop --strict；git diff --check；tester核对CL-001/002 | 当前真实可执行，仅结构/差异检查 |
| PV2 | WP1从上游README和工程配置核对实际Rust/Web编译、格式、类型和测试入口，根目录及实际工程目录；CL-001–004 | 现有 npm run check、npm run build:rust 等入口按 README 执行；核对是否覆盖本 change，缺专用目标/合同入口记受阻/未执行，不虚构通过 |
| PV3 | 独立tester按tests.md共用执行安排及各例操作步骤执行CL-005–030、CL-032–042；独立Windows与浏览器profile | 引用精确需求和断言，正常链路真实；故障fixture不能代替真实Harness验收 |
| PV4 | 独立reviewer：按tests.md执行CL-031，固定与tester相同版本；源码/配置及真实浏览器、协议和脱敏证据 | 安全及交互检视有独立结论；不由tester或主执行自查代签，缺入口记受阻/未执行 |

固定同一提交或完整文件SHA-256清单，报告存testing/runs/deliver-codex-session-loop/。记录OS、工具、协议/适配器版本、逐例结果与清理；最终检查覆盖实际集成版本。不得动用户真实数据库、密钥或原生会话；专用测试资源本包拥有并在失败/取消时清理。

## Code Review

| Work Package | Reviewer Agent | Review Focus | Pass Condition |
| --- | --- | --- | --- |
| 全部编码前 | 独立reviewer | proposal/spec/design/tests及当前计划与上游依赖 | 每个引用场景存在且覆盖，无未闭环设计阻断 |
| 全部交付时 | 独立reviewer | 固定集成diff与实际安全/平台/验收证据 | 必需检查实际通过，reviewer问题由其独立复查关闭 |

## Main E2E

```yaml
mode: required
```

对应tests.md的L6真实Windows/Web链路，按角色使用真实浏览器/桌面工具；CL-031由独立reviewer通过工具完成L5安全及交互检视，当前无LX用例；可复用同版本、同范围且环境满足要求的有效证据，不要求用户重复手测。缺工具、环境、权限或入口记受阻/not-run，不转LX，不改成模拟通过。

## Risks

上游合同尚未实施，当前完成的是待实现文档。真实依赖/版本探测、平台证据、安全与供应链在实施时逐项交付。现有上游编译入口须核对覆盖范围，不将上游构建或本change用例定义记成会话闭环通过。设计发现违反已确认产品约束时先修文档，不能以门禁失败为由降级承诺。

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
| CL-001–042产品部分 | 独立tester | 产品尚不存在 | not-run | 用例定义不是运行证据，未运行正常/异常/平台验收 |

- 未执行项与已知限制：本change所有产品、安全、平台及真实Harness验收；结构检查不替代它们。
- 问题关闭依据：待reviewer首次独立检视及修复复查，不代关。
- 新版本证据判断：受影响spec/design/tests或上游/实现改变后，tester/reviewer独立确认重验范围。
- 归档后复核：未归档；后续核对spec/长期用例同步、移动映射及摘要一致性。

