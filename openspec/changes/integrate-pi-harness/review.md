# Pi 规划独立检视

## 结论与范围

2026-11-16，独立 reviewer 在不继承主执行者或 tester 对话的会话中完成只读规划检视。结论：**本轮规划通过；产品实施与兼容验收未完成。** 没有尚未闭环的规划阻断问题，HPLAN-R001 已由原提出者独立复查关闭。此结论仅覆盖创建 change 的 SC1 文档范围，不授权安装、升级、产品编码或真实 Harness 运行，不宣称 SC2–SC4 已达成。

依据为用户原话、AGENTS.md、openspec/config.yaml、三角色职责、当前模板、docs/code-standards.md、LESSONS.md、总体设计及相关长期规格。实际核对通用配置、目录登记和 Web 入口源码，并核对 package.json/clients/web/package.json 的现有检查命令。两项新 change 的六类规划及元数据、README、总体设计、能力地图均纳入下方快照。其他已有用户修改只核对范围与保护要求，不签署其内容通过。

Pi 本地安装仅有用户陈述，路径与版本未知；pi-acp 是第三方候选而非已采用依赖。设计明确 ACP 服务端与 Pi RPC 子进程的边界，要求分别取证 MCP/模型配置生效和结构保真，不将适配器接受参数或文本降级写成能力通过。

## 需求与用例逐项核对

已人工对照本 change 增量文件中的 6 个 Requirement、12 个 Scenario，精确名称均存在且由下表用例覆盖；这不是文件存在性或 CLI 校验推断。13 个稳定用例覆盖正常、拒绝、取消、竞态、失效、隔离和清理路径。长期依据中明确点名的 Requirement 也已核对真实名称；裸 specs 路径只定位本次增量，没有跨目录补位。

| 用例 | Requirement | Scenario |
|---|---|---|
| PI-004 | Pi 接入身份与入口可追溯 | 通过外部 ACP 服务端接入；入口缺失或提供非 ACP 输出 |
| PI-005 | Pi 能力按当前 ACP 连接处理 | 标准操作不支持 |
| PI-006 | Pi 能力按当前 ACP 连接处理 | 程序或适配器升级 |
| PI-007 | Pi 内容保真与限制可见 | 收到结构化工具内容；适配器存在结构丢失或输入降级 |
| PI-008 | Pi 恢复与历史分别验收 | 恢复成功但无完整重放 |
| PI-009 | Pi 恢复与历史分别验收 | 原生引用或适配器映射失效 |
| PI-010 | Pi 权限与进程链受节点管理 | 真实权限请求与页面断线 |
| PI-011 | Pi 权限与进程链受节点管理 | 取消或关闭未确认 |
| PI-012 | Pi 兼容证据与用户数据隔离 | 缺少本地路径或隔离环境；验证失败后清理 |
| PI-001–003、PI-013 | proposal.md `Success Criteria` | SC1–SC4；PI-013 复核以上场景的真实可达分支及逐能力限制 |

PI-001 的规划检查与后续真实依赖取证已区分；本轮不能将整个用例记为产品验收通过。PI-002 引用现有构建检查及新增行为适用 Red Check；PI-003 的启动回显夹具不冒充真实程序参数生效。PI-004–012 的协议替身和真实节点边界明确，PI-013 单独要求真实 Windows/浏览器/程序/原生会话与所属进程树。可选能力不支持需实际依据，未触达分支记未验证，必要安全或清理未验证阻断交付。没有把工具可检验项目或独立安全审查转为 LX。

## 执行安排核对

- plan 必备节均有实质内容，Work Packages 表为 7 列，WP1–WP5 唯一。WP2 配置/脚本/说明与 WP3 Web 测试/说明的写范围互斥；并行使用隔离工作区、资源和构建目录。
- WP1 固定入口、隔离、参数合同与清理方案；真实参数生效和完整进程树清理效果由 WP2/WP4/WP5 证明。未把待建设的真实验收设为编码前循环前置。
- 同 change 的集成按 Merge Order 顺序开展；两个 change 对 README/总体设计/能力地图的 WP5 重叠明确串行，Pi 先、OMP 后，后合入者复核两者结构、链接、状态及受影响合同。
- 两份 plan 的 Main E2E 各只有一个 YAML 配置块，唯一字段 mode 为字符串 required，无重复键/别名/类型冲突，并分别指向各自 013 真实用例。本轮不运行不改变 required。
- 本 change 共 13 个真实 checkbox，全部未勾选；每项正文有完成判据并精确引用声明的 WP，WP1–WP5 均有对应任务，无仅前缀匹配或未声明工作。开发测试/说明、独立验收和独立检视各有任务；归档留在非 tracked follow-up。
- 现有命令与拟建专项入口已区分。专项脚本参数合同在 WP1 固定、脚本分别由 WP2/WP4 交付；当前规划通过不表示这些入口已经可执行。若需公共代码修复，先调整写范围、规格与验证计划，再重新检视。
- 计划明确 tester/reviewer 独立上下文、固定同一最终版本、逐例证据摘要及问题原提出者复查；不得用本轮规划结论或另一 Harness 证据代签真实交付。

## 问题与独立复查

| ID | 严重性 | 目标版本及位置 | 原问题与依据 | 复查版本及结论 |
| --- | --- | --- | --- | --- |
| HPLAN-R001 | P2 | 本轮未提交初稿；本 change spec 的取消/关闭场景 | 初稿将取消确认与实例进程链收敛混用；session-execution 依据原生工作终态确认取消，native-session-recovery 另行约束释放 | 下方摘要固定的 spec、tests、design、plan；取消可靠结束允许常驻进程存在，实例关闭未收敛保留清理责任，011 用例断言二者独立。原提出 reviewer 复查通过，已关闭 |

没有其他未闭环规划阻断。术语沿用显式来源、原生身份、启动配置和连接代际，本轮未新建业务对象，GLOSSARY 无变化及不新增 ADR 的理由与范围相符。README 与总体文档明确 Pi/OMP 仅规划，旧 DeepSeek/ZCode 内容仅历史依据；未恢复旧 change 或旧验收结论。

## 验证状态与限制

| 检查 | 执行者 | 状态 | 证据与范围 |
| --- | --- | --- | --- |
| 六类规划、精确场景映射、工作包/任务及 Main E2E 手工检视 | 独立 reviewer | passed | 本文逐项记录，仅规划 |
| node node_modules/@fission-ai/openspec/bin/openspec.js validate --all --strict | 独立 reviewer，仓库根 | passed | 实际执行，19 passed / 0 failed，退出码 0；不证明产品语义或真实兼容 |
| git diff --check | 独立 reviewer，仓库根 | passed | 实际执行，退出码 0；README 有 Git 行尾转换提示，无 whitespace error；新未跟踪文件不由该命令完整覆盖 |
| 产品构建、Rust/Web 测试、Red Check | 未执行 | not-run | 本轮只有规划，无产品实现修改；不借旧报告填通过 |
| Pi/OMP 安装、真实协商、模型访问、Windows 清理及端到端 | 未执行 | not-run | 无本轮执行授权，固定程序与隔离条件未取得 |
| 固定依赖供应链/安全及独立产品交付验收 | 未执行 | not-run | 后续 WP1/WP4/WP5 工作，本轮只检视其安排 |

后续必须先取得新的实施授权、完成 WP1 的真实输入及方案复核。程序、适配器、配置、产品或测试变化后须重新判断证据有效性；本次没有可外推的产品测试结果。归档与归档后检查尚未执行。

## 固定版本与受审文件摘要

Git HEAD：53034ffc0c9f7644a3125788d1d2b98e51619f88。本次为该基线上的未提交规划快照，不能只凭 HEAD 重建下方新文件。除 plan 特例外均取原始文件字节 SHA-256。两份 plan 按 UTF-8 解码，将 CRLF/CR 统一为 LF，仅删除二级标题 Findings、Verification Results 及各自到下一二级标题或文件末尾的全部内容，保留其他字节语义，再以无 BOM UTF-8 计算 SHA-256；不执行 Trim。排除仅供结论状态回填，不能借此排除行为、配置、测试或工作安排变更。review.md 自身不纳入自身摘要。

| 文件 | 摘要方式 | SHA-256 |
| --- | --- | --- |
| README.md | 原始文件字节 | 53b188cd4fb95fe81fe19ef7befdeb3aec172c1d52184f3c6bce268ac8275f72 |
| docs/HARNESS_CONTROL_PLANE_REDESIGN.md | 原始文件字节 | d67da0fe87511d369e25be460523e913f6e04b017118374fef6ce000e9705073 |
| docs/REBUILD_CAPABILITY_MAP.md | 原始文件字节 | 1a5acc39961a29a28940a9cd901045f719d924ded3e3f4513ee8cbf157a4f6b7 |
| openspec/changes/integrate-pi-harness/.openspec.yaml | 原始文件字节 | db997d460a2d6d123d14faf903215b90f63faf4e7e2dadacfdab82c07857e113 |
| openspec/changes/integrate-pi-harness/proposal.md | 原始文件字节 | e570aa3860db542a8b0fc48de14b456145855e94b0a6f3c3c3c8421b2c2c53d0 |
| openspec/changes/integrate-pi-harness/design.md | 原始文件字节 | f815c77ef8530b3bcb78c8d5215baaee914ae981f43288c87394b01b5667abb7 |
| openspec/changes/integrate-pi-harness/specs/pi-harness-compatibility/spec.md | 原始文件字节 | fd580966c8c27a2bbeabfaff36ab6c4bc1ad1d98be2731d1ca14de5ff8335b92 |
| openspec/changes/integrate-pi-harness/tests.md | 原始文件字节 | a41edb366de099c6bb6f8cb9d194595f9528269d6338ddc924e083da5e5958aa |
| openspec/changes/integrate-pi-harness/plan.md | 正文规范化摘要 | af021ae4679ac76fd60be1eb0ad43c93b4227e9b749f8aacba9bb65d0dcf87a6 |
| openspec/changes/integrate-pi-harness/tasks.md | 原始文件字节 | 286f1fa6e36f5d81bd38d81003fdd4e1a62e613fcbc740ec1c7a2f4fd1c228ea |
| openspec/changes/integrate-omp-harness/.openspec.yaml | 原始文件字节 | db997d460a2d6d123d14faf903215b90f63faf4e7e2dadacfdab82c07857e113 |
| openspec/changes/integrate-omp-harness/proposal.md | 原始文件字节 | 65330d8d58254c6972c18505751f772c43c9ee9af12f71ee77861504a4444027 |
| openspec/changes/integrate-omp-harness/design.md | 原始文件字节 | 000b040514fda0dea15c383d830d0c283b4168b376f155a04d77d10f01a7799e |
| openspec/changes/integrate-omp-harness/specs/omp-harness-compatibility/spec.md | 原始文件字节 | 2b7bf5e31b50a61937e165ef744f887cbbcad3215c44251578a907dd4a6dd3ee |
| openspec/changes/integrate-omp-harness/tests.md | 原始文件字节 | 6a925b4aa2900fedfc20c48933145375813b39c18c0b198eba7117142af4e493 |
| openspec/changes/integrate-omp-harness/plan.md | 正文规范化摘要 | 4511861a3a06267a7cfa31f446bbeb8a72a9d5971556039559c43ff4088654b7 |
| openspec/changes/integrate-omp-harness/tasks.md | 原始文件字节 | 3fa9a36168116d852262edfa6ef5358e7b591a7017a09adce544838a5086d579 |

本快照同时核对以下已存在删除项；这是此前移出接入范围的工作区状态，不由本 reviewer 执行删除，也不重新验证旧方案结论：

- openspec/changes/integrate-deepseek-harness/.openspec.yaml
- openspec/changes/integrate-deepseek-harness/design.md
- openspec/changes/integrate-deepseek-harness/plan.md
- openspec/changes/integrate-deepseek-harness/proposal.md
- openspec/changes/integrate-deepseek-harness/review.md
- openspec/changes/integrate-deepseek-harness/specs/deepseek-harness-compatibility/spec.md
- openspec/changes/integrate-deepseek-harness/tasks.md
- openspec/changes/integrate-deepseek-harness/tests.md
- openspec/changes/integrate-zcode-harness/.openspec.yaml
- openspec/changes/integrate-zcode-harness/design.md
- openspec/changes/integrate-zcode-harness/plan.md
- openspec/changes/integrate-zcode-harness/proposal.md
- openspec/changes/integrate-zcode-harness/review.md
- openspec/changes/integrate-zcode-harness/specs/zcode-harness-compatibility/spec.md
- openspec/changes/integrate-zcode-harness/tasks.md
- openspec/changes/integrate-zcode-harness/tests.md

其他已有修改位于 docs/REBUILD_PLANNING_REVIEW.md 以及 agent-session-tree、backup-recovery、history-navigation、native-session-import、session-capacity、session-data-lifecycle 的既有规划；本轮保持原状，未将它们隐式纳入此专项的产品验收。
