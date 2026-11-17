# Pi 规划独立检视

## 结论与范围

2026-11-16，独立 `tester` 与独立 `reviewer` 在不继承开发对话的只读子会话中对基于 Git `65d734f638bcb951e237b69faa592917b3a6d2c7` 的二轮规划修订完成用例预期核验与全量规划复查；2026-11-17 按用户最新确认“先去掉 omp 的适配吧，暂时先支持 codex 和 pi”移除 `integrate-omp-harness` 并同步收敛本变更及顶层文档范围。结论：**本轮规划修订通过；产品实施与本机真实 Pi 兼容验收未完成。** 首轮 `HPLAN-R001`、源码对照审查提出的 `HPLAN-R002`–`HPLAN-R006` 以及二轮独立检视提出的 `HPLAN-R007`–`HPLAN-R009` 均已逐项核实关闭，无剩余规划阻断问题。本结论仅覆盖 `SC1` 规划修订范围，不授权自动安装或改动用户本机 `pi`、不代表 `WP1` 真实环境取证已完成、不宣称 `SC2`–`SC4` 已达成。

依据为用户各轮原话（审查 `integrate-pi-harness` 并修复 5 个问题；“本地已经安装了pi。但是当前项目不要绑定pi的版本”；“先去掉omp的适配吧，暂时先支持codex和pi”）、`AGENTS.md`、`openspec/config.yaml`、`openspec/role/{coder,tester,reviewer}.md`、`docs/code-standards.md`、`LESSONS.md`、`docs/HARNESS_CONTROL_PLANE_REDESIGN.md` 及相关长期规格（`harness-management`、`native-session-recovery`、`acp-platform-constraints`、`session-execution`、`structured-history`、`permission-approvals`、`management-input-upload`）。实际核对真实源码边界：`app/src/adapters/acp/node_configuration_profile.rs`、`app/src/domain/native_catalog.rs`、`app/src/adapters/acp/strategy.rs`、`app/src/adapters/acp/transport/state.rs`、`app/src/platform/process_owner.rs`、`clients/web/src/management/workspace-catalog-view.ts` 及根 `package.json`。

本轮严格遵循“不绑定 `pi` 版本”硬约束：用户本机已安装 `pi`，项目任何 `package.json`、锁文件、配置模板、探针脚本与运行时代码均不声明、锁定或强校验特定 `pi` 版本；节点仅存储单一 ACP 对端启动配置并按当前连接动态协商，单次兼容性报告只读记录当次实测观察到的本机 `pi` 与外部 ACP 服务端版本供证据归属追溯。

## 需求与用例逐项核对

已人工对照本 change 增量文件 `specs/pi-harness-compatibility/spec.md` 中的 6 个 Requirement、12 个 Scenario：全部英文规范关键词（`SHALL`、`MUST`）两侧均留有单个空格，未写入内部类名或函数名，且精确名称由 `tests.md` 下表完整覆盖。13 个稳定用例（`PI-001`–`PI-013`）保留原编号与标题，覆盖正常、配置/路径拒绝、反向委托 `-32601`、组件升级重新协商、底层配置与通用 Web 表单分层保真、恢复/映射失效、断线审批、取消竞态与 Job 进程树收敛、缺环境阻断及用户数据零改动路径。裸 `specs/...` 路径仅指向本次增量，长期规范使用显式 `openspec/specs/...` 路径，无跨目录补位。

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

各用例边界与真实源码完全对齐：
- `PI-001` 区分当前规划/无版本绑定检查与后续 `WP1` 真实取证，未将规划通过冒充产品验收通过；`PI-002` 引用根 `npm run check` 与 `npm run build:rust` 及适用 Red Check。
- `PI-003`、`PI-004` 对齐 `OwnedProcess::spawn` 的绝对路径前置校验与无隐式 Shell `CreateProcessW` 行为，使用 Windows 绝对路径原生 PE 可执行文件启动，并将相对路径可执行名与 `.ps1` 脚本入口纳入明确拒绝断言；节点保存单一启动入口，双组件观测版本仅由验收报告只读记录。
- `PI-005`、`PI-010` 对齐 `Strategy::initialize` 发送空客户端能力（`clientCapabilities: {}` / `capabilities: {}`）与 `reserve_unsupported` 对非 `session/request_permission` 反向请求返回 `-32601 Method not found` 的真实实现。
- `PI-007` 区分底层 `format: 1` 配置（含 `validate_setup_caps` 能力位核对下的 `mcp_servers`/`extra` 保真）与当前通用 Web 表单（`parseLaunchArgsLines` 按行 `trim`、硬编码 `mcpServers: []`/`extra: {}`）的两层范围。
- `PI-013` 要求真实 Windows 节点、真实浏览器、本机已安装的真实 `pi`（不改版本）、真实外部 ACP 服务端、`.tmp/` 隔离原生会话及所属 Job 进程树全部真实，未设 `LX`。

## 执行安排核对

- `plan.md` 必备节齐全，`Work Packages` 为标准 7 列表，`WP1`–`WP5` 唯一且写范围两两互斥：`WP1` 含 `testing/probes/pi-harness/discovery/` 承载候选适配器隔离探针（不安装或锁定 `pi`），`WP2`（节点模板/探针/说明）与 `WP3`（Web 测试/说明）完全互斥可隔离并行。
- 全部拟建产物收敛到仓库既有目录体系（`testing/probes/pi-harness/`、`docs/implementation/`、`testing/runs/integrate-pi-harness/`、`openspec/specs/pi-harness-compatibility/`、`testing/suites/pi-harness-compatibility/`、`testing/suites/traceability/`），不再引入不存在的顶层 `configs/`、`scripts/`、`testing/harnesses/`、`docs/harnesses/`。
- `WP5` 与 `tasks.md` `5.2` 同包同步 `openspec/specs/pi-harness-compatibility/spec.md`、`testing/suites/pi-harness-compatibility/tests.md` 与 `testing/suites/traceability/integrate-pi-harness.md`，满足长期套件裸路径指向长期规范、禁止跨目录补位的规则；共享文档（`README.md`、总体设计、能力地图）在 `Merge Order` 声明串行合入并保护主工作区其他在途变更。
- `Main E2E` 仅含单个 YAML 块且唯一字段 `mode: required` 为字符串，指向 `PI-013` 真实闭环，未因本轮不执行而改为 `not-applicable`。
- `tasks.md` 共 13 个未勾选真实 checkbox（`1.1`–`5.3`），每项同一行包含明确完成判据并完整引用 `WP1`–`WP5`，无示例 checkbox 或越界未声明工作；归档与归档后检查留在非 tracked `Workflow follow-up`。

## 问题与独立复查

| ID | 严重性 | 目标版本及位置 | 原问题与依据 | 复查版本及结论 |
| --- | --- | --- | --- | --- |
| HPLAN-R001 | P2 | 首轮未提交初稿；`spec.md` 取消/关闭场景 | 初稿将取消确认与实例进程链收敛混用，偏离 `session-execution` 与 `native-session-recovery` 边界 | 首轮及下方最新快照均保持取消终态与实例关闭收敛独立判定（`PI-011`）；原独立 reviewer 复查通过，已关闭 |
| HPLAN-R002 | P1 | `65d734f6` 规划版；`proposal`/`spec`/`design`/`tests` `PI-004` | 要求系统区分记录双组件身份与固定版本，与单一 `launch`/`HarnessSummary` 代码模型及用户“不绑定 `pi` 版本”要求冲突 | 下方摘要快照；节点仅存单一启动配置并动态协商，全链路移除 `pi` 版本绑定，报告只读记录当次观测双组件版本。独立 tester/reviewer 复核通过，已关闭 |
| HPLAN-R003 | P1 | `65d734f6` 规划版；`design`/`tests` `PI-003` | 拟选 `.ps1`/`.cmd` 启动链，与 `OwnedProcess::spawn` 绝对路径前置校验及无隐式 Shell `CreateProcessW` 实现直接冲突 | 下方摘要快照；改为 Windows 绝对路径原生 PE 启动，并将相对路径名与 `.ps1` 绝对路径列为 `PI-003`/`PI-004` 明确拒绝断言。独立 tester/reviewer 复核通过，已关闭 |
| HPLAN-R004 | P1 | `65d734f6` 规划版；`spec`/`design`/`tests` `PI-005`/`PI-007`/`PI-010` | 未区分底层 `format: 1` 配置与当前硬编码 `mcpServers: []`/`extra: {}` 且按行 `trim` 的通用 Web 表单，也未覆盖 `clientCapabilities: {}` 与反向 `-32601` 边界 | 下方摘要快照；明确两层配置与表单范围限制，覆盖 `validate_setup_caps`、空客户端能力及非审批反向请求 `-32601` 断言。独立 tester/reviewer 复核通过，已关闭 |
| HPLAN-R005 | P2 | `65d734f6` 规划版；`design`/`plan`/`tasks` | 拟建 `configs/`、`scripts/`、`testing/harnesses/`、`docs/harnesses/` 偏离仓库目录约定，且缺长期套件同步写范围 | 下方摘要快照；统一收敛至 `testing/probes/pi-harness/`、`docs/implementation/`、`openspec/specs/`、`testing/suites/`。独立 reviewer 复核通过，已关闭 |
| HPLAN-R006 | P2 | `65d734f6` 规划版；`plan`/`tasks` `WP1` | `WP1` 写范围不含隔离探针目录，无法在不改根依赖且不绑 `pi` 版本的前提下实测候选适配器 | 下方摘要快照；`WP1` 增补互斥写范围 `testing/probes/pi-harness/discovery/` 且明确不安装/不锁定 `pi`。独立 reviewer 复核通过，已关闭 |
| HPLAN-R007 | P2 | 二轮修订初稿；`spec.md:65` | `版本。MUST 不绑定` 中 `MUST` 左侧紧邻中文句号缺空格，违反 `openspec/config.yaml` 规范关键词两侧留空格规则 | 下方摘要快照；已修正为 `版本；验收与运行时 MUST 不绑定`，全文 `SHALL`/`MUST` 两侧空格逐处复核无误。原提出独立 reviewer 复查通过，已关闭 |
| HPLAN-R008 | P2 | 二轮修订初稿；`plan.md` `WP5`、`tasks.md` `5.2` | `WP5` 纳入 `testing/suites/pi-harness-compatibility/` 但将新能力 `openspec/specs/` 同步留在 `Workflow follow-up`，违反长期套件禁止跨目录补位规则 | 下方摘要快照；`WP5` 写范围与 `5.2` 已同包同步 `openspec/specs/pi-harness-compatibility/spec.md` 与 `testing/suites/`。原提出独立 reviewer 复查通过，已关闭 |
| HPLAN-R009 | P2 | 二轮修订初稿；`review.md` | `review.md` 仍停留在旧 `53034ffc` 快照摘要，与基于 `65d734f6` 重写的六类规划产物不一致 | 本文及下方最新 SHA-256 快照；已对齐 `65d734f6` 基线及移除 OMP 后的最新修订产物，已关闭 |

## 验证状态与限制

| 检查 | 执行者 | 状态 | 证据与范围 |
| --- | --- | --- | --- |
| 六类规划、精确场景映射、无版本绑定约束、源码边界与工作包/任务手工检视 | 独立 tester、独立 reviewer（只读子会话）+ 2026-11-17 范围移除同步复核 | passed | 独立核对本变更七份文件、长期规格与真实 Rust/Web 接入源码，并同步移除 `integrate-omp-harness` 交叉引用；仅覆盖规划一致性 |
| `node node_modules/@fission-ai/openspec/bin/openspec.js validate --all --strict` | 主执行者（仓库根） | passed | 主执行者实跑退出码 0（18 passed / 0 failed，移除 OMP 后共 18 项）；独立子会话因只读沙箱限制在自身会话内记 CLI 执行为 `not-run` |
| `git diff --check -- openspec/changes/integrate-pi-harness README.md docs/HARNESS_CONTROL_PLANE_REDESIGN.md docs/REBUILD_CAPABILITY_MAP.md` | 主执行者（仓库根） | passed | 实际执行退出码 0，无空白或冲突标记错误 |
| 产品构建、Rust/Web 测试、Red Check | 未执行 | not-run | 本轮仅修订规划与文档范围，未修改产品源码；不借其他在途变更或旧报告填通过 |
| 本机 `pi` 探针取证、真实 ACP 协商、模型访问、Job 进程树清理及 `PI-013` 端到端 | 未执行 | not-run | 本轮无真实运行与实施授权，`WP1` 实证输入尚未采集 |
| 候选外部适配器许可证/维护/安全审查及独立产品交付验收 | 未执行 | not-run | 后续 `WP1`/`WP4`/`WP5` 工作，本轮仅检视其门禁与隔离安排 |

后续进入实施前必须先取得实施授权并完成 `WP1` 本机 `pi` 只读取证与候选适配器审查。运行时不绑定 `pi` 版本，但本机 `pi`、外部适配器、配置或产品代码变化后，既有测试证据仅归属原观测组合，须按受影响范围重新判断有效性。

## 固定版本与受审文件摘要

Git HEAD：`65d734f638bcb951e237b69faa592917b3a6d2c7`。本次受审范围为该基线上移除 `openspec/changes/integrate-omp-harness/` 后的 `openspec/changes/integrate-pi-harness/` 规划快照。除 `plan.md` 特例外均取原始文件字节 SHA-256；`plan.md` 按 UTF-8 解码并将 `CRLF`/`CR` 统一为 `LF`，仅删除二级标题 `## Findings`、`## Verification Results` 及各自到下一二级标题或文件末尾的全部内容，保留之前字节语义（不执行 `Trim`），再以无 BOM UTF-8 计算 SHA-256。`review.md` 自身不纳入自身摘要。

| 文件 | 摘要方式 | SHA-256 |
| --- | --- | --- |
| openspec/changes/integrate-pi-harness/.openspec.yaml | 原始文件字节 | db997d460a2d6d123d14faf903215b90f63faf4e7e2dadacfdab82c07857e113 |
| openspec/changes/integrate-pi-harness/proposal.md | 原始文件字节 | cc9fe5cc272b81f91b6c7b3fddde4a9ddb957e1e401ad49c73c26ce0c879302d |
| openspec/changes/integrate-pi-harness/design.md | 原始文件字节 | 20eb7654238d53df263f33a00e5143cd020753e798a774ab271ec3807a625bac |
| openspec/changes/integrate-pi-harness/specs/pi-harness-compatibility/spec.md | 原始文件字节 | 6f25f4f22d660e364703b7b08b59f291c4850eafbf2889b6bfd48ae3e022c9a8 |
| openspec/changes/integrate-pi-harness/tests.md | 原始文件字节 | 5528870bbb6a50b492858352c82c87205caf06bb3ec9484f647368ddd5095eef |
| openspec/changes/integrate-pi-harness/plan.md | 正文规范化摘要 | c116ac5050e281dac9d82bf852555dfdf0d2d184294f1795b38b598b92d18817 |
| openspec/changes/integrate-pi-harness/tasks.md | 原始文件字节 | d03389c07d1cc71f659765b68eebb4ea7100b18fdf7722d2953ee815a3cbec84 |

本快照同时确认以下目录已按 2026-11-17 用户要求从活跃变更中移除（OMP 暂缓适配）：
- `openspec/changes/integrate-omp-harness/`（含 `.openspec.yaml`、`proposal.md`、`design.md`、`specs/omp-harness-compatibility/spec.md`、`tests.md`、`plan.md`、`tasks.md`、`review.md`）

主工作区当前同时存在 `deliver-session-capacity` 等其他在途变更的未提交源码与规划修改（`app/`、`clients/web/`、`openspec/changes/deliver-session-capacity/` 等，且 `README.md`、`docs/HARNESS_CONTROL_PLANE_REDESIGN.md`、`docs/REBUILD_CAPABILITY_MAP.md` 已在完整保留其在途容量修改的前提下同步收敛 Codex + Pi 目标范围）；本轮对在途产品源码保持零触碰，不将其纳入本专项快照签署范围。
