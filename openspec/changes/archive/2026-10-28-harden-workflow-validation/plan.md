# Plan

## Execution Model

主执行者兼coder，独立tester维护tests并正式验收，独立reviewer先检视方案/用例再检视交付。基线dfe136128844f016e3d806948997a2df7a61b712，开始时无用户未提交改动。开发在当前工作区；独立验收使用临时目录和独立进程，reviewer只读固定清单。未提交内容通过SHA-256清单固定。

## Parallel Assessment

实现、规则与回归用例互相依赖，coder串行集成避免共享文件冲突；tester仅写自己的tests和报告，不写实现。正式验收与交付检视可并行，二者只读取固定内容。无需并行coder工作树。

## Work Packages

| ID | Goal / Scenarios | Dependencies | Agent | Write Scope | Deliverable | Verify |
| --- | --- | --- | --- | --- | --- | --- |
| WP1 | 三项漏洞及阶段边界 | 独立方案/用例通过 | 主执行者coder | scripts/**、package.json、package-lock.json | 修复与开发回归 | npm run test:workflow |
| WP2 | 规则及证据留存 | WP1接口确定 | 主执行者coder、tester | openspec/config.yaml、openspec/role/**、openspec/schemas/**、openspec/testing/suites/** | 规则/模板/长期用例 | npm run check |
| WP3 | 独立验收及检视 | WP1、WP2 | tester、reviewer | 本变更证据记录、忽略的运行报告 | 固定版本结论和问题闭环 | tests.md用例、npm run verify |

## Merge Order

WP1后WP2，WP3对集成快照执行。tester独占tests.md及测试报告；coder不调整验收预期。重叠修改均由主执行者串行完成后重新生成快照。

## Project Verify

根目录已安装锁定依赖。执行tests.md的WF-HARDEN-001～008：002～004用Node正反向矩阵，005用真实CLI与临时目录，006～008由独立Agent复核规则及证据，001汇总npm run test:workflow、npm run check和npm run verify。无前端或产品改动。失败保持未完成，原始输出存testing/runs，摘要保留本文件。

## Code Review

独立reviewer编码前核对proposal/design/tests/plan；通过后实施。交付检查实际diff、任务ID边界、YAML类型/错误、规范根目录、阶段依赖、长期suite和报告可追溯性；阻断问题由提出者复核关闭。

## Main E2E

```yaml
mode: not-applicable
reason: 不修改产品用户链路
basis: 变更范围为开发工作流与检查脚本
alternative_checks: [真实OpenSpec CLI生命周期、结构正反向测试、独立规则复核]
```

## Handoff

独立角色收到用户修复授权、基线、AGENTS/config/role及本变更规划路径、只读实现范围和验收判据；不继承开发对话。内容版本清单另行生成，报告和plan结果/tasks状态不计入受测实现摘要。

## Findings

| ID | Source | Severity | Target Version | Finding | Fix Version | Status | Retest / Re-review |
| --- | --- | --- | --- | --- | --- | --- | --- |
| PRE-001 | reviewer | 阻断 | 规划基线 | tests尚未生成 | 编码前基线 | reviewer关闭 | 已读取独立tester用例 |
| PRE-002 | reviewer | 阻断 | 初版tests | 中文写成字面问号 | UTF-8修订 | reviewer关闭 | 亲自复读中文步骤/预期；tests摘要CE965230943EC328CAE1E55AF3E3B7B93C9C7B0A9BE5A60A051857D4F38B2A16 |
| PRE-003 | reviewer | 方案补充 | 阶段实现 | 全入口需按阶段选择delta strict | 新接线方案 | reviewer前置通过 | 要求长期全量strict及全入口正反例；交付另复查 |
| REV-001 | reviewer | P2阻断 | S1 | required字段把TODO/TBD/HTML字符串视为空，且Markdown过滤误删YAML原值 | S2 | reviewer复查关闭 | 独立6反例均拒绝；真实空值、引号HTML/列表原值和围栏内标题通过 |

## Verification Results

受测版本及排除规则见本变更verification-manifest.json；基线为dfe136128844f016e3d806948997a2df7a61b712，文件清单含未提交内容。plan从Findings起的证据记录、tasks勾选和运行报告不影响实现摘要；其他规划内容参与摘要。

S1清单保存在verification-manifest-s1.json，文件摘要bc27344cdbfcbff0264cb138a1978009efc0fdea4384fd7962de6f92db27ea02；独立tester/reviewer均复算18文件一致。S1因REV-001不通过，不将该版本旧结论套到S2。S2只调整检查器及其开发回归，其他文件无变；独立角色复验YAML和Markdown受影响范围，其余证据按独立影响判断复用。

S2清单verification-manifest.json的SHA-256为ade09aa0591097f9a908ab8be34af9c37c79238a6faea980999a26ae44350396。reviewer独立复算18项一致，对两个变化脚本重新检视，其余16项明确复用S1；required的reason/basis各TODO、TBD、引号HTML共6反例均拒绝，真实空值及not-applicable引号HTML、列表和围栏内标题均通过，REV-001由reviewer关闭。完整实现检视通过；正式tester及归档复核另计。附加报告位于openspec/testing/runs/harden-s2/reviewer.md。

环境为Windows x64、Node v22.20.0、仓库锁定依赖；coder执行根目录npm run test:workflow，47/47通过，含真实完整门禁及CLI归档回归。首次完整npm run verify退出0，覆盖合同、fmt、Clippy和Rust工作区测试；之后新增阶段接线与测试，Rust实现/依赖未改，最终合同检查另执行。未改前端，不运行前端检查。CI的供应链/安全公告/密钥专项不在本地verify范围。

主执行者最终在S2再次实际运行根目录npm.cmd run verify，退出0；包括所有合同门禁、47/47工作流测试、cargo fmt、Clippy -D warnings及Rust工作区测试。原始日志.worktrees/harden-verify-s2.log仅为附加证据。预存real_browser_creates_three_levels_then_selects_and_cancels用例标记ignored，需本机Chromium及构建Web UI；本次无产品/前端改动，不声称该浏览器用例执行。独立tester另在具临时文件操作权限环境运行npm.cmd run check退出0；沙箱首次归档/junction的EPERM仅是环境受阻，未削弱测试或将失败标通过。

| Case ID | Executor | Target Version | Status | Evidence / Limitations |
| --- | --- | --- | --- | --- |
| WF-HARDEN-001 | 独立tester | S2 ade09a…0396 | pass | 独立npm.cmd run check退出0，47/47及全部合同通过；同版本范围Rust/fmt/Clippy复用主verify，明确非tester亲跑cargo |
| WF-HARDEN-002 | 独立tester | S2 ade09a…0396 | pass | 独立12项任务矩阵；前缀/连接符/注释/围栏/普通段落/未知ID拒绝，标点/反引号/列表正例通过 |
| WF-HARDEN-003 | 独立tester | S2 ade09a…0396 | pass | 独立64项YAML矩阵重跑；REV-001双模式12断言及围栏内标题1断言通过；required非空拒绝而真实非空值保真 |
| WF-HARDEN-004 | 独立tester | S2 ade09a…0396 | pass | 独立磁盘5布局验证活动裸路径/长期显式路径/suite裸路径不跨根补位 |
| WF-HARDEN-005 | 独立tester | S2 ade09a…0396 | pass | 独立真实完整门禁9场景，规划前缀过、坏delta/缺前置/未skip无spec/坏长期拒绝；真实CLI依赖4断言通过 |
| WF-HARDEN-006 | 独立tester | S2 ade09a…0396 | pass | 首次按proposal/specs/design编tests，plan后核对；schema开工依赖保留，场景名称/覆盖由reviewer核查，文件检查不冒充覆盖 |
| WF-HARDEN-007 | 独立tester | S2 ade09a…0396 | pass | 模板有可重建清单、执行者/环境/命令、逐ID状态、限制/关闭依据；本表保存独立断言摘要，不依赖本机原始报告 |
| WF-HARDEN-008 | 独立tester | S2 ade09a…0396 | pass | 需求/预期重检基线、实现/合同/入口按影响重验；真实S1摘要变化后停验，S2复算并重验，移动复核及结果记录分离明确 |

- 问题关闭依据：PRE-001/002由独立reviewer编码前关闭，PRE-003接线交付已复核；REV-001由原reviewer在S2复查关闭。tester未新增阻断，归档前正式验收与实现检视均通过。附加原始报告openspec/testing/runs/harden-s2/tester.md。
- 新版本证据判断：S1因REV-001未通过，S2针对变化重新验收；reviewer复用未变16项并检查两脚本，tester重跑自有矩阵及完整check，独立确认同范围verify证据可复用。主执行者仍对最终S2完整重跑verify退出0。
- 未执行项与已知限制：L1无类型改动、L6无产品链路改动、LX无人类专属操作，见tests不适用依据；预存浏览器ignored及CI专项限制如上。不改前端，不声称前端验收；归档后复核待执行。
- 归档后复核：待归档，随后比对移动映射、摘要及受影响检查，不提前宣称完成。
