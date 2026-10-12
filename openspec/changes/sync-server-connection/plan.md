<!-- 执行安排：把 specs 的行为要求与 design 的技术方案转成可派发的并行工作包。
     plan 只写“谁在哪个隔离工作区做什么、按什么顺序合入、怎么验”，不重复 specs/design 的内容。

     本仓库的执行原语是 Oh My Pi（omp），不是外部调度器：
     - 工作包由主 Agent 用 `task` 工具批量派发（batch 形状：共享 `context` + `tasks[]`）；
     - 每个工作包跑在隔离工作区（`task.isolation.enabled: true`、`merge: patch`、`apply: false`）；
     - 交付物是 `<id>.patch`（改动）+ `agent://<id>`（报告）+ `history://<id>`（过程），
       不是共享目录，也不是 branch；
     - 合入由主 Agent 串行执行（`git apply --check` → `git apply` → 主检出 Project Verify → commit）。

     填写时机：生成 tasks 之前定完 WP/写范围/依赖/合入顺序/验证策略；用例 ID 与执行分配在 apply 阶段补齐。
     表单里的每一列都必须可核对，不能写“按需”“视情况”这类无法验证的描述。 -->

# Plan

## Execution Model

- 派发：主 Agent 用 `task` 工具的 batch 形状一次性扇出多个工作包；每个工作包是**一次 spawn**。
- 隔离：`task.isolation`（`patch` 模式、`apply: false`）。工作包之间不共享目录。
- 交付：`<id>.patch` + `agent://<id>` + `history://<id>`；工作包不产生 commit、不产生分支。
- 合入：主 Agent 按 `## Merge Order` 串行 `git apply`，每次合入后跑主检出 Project Verify。
- 验证：隔离工作区内**没有** `target/` 与 `node_modules/`，因此仓库级检查只在主检出执行。
- 检视：每个工作包由一个**独立** spawn（不与作者同实例）读 patch 与 `agent://<id>` 后给出结构化结论。

## Work Packages

| ID | Goal / Scenarios | Dependencies | Agent | Write Scope | Deliverable | Verify |
| --- | --- | --- | --- | --- | --- | --- |
| WP1 | core 的三处 Sync 接缝：Sync 设备握手窄读面（覆盖 `sync-device-auth`「未知与已撤销设备的可见行为」的信任读取前置）、设备认证收尾（`last_seen_at` + `device.authenticated` 审计）、自有+已导入合并且按 scope 过滤的会话清单读面（覆盖 `sync-event-delivery`「设备可见的会话清单范围」） | none | wp-coder | `crates/core/src/use_cases.rs`, `crates/core/src/ports.rs`, `crates/core/src/model/`（仅在确需新增值对象时） | patch + 「三个新入口的签名与 doc 注释、它们在既有 `require_local` 守卫之外的理由、失败关闭路径」 | 主检出 `cargo test -p core`；`cargo test -p storage-sqlite` 全绿（合并读面若触及端口签名） |
| WP2 | 接入层把**已验证的 HTTP `Origin`** 交给 WS 会话（覆盖 `sync-device-auth`「服务端来源绑定来自已验证的 HTTP Origin」）：`WsHandler::handle` 增加来源参数、升级路径从请求头取并归一化、两个既有实现同步改造 | none | wp-coder | `crates/server/src/transport/net/`, `crates/server/src/node_link/conn/session.rs`, `crates/server/src/node_link/conn/mod.rs` | patch + 「归一化口径与 §3.2 的逐条对应、两个既有实现各自的取值来源、无 Origin 头时的取值」 | 主检出 `cargo test -p server`；`cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` |
| WP3 | `server::sync` 的连接面：模块骨架与 `lib.rs` 注册、四消息握手与 15 秒窗口、单设备替换（4411）、撤销传播（4410）、心跳与静默超时、close code 映射、连接注册表与有界出站队列（覆盖 `sync-device-auth` 除来源校验与限额下发外的全部要求） | code:WP1（握手读面）、code:WP2（来源参数） | wp-coder | `crates/server/src/sync/mod.rs`, `crates/server/src/sync/conn/`, `crates/server/src/lib.rs` | patch + 「四消息序列与状态机、注册表不是事实来源的落地方式、出站队列双上限与高水位行为、close code 映射表」 | 主检出 `cargo test -p server`；`cargo clippy … -D warnings`；`npm run check:boundaries` |
| WP4 | `server::sync` 的投递面：订阅与 cursor 判定、增量重放与 barrier、`reset_required`、三类资源快照组装与分块、`snapshotDigest`、设备可见会话清单在快照中的投影、ACK 单调性（覆盖 `sync-event-delivery` 全部要求） | code:WP3（连接面与出站队列）、code:WP1（合并清单读面） | wp-coder | `crates/server/src/sync/subscribe/`, `crates/server/src/sync/snapshot/`, `crates/server/src/sync/mod.rs`（仅追加模块声明行） | patch + 「cursor 四种 reason 的判定点、barrier 与实时投递的衔接方式、摘要按原始帧字节的实现、ACK 只前进的实现」 | 主检出 `cargo test -p server`；`cargo clippy … -D warnings` |
| WP5 | `server::sync` 的设备配对 HTTP：claim 的原子校验与一次性、status 的恒定 200 与业务状态、四个安全响应头（含预处理响应）、限流与失效（覆盖 `sync-pairing-http` 全部要求） | code:WP3（模块骨架） | wp-coder | `crates/server/src/sync/pairing.rs`, `crates/server/src/sync/mod.rs`（仅追加模块声明行） | patch + 「原子校验的顺序、status 各分支与已批准字段集、安全头的注入位置（含 413/405）、限流口径」 | 主检出 `cargo test -p server`；`cargo clippy … -D warnings` |
| WP6 | 组合根接线：`/sync/v1` 与两条 pairing POST 的注册（覆盖 `node-link-listener` 增量的「Sync 端点正常升级」「Sync 配对端点可用」「两侧不互相进入对方逻辑」）、Sync 事件扇出分支、`sync.*` 三键接线并从 `unwired` 移除（覆盖 `sync-device-auth`「限额下发只下调不下调之外」）、`close_device` 撤销钩子接上真实连接表 | code:WP3, code:WP4, code:WP5 | wp-coder | `crates/app/src/compose.rs`, `crates/app/src/daemon.rs`, `crates/app/src/config.rs` | patch + 「注册点与 subprotocol 常量、扇出按 scope 过滤的位置、三个键从 `unwired` 移除的证据、撤销钩子的连接来源」 | 主检出 `cargo test -p app`；`npm run check:docs`；`npm run check:boundaries` |
| WP7 | 文档同步：`SYNC_PROTOCOL.md` 的实绩写回、`CONFIG_REFERENCE.md` §2 撤下「未接线」注记、`MODULE_ARCHITECTURE.md` §3/§4.9 现状、`FRONTEND_DESIGN.md` §4.3 与 `INITIAL_DESIGN.md` §6.3 的 `/sync`→`/sync/v1` 冲突修正、`SECURITY_DESIGN.md` 的 Origin 与撤销传播实绩、`README.md` 状态表 | none（只读其他 WP 的产物结论） | wp-coder | `docs/`, `README.md` | patch + 「逐份文档的改动点与依据、两处 `/sync` 冲突的裁定记录、`README.md` 表格行的更新」 | 主检出 `npm run check:docs`；`npm run check:drift` |
| WP8 | 协议级端到端验收：脚本化 fake Sync 客户端经**真实 loopback listener + 真实 SQLite + 真实 `Authority`** 走完配对 claim → 本机确认 → status approved → 四消息握手 → 订阅 → 快照（含两级摘要复算）→ ACK；并覆盖撤销传播与 cursor 四种错误 | code:WP6 | wp-tester | `crates/app/tests/sync_e2e.rs`, `crates/app/tests/support/sync.rs`, `crates/app/tests/support/mod.rs` | patch + 断言清单与实测输出 | 主检出 `cargo test -p app --test sync_e2e` |

## Merge Order

| Order | Work Package | Reason | Re-verify After Merge |
| --- | --- | --- | --- |
| 1 | WP1 | 无重叠（`core` 独立） | `cargo test -p core` |
| 2 | WP2 | 无重叠（`transport` 与 `node_link` 的连接实现） | `cargo test -p server` |
| 3 | WP3 | 与 WP4、WP5 写范围重叠（`crates/server/src/sync/mod.rs`） | `cargo test -p server` |
| 4 | WP4 | 与 WP3 在 `sync/mod.rs` 重叠，按序在后 | 重跑 `cargo test -p server`（WP3 已改该文件） |
| 5 | WP5 | 与 WP3/WP4 在 `sync/mod.rs` 重叠，按序在后 | 重跑 `cargo test -p server` |
| 6 | WP6 | 依赖 WP3/WP4/WP5 的公开面；写范围不与前三者重叠 | `cargo test -p app`、`npm run check:boundaries` |
| 7 | WP7 | 无重叠（纯 `docs/` 与 `README.md`） | `npm run check:docs`、`npm run check:drift` |
| 8 | WP8 | 依赖 WP6 的接线；写范围仅 `crates/app/tests/`（与 WP6 的 `crates/app/src/` 不重叠） | `cargo test -p app --test sync_e2e` |

说明：WP3/WP4/WP5 三者在 `crates/server/src/sync/mod.rs` 上重叠，是**模块声明的天然汇聚点**（各包把自己的子模块登记进同一文件）。三者已在合入序列中串行，且后合入者重跑 `cargo test -p server`。WP5 本身仍是独立可解耦的交付（配对 HTTP 与连接面、投递面没有共同的代码路径）。

## Project Verify

| Check ID | Command / Working Directory | Scope / Pass Criteria |
| --- | --- | --- |
| PV1 | `cargo test --locked --workspace --all-features`（主检出） | 全绿；含 WP1–WP8 各自新增的用例 |
| PV2 | `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`（主检出） | 无警告 |
| PV3 | `cargo fmt --all -- --check`（主检出） | 无格式差异 |
| PV4 | `npm run check`（主检出） | 十道合同脚本全绿（含 `check:boundaries`、`check:drift`、`check:docs`、`check:openspec`） |
| PV5 | `cargo test -p app --test sync_e2e`（主检出） | WP8 的全部断言通过，输出含握手、快照摘要复算、撤销传播的实测证据 |

## Code Review

| Work Package | Reviewer Agent | Review Focus | Pass Condition |
| --- | --- | --- | --- |
| WP1 | reviewer | 新入口是否确实收窄（不放开 `require_local`）；合并清单读面是否成为第二条可见性判定路径；失败关闭与审计动作是否符合 `SECURITY_DESIGN.md` §14.2 | 无 CRITICAL/MAJOR 未闭环项 |
| WP2 | reviewer | 来源归一化是否与 §3.2 一致；两个既有实现是否都正确取值；无 Origin 头时是否按协议取空字节串而非降级；是否误把 `Host` 当来源 | 同上 |
| WP3 | reviewer | 四消息序列与 15 秒窗口；未知设备是否照常签发挑战；注册表是否被当作事实来源；出站队列双上限与高水位是否可能丢事件；close code 映射是否与 `errors.json` 登记一致 | 同上 |
| WP4 | reviewer | cursor 四种 reason 是否可区分且不钳制；barrier 与实时投递的先后；摘要是否按原始帧字节而非重新序列化；快照资源范围是否包含了明细资源；ACK 是否可能被回退或推进 head | 同上 |
| WP5 | reviewer | claim 原子性与一次性；status 是否可能返回非 200；安全头是否覆盖 413/405；proof 无效时是否泄露差异；secret 是否可能进日志或错误消息 | 同上 |
| WP6 | reviewer | 路由注册是否会把 Sync 载荷送进 Node Link 逻辑；三个配置键是否真从 `unwired` 移除；撤销钩子是否以持久状态为准；`app` 是否承载了业务规则 | 同上 |
| WP7 | reviewer | 文档是否写成「已落地」而非「计划」；两处 `/sync` 冲突的裁定是否与 `SYNC_PROTOCOL.md` 一致；是否误改了与本变更无关的历史内容 | 同上 |
| WP8 | reviewer | 是否绕过真实路径（真实 listener/SQLite/Authority）；是否有断言只证明「没抛错」；撤销与 cursor 错误是否有独立反例；是否有断言在服务端实现错误时仍会通过 | 同上 |

## Main E2E

```yaml
mode: not-applicable
reason: 本变更没有可运行的用户可见入口——静态资源托管属同批第 3 个变更（页面挂根目录、资源嵌入二进制，用户 2026-10-12 裁定），前端连接路径与 subprotocol 修正属独立变更，因此本变更合入后浏览器仍无法加载并连接真实 Daemon（PWA 无 endpoint 配置，只能与 Daemon 同源加载）。
basis: proposal.md 的「验收边界」明确本变更的可观察结果是协议级闭环；docs/WORKFLOW.md §7 要求 E2E 断言可观察行为，而本变更尚无可由浏览器触达的行为面。把「前端连不上」当成缺陷会让本变更无法验收，因为缺口在同批后续变更。
alternative_checks:
  - PV5：脚本化 fake Sync 客户端经真实 loopback listener + 真实 SQLite + 真实 identity-auth Authority 走完配对、握手、订阅、快照与 ACK，断言的是真实协议行为而非替身返回值。
  - PV4 的 check:boundaries：确认 server::sync 只依赖 core 与 wire protocol，未横向依赖 node_link 或 storage-sqlite。
  - 客户端一侧的浏览器级证据已存在（clients/app 的 e2e 对假主机 48/48），两侧在同一批已冻结合同与固定向量上各自独立判定；真正的浏览器级闭环证据留待第 3、4 个变更合入后补齐。
```

## Risks

- **[WP1 的端口改动面]** 合并清单读面若需要改 `SessionStore`/`RemoteDeliveryStore` 之外的端口签名，会牵动 `storage-sqlite` 与 `check:drift` → WP1 的写范围已含 `ports.rs` 与 `core/model`，但**不含** `storage-sqlite`；若发现必须改存储层，停止并回到本文件重新划分工作包，不得让 WP1 越界。
- **[WP2 的签名变更]** `WsHandler::handle` 是既有 trait，改造必须一次到位并覆盖两个既有实现 → WP2 的写范围显式包含 `node_link/conn/{session,mod}.rs`；漏改会让 `cargo test -p server` 直接失败，不会静默通过。
- **[WP3/WP4/WP5 的 mod.rs 汇聚]** 三个包都要在 `sync/mod.rs` 追加声明行，patch 上下文可能冲突 → 已串行并登记重跑；若 `git apply --check` 失败，用后合入者自己的 patch 重建该文件的两行，不手工改代码内容。
- **[WP8 的隔离区限制]** 隔离工作区没有 `target/`，WP8 作者无法在本区运行端到端用例 → WP8 的交付是**用例代码 + 作者在隔离区能做的最小事实核对**，真实运行由主 Agent 在合入后按 PV5 执行；报告里必须写明这一点，不得声称已在隔离区通过。
- **[限额数值的来源]** WP6 需要协议默认值（1 MiB / 256 KiB / 500），必须取自 `SYNC_PROTOCOL.md` §14 而非自定 → 检视项已列入 WP6 的 review focus。
