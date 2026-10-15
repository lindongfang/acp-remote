<!-- 执行安排：把 specs 的行为要求与 design 的技术方案转成可派发的并行工作包。
     执行原语固定为 omp：batch task 派发、patch 模式隔离、`<id>.patch` 交付、主 Agent 串行 git apply。 -->

# Plan

## Execution Model

- 派发：主 Agent 用 `task` 工具的 batch 形状一次性扇出多个工作包；每个工作包是**一次 spawn**。
- 隔离：`task.isolation`（`patch` 模式、`apply: false`）。工作包之间不共享目录。
- 交付：`<id>.patch` + `agent://<id>` + `history://<id>`；工作包不产生 commit、不产生分支。
- 合入：主 Agent 按 `## Merge Order` 串行 `git apply`，每次合入后跑主检出 Project Verify。
- 验证：隔离工作区内**没有** `target/` 与 `node_modules/`，因此仓库级检查只在主检出执行。
- 检视：每个工作包由一个**独立** spawn（不与作者同实例）读 patch 与 `agent://<id>` 后给出结构化结论。
- **前置**：`sync-server-connection` 必须先合入（本变更复用它的连接面、投递队列与模块骨架）。

## Work Packages

| ID | Goal / Scenarios | Dependencies | Agent | Write Scope | Deliverable | Verify |
| --- | --- | --- | --- | --- | --- | --- |
| WP1 | 事件视图投影与 ACP 原文装配（覆盖 `sync-event-projection` 全部要求）：调 `sync-protocol::views::project` 做形状判定、装配 `payload.acp` 并处理不可用标记、未登记判别子显式降级 | none（前置变更已提供模块骨架） | wp-coder | `crates/server/src/sync/event/`, `crates/server/src/sync/mod.rs`（仅追加模块声明行） | patch + 「视图判定失败仍下发的落地方式、原文逐字节保真的实现、256 KiB 与 media type 口径的来源、降级路径」 | 主检出 `cargo test -p server`；`cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` |
| WP2 | 命令管线骨架与查询类命令（覆盖 `sync-command-handling`「查询命令同步完成」「逐命令的 scope 授权与零副作用拒绝」「命令速率限流与超限关闭」）：命令解码分派、授权先于读取、5 条查询命令、结果超限显式失败、限流计数与 `4429` | code:WP1（事件轨道提供诊断日志口径） | wp-coder | `crates/server/src/sync/command/`, `crates/server/src/sync/mod.rs`（仅追加模块声明行） | patch + 「管线顺序与 §11.2 的逐条对应、授权先于读取的证据、限流按设备与命令族计数的实现、结果超限的判定点」 | 主检出 `cargo test -p server`；`cargo clippy … -D warnings` |
| WP3 | mutation 的接受/幂等/终态（覆盖 `sync-command-handling`「mutation 先接受后终态」「命令幂等与冲突判定」「乐观并发控制的版本冲突」「终态观察者随连接生存」）：7 条 mutation 的接受路径、`(deviceId, requestId)` 幂等与冲突判定（用 ACPR-CJ1 比较语义）、`expectedVersion` 冲突、终态观察者与断开取消 | code:WP2（命令管线骨架） | wp-coder | `crates/server/src/sync/command/`（同一模块目录，见 Merge Order） | patch + 「幂等键的落点、冲突判定的比较方式、观察者的所有者与取消路径、与既有 `owned_command` 的衔接」 | 主检出 `cargo test -p server`；`cargo clippy … -D warnings` |
| WP4 | 端到端验收：脚本化客户端对真实 Daemon 派发命令并观察终态（覆盖两个能力的跨命令面：`accepted` → 领域事件 → 终态、未授权零副作用、幂等重发、限流、事件视图字段与原文保真） | code:WP3 | wp-tester | `crates/app/tests/sync_commands_e2e.rs`, `crates/app/tests/support/sync.rs`（前置变更已建该文件，本变更追加命令相关方法） | patch + 断言清单与实测输出 | 主检出 `cargo test -p app --test sync_commands_e2e` |
| WP5 | 文档同步（任务 4.1/4.2）：按实绩写回协议与模块文档，**并写明 §6.2/§6.3 的已裁决降级**（本期不展示 `turns`/`pending_interactions`/`capabilities` 与 imported 会话明细） | none（与实现无代码依赖；但**必须最后合入**，否则写回的“实绩”早于事实） | wp-coder | `docs/SYNC_PROTOCOL.md`, `docs/MODULE_ARCHITECTURE.md`, `docs/FRONTEND_DESIGN.md`, `README.md` | patch + 「每处『已落地』断言的证据文件:行」+「刻意保留为『尚未实现』的清单」 | 主检出 `npm run check:docs` 与 `npm run check:drift` |

## Merge Order

| Order | Work Package | Reason | Re-verify After Merge |
| --- | --- | --- | --- |
| 1 | WP1 | 无重叠（`sync/event/` 新目录） | `cargo test -p server` |
| 2 | WP2 | 与 WP1 在 `sync/mod.rs` 重叠（各追加模块声明行） | 重跑 `cargo test -p server`（WP1 已改该文件） |
| 3 | WP3 | 与 WP2 写范围重叠（同一 `sync/command/` 目录：WP2 建骨架与查询命令，WP3 加 mutation） | 重跑 `cargo test -p server`（WP2 已改该目录） |
| 4 | WP4 | 依赖 WP3 的公开面；写范围仅 `crates/app/tests/`，与前三者不重叠 | `cargo test -p app --test sync_commands_e2e` |
| 5 | WP5 | 写范围仅 `docs/` 与 `README.md`，与实现包不重叠；但**必须最后合入** —— 它写回的是「实绩」，早于实现合入就会把计划写成事实 | `npm run check:docs` 与 `npm run check:drift` |

说明：WP2 与 WP3 在同一模块目录内，必须串行（WP2 先建命令管线的分发与查询路径，WP3 在此基础上加 mutation 与幂等）。这是**同一模块的两半**，不是可以拆开并行的两块。

## Project Verify

| Check ID | Command / Working Directory | Scope / Pass Criteria |
| --- | --- | --- |
| PV1 | `cargo test --locked --workspace --all-features`（主检出） | 全绿；含 WP1–WP4 新增用例与前置变更的既有用例 |
| PV2 | `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`（主检出） | 无警告 |
| PV3 | `cargo fmt --all -- --check`（主检出） | 无格式差异 |
| PV4 | `npm run check`（主检出） | 十道合同脚本全绿（本变更预期零机器合同改动；若门禁报红说明实现偏离了已冻结合同） |
| PV5 | `cargo test -p app --test sync_commands_e2e`（主检出） | WP4 全部断言通过，输出含终态投递、权限拒绝、幂等重发与限流的实测证据 |

## Code Review

| Work Package | Reviewer Agent | Review Focus | Pass Condition |
| --- | --- | --- | --- |
| WP1 | reviewer | 视图判定失败是否仍下发（不得静默丢弃）；原文是否逐字节保真（不得重新序列化）；未登记判别子是否走降级；256 KiB 与 media type 口径是否与协议文本一致；是否误用了 Node Link 的类型 | 无 CRITICAL/MAJOR 未闭环项 |
| WP2 | reviewer | 授权是否真在读取之前；查询命令是否可能返回 `accepted`；结果超限是否可能截断后当完整结果返回；限流是否按设备与命令族计数、连续超限是否才关连接；是否存在 `unwrap()` | 同上 |
| WP3 | reviewer | 幂等是否跨重连存活且不被提前清理；冲突判定是否用语义比较（键顺序不同不得判冲突）；`expectedVersion` 冲突是否零副作用；观察者是否在断开时取消、不留 detached task；`uncertain` 是否不被自动重派 | 同上 |
| WP4 | reviewer | 是否绕过真实路径；是否存在只证明「没抛错」的断言；未授权零副作用与幂等重发是否有独立反例；限流断言是否会在实现漏掉计数时仍通过；测试是否夹带了实现代码 | 同上 |
| WP5 | reviewer | 每处「已落地」断言是否都有代码或已合入 PR 作证（不得把计划写成事实）；**§6.2/§6.3 的降级是否被如实写明**（不得写成「已支持」也不得省略）；「尚未实现」清单是否与代码一致；是否把未实现项写成与 `docs/` 其他章节冲突的口径 | 同上 |

## Main E2E

```yaml
mode: not-applicable
reason: 本变更仍没有可由浏览器触达的入口——静态资源托管属同批第 3 段（`sync-ui-hosting`），前端连接路径与 subprotocol 修正属独立变更（`frontend-sync-wiring`）。命令面与事件面的可观察行为本期只能经脚本化客户端验证。
basis: proposal.md 的「验收边界」明确本变更的可观察结果是协议级闭环；docs/WORKFLOW.md §7 要求 E2E 断言可观察行为，而浏览器路径尚未接通（PWA 无 endpoint 配置，只能与 Daemon 同源加载）。
alternative_checks:
  - PV5：真实 Daemon + 真实 SQLite + 真实 Authority 上派发命令，断言 `accepted` → 领域事件 → 持久化终态、未授权零副作用、幂等重发与限流；断言的是真实协议行为与持久化结果，不是替身返回值。
  - PV4：合同门禁确认实现未偏离已冻结的 Sync 命令与事件视图 schema。
  - 前端一侧的事件视图消费逻辑已有浏览器级证据（`clients/app` 的 e2e 对假主机 48/48，含时间线、降级、变更文件右栏）；两侧在同一批已冻结合同上各自独立判定。真正的浏览器级闭环留待第 3 段与独立变更合入后补齐。
```

## Risks

- **[WP2/WP3 同目录串行]** 两者写同一模块目录，若并行会互相覆盖 → 已在 Merge Order 串行并登记重跑；若 `git apply --check` 失败，用后合入者自己的 patch 重建，不手工改代码。
- **[前置未合入]** 本变更的四个包都依赖 `sync-server-connection` 提供的模块骨架与投递队列 → 派发前必须确认前置已合入主检出（`crates/server/src/sync/` 存在且 `cargo test -p server` 绿）。
- **[常量重复]** Sync 与 Node Link 各自持有 256 KiB 与 `application/json` 常量 → 两处各有测试断言协议文本口径；检视项已列入 WP1。
- **[隔离区限制]** WP4 作者无法在隔离区运行端到端用例 → 交付是用例代码 + 隔离区能做的核对，真实运行由主 Agent 按 PV5 执行；报告须写明，不得声称已在隔离区通过。
