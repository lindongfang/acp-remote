# 单会话客户端控制权接管与只读隔离实施计划

## Execution Model

- 执行：主执行者统筹，在编码前门禁登记 `passed` 后由独立编码会话（`coder`，不与编写 `tests.md` 的会话混用）按 `WP1`–`WP3` 完成代码实现、核心状态机与权限门禁 Red Check、单元/集成自测及实现说明文档同步；锁定单一受跟踪文件干净的本地分支集成 `commit SHA` 后，在 `WP4` 并行拉起独立 `tester` 执行正式验收与独立 `reviewer` 执行交付检视。
- 隔离：`tester` 与 `reviewer` 均使用不继承 `coder` 开发对话的独立子会话；若 `WP2`（Rust 队列/审批门禁与节点集成测试）与 `WP3`（Web 协议/工作台控制器与 Vitest 单测）并行推进，须分别使用隔离工作树及独立 `CARGO_TARGET_DIR`，合入后再统一执行单二进制内嵌构建与全量检查；`WP4` 在同一只读集成 `commit SHA` 上并行验收与检视。
- 门禁：严格执行两道独立门禁——① 编码前门禁（`node testing/suites/verify-suite.mjs --change deliver-session-control-takeover` 与 `npm run check:spec` 通过后，由独立 `reviewer` 一次性合审 `proposal.md`、`specs/`、`design.md`、`tests.md`、`plan.md`、`tasks.md` 并登记 `passed`）；② 集成交付门禁（受跟踪工作区干净且固定 `commit SHA` 后，独立 `tester` 跑通 `SCT-001`–`SCT-016` 且独立 `reviewer` 检视全量 diff 零阻断闭环）。

## Parallel Assessment

- **可并行部分**：在 `WP1` 冻结线上协议结构（`session.control.inspect/takeover/release`、`session.control.changed`、可选 `expectedControlEpoch` 及 `session_not_controlled`）与单实例 `SessionControlCoordinator` 核心端口后，`WP2`（聚焦 `app/` 队列/审批双步准入与 Rust L5 集成测试）与 `WP3`（聚焦 `clients/web/` 协议子模块、控制权横幅控制器、只读草稿保护与 Vitest 单测）写范围完全正交，可在独立工作树并行开发；`WP4` 中独立 `tester` 与独立 `reviewer` 对受跟踪文件只读，完全并行执行。
- **串行部分及原因**：`WP1` 提供单实例 `SessionControlCoordinator`、专用控制权主树根解析 `resolve_control_root`、生命周期钩子与 Rust 侧 `SessionAction` 定义，是 `WP2` 接入 `NodeRequestAuthority` / `ApprovalAuthority` 的直接编译前置，因此 `WP1 -> WP2` 串行执行；`npm run build:rust` 需将 `WP3` 编译出的前端静态产物内嵌至 `acp-remote.exe` 供 `WP4` L6 双浏览器端到端验收，故最终按 `WP1 -> (WP2 || WP3) -> WP4` 汇合。

## Work Packages

| ID | Goal / Scenarios | Dependencies | Agent | Write Scope | Deliverable | Verify |
| --- | --- | --- | --- | --- | --- | --- |
| WP1 | 完成 Rust 单实例主会话控制权协调器、专用控制权根解析、生命周期联动、安全审计与管理协议编解码（覆盖 `SCT-001`、`SCT-003`、`SCT-004`、`SCT-006`、`SCT-014`、`SCT-015` 后端状态机与协议基础）：在 `Storage`/`ApplicationPolicy`/`NodeRuntime` 组装唯一 `Arc<SessionControlCoordinator>`（按根 `NativeSessionKey` 维护进程内严格单调递增 `controlEpoch`、`evicted_epoch_floor`、`root_target_id` 缓存与有界 `(actor_id, requestId)` 幂等收据，严禁持锁跨 `.await`），实现专用 `resolve_control_root`（保持 `NativeCoordinator` 容量 `require_root` 不变）、`AuthenticatedActor` 展示名透传、`SourceSessionService` 新建主会话 `bind_created_capacity` 自动授予初始控制权、`TrustStore::revoke`（WSS `device.revoke` + CLI 命名管道，单连接 SQLite 防死锁）撤销即时归零、`NativeCoordinator::capacity_closed`（手动释放、30 分钟空闲回收、满额淘汰）与冷会话预接管超时递增代际归零、`security_audit` 追加审计，以及 `session_protocol.rs` / `management_ws.rs` 对 `session.control.inspect/takeover/release`、严格非 `null` 可选 `expectedControlEpoch` 与 `session.control.changed` 广播帧的支持 | none | coder | `app/src/access_policy.rs`、`app/src/trust.rs`、`app/src/storage.rs`、`app/src/storage/**`、`app/src/node.rs`、`app/src/application/mod.rs`、`app/src/application/session_control.rs`、`app/src/domain/**`、`app/src/application/native_dispatch/**`、`app/src/node/**`、`app/src/session_storage/**`、`app/src/management_ws.rs`、`app/src/management_ws/**` | Rust 单实例会话控制权协调器、`resolve_control_root`、四类生命周期归零钩子、`security_audit` 审计记录、管理协议命令/广播实现及 L2/L3 单测 | `cargo test --manifest-path app/Cargo.toml --lib --locked` |
| WP2 | 完成队列与严格权限审批双步写准入门禁、并发竞态防护、节点 L5 集成测试与文档同步（覆盖 `SCT-003`–`SCT-015` 节点侧全链路）：在 `NodeRequestAuthority::authorize`、`QueueService`（`input.register` 5 类动作与 `input.consume` 提交点）及 `ApprovalAuthority::authorize_decision`、`NativeApprovalService`（`approval.register` 与 `approval.decide` 提交点）接入根会话控制权与登记代际一致性核验（放行已落库 `RequestFact` / `StoredApprovalDecision` 断线重连幂等读取，未持控或跨代际未提交请求均返回 `session_not_controlled`），保持 `input.upload.*` 连接暂存、`input.queue`、`input.body.open`、`approval.list/read/status`、`view.open/open_source` 及全局容量 `native.release.*` 不受阻塞且已落库裁决继续由节点托管交付；同步适配受 BREAKING 默认只读策略影响的 `app/src/node/**` 与 `app/tests/**` 既有测试基线，编写节点实现文档并更新能力地图 | WP1 | coder | `app/src/domain/**`、`app/src/application/session_control.rs`、`app/src/application/queue_owner.rs`、`app/src/application/queue_owner/**`、`app/src/application/request_registry.rs`、`app/src/application/approval_owner.rs`、`app/src/application/native_dispatch/approval_service.rs`、`app/src/node.rs`、`app/src/node/**`、`app/src/test_support.rs`、`app/src/management_ws/session_commands.rs`、`app/tests/**`、`docs/implementation/session-control-takeover.md`、`docs/architecture/capability-map.md` | 队列/审批双步控制权门禁（含 Red Check）、并发顶号与生命周期归零 L5 集成测试、既有套件适配及 `docs/implementation/session-control-takeover.md` | `npm run check:rust` |
| WP3 | 完成 Web 管理协议扩展、控制权工作台控制器、只读草稿选区保护与独立浏览器夹具兼容（覆盖 `SCT-001`、`SCT-002`、`SCT-005`、`SCT-007`、`SCT-010`、`SCT-012`、`SCT-016` 前端部分）：新增 `clients/web/src/management/session-protocol/control.ts` 与 `clients/web/src/workbench/session-control-controller.ts`（确保全部 `session-protocol/*.ts` 与 `workbench/*.ts` 单文件 `<= 650` 行、`session-protocol.ts <= 120` 行且零循环依赖），在 `protocol.ts`、`error-messages.ts`、`primitives.ts`、`wire-codec.ts`、`client.ts` 接入控制权命令、`session.control.changed` 推送帧、严格拒绝 `null` 的可选 `expectedControlEpoch` 与 `session_not_controlled` 中文错误映射；在队列与审批面板实现只读写按钮禁用、一键接管/顶号接管与释放交互，保持 `#session-input-body` 可聚焦且重绘无损保留草稿与光标选区；更新 `management-peer.ts`（及 `FIXTURE-SHA256.md`）兼容 `WV-13`–`WV-16` 并同步 `docs/implementation/web-workbench.md` | WP1 | coder | `clients/web/src/management/**`、`clients/web/src/workbench/**`、`clients/web/src/**/*.test.ts`、`clients/web/tests/**/*.test.ts`、`clients/web/tests/e2e/**`、`clients/web/tests/browser/independent-dom/management-peer.ts`、`clients/web/tests/browser/independent-dom/FIXTURE-SHA256.md`、`docs/implementation/web-workbench.md` | Web 会话控制权协议子模块、控制权横幅与只读门禁控制器、浏览器夹具同步、Vitest 单测及 `docs/implementation/web-workbench.md` 更新 | `npm run check:web`；`npm run build:rust`；`npm run check` |
| WP4 | 完成集成交付门禁（覆盖全量 `SCT-001`–`SCT-016` 与 `SC1`–`SC4`）：在完成 `WP1`–`WP3`（及需入库的自动化验收脚本）并锁定受跟踪工作区干净的本地分支集成 `commit SHA` 后，并行执行独立 `tester` L2–L6 正式验收与独立 `reviewer` 集成 diff 检视，闭环全部阻断项并回填 `plan.md` 与 `tasks.md` | WP2, WP3 | tester + reviewer | `openspec/changes/deliver-session-control-takeover/plan.md`、`openspec/changes/deliver-session-control-takeover/tasks.md`（若需补充纳入版本库的自动化端到端验收测试则在锁定受测 `commit SHA` 前合入 `app/tests/` 或 `clients/web/tests/`；详细运行日志写本地被忽略路径 `testing/runs/deliver-session-control-takeover/`） | 独立验收报告、独立代码检视结论及门禁通过状态回填 | `PV1`–`PV5` 全绿且 `Code Review` 集成交付门禁登记为 `passed` |

## Merge Order

| Order | Work Package | Reason | Re-verify After Merge |
| --- | --- | --- | --- |
| 1 | WP1 | 先落地单实例 `SessionControlCoordinator`、专用控制权根解析 `resolve_control_root`、生命周期钩子与管理通道线上协议结构，作为后端业务门禁与前端联调基座 | `cargo fmt --manifest-path app/Cargo.toml --all --check` 与 `cargo test --manifest-path app/Cargo.toml --lib --locked` 通过 |
| 2 | WP2 | 在 `WP1` 协调器之上打通队列与审批两步写准入、并发顶号竞态拦截与节点 L5 集成回归，并完成既有 Rust 测试基线适配 | `npm run check:rust` 全绿（若遇基线并行时钟碰撞按需追加 `-- --test-threads=1` 复核并记录） |
| 3 | WP3 | 合入前端协议编解码、工作台控制权控制器、只读草稿保护及浏览器夹具更新，完成内嵌前端的 `acp-remote.exe` 完整单二进制构建 | `npm run check:web`、`npm run build:rust`、`npm run check`、`npm run check:spec` 通过 |
| 4 | WP4 | 在单一锁定集成 `commit SHA` 上完成独立 `tester` L2–L6 端到端验收与独立 `reviewer` 全量 diff 交付检视闭环 | `node testing/suites/verify-suite.mjs --change deliver-session-control-takeover`、`npm run check:spec` 与 `git diff --check` 通过 |

## Project Verify

| Check ID | Command / Working Directory | Scope / Pass Criteria |
| --- | --- | --- |
| PV1 | 仓库根目录执行 `SCT-001` 与 `SCT-002`：运行 `npm run check:web` 及静态行数/有向图 DFS 检查脚本 | `clients/web/src/management/session-protocol.ts <= 120` 行，全部 `session-protocol/*.ts`（含 `control.ts`）与 `workbench/*.ts`（含 `session-control-controller.ts`）单文件 `<= 650` 行，Web 模块环路与跨层反向依赖为 `0`；闭合协议（含显式 `null` 拒绝）、错误映射、只读按钮禁用与 `#session-input-body` 草稿/光标选区保留单测全绿 |
| PV2 | 仓库根目录执行 `SCT-001`、`SCT-003`–`SCT-015`：运行 `npm run check:rust`（含 `registration-acceptance-fixtures` 与 `acp-runtime-acceptance-fixtures` 相关回归核验） | `cargo fmt --check`、`cargo clippy -- -D warnings` 与全量 Rust 单元/集成测试退出码 `0`；`resolve_control_root` 主树继承与顶层注册目标兼容、进程内单调递增 `controlEpoch`、跨代际两步拦截、`input.upload.*` 失败自动释放零 `4400` 断连、队列交错、审批唯一裁决与托管交付、双入口撤销及回收/重启归零、`security_audit` 记录与 `node_management_audit` 冻结表无损断言全部通过 |
| PV3 | 仓库根目录执行 `SCT-002` 独立浏览器夹具回归（`WV-13`–`WV-16`）与 `FIXTURE-SHA256.md` 一致性校验 | `clients/web/tests/browser/session-view-main.ts` 在专用测试源下跑通 `WV-13`–`WV-16`（含 `WV-15` `#session-input-body` 焦点选区恢复），且 `FIXTURE-SHA256.md` 记录哈希与实际文件字节 100% 一致 |
| PV4 | 仓库根目录执行 `SCT-016`：运行 `npm run build:rust` 产出内嵌前端的真实 `acp-remote.exe`，在隔离临时数据根与随机端口拉起节点及双独立浏览器上下文（Client A / Client B）执行端到端接管、顶号移交、主动释放与重启归零验收 | 双浏览器实时同步控制权横幅与代际、只读端写按钮禁用且可旁观流式输出/队列/排队项正文/审批、顶号后队列与待决审批无缝移交且原主控未发草稿与选区保留、重启后归零为无主只读态，退出后节点进程树、Windows Jobs、浏览器实例与临时目录完全清理（缺真实 Codex 凭据时真实宿主子项如实记 `not-run`） |
| PV5 | 仓库根目录依次执行 `node testing/suites/verify-suite.mjs --change deliver-session-control-takeover`、`npm run check:spec`、`git diff --check` | 全部命令退出码 `0`，OpenSpec 规格、四列需求关联覆盖、任务双向绑定与空白字符检查全绿 |

## Code Review

| Gate / Scope | Reviewer Agent | Review Focus | Pass Condition | Status | Reviewed Snapshot / Notes |
| --- | --- | --- | --- | --- | --- |
| 编码前门禁 | 独立 reviewer | 一次性合审 `proposal.md`、`specs/`、`design.md`、`tests.md`、`plan.md`、`tasks.md`：用户确认依据完整性、线上协议动作与闭合错误码/推送帧契约、主执行树根解析与两步代际校验、SQLite 冻结表/禁触表边界、Web 650 行模块预算及 `SCT-001`–`SCT-016` 可证伪性 | `verify-suite.mjs --change deliver-session-control-takeover` 与 `npm run check:spec` 通过，且独立 reviewer 无未闭环阻断问题 | passed | 基线 `HEAD` `bc92997c` 之上规划产物经独立 `reviewer` 增量复核通过（`RV-PRE-001`–`RV-PRE-004` 全部闭环 `closed`），审查记录见本地忽略路径 `testing/runs/deliver-session-control-takeover/reviewer/pre-code-gate-review.md` |
| 集成交付门禁 | 独立 reviewer | 针对受测集成 `commit SHA` 全量 diff 审查：控制权互斥门与并发竞态安全性、核心改动 Red Check 证据、只读与上传通道零副作用、审批托管交付与撤销/释放清理、Web 模块行数与无环约束、验收代码防假阳性及 `Verification Results` 真实性 | 全部阻断问题在修复 commit 上经原独立提出者增量复核关闭 | passed | 经独立 `reviewer` 对 `33bbdaeb..9a47a199` 全量检视及 `9a47a199..ba71482f` 增量复核通过（`0 blocker`、`0 major`、`2 minor` 观察项；`RV-INT-001` 已于 `ba71482f` 闭环），记录见本地忽略路径 `testing/runs/deliver-session-control-takeover/reviewer/delivery-gate-review.md` |

## Main E2E

```yaml
mode: required
```

- **真实入口**：通过 `npm run build:rust` 编译生成内嵌生产前端静态资源的真实 `acp-remote.exe` Windows 节点二进制，启动真实 HTTPS 配对入口与 `/api/access/v1/control` TLS WSS 管理通道，由两个独立浏览器无痕上下文（Client A 与 Client B，各自生成独立 WebCrypto P-256 密钥对并完成真实配对入网）驱动完整 Web 工作台（对应 `SCT-016`）；Harness 侧在具备本机认证环境时接入真实 Codex Harness，同时在受控标准 ACP V2 对端上覆盖确定性顶号交错、待决审批移交与重启归零（若缺真实 Codex 凭据则真实宿主子项如实记 `not-run`，不以模拟对端冒充真实宿主）。
- **数据隔离**：每次运行创建独占临时目录（如系统临时根下的带唯一运行标识子目录）承载节点 SQLite 数据库、DPAPI 测试保护材料、工作空间沙箱与双浏览器独立 Profile，绑定 `127.0.0.1:0` 随机空闲端口，绝不触碰用户真实 `%LOCALAPPDATA%` / 默认 HOME 数据。
- **清理验证**：测试收尾（含异常中断 `finally` 分支）显式关闭双浏览器上下文、优雅停止 `acp-remote.exe` 并核验其受管 Windows Job 子进程树全部退出、监听端口释放、SQLite 句柄关闭且独占临时目录物理删除无残留。

## Risks

- **[管理协议闭合解析导致未知错误码或推送帧误断连风险]**：`clients/web/src/management/protocol.ts` 对 `ManagementErrorCode`、`CommandResultFrame.error`（仅允许 `["code"]` 单键）及 `parseManagementFrame`（`frame.type`）执行严格闭合校验，一旦遗漏同步会导致客户端以 `4400` 主动断开整条管理连接。缓解：在 `WP1` 与 `WP3` 严格保持错误对象 `{ "code": "session_not_controlled" }` 单键结构，同步注册 `"session.control.changed"` 帧并在 `SCT-001` 双端往返单测锁死。
- **[连接私有 `ACPI` 分块上传在顶号瞬间触发 `4400` 断连风险]**：若在 `input.upload.*` 或 `ACPI` 二进制帧层拦截控制权，按 `management-input-upload` 规范二进制业务错误将以 `4400` 关闭原客户端连接，破坏原主控平滑退回只读态体验。缓解：保持连接私有上传暂存层不变，统一在队列受理入口（`input.register` / `input.consume`）拦截并返回 JSON 错误 `session_not_controlled`，复用前端既有 `releaseInputUpload` 异常清理分支零断连回收暂存槽位（`SCT-005` 专项验证）。
- **[Web 控制器 650 行硬上限与 `WV-15` 焦点选区回归风险]**：`source-session-controller.ts`（637 行）、`session-queue-controller.ts`（604 行）与 `management-workbench.ts`（599 行）余量紧张，且若将 `#session-input-body` 设为 `disabled = true` 会导致浏览器无法聚焦从而破坏 `WV-15` 选区恢复与本地草稿编辑。缓解：新增独立 `session-protocol/control.ts` 与 `workbench/session-control-controller.ts` 承载控制权逻辑，只读态仅禁用写动作 `<button>` 而保持 `#session-input-body` 可聚焦（`SCT-002` 与 `PV1`/`PV3` 强制校验）。
- **[BREAKING 默认只读规则对既有单端测试基线的影响]**：历史集成测试若绕过 `source.session` `create` 直接写入 `product_targets` 顶层目标并提交输入或裁决审批，经 `resolve_control_root` 识别为顶层目标后在默认无主只读策略下将被 `session_not_controlled` 拒绝。缓解：在 `WP2` 梳理 `app/src/node/**` 与 `app/tests/**` 受影响测试夹具，显式先执行 `session.control.takeover` 准备控制权，同时新增专向拒绝测试执行 Red Check。

## Handoff

- **基线分支与快照**：当前分支 `v2`，规划基线提交 `bc92997c`（受跟踪工作区干净）；正式实施须在编码前门禁更新为 `passed` 后，在全新 `coder` 会话中启动（遵循 `openspec/config.yaml` 编写 `tests.md` 与 `apply` 编码分会话硬约束）。
- **权威输入路径**：[proposal.md](proposal.md)、[design.md](design.md)、[tests.md](tests.md)、[tasks.md](tasks.md)、[session-control-takeover 增量规格](specs/session-control-takeover/spec.md)、[session-execution 增量规格](specs/session-execution/spec.md)、[permission-approvals 增量规格](specs/permission-approvals/spec.md)、[AGENTS.md](../../../AGENTS.md)、[硬规则清单](../../../docs/hard-rules.md)、[总体架构](../../../docs/architecture/overview.md)。
- **读写边界**：`coder` 严格按 `WP1`–`WP3` 声明的 `Write Scope` 修改 `app/`、`clients/web/` 与 `docs/`，严禁触碰 `node_management_audit` 冻结表结构、严禁创建 `session_commands` 表、严禁修改 `.agents/`；`tester` 与 `reviewer` 在门禁阶段对产品源码只读，详细运行与检视原始记录写入本地被忽略目录 `testing/runs/deliver-session-control-takeover/`。
- **验证入口**：见 `## Project Verify` 表中 `PV1`–`PV5`。

## Findings

| ID | Source | Severity | Commit SHA | Finding | Fix Commit | Status | Retest / Re-review |
| --- | --- | --- | --- | --- | --- | --- | --- |
| RV-PRE-001 | reviewer | blocker | bc92997c | `design.md` §1.1/§Risks、`tasks.md` 1.2、`tests.md` `SCT-004` 要求所有控制权与队列/审批入口直接调用 `SessionTreeRepository::resolve_session_tree` 校验 `node_session_capacity.root_verified = 1`，但 `session_capacity_schema.rs` 强制 `CHECK(root_verified=0 OR (native_id IS NOT NULL AND create_operation IS NOT NULL))`，导致 CLI `target.register`、Web `discover -> target.register`（含 `WV-16`）及全仓既有直接写入 `product_targets` 的测试目标全部因 `session_tree_unverified` 失败（含 `design.md` 第 101 行的测试适配方案自身）；需在不削弱容量加载 `require_root` 的前提下为控制权定义专用主树根解析（存在父链时强制顶层祖先 `root_verified = 1`；无父链顶层目标若无仅 `root_verified = 0` 的未验证记录则以自身作为根键） | working-tree | closed | 独立 reviewer 增量复核确认：`design.md` §1.1/§Risks、`tasks.md` 1.2 与 `tests.md` `SCT-004` 已定义并覆盖 `resolve_control_root` 三分支且保持容量 `require_root` 不变，闭环关闭 |
| RV-PRE-002 | reviewer | blocker | bc92997c | `design.md` §2.1/§2.2/§3.3 与 `tests.md` `SCT-015` 在进程内 `capacity_closed`/源关闭/冷会话预接管超时后删除记录将 `controlEpoch` 倒退归零为 `"0"`，违反 `specs/session-control-takeover/spec.md`“单调递增控制代际”要求，在同进程“释放后再由原主控接管（`1 -> 0 -> 1`）”时产生旧两步请求与旧 `expectedControlEpoch` 的 ABA 绕过，并导致前端 `session.control.changed` 与并发 `session.control.inspect` 单调收敛竞态；同一 `NodeRuntime` 进程内释放归零须仅清空 `controllerClientId` 并单调递增 `controlEpoch`（若淘汰条目须保留全局代际下界水位），仅从未受控会话与节点进程重启后为 `"0"` | working-tree | closed | 独立 reviewer 增量复核确认：`design.md` §2.1/§2.2/§3.3/§4.1、`tasks.md` 1.1/1.2/3.2 与 `tests.md` `SCT-008`/`SCT-015` 已冻结同进程单调递增与 `evicted_epoch_floor` 水位及 ABA 拦截断言，闭环关闭 |
| RV-PRE-003 | reviewer | blocker | bc92997c | `plan.md` `WP1`–`WP3` 的 `Write Scope` 含不存在目录 `app/src/application/request_registry/*`（实际为 `request_registry.rs`），且遗漏 `app/src/node.rs`、`app/src/application/mod.rs`、`app/src/application/queue_owner.rs`、`app/src/application/native_dispatch/source_sessions/operations.rs`、`app/src/node/session_services.rs`、承载既有节点队列/WSS/审批测试的 `app/src/node/**` 及 `clients/web/src/**/*.test.ts`，按此写边界执行无法完成编译与 `npm run check:rust` | working-tree | closed | 独立 reviewer 增量复核确认：`plan.md` `WP1`–`WP3` `Write Scope` 已修正为真实模块路径与递归通配，且 `WP2` 与 `WP3` 保持正交零冲突，闭环关闭 |
| RV-PRE-004 | reviewer | major | bc92997c | `design.md` 需明确冻结三项实现安全契约：① `SessionControlCoordinator` 在 `NodeRuntime`/`ApplicationPolicy`/`Storage` 层保持单实例以覆盖 `NodeRuntime::request_authority()` 与 `approval_owner()` 组件入口；② `TrustStore::revoke` 在单连接 `SqliteRequestRegistry` 下严禁在归还连接前新开 SQLite 连接或持协调器锁跨 `.await`（缓存 `root_target_id` 并复用原事务或归还后写审计），且队列/审批两步代际复核落在提交点同时放行已落库结果的幂等读取；③ `expectedControlEpoch` 在 Rust/Web 闭集解析器中仅允许省略或合法十进制 `u64` 字符串（显式 `null` 与未知键均拒绝） | working-tree | closed | 独立 reviewer 增量复核确认：`design.md` §1.1/§2.1/§2.3/§3.2/§4.1、`tasks.md` 1.1–3.1 与 `tests.md` `SCT-001`/`SCT-014` 已完整冻结三项安全与协议闭合契约，闭环关闭 |
| RV-INT-001 | reviewer | major | 9a47a199 | `NativeApprovalService::decide` 在释放 `slot.state` 锁后进入 `source.jobs.spawn_guarded` 落库前未再次复核 `verify_approval_decide`，存在释放锁至 `approval_begin` 获取 SQLite 事务锁窗口内的跨代际落库竞态 | ba71482f | closed | 在 `spawn_guarded` 任务内部落库前执行 `authority.verify_approval_decide` 二次复核，并新增并发回归用例 `sct_rv_int_001_approval_decide_rechecks_control_before_commit_and_releases_admission` 经独立 `reviewer` 与 `tester` 增量复验关闭 |
| ACT-001 | tester | blocker | 9a47a199 | `SCT-016` 双浏览器 E2E 中 `pageA` 首条 `first streaming prompt` 短暂处于等待队列时提前放行，`pageB` 在第二条 `input.consume` 落库前读到空等待队列导致超时 | ba71482f | closed | `sct-016-browser-driver.mjs` 改为等待 `pageA` 呈现 2 个队列条目并在 `pageB` 轮询刷新等待队列，连续两次实跑通过（6.23s、5.78s） |
| ACT-002 | tester | major | 9a47a199 | `sct_red_check_approval_uncontrolled_and_cross_epoch_two_step_rejected` 仅等待审批确认而未等待 `discover` 伴随 `node_session_parents` 父链落库导致串行偶发 `session_not_controlled` | ba71482f | closed | `session_control_takeover_acceptance.rs` 与 `session_control_lifecycle_acceptance.rs` 补充 `resolve_control_root` 就绪等待，`r3_source_wss_development::sct` 8/8 全绿 |
| RV-INT-002 | reviewer | minor | 9a47a199 | `session_commands.rs` 与 `source_sessions/operations.rs` 中控制权切换先修改内存/广播后再异步写 `security_audit` | — | open-minor | 非阻断 `minor` 观察项，记入交付检视报告供后续演进 |
| RV-INT-003 | reviewer | minor | 9a47a199 | 仅点击“打开共享视图”而未“载入队列”时审批控制权横幅停留在“正在读取会话控制权…”（写按钮保持闭锁禁用） | — | open-minor | 非阻断 `minor` 观察项，记入交付检视报告供后续演进 |

## Verification Results

- 受测版本：本地分支 `v2` 集成提交 `ba71482fa339094d7ffa02e0c48f661bd0cc573e`（主实现与 WP4 门禁）及 `593ec34d8a7588e42b15d632c87d1ca3bca06cfc`（补充真实 Codex Harness `SCT-016` E2E 测试，受跟踪工作区干净）
- 执行者与环境：独立 `tester`（Windows 11 / PowerShell 5.1 / `CARGO_BUILD_JOBS=2`，清除继承 `NoDefaultCurrentDirectoryInExePath` / Edge Headless + Playwright + Rust debug 二进制 `acp-remote.exe`），完整记录见本地忽略路径 `testing/runs/deliver-session-control-takeover/tester/acceptance-report.md`、`testing/runs/deliver-session-control-takeover/coder/sct-016-e2e.json` 与 `testing/runs/deliver-session-control-takeover/coder/sct-016-real-codex-e2e.json`
- 验证命令及退出码：
  - `PV1`：`npm run check:web`（`tsc --noEmit`、Vitest 43 文件 / 1054 用例、`vite build`）及前端静态行数/零环路脚本 -> `0`（继承自 `9a47a199`，后续提交未改 Web 生产源码）
  - `PV2`：`npm run check:rust` -> `0`（`cargo fmt --all --check`、`cargo clippy --all-targets --locked -- -D warnings`、`lib` 1055 passed、`main` 2 passed 及全部默认集成测试通过）；`--features native-dispatch-dev-fixtures` `r3_source_wss_development::sct -- --test-threads=1` -> `0`（8 passed, 0 failed）；`registration-acceptance-fixtures`（56 passed）与 `acp-runtime-acceptance-fixtures`（50 passed）-> `0`（继承自 `9a47a199`）
  - `PV3`：`node testing/runs/deliver-session-control-takeover/coder/run-wv13-16.mjs` 与 `FIXTURE-SHA256.md` 校验 -> `0`（`WV-13`–`WV-16` 全过，继承自 `9a47a199`）
  - `PV4`：`cargo test --manifest-path app/Cargo.toml --test session_control_e2e_acceptance --locked -- --ignored --nocapture` -> `0`（2 passed, 0 failed，耗时 19.17s，覆盖受控 ACP V2 与真实 `@agentclientprotocol/codex-acp` + `codex.exe`）
  - `PV5`：`npm run check:spec`、`node testing/suites/verify-suite.mjs --change deliver-session-control-takeover`、`git diff --check` -> `0`

| Case ID | Executor | Commit SHA | Status | Evidence / Limitations |
| --- | --- | --- | --- | --- |
| SCT-001 | tester | ba71482f | passed | Rust `session_protocol::tests`（`npm run check:rust`）与 Web `session-control.test.ts`（`npm run check:web`）通过 |
| SCT-002 | tester | ba71482f | passed | `session-control.test.ts`、`workbench-controllers.test.ts`、`run-wv13-16.mjs`（`WV-13`–`WV-16`）与静态行数/零环路检查通过 |
| SCT-003..SCT-015 | tester | ba71482f | passed | `npm run check:rust`、`queue_u2_`（8 passed）、`sctm_*` + `sctl_*` + `sct_red_check_*` + `sct_rv_int_001_*`（`native-dispatch-dev-fixtures` 22 passed）全绿 |
| SCT-016 (Controlled ACP V2) | tester | 593ec34d | passed | `session_control_e2e_acceptance` 真实 `acp-remote.exe` + 双独立 Edge 上下文通过（`sct-016-e2e.json`） |
| SCT-016 (Real Codex Harness) | coder (user-authorized) | 593ec34d | passed | 经用户授权在隔离临时 `CODEX_HOME` 下使用本机 `codex.exe` + `@agentclientprotocol/codex-acp` 2.5.0 跑通双浏览器接管、草稿保护与重启归零并验证临时目录全清（`sct-016-real-codex-e2e.json`） |

- 未执行项与已知限制：`r3_source_wss_development::sct` 等 L5 验收处于 `native-dispatch-dev-fixtures` 特性门控下，日常仅执行默认 `npm run check:rust` 不会自动触发该扩展套件；真实 Codex Harness 验收依赖本机已安装 `codex.exe`、`@agentclientprotocol/codex-acp` 与用户 `~/.codex/auth.json`（测试期间仅复制至独占临时目录并在结束后自动物理删除）。
- 问题关闭依据：`RV-PRE-001`–`RV-PRE-004` 于基线 `bc92997c` 关闭；`RV-INT-001`、`ACT-001`、`ACT-002` 于修复提交 `ba71482f` 经原独立 `reviewer` 与 `tester` 增量复核全部关闭（记录见 `testing/runs/deliver-session-control-takeover/reviewer/delivery-gate-review.md` 与 `testing/runs/deliver-session-control-takeover/tester/acceptance-report.md`）。
