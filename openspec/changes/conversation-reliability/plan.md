# Plan

## Execution Model

- 主 Agent 使用 omp `task` 的 batch 形状派发已满足依赖的工作包；共享背景放 context，每包一次 spawn。
- `task.isolation` 使用 patch 模式、`apply: false`；工作包不共享目录、不创建提交或分支。
- 每包交付 `<id>.patch` + `agent://<id>` + `history://<id>`。主 Agent 串行 `git apply --check`、`git apply`；仅应用独立检视通过的最终 patch。
- 仓库级验证只在主检出运行，隔离区没有 target/node_modules。每包由不同于作者的内置 reviewer spawn 检视。
- 当前规划尚未获得独立检视结论；执行前完成规划检视。原有未提交修改属于基线，禁止覆盖，不把它们当作本变更交付。

## Work Packages

| ID | Goal / Scenarios | Dependencies | Agent | Write Scope | Deliverable | Verify |
| --- | --- | --- | --- | --- | --- | --- |
| WP1 | 冻结 D1/D2 受理写集、关联和单槽位合同，消除旧文档冲突 | none | wp-coder | docs/INITIAL_DESIGN.md, docs/CORE_PORTS_AND_STORAGE.md, docs/MODULE_ARCHITECTURE.md, docs/SYNC_PROTOCOL.md, docs/FRONTEND_DESIGN.md, docs/SECURITY_DESIGN.md, docs/ACP_COMPATIBILITY_MATRIX.md, compatibility/, schemas/sync/v1/, fixtures/sync/v1/ | patch + 精确端口/DDL/身份关联及兼容表；计划 DDL 写入顶层 **§12 `sql-planned`**、计划端口写入 **§5.12 `rust-planned`**，不得写进 §7/常规 rust 块 | PV1 |
| WP2 | 所有受理输入保存、稳定回显和故障恢复 | contract:WP1 | wp-coder | crates/core/, crates/storage-sqlite/, crates/agent-host/, crates/server/, crates/node-link-client/, crates/app/tests/, docs/CORE_PORTS_AND_STORAGE.md, docs/MODULE_ARCHITECTURE.md, scripts/check-contract-drift.mjs | patch + 事务/回显/迁移测试证据 | PV1, PV2 |
| WP3 | 单一准备槽位、刷新/切换/新建复用；换 Agent 替换，显式放弃入口；换目录只能放弃后新建 | contract:WP1 | wp-coder | clients/app/src/platform/preparation-draft*, clients/app/src/features/, clients/app/app/ | patch + 单槽位状态机和平台测试 | PV3 |
| WP4 | 稳定输入替换、过程状态及原型样式 | code:WP2, code:WP3 | wp-coder | clients/app/src/features/, clients/app/src/components/, clients/app/app/, prototypes/IMPLEMENTATION-GAPS.md | patch + 页面测试和差异清单 | PV3 |
| WP5 | 跨层、真实浏览器与分段延迟验收 | code:WP4 | wp-tester | crates/app/tests/conversation_reliability_e2e.rs, clients/app/e2e/, docs/evidence/conversation-reliability.md, openspec/changes/conversation-reliability/tasks.md | patch + E1–E9 脱敏报告/实测限制 | PV1–PV5 |

WP1 已合入并检视后，WP2/WP3 可并行派发；WP4 等两者完成。WP5 不写 app 组合根；确需接线变更时先更新写范围并审查用户 compose.rs 基线，不能擅自覆盖。

## Merge Order

| Order | Work Package | Reason | Re-verify After Merge |
| --- | --- | --- | --- |
| 1 | WP1 | 合同先于代码 | PV1 |
| 2 | WP2 | 与 WP1 的 CORE_PORTS/MODULE 重叠，计划块转为实现状态 | PV1, PV2 |
| 3 | WP3 | 与 WP2 代码无重叠 | PV3 |
| 4 | WP4 | 与 WP3 的 features/app 重叠，基于其最终状态重新生成 patch | PV1, PV3 |
| 5 | WP5 | 只添加验收，主 Agent 串行更新任务勾选 | PV1–PV5 |

WP4 不与 WP3 并发写；后合入者重跑完整前端验证。实现中的规划更新由主 Agent 串行处理，不另开平行台账。

## Project Verify

| Check ID | Command / Working Directory | Scope / Pass Criteria |
| --- | --- | --- |
| PV1 | `npm.cmd run check`；仓库根 | 所有合同资产、边界、文档链接、漂移与 OpenSpec 门禁通过 |
| PV2 | `cargo test --locked -p core -p storage-sqlite -p agent-host -p server --all-features`；仓库根 | 输入受理事务、故障回滚、重复请求、回显保真、旧库升级与读取通过 |
| PV3 | `npm.cmd run check`；clients/app | 类型检查、全部组件/feature/平台测试及 Web 导出通过 |
| PV4 | `node e2e/run-e2e.mjs`；clients/app | 真实 Chromium + 假主机页面回归通过；增补 E2/E4/E6/E7/E8/E9 断言 |
| PV5 | `npm.cmd run verify`；仓库根；`cargo test --locked -p app --test conversation_reliability_e2e --all-features`；仓库根 | Rust 全门禁和真实组合根跨层套件通过；另记录 Main E2E，不将该套件冒充真实 Agent |

独立 review 只读文件与调用方，不在隔离区尝试 PV。上述后新增的测试入口由 WP5 交付；当前不存在不是已通过。

## Code Review

| Work Package | Reviewer Agent | Review Focus | Pass Condition |
| --- | --- | --- | --- |
| WP1 | reviewer（独立 spawn） | 原始四点、单槽位、端口/DDL/关联完整及兼容边界 | 无未闭环重大问题，身份关联在所有查询/事件路径可证明 |
| WP2 | reviewer（独立 spawn） | 原子受理、失败注入、回显保真、旧库、无远程持久化 | 无未闭环重大问题，读取与投影同源且副作用不重复 |
| WP3 | reviewer（独立 spawn） | 各新建入口、B 页刷新找回 A、分配并发与身份隔离 | 无未闭环重大问题，不产生第二临时会话 |
| WP4 | reviewer（独立 spawn） | 实际调用方、相同文字、不确定结果、过程反馈、原型 | 无未闭环重大问题，不靠同文或假思考填空 |
| WP5 | reviewer（独立 spawn） | 跨层/真实浏览器证据、阶段计时、日志脱敏 | E1–E9 有可读断言与限制，未跑不得勾选 |

检视输入为 patch 绝对路径及 agent/history 报告。changes-requested 使用新 spawn 修复及新 reviewer 复核，最多 3 轮；blocked 补证据后重审，不用作者自评替代。

## Main E2E

```yaml
mode: required
reason: "对话正文、过程及时性与单一临时会话均是直接用户行为"
basis: "真实浏览器连接隔离 data_dir 的真实 Daemon；可控 ACP Agent 注入边界，真实 Codex 单独验证兼容"
alternative_checks: []
```

主检出通过 omp browser prelude 完成以下断言，桌面及窄屏、明暗主题覆盖 E4/E8：

- E1：无用户回显的首条和两条后续输入，页面刷新及第二客户端读取完整；Owner 重启仍可读。
- E2：历史“继续”后再次“继续”；两个独立输入均立即可见，正式读回不增不减；同 requestId 重试只执行一次。
- E3：输入保存失败、保存后派发失败、派发边界退出与 uncertain 分别验证，无静默丢输入或自动重发。
- E4：思考、工具输入/更新/输出、授权、回复渐进显示；无首个输出有等待反馈；恢复完成/失败反馈准确。
- E5：文字与附件混合，上传/封存失败不派发，已保存输入在执行失败后保留；异内容回显不误吞。
- E6：临时 A → 正式 B → 刷新 B → 新建仍回 A；目录页刷新同样恢复唯一槽位与草稿/附件。
- E7：恢复中重复新建、并发新建、Agent 替换、显式放弃后换目录、未放弃时普通新建不改目录、迟到响应、复制标签页；均无双临时会话和跨身份接管。
- E8：与两个原型核对，折叠/键盘/滚动/输入保持可用，记录有意差异。
- E9：前台可控负载至少 30 次提交/事件，报告 D3 各段耗时及预算断言；真实 Codex 的启动与输出耗时单独列出，无法计时的上游阶段明确标为未测。

只用隔离目录和新会话，不操作用户原会话；不上传测试资料到外部服务。缺少真实 Agent 或浏览器时明确记未执行，保持任务未完成。

## Risks

- 前置生命周期/准备变更仍有待验收项，先记录基线，不顺带归档它们。
- omp 执行原语若当前宿主不可用，保留规划与未完成状态，不能用直接共享目录子 Agent 冒充隔离 patch 流程。
- 主检出用户 compose.rs 在途改动可能变化，每次合入前重核状态和 patch，不做 reset。
- 后续 wire 合同若必须扩大，WP1 先冻结完整共享资产；未闭环不得派发下游实现。
