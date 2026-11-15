# PI 接入实施计划

## Execution Model

本轮仅规划，不执行下述产品任务。基线 Git 53034ffc0c9f7644a3125788d1d2b98e51619f88；工作区已有用户授权的 DeepSeek/ZCode 删除及规划引用修订，必须保留。开工时重新记录实际基线、完整文件清单及删除项，不把此 SHA 当未来受测版本。

主执行者统筹，coder 交付配置和开发合同测试，独立 tester 编写/执行验收，独立 reviewer 检视。职责引用 openspec/role/coder.md、tester.md、reviewer.md；tester/reviewer 不继承 coder 对话。后续实际工作在隔离工作区开展，独占端口、配置/会话根、数据库及 CARGO_TARGET_DIR。主工作区仅串行集成，直接 diff 不强制绕行 patch；若采用 patch，先 git apply --check。

## Parallel Assessment

WP1 先固定环境与合同，WP2/WP3 在接口冻结后可于不同工作区并行；写范围不重叠。WP4 依赖两者集成，WP5 最终检视与 WP4 正式验收可在同一冻结版本上并行只读，但最终结论必须等双方闭环。Pi/OMP 互不依赖，独立配置和测试可并行；共享入口修复必须先更新明确写范围并串行集成，重跑两者受影响合同。本轮规划因共享工作区采用串行写入，独立只读检视可并行，不复制产品数据。

## Work Packages

下表新增路径均为后续拟建，当前不声称已有专项脚本。通用产品实现已存在；不默认新增厂商模块。若实测发现公共缺陷，先记录并调整共享修复范围、对应长期规格及验证安排，不私建兼容分支。

| ID | Goal / Scenarios | Dependencies | Agent | Write Scope | Deliverable | Verify |
| --- | --- | --- | --- | --- | --- | --- |
| WP1 | 真实入口、隔离条件、依赖及启动/清理方案定稿 | 新的实施授权，现有长期规格与实际通用入口 | 主执行者、独立reviewer | 本change proposal.md/design.md；testing/runs/integrate-pi-harness/discovery/ | 固定版本与来源、许可证/维护/安全审查、隔离与清理方案及可用检查入口清单；缺口明确阻断 | PI-001；openspec validate integrate-pi-harness --strict；Git差异与路径核验 |
| WP2 | 启动配置和节点合同开发验证 | WP1条件及方案/用例检视通过 | coder | configs/harnesses/pi/；scripts/harnesses/pi/；docs/harnesses/pi.md | 无密钥配置模板、开发验证脚本及参数/来源/结构/取消关闭说明；开发断言随交付 | PI-003–009、PI-011开发范围；拟建 scripts/harnesses/pi/verify.ps1；现有 npm run check:rust |
| WP3 | 通用 Web 接入与限制展示验证 | WP1冻结合同，可与WP2隔离并行 | coder | clients/web/tests/pi-harness.test.ts；docs/harnesses/pi-web.md | 通用登记、能力错误、历史提示、审批与取消展示的开发测试及说明 | PI-004–011页面范围；npm --prefix clients/web run check；npm --prefix clients/web run test -- pi-harness.test.ts；npm --prefix clients/web run build |
| WP4 | 独立验收程序、故障注入及真实闭环 | WP2/WP3集成 | 独立tester | testing/harnesses/pi/；testing/runs/integrate-pi-harness/tester/；本change tests.md | 独立验收入口与逐ID结果、固定真实版本、Windows完整进程树及隔离证据 | PI-001–013；拟建 testing/harnesses/pi/run.ps1；最终 npm run check 与 npm run build:rust |
| WP5 | 同版集成、独立交付检视与证据摘要 | WP4必需结果闭环 | 主执行者、另一独立reviewer | 本change plan.md/tasks.md/review.md；testing/runs/integrate-pi-harness/reviewer/；README.md；docs/HARNESS_CONTROL_PLANE_REDESIGN.md；docs/REBUILD_CAPABILITY_MAP.md | 固定文件清单、独立结论、逐项验证摘要及能力说明 | PI-001–013证据核验；openspec validate --all --strict；git diff --check |

## Merge Order

| Order | Work Package | Reason | Re-verify After Merge |
| --- | --- | --- | --- |
| 1 | WP1 | 实际入口及隔离/清理方案先固定；方案变化须tester/reviewer再核对 | PI-001、独立方案检视；尚未完成的真实产品清理效果留给WP4 |
| 2 | WP2 | 生成既有平台可消费的配置，不含用户秘密 | PI-003–009、PI-011开发检查和配置解析回归 |
| 3 | WP3 | 在WP2最终配置合同上集成页面测试 | PI-004–011页面范围与 check:web |
| 4 | WP4 | 对集成结果验收，不能沿用包内独立版本替代 | PI-001–013；缺入口或必要真实证据记not-run并阻断交付 |
| 5 | WP5 | 合并证据及说明；与另一change共享README/总体设计/能力地图 | Pi的WP5先、OMP的WP5后；后合入者重验两份change strict、文档链接和能力状态；涉及代码/配置/测试变化重新运行受影响合同与最终检查 |

## Project Verify

根目录 D:/Project/acp-remote，后续隔离工作区使用同结构。依赖安装按 README，Rust 工具链从 rust-toolchain.toml 读取，不复制版本。真实环境为 Windows、可信 HTTPS、真实浏览器、已明确授权的测试身份/工作区和隔离原生会话。模型访问与费用边界在正式运行前确定，本轮不运行。

| Check ID | Command / Working Directory | Scope / Pass Criteria |
| --- | --- | --- |
| PV1 | 根：openspec validate integrate-pi-harness --strict；git diff --check；reviewer逐项检视 | PI-001规划部分；文件存在、Requirement/Scenario映射及工作包由人工审查，不能以CLI代替语义覆盖 |
| PV2 | 根：npm run check；npm run build:rust | PI-002，WP4在最终集成版本执行；记录非零断言和全部实际退出码，既有失败不得隐去 |
| PV3 | 根：pwsh -NoProfile -File scripts/harnesses/pi/verify.ps1（WP2新建，参数合同在WP1固定）及WP3表内Web命令 | PI-003–011开发范围；脚本交付后先验证可执行并记命令，当前缺脚本不是通过 |
| PV4 | 根：pwsh -NoProfile -File testing/harnesses/pi/run.ps1（WP4新建，支持用例选择、隔离根、版本和报告目录参数） | PI-003–013独立验收；逐ID、逐分支标真实或模拟；L6不能替身替代，真实不支持可选能力需实际依据，必要安全/清理未验阻断交付 |
| PV5 | 根：openspec validate --all --strict；git diff --check；reviewer同版文件清单核对 | PI-001–013证据闭环及最终diff；规范、测试、配置与文档版本一致 |

记录到 testing/runs/integrate-pi-harness/ 的发现、tester、reviewer子目录，附 Windows/程序/协议版本、依赖锁定、命令、断言数量、脱敏轨迹、清理状态及未执行原因。原始报告不可访问时，下面持久摘要仍须能判断结果。升级只复用同版本、同范围、同环境的有效证据，其余重新验证。

## Code Review

| Work Package | Reviewer Agent | Review Focus | Pass Condition |
| --- | --- | --- | --- |
| WP1 | 独立reviewer | 探索分支、候选依赖、六类规划及tester用例 | 无未闭环设计阻断；实施输入明确，不以规划通过代替WP1真实取证 |
| WP2/WP3 | 独立reviewer | ACP边界、参数/内容保真、错误与安全语义 | 新行为断言及开发检查有效，未添加厂商兜底或不当准入 |
| WP4/WP5 | 与tester不同的独立reviewer | 同一冻结版本、逐例真实证据、完整树清理及范围限制 | 必需结果均有证据，独立问题经原提出者复查关闭，不能自测代签 |

## Main E2E

```yaml
mode: required
```

后续由独立tester通过可用真实浏览器执行 PI-013，角色及真实/模拟边界见 tests.md。本轮仅规划，端到端未执行；不因缺本地入口把 required 改成 not-applicable。

## Risks

本地版本与路径未确定、Pi候选适配器限制或OMP入口差异可能阻断正式接入；WP1记录具体差异，不靠私有桥接消除。独立测试入口尚需建设，不把命令列入计划视为可执行。取消终态与实例释放分开，常驻进程可在取消成功后保留。

## Handoff

各角色仅接收用户原话、AGENTS/config/角色文件、本change六类产物、长期规格、固定版本清单、允许写范围、检查入口与隔离资源归属，不传开发对话。tester独立维护预期，reviewer只读产品或使用隔离工作区。新版本需说明哪些证据失效及独立复验依据。

## Findings

| ID | Source | Severity | Target Version | Finding | Fix Version | Status | Retest / Re-review |
| --- | --- | --- | --- | --- | --- | --- | --- |
| HPLAN-R001 | 独立reviewer初审 | P2 | 本轮初稿 | 取消与实例释放条件混淆 | 当前spec与tests | 已关闭 | 原reviewer已对修订spec与011用例独立复核，见review.md |

## Verification Results

- 受测版本：本轮仅规划工作区；最终规划文件摘要由独立 review.md 固定。产品版本尚未取得，不能记为兼容通过。
- 摘要排除范围：review报告自身及 plan 的 Findings/Verification Results 状态摘要；产品代码、配置和测试变化不可排除。
- 执行者与环境：主执行者进行文档校验；独立tester定义用例，独立reviewer检视；本轮未运行真实Harness。
- 验证命令及目录：2026-11-16根目录 openspec validate --all --strict 通过19/19，git diff --check退出0；主执行者与独立reviewer分别执行。独立tester复核计划安排通过，reviewer逐项规划检视通过，详见review.md。文档校验不能签署整个 PI-001（含后续真实依赖审查）通过。

| Case ID | Executor | Target Version | Status | Evidence / Limitations |
| --- | --- | --- | --- | --- |
| PI-001 | 独立tester/reviewer | 产品版本待定 | not-run | 仅规划校验/检视可在本轮完成；真实组件及依赖证据尚缺 |
| PI-002–013 | 独立tester | 待WP1固定 | not-run | 本轮未实施，专项入口和真实运行条件未齐 |

- 未执行项与已知限制：全部产品构建、真实会话、Windows清理、供应链与交付验收未执行；只交付规划。
- 问题关闭依据：HPLAN-R001由原reviewer复核修订后的取消/释放场景及011用例后关闭；无未闭环规划问题。独立tester对全部用例与plan执行安排复核无问题。结论仅属本轮规划，不覆盖产品交付。
- 新版本证据判断：本轮没有可外推的产品验收结果，后续程序/适配器/配置或平台变化需独立判断复验范围。
- 归档后复核：待真实交付、长期规格及用例同步后执行；本轮不归档。
