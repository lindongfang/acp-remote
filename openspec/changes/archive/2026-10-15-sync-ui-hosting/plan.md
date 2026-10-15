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
- **前置**：`sync-server-connection` 与 `sync-server-commands` 必须先合入。

## Work Packages

| ID | Goal / Scenarios | Dependencies | Agent | Write Scope | Deliverable | Verify |
| --- | --- | --- | --- | --- | --- | --- |
| WP1 | 路由表第三类端点与静态处理器（覆盖 `ui-asset-hosting`「根目录入口与单页回退」「协议端点优先于静态回退」「内容类型与安全响应头」「未知路径仍显式拒绝」）：新增静态 GET 类别、解析顺序、MIME 映射、CSP 与安全头、非读取方法拒绝；并更新既有 「`/` 必须 404」的断言 | none（前置已提供路由表） | wp-coder | `crates/server/src/transport/net/` | patch + 「解析顺序的实现方式与协议优先的保证、CSP 取值来源、既有断言为何改写、非 GET 的拒绝语义」 | 主检出 `cargo test -p server`；`cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` |
| WP2 | 嵌入与构建期关闭（覆盖 `ui-asset-hosting`「前端构建版本与 Daemon 可检查关联」的资源侧 + design D2）：引入嵌入依赖、资源表与 MIME、产物缺失时的占位入口、发布构建检测到占位时失败关闭 | code:WP1（静态处理器消费该资源表） | wp-coder | `crates/server/Cargo.toml`, `Cargo.toml`, `crates/server/src/transport/net/`（资源表子模块）, `clients/app/package.json`（仅在需要构建钩子时） | patch + 「依赖选型与许可证/维护状态核对、缺失产物的两条路径（测试放行 / 发布失败关闭）、资源表与 MIME 的实现」 | 主检出 `cargo test -p server`；`check:boundaries` 断言依赖方向未变 |
| WP3 | 组合根注册与版本暴露（覆盖「前端构建版本与 Daemon 可检查关联」的查询侧 + design D3/D5）：`daemon.rs` 注册静态端点、`daemon.status` 增加构建版本字段（不新增本地管理方法） | code:WP2（资源表提供版本） | wp-coder | `crates/app/src/daemon.rs`, `crates/app/src/compose.rs`, `crates/server/src/local_admin/daemon.rs`（仅追加字段） | patch + 「注册点与既有协议端点的相对顺序、字段来源、为何不新增方法」 | 主检出 `cargo test -p app`；`npm run check`（含 `check:drift`，确认未漂移本地管理方法集） |
| WP4 | 文档同步与浏览器级验收：`FRONTEND_DESIGN.md` §4.3/§11、`CONFIG_REFERENCE.md` §1 路由清单、`SECURITY_DESIGN.md` 的 CSP 与同包发布、`MODULE_ARCHITECTURE.md` §4.9、`README.md`；并用 browser prelude 断言页面与资源可加载、协议端点不受影响 | code:WP3 | wp-coder | `docs/`, `README.md` | patch + 文档改动点清单 + 浏览器实测输出 | 主检出 `npm run check:docs`、`npm run check:drift`；浏览器 prelude 断言（见 Main E2E） |

## Merge Order

| Order | Work Package | Reason | Re-verify After Merge |
| --- | --- | --- | --- |
| 1 | WP1 | 无重叠（`transport/net` 的路由与处理器） | `cargo test -p server` |
| 2 | WP2 | 与 WP1 在 `crates/server/src/transport/net/` 重叠（WP1 建处理器，WP2 供资源表） | 重跑 `cargo test -p server`（WP1 已改该目录） |
| 3 | WP3 | 依赖 WP2 的公开面；与 WP1/WP2 的写范围不重叠 | `cargo test -p app`、`npm run check:drift` |
| 4 | WP4 | 依赖 WP3 的端点与版本字段；写范围仅 `docs/` 与 `README.md` | `npm run check:docs`；浏览器断言 |

说明：WP1 与 WP2 同在 `transport/net`，必须串行（前者建端点类别，后者供嵌入资源表与 MIME）。WP2 涉及新增依赖，其合入后必须让 `check:boundaries` 与 CI 的 `deps`/`advisories` job 通过——后者只在 CI 运行，本地无法等价验证，报告中不得声称本地已过。

## Project Verify

| Check ID | Command / Working Directory | Scope / Pass Criteria |
| --- | --- | --- |
| PV1 | `cargo test --locked --workspace --all-features`（主检出） | 全绿；含改写后的路由断言与新增静态托管用例 |
| PV2 | `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`（主检出） | 无警告 |
| PV3 | `cargo fmt --all -- --check`（主检出） | 无格式差异 |
| PV4 | `npm run check`（主检出） | 十道合同脚本全绿；重点看 `check:boundaries`（新依赖的依赖方向）与 `check:drift`（本地管理方法集未漂移） |
| PV5 | `cd clients/app && npm run check`（主检出，独立工程） | typecheck + vitest + `expo export` 全绿，产出 `dist/` 供嵌入 |
| PV6 | 浏览器断言（见 Main E2E） | 页面与资源可加载、协议端点不受影响、未知方法被拒 |

## Code Review

| Work Package | Reviewer Agent | Review Focus | Pass Condition |
| --- | --- | --- | --- |
| WP1 | reviewer | 协议端点是否真的优先（不能被回退吞掉）；CSP 是否由响应头下发且不宽于基线；内容类型映射是否会误导浏览器执行；非 GET 是否被明确拒绝；改写的既有断言理由是否成立 | 无 CRITICAL/MAJOR 未闭环项 |
| WP2 | reviewer | 新依赖的必要性/许可证/平台支持是否登记；缺失产物的两条路径是否真的分离（测试放行、发布失败关闭）；是否可能把占位入口发布出去；资源表是否可能路径穿越 | 同上 |
| WP3 | reviewer | 是否真的未新增本地管理方法（`check:drift` 能证明）；版本字段是否来自同一次构建；注册顺序是否影响协议端点；`app` 是否承载了业务规则 | 同上 |
| WP4 | reviewer | 文档是否写成「已落地」而非「计划」；CSP 与内联样式的冲突是否被如实登记（不得用放宽 CSP 掩盖）；浏览器断言是否只证明「启动成功」（不算通过） | 同上 |

## Main E2E

```yaml
mode: required
reason: 本变更首次产生可由浏览器触达的界面（页面与资源托管），因此必须用 browser prelude 断言可观察行为。注意本期仍**不能**断言「连上 Daemon」——前端连接路径与 subprotocol 的修正属独立变更 `frontend-sync-wiring`。
basis: docs/WORKFLOW.md §7 要求按被测面选 prelude；本变更是 Web/PWA 面，用 browser prelude（默认开）。
alternative_checks: []
```

浏览器断言清单（WP4 必须给出实测输出）：

1. `GET /` 返回 HTML 且内容类型正确。
2. 入口引用的脚本/样式资源可被加载（不发生 404 或类型错误）。
3. 直接访问一个前端路由深链（如 `/pair`）返回同一入口文档而非 404。
4. Sync 与 Node Link 的 WebSocket 端点仍为升级端点（请求返回的不是入口文档）；两条配对 POST 端点行为不变。
5. 对静态路径发起非读取方法被明确拒绝（不是 200 入口文档）。
6. 响应头含 CSP，取值不宽于基线。
7. **已知冲突的如实记录**：若内联样式被 CSP 拒绝导致页面样式异常，必须把该现象与「交由 `frontend-sync-wiring` 处理」一并写进报告，不得通过放宽 CSP 掩盖。

## Risks

- **[WP1/WP2 同目录串行]** 两者写 `transport/net` → 已串行并登记重跑；`git apply --check` 失败时用后合入者的 patch 重建，不手工改代码。
- **[既有断言必须改写]** `net/tests.rs:301` 断言 `"/"` 必须 404，与新行为直接冲突 → 改写该用例并说明理由；MUST NOT 用「删掉断言」的方式让它通过（检视项已列入 WP1）。
- **[新依赖的 CI 判定]** `rust-embed` 的许可证/公告判定只在 CI 运行，本地无等价物 → 报告不得声称本地已过；若 CI 报红，按 `deny.toml` 或豁免清单处理。
- **[构建顺序]** 发布构建需要 `clients/app/dist` 先产出 → 测试环境走占位入口不阻断；发布路径在 `daemon start` 检测到占位时失败关闭，避免静默发空页面。
- **[CSP 与内联样式]** Expo 产物含内联 `<style>`，基线 CSP 会拒它 → 这是真实冲突，登记并交前端变更处理；本变更不放宽基线。
- **[隔离区限制]** WP4 的作者在隔离区无法跑浏览器断言（无 `target/`、无前端产物）→ 文档改动可用 diff 核对，浏览器断言由主 Agent 在合入后执行；报告须写明。
