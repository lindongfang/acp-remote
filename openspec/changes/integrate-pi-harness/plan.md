# Pi 接入实施计划

## Execution Model

本轮仅修订规划与范围，不执行下述产品或真实验收任务。基线为 Git `65d734f638bcb951e237b69faa592917b3a6d2c7`（`65d734f6`）之上的当前工作区（已按用户 2026-11-17 确认移除 `integrate-omp-harness` 并同步收敛顶层设计范围）；主工作区现有 `deliver-session-capacity` 等其他在途变更的未提交源码与规划修改必须完整保护。后续正式开工时先核对并记录当时实际基线、完整文件清单及未提交修改边界，不在未收敛共享文件上直接并发覆盖。

主执行者统筹，`coder` 交付无版本绑定配置模板、开发验证脚本、Web 专项测试与说明，独立 `tester` 维护验收预期并实现/执行独立验收，独立 `reviewer` 检视方案与交付。职责遵循 `openspec/role/coder.md`、`tester.md`、`reviewer.md`；`tester`/`reviewer` 不继承 `coder` 对话。后续执行在隔离工作区及 `.tmp/` 独立数据根开展，独占端口、隔离配置/会话根、SQLite 与 `CARGO_TARGET_DIR`，绝不安装、升级、降级或在仓库绑定本机 `pi` 版本。主工作区串行集成，若采用 patch 先 `git apply --check`。

## Parallel Assessment

`WP1` 先在隔离探针子目录完成本机 `pi` 只读摸底、候选外部 ACP 服务端审查及最小协议实证并冻结参数合同；随后 `WP2` 与 `WP3` 写范围完全互斥，可在不同隔离工作区并行开展。`WP4` 依赖 `WP2`/`WP3` 集成结果；`WP5` 交付检视可与 `WP4` 在同一冻结快照上只读并行，最终汇总、长期规格与长期套件同步在双方闭环后串行落盘。OMP 适配已暂缓移除，本变更与其他通用能力变更互不阻塞，仅 `WP5` 更新顶层共享说明文档时与主工作区在途变更串行协调合入。若实测暴露通用平台缺陷或需扩展 Web 表单/客户端能力，必须先停下另立或更新对应通用变更，不在本专项私改 `app/src/` 或 `clients/web/src/`。

## Work Packages

下表专项脚本与文档均为后续拟建，全部对齐仓库既有 `testing/probes/`、`docs/implementation/`、`testing/runs/`、`openspec/specs/`、`testing/suites/` 目录体系，不新建 `configs/`、`scripts/`、`testing/harnesses/` 或 `docs/harnesses/`。项目任何工作包均不绑定或锁定 `pi` 本体版本。若 `WP1` 实证触发 `tests.md` 预期调整，须先按 `tester.md` 补充 `tester` 写范围并经独立 `reviewer` 复核。

| ID | Goal / Scenarios | Dependencies | Agent | Write Scope | Deliverable | Verify |
| --- | --- | --- | --- | --- | --- | --- |
| WP1 | 本机 `pi` 只读定位、候选适配器隔离探针与方案定稿 | 新的实施授权，现有长期规格与通用入口源码 | 主执行者、独立reviewer | 本change proposal.md/design.md；testing/probes/pi-harness/discovery/；testing/runs/integrate-pi-harness/discovery/ | 本机 `pi` 入口只读取证（不锁 `pi` 版本）、候选适配器许可证/维护/安全审查与隔离探针、空客户端能力及 `-32601` 委托实测结论、隔离与清理方案及门禁检视记录；缺口明确阻断 | PI-001；openspec validate integrate-pi-harness --strict；Git差异与写范围核验 |
| WP2 | 无版本绑定配置模板、Windows 绝对路径无 Shell 启动及节点合同开发验证 | WP1门禁与独立方案/用例检视通过 | coder | testing/probes/pi-harness/profiles/；testing/probes/pi-harness/verify.ps1；docs/implementation/pi-harness.md | Windows 绝对路径原生 PE 启动的无密钥 `format: 1` 配置模板（不含 `pi` 版本绑定）、开发验证脚本及单入口/底层配置/结构/取消关闭说明 | PI-003–009、PI-011底层与节点开发范围；拟建 testing/probes/pi-harness/verify.ps1；npm run check:rust |
| WP3 | 通用 Web 已暴露表单范围与限制展示验证 | WP1冻结合同，可与WP2隔离并行 | coder | clients/web/tests/pi-harness.test.ts；docs/implementation/pi-harness-web.md | 通用登记（已暴露字段与行级 `trim` 边界）、能力/委托错误、历史不完整提示、审批与取消/释放分开展示的 Web 测试及范围说明（明确标注 `mcpServers`/`extra` 未在当前表单暴露） | PI-004–011页面范围；npm --prefix clients/web run check；npm --prefix clients/web run test -- pi-harness.test.ts；npm --prefix clients/web run build |
| WP4 | 独立验收程序、故障注入及本机真实 Pi 闭环 | WP2/WP3集成 | 独立tester | testing/probes/pi-harness/acceptance/；testing/runs/integrate-pi-harness/tester/；本change tests.md | 独立验收入口、故障/映射/委托替身夹具及逐ID实测报告（只读记录当次观测到的本机 `pi` 与适配器版本，不绑定版本）、Job 进程树收敛及用户数据零改动证据 | PI-001–013；拟建 testing/probes/pi-harness/acceptance/run.ps1；最终 npm run check 与 npm run build:rust |
| WP5 | 同版集成、独立交付检视、长期规格与套件同步及证据摘要 | WP4必需结果闭环 | 主执行者、另一独立reviewer | 本change plan.md/tasks.md/review.md；testing/runs/integrate-pi-harness/reviewer/；openspec/specs/pi-harness-compatibility/；testing/suites/pi-harness-compatibility/；testing/suites/traceability/integrate-pi-harness.md；README.md；docs/HARNESS_CONTROL_PLANE_REDESIGN.md；docs/REBUILD_CAPABILITY_MAP.md | 冻结文件清单、独立交付结论、长期规格与长期验收套件及追溯矩阵同步、逐项验证摘要及真实能力说明 | PI-001–013证据核验；openspec validate --all --strict；git diff --check |

## Merge Order

| Order | Work Package | Reason | Re-verify After Merge |
| --- | --- | --- | --- |
| 1 | WP1 | 先只读确认本机 `pi` 入口、候选适配器可行性及空客户端能力表现；方案调整须由 tester/reviewer 复核 | PI-001、独立方案与用例检视；未满足门禁不进入 WP2/WP3 |
| 2 | WP2 | 落盘绝对路径原生可执行 `format: 1` 无密钥配置模板与节点侧验证脚本，不绑定 `pi` 版本 | PI-003–009、PI-011节点开发检查与 check:rust |
| 3 | WP3 | 在 WP1/WP2 合同基础上合入通用 Web 测试与表单边界说明（与 WP2 写范围无重叠） | PI-004–011页面范围与 check:web |
| 4 | WP4 | 对 WP2+WP3 集成后的同一快照执行独立验收与真实闭环 | PI-001–013；缺入口或必要真实证据记 not-run 并阻断交付 |
| 5 | WP5 | 汇总同版证据、一并同步 `openspec/specs/` 与 `testing/suites/` 并更新共享说明文档；须保护主工作区其他在途 change 的修改 | 与其他共享 README/总体设计/能力地图的在途变更严格串行；合入后重跑 openspec validate --all --strict、长期套件映射核验、文档链接核验及受影响检查 |

## Project Verify

根目录 `D:/Project/acp-remote`，隔离工作区保持相同结构。依赖按 README 安装，Rust 工具链只读 `rust-toolchain.toml`。真实运行环境为 Windows、可信 HTTPS、真实浏览器、已授权测试身份/工作区及 `.tmp/` 下的隔离配置与会话根；直接调用本机已安装的 `pi`（不绑定版本），模型访问与费用边界在正式运行前确认，本轮不运行。

| Check ID | Command / Working Directory | Scope / Pass Criteria |
| --- | --- | --- |
| PV1 | 根：`openspec validate integrate-pi-harness --strict`；`git diff --check`；独立 reviewer 逐项检视 | PI-001规划部分；文件存在、Requirement/Scenario 精确映射、源码边界一致性与工作包互斥由人工核对，不以 CLI 代替语义审查 |
| PV2 | 根：`npm run check`；`npm run build:rust` | PI-002，WP4 在最终集成快照执行；记录非零断言数与全部退出码，既有失败不得隐去 |
| PV3 | 根：`pwsh -NoProfile -File testing/probes/pi-harness/verify.ps1`（WP2 新建，参数合同在 WP1 冻结）及 WP3 表内 Web 命令 | PI-003–011开发范围；验证绝对路径原生 PE 启动、拒绝相对路径/`.ps1`、底层 `format: 1` 保真及通用 Web 已暴露范围，当前缺脚本不计通过 |
| PV4 | 根：`pwsh -NoProfile -File testing/probes/pi-harness/acceptance/run.ps1`（WP4 新建，接收用例选择、隔离根与报告目录参数，不接收硬编码 Pi 版本要求） | PI-003–013独立验收；逐 ID、逐分支标真实或模拟并只读记录当次观测版本；L6 禁止替身代签，必要安全/进程树清理未证实阻断交付 |
| PV5 | 根：`openspec validate --all --strict`；`git diff --check`；reviewer 核对同版快照及 `openspec/specs/` 与 `testing/suites/` 同步一致性 | PI-001–013证据闭环、长期规范与套件精确映射及最终 diff；规范、测试、配置模板与说明一致且无 Pi 版本硬编码绑定 |

运行证据写入 `testing/runs/integrate-pi-harness/` 下 `discovery/`、`tester/`、`reviewer/` 子目录，记录 Windows 环境、当次只读观测到的本机 `pi` 与适配器版本、探针适配器锁文件摘要（不含 `pi` 本体）、命令、断言数、脱敏轨迹、Job 进程树收敛状态及未执行原因。本机 `pi` 或适配器版本变化后，运行时无需改配置即可按 ACP 协商连接，但历史实测证据只归属原观测组合，差异范围须重新判断。

## Code Review

| Work Package | Reviewer Agent | Review Focus | Pass Condition |
| --- | --- | --- | --- |
| WP1 | 独立reviewer | 无版本绑定约束、源码边界（单入口/绝对路径无 Shell/空客户端能力/Web 表单范围）、候选适配器审查及 tester 用例 | 无未闭环规划阻断；实施门禁明确，未把规划通过当成真实取证完成 |
| WP2/WP3 | 独立reviewer | 绝对路径原生 PE 启动配置、无密钥与无 Pi 版本锁定、底层 profile 与 Web 表单分层保真、错误与安全语义 | 新增断言有效，未私改通用源码或引入隐式 Shell/厂商兜底 |
| WP4/WP5 | 与tester不同的独立reviewer | 同一冻结快照、逐例真实证据、Job 完整树清理、用户本机数据零改动及长期规格/套件同步 | 必需结果均有同版证据，独立问题由原提出者复查关闭，无自测代签 |

## Main E2E

```yaml
mode: required
```

后续由独立 `tester` 在真实 Windows + 真实浏览器 + 本机已安装 `pi` + 外部 ACP 服务端（隔离会话/配置根）下执行 `PI-013`，真实/模拟边界见 `tests.md`。本轮仅修订规划，端到端未执行；不因尚未进入实施阶段将 `required` 改为 `not-applicable`。

## Risks

候选外部 ACP 服务端在平台空客户端能力（`clientCapabilities: {}`）及反向 `-32601` 下的工具/审批行为尚未实测；通用 Web 表单尚未暴露 `mcpServers`/`extra` 编辑控件；本机 `pi` 随用户日常升级可能改变行为。上述均由 `WP1` 实证摸底与分层报告承接，不在本专项锁死用户 `pi` 版本或私造桥接。

## Handoff

各角色仅接收用户原话、`AGENTS.md`、`openspec/config.yaml`、对应 `openspec/role/*.md`、本 change 六类产物、相关长期规格、相关通用入口源码路径、允许写范围与隔离资源约束，不传递开发对话。`tester` 独立维护 `tests.md` 预期，`reviewer` 只读检视同一快照。

## Findings

| ID | Source | Severity | Target Version | Finding | Fix Version | Status | Retest / Re-review |
| --- | --- | --- | --- | --- | --- | --- | --- |
| HPLAN-R001 | 首轮独立reviewer | P2 | 初稿 | 取消确认与实例释放进程树收敛条件混淆 | 65d734f6 规划版 | 已关闭 | 原 reviewer 已复核 spec 与 PI-011 分离表述 |
| HPLAN-R002 | 二轮源码对照审查 | P1 | 65d734f6 规划版 | spec 与 PI-004 要求系统区分记录双组件身份/版本，且原表述绑定固定 Pi 版本，与单一 `launch`/`HarnessSummary` 代码模型及“不绑定 Pi 版本”要求冲突 | 二轮规划修订版 | 已关闭 | 独立 tester 与独立 reviewer 对照锁定快照终审通过 |
| HPLAN-R003 | 二轮源码对照审查 | P1 | 65d734f6 规划版 | design 与 PI-003 拟选 `.ps1`/`.cmd` 启动链，与 `OwnedProcess::spawn` 绝对路径无 Shell `CreateProcessW` 实现冲突 | 二轮规划修订版 | 已关闭 | 独立 tester 与独立 reviewer 复核绝对路径原生 PE 启动及相对路径/`.ps1` 拒绝断言通过 |
| HPLAN-R004 | 二轮源码对照审查 | P1 | 65d734f6 规划版 | 未区分底层 `format: 1` 配置与当前硬编码 `mcpServers: []`/`extra: {}` 且按行 `trim` 的通用 Web 表单，也未覆盖 `clientCapabilities: {}` 与反向 `-32601` 委托边界 | 二轮规划修订版 | 已关闭 | 独立 tester 与独立 reviewer 复核两层配置、`validate_setup_caps` 与 `-32601` 断言通过 |
| HPLAN-R005 | 二轮源码对照审查 | P2 | 65d734f6 规划版 | 拟新建 `configs/`、`scripts/`、`testing/harnesses/`、`docs/harnesses/` 脱离仓库目录约定，且缺长期套件写范围 | 二轮规划修订版 | 已关闭 | 独立 reviewer 复核既有目录体系收敛及长期规格/套件同包同步通过 |
| HPLAN-R006 | 二轮源码对照审查 | P2 | 65d734f6 规划版 | WP1 写范围不含隔离探针目录，无法在不污染主工程且不绑 `pi` 版本的前提下实测候选适配器 | 二轮规划修订版 | 已关闭 | 独立 reviewer 复核 `testing/probes/pi-harness/discovery/` 互斥写范围通过 |
| HPLAN-R007 | 二轮独立reviewer | P2 | 二轮初稿 | `spec.md:65` 中 `。MUST ` 缺少左侧空格，违反 `config.yaml` 规范关键词两侧空格硬规则 | 二轮规划修订版 | 已关闭 | 原提出独立 reviewer 逐处复核 `spec.md` 全文 12 处 `SHALL`/`MUST` 空格合规通过 |
| HPLAN-R008 | 二轮独立reviewer | P2 | 二轮初稿 | WP5 仅纳入 `testing/suites/` 而将新能力 `openspec/specs/` 同步留在 `Workflow follow-up`，导致长期套件违反禁止跨目录补位 | 二轮规划修订版 | 已关闭 | 原提出独立 reviewer 与独立 tester 复核 `WP5` 及 `tasks.md` `5.2` 同包同步通过 |
| HPLAN-R009 | 二轮独立reviewer | P2 | 二轮初稿 | `review.md` 仍绑定旧 `53034ffc` 快照且宣称已通过，与基于 `65d734f6` 重写的六类产物不一致 | 二轮及范围同步修订版 | 已关闭 | 独立 reviewer 已复核快照并随 2026-11-17 移除 OMP 同步刷新 SHA-256 |

## Verification Results

- 受测版本：基于 `65d734f638bcb951e237b69faa592917b3a6d2c7` 移除 `integrate-omp-harness` 后的 Codex + Pi 规划修订快照（最终文件 SHA-256 由 `review.md` 锁定）。未运行真实 Harness，不声明产品兼容通过。
- 摘要排除范围：`review.md` 自身及本文件 `Findings`、`Verification Results` 节；六类规划实质内容不可排除。
- 执行者与环境：主执行者在仓库根运行 CLI 校验；独立 `tester` 与独立 `reviewer` 在不继承开发对话的只读子会话中核对全部规划文件、长期规格与真实接入源码。
- 验证命令及目录：2026-11-17 主执行者于仓库根执行 `node node_modules/@fission-ai/openspec/bin/openspec.js validate --all --strict`（18 passed / 0 failed，退出码 0）与 `git diff --check -- openspec/changes/integrate-pi-harness README.md docs/HARNESS_CONTROL_PLANE_REDESIGN.md docs/REBUILD_CAPABILITY_MAP.md`（退出码 0）。

| Case ID | Executor | Target Version | Status | Evidence / Limitations |
| --- | --- | --- | --- | --- |
| PI-001 | 主执行者 + 独立tester/reviewer | 二轮及范围同步修订版（产品待实测） | not-run | 规划一致性、源码边界与无版本绑定约束已通过终审；WP1 真实环境与候选适配器供应链实证待后续实施授权执行，故整例保持 not-run |
| PI-002–013 | 独立tester | 待WP1实证后执行 | not-run | 本轮仅修订规划，专项脚本尚未创建、真实运行未执行 |

- 未执行项与已知限制：全部产品编译测试、探针脚本、本机 `pi` 实连、Windows Job 进程树清理与供应链审查均未执行；主工作区 `deliver-session-capacity` 等既有在途源码修改保持原状未触碰。
- 问题关闭依据：原独立 `tester` 与原独立 `reviewer` 已完成二轮实质终审关闭 `HPLAN-R001`–`HPLAN-R009`，2026-11-17 仅按用户确认同步移除 OMP 交叉引用并刷新快照摘要。
- 新版本证据判断：本轮无产品实测证据；后续本机 `pi` 或外部适配器变化不影响无版本绑定配置启动，但兼容性报告结论仅归属各自当次观测版本组合。
- 归档后复核：待真实交付、长期规格与 `testing/suites/` 同步后执行；本轮不归档。
