<!-- 记录动机、范围、能力与影响；行为写 specs，方案写 design，协作写 plan。 -->

## Why

**PWA 的解码边界会拒收协议合法的帧，而且拒收之后游标再也推不动。**

`clients/app/src/sync-client/wire.ts` 的运行期封闭词表 `EVENT_ORIGIN_KINDS` 是**四值**（`agent`/`device`/`daemon`/`local_cli`），而协议与 TS 类型都已经是**五值**：`schemas/sync/v1/event.schema.json` 的 `$defs.eventOrigin.kind` 枚举含 `node`，`clients/app/src/protocol/event.ts` 的 `EventOriginKind` 也含 `node`（其注释明确写着它随 `core.event-origin.v1` 进 wire）。漏掉的这一处让 `origin.kind: "node"` 的**协议合法**事件在客户端被判成 `field_type` 拒收。

为什么这不是「显示不出来」那么轻：`node` 是**另一个节点经 Node Link 下令、在本机事件库落库**的事件来源（`docs/SYNC_PROTOCOL.md` §10.1 的 `kind` 取值表第 5 行）。而这恰恰是本产品的首个纵向切片——Access 节点远程 `session.create` 之后，Owner 上落库的 `session.created` 就带 `origin.kind = "node"`（`crates/core/src/broker.rs:4162-4167` 的 `event_origin` 对 `Actor::Node` 给出 `Node`）。也就是说：**只要有人用另一个节点建过一次会话，连在这台 Owner 上的浏览器就会看到一条自己永远解不开的事件。**

后果链（`clients/app/src/sync-client/connection.ts`，已按代码逐点核对，非推测）：

1. `decodeOrReject()` 在解码失败时只发一个 `rejected` 诊断事件并返回 `null`（`connection.ts:1054-1062`），**不升级为阻断态**——设计如此，一条坏消息不该让连接停摆。
2. 该帧因此不进入 `#onEventMessage()`，而**只有** `ledger.commit()` 之后才 `#sendAck(cursor)`（`connection.ts:733`）。于是这条事件**不被 ACK**。
3. 服务端只能按最后 ACK 的 cursor 补发，于是同一条事件被反复投递、反复拒收，客户端游标**永久停在这条事件之前**：它之后的全部事件都进不来。

**立项核查时又发现同族第二处。** 为了确认「还有没有同类的漏判」，我把 `fixtures/sync/v1/manifest.json` 里标记 `valid` 且 schema 指向 `message.schema.json` 的 **101 条**固定样例逐条喂给客户端解码器，结果 **99 条通过、2 条被拒**：除 `valid/event-node-origin.json` 外，还有 `valid/error-before-auth.json`——它的 `type` 是 `error`，而 wire.ts 把 `error` 归进「认证后必须有 `connectionId`」的集合；协议并非如此：`schemas/sync/v1/error.schema.json#/$defs/error` 是 `preAuthBase` 与 `postAuthBase` 两种信封的 `oneOf`，`clients/app/src/protocol/error.ts` 的 `ErrorMessage` 也是这两种信封的联合类型。**同一天花板、同一个后果**：认证前的错误帧被拒收，诊断丢失。

**为什么现在做。** 三件事同时存在：(a) 缺陷已经在线（`node` 来源从 `sync-server-connection` 起就在 wire 上）；(b) 现有测试抓不到它——`wire.test.ts` 用手写样例，`protocol/contract.test.ts` 的 `ORIGIN_KIND_MIRROR` 只做**类型层**镜像（schema ↔ `EventOriginKind`），运行期词表不在任何门禁的覆盖里，两处都绿而缺陷在；(c) 同一份仓库里已经有能证明它的固定样例（`fixtures/sync/v1/valid/event-node-origin.json`、`valid/error-before-auth.json`），只是**没有一条用例把固定样例真的喂给解码器**。

**用户原始要求**（2026-10-17）：`clients/app/src/sync-client/wire.ts` 的 `EVENT_ORIGIN_KINDS` 还是四值、协议与 `protocol/event.ts` 已是五值（少了 `node`），`origin.kind: "node"` 的合法事件会被客户端以 `field_type` 拒收——**这是现存缺陷，单独一个变更修掉**；另外两处（`crates/server/src/sync/mod.rs` 仍写「事件投递腿尚未落地」、`README.md` 与 `docs/FRONTEND_DESIGN.md` §9 对「浏览器级验收是否已完成」口径冲突）**只是文档过期，不紧急**。

**硬约束**：

- 不改变 wire：`schemas/`、`fixtures/`、`compatibility/` 与两份协议文档一字不动。本次修的是**客户端对这一份既有合同的消费**，不是合同本身。若核查发现是合同/服务端侧的问题，必须先停下来重新定范围。
- 只做**加性**地放宽拒收面，不收紧任何既有接受面：`kind: "device"` 且 `deviceId: null` 这类 schema 合法组合**继续接受**——`docs/SYNC_PROTOCOL.md` §10.1 明文规定「发出方 MUST NOT 产生，但接收方不得因此拒绝」。
- `node` 的配对规则按 §10.1 唯一那条执行：`kind = "node"` 时 `nodeId` 必须存在且为非空 UUID、`deviceId` 必须为 `null`。
- 不做「客户端全量 JSON Schema 校验」：解码器的既定口径是「信封 + 判别字段 + 必填键的存在与基本类型」，视图值域归 `schemas/sync/v1/**` 与 `projectView()` 的降级路径（`wire.ts` 开头已写明）。本次不改这条口径。
- 前端门禁 `cd clients/app && npm run check` 必须绿；改动只在该独立 npm 工程与文档内，不触碰根 `npm run check` 链上的机器资产。

**非目标**：

- 不改 `origin` 的 wire 形状、不新增/删除 feature、不动 `core.event-origin.v1` 的协商口径。
- 不把 `error` 的两种信封变成服务端行为变更（服务端早已按 schema 发；这里只修客户端的接收侧）。
- 不新增前端功能（不新增页面、不新增视图、不新增状态机分支）。
- **另外登记、不在本次做**：`docs/FRONTEND_DESIGN.md` §9 第 11 条的前置条件（「展示路径与结构化差异行由变更 `sync-path-browse-and-diff-delivery` 下发，该变更合入前不得据此声称已验收」）所指的变更已在 2026-10-17 归档，那段前置条件同样过期；本次只按用户点名的一处口径冲突（浏览器级验收）收口，第 11 条留给后续变更，免得一次改动顺手扩大范围。
- 不顺手清理 `prototypes/` 中尚未同步到权威文档的结论。

**成功判据**：

- `valid/event-node-origin.json` 与 `valid/error-before-auth.json` 都被客户端解码边界接受，`origin.kind: "node"` 的合法事件能被 ACK、能被呈现（有连接级用例证明游标前进）。
- `fixtures/sync/v1/manifest.json` 里 `valid` 且 schema 为 `message.schema.json` 的**每一条**固定样例都被解码边界接受，且这条断言**由固定样例驱动、在 `clients/app` 的 `npm run check` 里执行**：把运行期词表改回四值、或把 `error` 放回纯认证后集合，该用例必须变红（判别力可演示）。
- 两个 `invalid/event-origin-node-*` 固定样例被拒收且拒绝原因可区分（`nodeId` 缺席 → `missing_field`；`nodeId: null` → `field_type`）。
- `origin.kind` 的类型、schema 枚举与运行期词表三者在编译期互相钉住：给 `EventOriginKind` 加第六个取值而不同步运行期词表会**编译失败**，不再是运行期静默拒收。
- `crates/server/src/sync/mod.rs` 的模块文档与 `crates/server/src/sync/` 的实际落地一致（`event` 腿已落地、`Broker::replay` 的直连只剩保留窗口探测一处）。
- `README.md` 与 `docs/FRONTEND_DESIGN.md` 对「浏览器级验收是否已完成」只有一种口径，且与归档的真机验收记录一致。
- `cd clients/app && npm run check` 全绿；`npm run check`（仓库根）全绿（它不覆盖 `clients/app`，但必须证明未被波及）。

## What Changes

**一、运行期词表补齐第五个取值，并与类型钉住（修缺陷 A：`origin.kind: "node"`）**

- `wire.ts` 的 `EVENT_ORIGIN_KINDS` 增加 `node`。
- 把该表的类型从 `Record<string, true>` 改为 `Record<EventOriginKind, true>`：它因此成为 `EventOriginKind`（已被 `protocol/contract.test.ts` 的编译期镜像钉在 schema 枚举上）的运行期投影，三者形成 `schema → 类型 → 运行期词表` 的闭合链，未来再漏一处会在 `tsc` 阶段报错，而不是变成「合法事件被拒收」。

**二、`origin` 的 node 配对规则在解码边界生效（修缺陷 A 的另一半）**

- 按 `docs/SYNC_PROTOCOL.md` §10.1 唯一那条配对规则校验：`kind = "node"` 时 `nodeId` 必须存在且为非空 UUID、`deviceId` 必须为 `null`。
- 其余四种 `kind` **不新增任何**身份字段必填性（尤其不要求 `kind: "device"` 必须带非空 `deviceId`）——那是 §10.1 与 §16.2 明确列为破坏性、不得顺手加的收紧。
- 未协商 `core.event-origin.v1` 的冻结形状（`{kind, deviceId}`，无 `nodeId` 键）继续被接受：它今天就在通过（`valid/event-origin-legacy-shape.json`），不得因本次改动变红。这一条同时解释了为什么上一条是安全的——服务端 MUST NOT 向未协商的连接投递 `kind = "node"`。

**三、`error` 帧接受两种信封（修缺陷 B：认证前错误被拒收）**

- `error` 从「必须有 `connectionId`/`connectionSequence`」的集合里拿出来，改为「两者**同时出现或同时缺席**」：同时出现时按既有 UUID/`decimalString` 形状校验；只出现一个时按信封违规拒收（与 schema `oneOf` + `unevaluatedProperties: false` 的判定一致）。
- `control.ping`/`control.pong` 保持在认证后集合里不动（schema 里它们只有 `postAuthBase` 一种信封）。

**四、把固定样例变成解码器的门禁（不再靠手写样例）**

- 新增一条**固定样例驱动**的用例：`fixtures/sync/v1/manifest.json` 里 `valid: true` 且 schema 指向 `schemas/sync/v1/message.schema.json` 的每条样例，其磁盘原文都必须被解码边界接受。今天这份清单是 101 条，且**覆盖解码器支持的每一个 `type`**（18 个），因此这条门禁对每个消息类型都有一次正面覆盖。
- 同一份用例里对两个 `invalid/event-origin-node-*` 样例断言拒绝原因（`missing_field` / `field_type`），钉住配对规则的两个分支。
- 复用既有 `clients/app/src/protocol/manifest.ts` 的 `readManifest`/`resolveFixtureCase`，不新增第二份 manifest 解析（也不经 `protocol` 的 barrel 导入：`contract.test.ts` 断言该 barrel 不在运行期导出 `manifest`）。
- 明确**不**做「所有 `invalid` 样例都必须被拒收」的门禁：今天有 20 条 schema 非法的样例会被解码器接受，而这是既定口径（值域归 `schemas/**` 与降级路径）。把它写成门禁等于要求客户端实现全量 JSON Schema 校验，属另一件事。

**五、连接级回归：合法事件真的推得动游标**

- 用既有 `clients/app/src/sync-client/testing/harness.ts` 的假 socket 驱动一条 `origin.kind: "node"` 的事件，断言它被提交进账本并触发 ACK（游标前进），而不是只被 `rejected` 掉。这是本变更的**可观察行为**证据：固定样例门禁证明「解得开」，这条证明「推得动」。

**六、文档过期（不承载规格要求）**

- `crates/server/src/sync/mod.rs` 的模块文档：已落地切片列表补上 `event`；删掉「尚未落地：事件投递腿」那段；把「过渡形态」段落（`UseCases::sync_replay` 的「任务 1.6 在途」、`Broker::replay` 直连范围）改成与 `crates/server/src/sync/subscribe/mod.rs` 今天的自述一致。
- `docs/FRONTEND_DESIGN.md`：§4.1 的未闭合边界①、§9 的 `[实现现状]`、以及 §9 上方的修订记录行，三处统一到「真实 Daemon 的浏览器级端到端验收已于 2026-10-16 执行」这一事实；口径以归档的 `openspec/changes/archive/2026-10-16-frontend-sync-wiring/plan.md` 的「真机验收记录」为准（逐条实测输出、7 项结果、两项当批修掉的缺陷）。`README.md` 与之一致，因此**不改**——除非核对时发现 README 才是不准的那一方，那就改 README 而不是改证据。

## Capabilities

### New Capabilities

（无。本变更是对既有客户端行为的修正，不引入新能力。）

### Modified Capabilities

- `pwa-web-client`: 客户端解码边界必须接受每一条协议合法帧（含 `origin.kind` 的五值词表与 `error` 的两种信封）、`origin` 的 node 配对规则、固定样例驱动的门禁，以及「浏览器级端到端验收的执行状态在各权威文档中只有一种口径」。
<!-- 唯一的 delta 文件：specs/pwa-web-client/spec.md。服务端模块文档与 README 的一致性核对不承载规格要求（无可观察行为变化），只落在 tasks/plan。 -->

## Impact

- **前端（本变更的主战场）**：`clients/app/src/sync-client/wire.ts`（词表、表类型、`origin` 配对规则、`error` 信封判定）、`clients/app/src/sync-client/` 下新增的固定样例门禁用例与连接级回归用例。
- **前端类型/资产**：`clients/app/src/protocol/event.ts` 与 `schema`、`fixtures/sync/v1/manifest.json` 的**读取**（只读，不改资产）。
- **文档**：`crates/server/src/sync/mod.rs`（模块文档）、`docs/FRONTEND_DESIGN.md`（§4.1①、§9、修订记录行）。`README.md` 大概率不动（它与真机验收记录一致）。
- **门禁**：`cd clients/app && npm run check`（typecheck + `typecheck:e2e` + vitest + `expo export`）是本变更的主门禁；CI 的必需检查 `前端检查（clients/app）` 覆盖它。根 `npm run check` 与 `cargo test` 不应受影响，但必须证明（`crates/server/src/sync/mod.rs` 的注释改动会过 `cargo fmt`/`clippy`，不改行为）。
- **不受影响**：wire schema、fixture 资产、feature registry、命令目录、`compatibility/`、Rust 行为、Node Link 面、安全边界。本变更不改变任何 feature 协商、授权或数据保护语义，因此**不需要**更新 `docs/SYNC_PROTOCOL.md`、`docs/SECURITY_DESIGN.md` 或新增 ADR。
