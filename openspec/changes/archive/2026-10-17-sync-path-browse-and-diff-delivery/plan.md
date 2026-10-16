<!-- 执行安排：把 specs 的行为要求与 design 的技术方案转成可派发的并行工作包。
     plan 只写“谁在哪个隔离工作区做什么、按什么顺序合入、怎么验”，不重复 specs/design 的内容。 -->

## Execution Model

- 派发：主 Agent 用 `task` 工具的 batch 形状一次性扇出多个工作包；每个工作包是**一次 spawn**。
- 隔离：`task.isolation`（`patch` 模式、`apply: false`）。工作包之间不共享目录。
- 交付：`<id>.patch` + `agent://<id>` + `history://<id>`；工作包不产生 commit、不产生分支。
- 合入：主 Agent 按 `## Merge Order` 串行 `git apply`，全部合入后在主检出跑 `## Project Verify`。
- 验证：隔离工作区内**没有** `target/` 与 `node_modules/`，因此仓库级检查只在主检出执行。
- 检视：每个工作包由一个**独立** spawn（不与作者同实例）读 patch 与 `agent://<id>` 后给出结构化结论。

## Work Packages

| ID | Goal / Scenarios | Dependencies | Agent | Write Scope | Deliverable | Verify |
| --- | --- | --- | --- | --- | --- | --- |
| WP1 | 机器合同与协议 DTO：`sync-workspace-browse`（命令名、载荷、结果、限额）、`sync-workspace-catalog`（目录资源路径键）、`core-derived-events`/`sync-event-projection`（差异行字段）、`sync-session-create`（建目录开关）；`docs/SYNC_PROTOCOL.md` 的 §5.2 / §9.1 / §10.2 / §10.3 / §11.5 / §14 表与正文 | none | wp-coder | `compatibility/commands/v1/`, `compatibility/features/v1/`, `schemas/sync/v1/`, `fixtures/sync/v1/`, `crates/sync-protocol/`, `docs/SYNC_PROTOCOL.md` | 新命令 `workspace.browse` 进入 `grant.observe` 分组；3 条新 feature；新键只在协商后出现；正反夹具与 `manifest.json`；`envelope_fixtures.rs` 计数同步 | 主检出：`npm run check`（`check:commands`/`check:features`/`check:schema-fixtures`）+ `cargo test -p sync-protocol` |
| WP2 | core：目录浏览端口与用例、路径下发取值（`strip_verbatim` 后的规范化值）、`session.create` 的建目录分支、差异行派生（有界 + 超限省略） | contract:WP1 | wp-coder | `crates/core/src/`, `docs/CORE_PORTS_AND_STORAGE.md`, `docs/MODULE_ARCHITECTURE.md` | 新端口签名与 §5 合同逐字一致；用例首参为 `actor`；差异行与既有统计同源；无新 crate 依赖 | 主检出：`cargo test -p core` + `npm run check`（`check:drift`/`check:boundaries`） |
| WP3 | server：`workspace.browse` 查询处理、命令族归类、差异行透传、组合根装配新端口 | code:WP2 | wp-coder | `crates/server/src/sync/`, `crates/app/src/daemon.rs` | `on_query` 新臂、`CommandFamily::of` 穷尽 match 覆盖新命令、未协商时 `protocol.feature_required`、超限 `resource.result_too_large` | 主检出：`cargo test -p server -p app` + `cargo clippy` |
| WP4 | 安全边界与 ACP 交付档位：`SECURITY_DESIGN.md` 相关段落改写（含 §10.2 命令表新行、§10.3 的自相矛盾处、§12.3 的路径口径）、`INITIAL_DESIGN.md` 同步、新 ADR、`ACP_COMPATIBILITY_MATRIX.md` 与机器矩阵把 `tool_content.diff` 的第一阶段交付扩为「统计 + 路径 + 有界逐行差异」 | none | wp-coder | `docs/SECURITY_DESIGN.md`, `docs/INITIAL_DESIGN.md`, `docs/adr/`, `docs/ACP_COMPATIBILITY_MATRIX.md`, `compatibility/acp/v1/` | 五处文档与一份 ADR 一致描述新边界，无一处仍写「路径永不出网」；矩阵与文档口径一致 | 主检出：`npm run check`（`check:acp`）+ 全仓 grep 断言「任何对端可见输出都不含路径」类表述已改写 |

## Merge Order

| Order | Work Package | Reason | Re-verify After Merge |
| --- | --- | --- | --- |
| 1 | WP1 | 无文件重叠；它是 WP2/WP3 的合同基线 | 无 |
| 2 | WP4 | 与 WP1 无文件重叠，但 `check:commands` 断言 `docs/SECURITY_DESIGN.md` §10.2 的命令表——两者都合入后该门禁才会绿 | `npm run check` |
| 3 | WP2 | 依赖 WP1 的合同（编译期） | `cargo test -p core` |
| 4 | WP3 | 依赖 WP2 的端口与用例 | `cargo test -p server -p app` |

## Project Verify

| Check ID | Command / Working Directory | Scope / Pass Criteria |
| --- | --- | --- |
| PV1 | `npm run check`（仓库根） | 十项合同门禁全绿；重点：`check:commands` 的命令 enum/grant 分组/`required_grant`/两份文档表格四处一致，`check:features` 的 registry↔文档表↔夹具三方一致，`check:schema-fixtures` 覆盖新夹具，`check:acp` 的矩阵与文档一致，`check:drift` 的端口签名与 §5 逐字一致 |
| PV2 | `cargo test --workspace`（仓库根） | 全绿；重点：`crates/sync-protocol` 的 `CommandName::ALL` 计数与 `envelope_fixtures.rs` 的夹具计数、`crates/server` 的命令族穷尽 match 断言 |
| PV3 | `cargo clippy --workspace --all-targets -- -D warnings`（仓库根） | 无告警 |
| PV4 | `npm run verify`（仓库根） | 以上两类的合并入口全绿 |

## Code Review

| Work Package | Reviewer Agent | Review Focus | Pass Condition |
| --- | --- | --- | --- |
| WP1 | reviewer | 新键是否只在协商后出现（封闭对象不给旧客户端塞键）；命令分组与 grant 是否自洽；夹具是否覆盖正反两侧 | 无 CRITICAL/MAJOR 未闭环项 |
| WP2 | reviewer | 路径取值口径（拒绝 `..` 先于 `canonicalize`、`strip_verbatim` 只用在下发形式）；建目录的失败分类与「不删除已建目录」；差异行上限与省略口径 | 无 CRITICAL/MAJOR 未闭环项 |
| WP3 | reviewer | 未认证/未协商两条拒绝路径是否在触碰文件系统之前；限流与超限是否复用既有机制而不是新造；是否有路径进入错误 `details` 或日志 | 无 CRITICAL/MAJOR 未闭环项 |
| WP4 | reviewer | 五处文档与 ADR 是否互相矛盾；是否有一处仍宣称路径永不出网；ADR 是否如实记录残余风险 | 无 CRITICAL/MAJOR 未闭环项 |

## Main E2E

```yaml
mode: not-applicable
reason: 本变更不改变任何用户可见界面——它只新增协议能力；前端呈现由变更 `pwa-prototype-parity` 承担，那一个变更的 Main E2E 才是浏览器级验收。
basis: 变更范围是 wire 合同、core 用例与 server 命令面；没有可运行的用户界面入口，因此没有可用 browser/computer prelude 断言的观察面。
alternative_checks:
  - 真实 daemon + 真实 SQLite + 假 ACP Agent 的 Rust 集成用例（`cargo test -p app`）：逐层浏览返回一层子目录、形状非法时不触碰文件系统、未协商 feature 时明确拒绝
  - 快照级用例（`cargo test -p server`）：协商路径 feature 时目录条目带路径、未协商时该键缺席且旧客户端校验通过
  - 事件级用例：带类型化 Diff 的工具调用产生带差异行的 `file.changed`，超限时按已登记原因省略
```

## Risks

- **合同面最广的工作包先合入**：WP1 一次改动 6 类机器资产，任一漏改都会让门禁红。缓解：WP1 的 Verify 就是全量 `npm run check`，且 `check:commands`/`check:features` 会逐条指出漏改的文件。
- **WP4 与 WP1 的门禁耦合**：`check:commands` 同时断言 `docs/SYNC_PROTOCOL.md` 与 `docs/SECURITY_DESIGN.md` 的命令表，两张表分属两个工作包。缓解：Merge Order 把两者排在 Project Verify 之前，并在 WP4 的任务里点名 §10.2 那一行。
- **core 的端口合同漂移**：新端口必须逐字同步 §5 的 ```rust 块，否则 `check:drift` 红。缓解：WP2 的写范围同时包含合同与实现，任务里点名逐签名比对。
- **降级路径容易被忽略**：超限省略、未协商拒绝、形状非法不触碰文件系统这三条都是负向行为，容易在实现里漏掉。缓解：三条各有一条规格场景与一条任务，检视时专门核对。
