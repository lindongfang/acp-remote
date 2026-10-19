# Plan

## Execution Model

- 按 `docs/WORKFLOW.md` 执行；本轮仅生成规划，不实现。
- 实现由主 Agent 用 omp `task` batch 派发；每个工作包为一次 `wp-coder` spawn，`isolated: true`，`task.isolation` 为 patch 模式、`apply: false`。
- 每包交付 `<id>.patch` + `agent://<id>` + `history://<id>`，不在隔离区提交或建分支；主 Agent 逐包 `git apply --check` 后串行 `git apply`。
- 仓库级检查只在主检出执行；隔离区没有 `target/` 和 `node_modules/`，不在那里跑 cargo/npm 门禁。
- 每包由另一次独立内置 `reviewer` spawn 检视，传 patch 绝对路径与报告；changes-requested 使用新 spawn 修复，最多三轮，blocked 补证据再审，不将作者自评当检视。
- 派发前记录 `conversation-page-prototype-parity` 的已检视基线和未提交文件清单；未完成旧变更合入时不得派发会覆盖它的 UI/wire 工作。先旧后新同步归档，不自动暂存、重置或覆盖用户改动。
- WP1 先固定合同；WP2 和 WP4 可基于 WP1 并行；WP3 在 WP2 后接线；WP5 在所有代码就绪后完成界面与跨层验收。每轮只 batch 派发已满足依赖的包。

## Work Packages

| ID | Goal / Scenarios | Dependencies | Agent | Write Scope | Deliverable | Verify |
| --- | --- | --- | --- | --- | --- | --- |
| WP1 | 冻结新 query/feature/限额、core 类型端口与会话根用例；覆盖认证、来源、非法输入与无副作用 | none | wp-coder | `crates/core/`, `crates/sync-protocol/`, `crates/server/src/sync/command/mod.rs`、`decode.rs`、`limits.rs`（仅为穷尽匹配补最小拒绝臂，不实现 I/O）、`schemas/sync/v1/`, `fixtures/sync/v1/`, `compatibility/commands/v1/commands.json`, `compatibility/features/v1/features.json`, `docs/SYNC_PROTOCOL.md`, `docs/CORE_PORTS_AND_STORAGE.md`, `docs/MODULE_ARCHITECTURE.md`, `docs/INITIAL_DESIGN.md`, `docs/SECURITY_DESIGN.md` §10.2 命令表（只补命令行以通过 check:commands） | WP1.patch + 报告/过程，含 fake 文件端口测试、Rust DTO/shared fixture 与文档；不新增 core 依赖，不提前宣告运行时 feature；server 最小臂在 feature 未宣告时拒绝且零 I/O | 主检出 PV1、PV2；新 query 的合法/非法 fixture、来源限制与端口零调用断言通过 |
| WP2 | 原生文件/Git 只读适配、根句柄与链接边界、内存快照、取消和进程树清理 | code:WP1 | wp-coder | `crates/app/src/session_files/`, `crates/app/src/session_files.rs`, `crates/app/src/lib.rs`, `docs/SECURITY_DESIGN.md`, `README.md` | WP2.patch + 报告/过程，含临时目录/仓库、恶意 Git 配置、Windows junction/替换/Job Object 测试与运行依赖说明 | 主检出 PV3；真实文件字节、Git 索引不变、根外拒绝与取消证据完整 |
| WP3 | Sync 入站门控/映射/限流、连接生命周期及组合根接线，完整实现才宣告能力 | code:WP1, code:WP2 | wp-coder | `crates/server/`, `crates/app/src/compose.rs`, `crates/app/src/daemon.rs`, `docs/SYNC_PROTOCOL.md` | WP3.patch + 报告/过程，含认证/撤销/旧 feature/imported/超限/断连测试，更新协议接线说明 | 主检出 PV1、PV3；真实命令经过用例端口，未协商零 I/O，断连释放资源 |
| WP4 | 客户端协议/领域/feature 状态与平台下载端口，归并会话记录、过期响应与刷新 | contract:WP1 | wp-coder | `clients/app/src/protocol/`, `clients/app/src/domain/`, `clients/app/src/features/`, `clients/app/src/platform/`, `clients/app/src/sync-client/connection.ts`（只登记 `core.session-files.v1`）、`clients/app/src/composition.ts`, `clients/app/src/composition.test.ts` | WP4.patch + 报告/过程，含共享 fixture 消费、归并/历史范围/读写状态/下载取消/迟到响应测试；无页面直接 I/O | 主检出 PV4；协议类型检查、客户端状态与平台测试通过 |
| WP5 | 文件三来源 UI、中间只读查看/关闭恢复、宽窄屏、跨层真实验收与交互文档 | code:WP3, code:WP4 | wp-coder | `clients/app/src/components/`, `clients/app/app/session/`, `clients/app/e2e/`, `crates/app/src/daemon/sync_ingress_tests.rs`, `docs/FRONTEND_DESIGN.md`, `prototypes/IMPLEMENTATION-GAPS.md`, `docs/evidence/session-file-browser.md` | WP5.patch + 报告/过程，含 UI/假主机/真实 Daemon 断言与证据；不修改设计原型来代替产品实现 | 主检出 PV3、PV4、PV5、PV6、PV7，全量验收场景通过且浏览器无未处理错误 |

## Merge Order

| Order | Work Package | Reason | Re-verify After Merge |
| --- | --- | --- | --- |
| 1 | WP1 | 冻结合同；以旧变更已检视版本为基线 | PV1、PV2 |
| 2 | WP2 | 基于 core 端口；与 WP4 无写范围重叠 | PV3 |
| 3 | WP3 | 依赖原生实现；与 WP1 重叠 `docs/SYNC_PROTOCOL.md` 及 `crates/server/src/sync/command/` 的三个文件（WP1 只放穷尽匹配的最小拒绝臂，WP3 替换为真实门控/映射），必须基于已合入 WP1 编写并保留其合同 | PV1、PV3；后合入重新核对协议文档与 fixture |
| 4 | WP4 | 基于 WP1 合同；与 WP2/WP3 无写范围重叠 | PV4 |
| 5 | WP5 | 消费全部接线；写子文件 `daemon/sync_ingress_tests.rs`，不改 WP3 的 `daemon.rs`；其余写范围不与 WP1–WP4 重叠 | PV1、PV3–PV7 |

若实现发现新共享文件，先调整本表与写范围，再派发；不能将重叠藏在目录通配中。主检出最终按工作流建分支、PR、等待必需 CI 检查和 squash，不能直推 main；本轮无提交/发布动作。

## Project Verify

所有命令仅在主检出执行；Windows 使用 `npm.cmd` 避开 PowerShell 脚本执行策略。

| Check ID | Command / Working Directory | Scope / Pass Criteria |
| --- | --- | --- |
| PV1 | `npm.cmd run check` / 仓库根 | 全部合同、边界、漂移、文档与 OpenSpec 检查通过 |
| PV2 | `cargo test --locked -p core -p sync-protocol` / 仓库根 | fake 端口与 core 用例、DTO、共享 fixture 测试通过 |
| PV3 | `cargo test --locked -p app -p server` / 仓库根 | 原生文件/Git、进程/资源释放、Sync 与真实 Daemon 入站测试通过；Windows 平台测试实际执行，不得以 Linux 跳过代替 |
| PV4 | `npm.cmd run check` / `clients/app` | TS、e2e 类型检查、单元测试与 Web 导出通过 |
| PV5 | `node e2e/run-e2e.mjs` / `clients/app` | 假主机真实组件树：三来源、中央开关、宽窄屏恢复与错误断言通过 |
| PV6 | `npm.cmd run verify` / 仓库根 | 合同 + 全 workspace fmt/clippy/test 全绿；CI 专属依赖/公告/密钥检查另等 CI，不声称本地已覆盖 |
| PV7 | omp browser prelude + 本机已就绪 Daemon / 主检出 | 真实临时目录/仓库、已认证 PWA 执行下方 Main E2E，保存每项断言与平台信息；不能以假主机结果替代 |

## Code Review

| Work Package | Reviewer Agent | Review Focus | Pass Condition |
| --- | --- | --- | --- |
| WP1 | reviewer（独立 spawn） | 新旧合同兼容、类型/端口纯度、未分组/remote、资源与 fixture 覆盖 | 无未闭环 blocker/major；新 feature 不进入 Node Link，不改变 workspace.browse |
| WP2 | reviewer（独立 spawn） | 实际打开对象边界、Windows links、危险 Git 配置、正文保密与快照释放 | 根外不能读取、仓库命令不执行、索引不变、进程树无残留，有 Windows 证据 |
| WP3 | reviewer（独立 spawn） | 读调用方与认证/撤销路径，零 I/O 门控、限流/取消与真实组合根 | 拒绝发生在 I/O 前，旧连接兼容，未完成实现不宣告 feature |
| WP4 | reviewer（独立 spawn） | DTO/领域分离、请求代次、记录归并、历史完整性与下载有界 | 迟到不覆盖，未知路径不误合并，重放不重复，内容不持久缓存 |
| WP5 | reviewer（独立 spawn） | 用户确认的完整交互、草稿/滚动/运行态、浏览器安全与真实链路证据 | 三来源与关闭行为一致，宽窄屏均通过，真实 Daemon 与假主机分层证据齐全 |

检视按工作流 per-call outputSchema 返回 pass / changes-requested / blocked；无完整 Windows 或真实 Daemon 证据时为 blocked，不冒充通过。

## Main E2E

```yaml
mode: required
```

主 Agent 在主检出通过 omp browser prelude 操作真实 PWA 与本机 Daemon，以临时工作目录及临时 Git 仓库验收，不改用户仓库内容：

1. 默认会话变更；全部文件能打开文本/图片，忽略项默认可见、开关可隐藏，`.git` 目录与 worktree 管理文件不可见。
2. 临时仓库创建新增、暂存后再修改、删除、无 HEAD 与非仓库目录，确认状态及 HEAD 合并差异准确，只有会话根内文件可见。
3. 会话产生同文件多次事件，归并后历次记录可查；其他会话/手动修改不混入；历史部分加载明确提示。
4. 写草稿、滚动离开底部、展开条目；打开 A、快速切 B、关闭恢复；Agent 期间继续运行。窄屏关闭返回原文件列表位置，跨断点不丢选中。
5. 根外 symlink/junction 条目可见不可打开；构造直接读取仍被拒；特殊文件和循环链接不挂起。Windows 主检出测试补充替换竞态与进程树证据。
6. 下载多块二进制并核对字节；文件变化、取消、断连、撤销、快照到期、大小超限均明确反馈，无不同时刻内容拼接或残留。
7. imported 与旧 feature 主机明确降级，不读取本机替代目录；Git 缺失、预览超限与恶意 HTML/SVG 不执行活动内容。

证据写入 `docs/evidence/session-file-browser.md`，标明假主机、真实 Daemon、平台和未执行项；启动成功、零退出码或截图本身不算断言通过。

## Risks

- 当前工作区有大量在途修改，实施前必须固定基线并记录。patch 失败返回作者新 spawn 修订，不能用覆盖式复制解决。
- Windows 真实链接/进程树及浏览器下载涉及平台权限；缺证据阻断对应工作包，不默认为已验证。
- Git 可执行文件不可用时测试显式降级；真实 Git 验收需要测试机具备 Git。
- 原生下载与资源限制可能导致页面 CSP 需增加 `img-src blob:`；只允许 WP3 在 `crates/server/` 范围内修改，并在 WP5 的真实托管验收中验证，不放宽脚本策略。
