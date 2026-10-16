# Design

## Context

见 `proposal.md` — Why。以下是重塑方案的现状事实（都按代码与资产逐点核对过，不是推测）。

**解码边界是客户端唯一的运行期判定点。** `src/protocol/**` 只描述类型、不做运行期校验（`wire.ts` 头部自述），`docs/FRONTEND_DESIGN.md` §3 与前端 `design.md` D8 禁止协议载荷直接进入视图模型，因此「一条帧接受还是拒收」这个决定只发生在 `clients/app/src/sync-client/wire.ts`。

**三处不同步的具体位置：**

| 事实 | 位置 |
|---|---|
| schema 的 `origin.kind` 是**五值**（含 `node`） | `schemas/sync/v1/event.schema.json:39`，配对规则在同一 `if/then`（`:44-53`） |
| 客户端类型也是五值 | `clients/app/src/protocol/event.ts:39`（`EventOriginKind`），`nodeId?: Uuid`（`:51`） |
| 运行期词表仍是**四值** | `clients/app/src/sync-client/wire.ts:156-161`，判定在 `:464-467` |
| 类型↔schema 有编译期镜像，但**运行期表没人管** | `clients/app/src/protocol/contract.test.ts` 的 `ORIGIN_KIND_MIRROR`（`Record<EventOriginKind, true>`）只钉前两者 |
| `error` 被归入「认证后必须有 `connectionId`」的集合 | `wire.ts:87-103`（`POST_AUTH_TYPES`）、判定在 `:247-256` |
| 协议里 `error` 是**两种信封**的 `oneOf` | `schemas/sync/v1/error.schema.json#/$defs/error`；客户端类型 `clients/app/src/protocol/error.ts:38` 已经是 `PreAuthBase \| ErrorPostAuthBase` 的联合 |

**固定样例已经存在，只是没人真的喂给解码器。** `fixtures/sync/v1/valid/event-node-origin.json`（`kind: node` + `nodeId`）、`fixtures/sync/v1/valid/error-before-auth.json`（无 `connectionId`）、`fixtures/sync/v1/invalid/event-origin-node-missing-node-id.json`（无 `nodeId` 键）、`fixtures/sync/v1/invalid/event-origin-node-without-node-id.json`（`nodeId: null`）。`wire.test.ts` 全部用手写样例，`contract.test.ts` 只把样例拿去校验 schema——**没有一条用例把样例交给解码边界**，所以两处都绿。

**本次立项时实测的基线**（临时探针脚本，跑在 OS 临时目录、未落仓库）：`manifest.json` 里 `valid` 且 schema 指向 `message.schema.json` 的 **101 条**样例中 **99 受理 / 2 拒收**，两条拒收正是上面那两个；这份清单**覆盖解码边界支持的每一个消息 `type`（18 个）**。同一探针显示 35 条 `invalid` 消息样例中有 **20 条**会被解码边界受理——这是**既定口径**（值域归 `schemas/**` 与 `projectView()` 的降级路径），不是缺陷。

**为什么拒收的后果是「游标停滞」而不是「少显示一条」：** 解码失败只发一条 `rejected` 诊断、不升级为阻断态（`connection.ts:1054-1062`），该帧因此进不了事件处理路径，而 ACK **只在账本提交成功之后**才发出（`connection.ts:733`）。于是这条事件永不被确认，服务端按最后 ACK 的游标反复补发同一条，客户端游标停在该事件之前。

**`node` 来源什么时候真的会出现：** `origin.kind = "node"` 表示事件由**另一个节点经 Node Link 下发的命令**在本机事件库落库（`docs/SYNC_PROTOCOL.md` §10.1 的取值表），core 侧由 `crates/core/src/broker.rs:4154-4168` 的 `event_origin` 对 `Actor::Node` 给出——即 Node Link 首个纵向切片的远程 `session.create` 落下的 `session.created`。同一节还写明：`origin` 的形状由 feature `core.event-origin.v1` 门控，**未协商的连接不得收到 `kind = "node"`**，且「发出方 MUST NOT 产生 `{kind: "device", deviceId: null}`，但 schema 与接收方不得因此拒绝它」。

**文档侧的现状事实：** `crates/server/src/sync/event/mod.rs` 与 `event/tests.rs` 已存在（402 + 899 行），而同一个父模块 `crates/server/src/sync/mod.rs:2-19` 的已落地列表里没有 `event`、并仍写「尚未落地：事件投递腿」；`subscribe/mod.rs:52-57` 的自述已经是「只剩保留窗口探测一处直连 `Broker::replay`」。`docs/FRONTEND_DESIGN.md` §4.1①、§9 的 `[实现现状]`、§9 上方修订记录行三处都称真实 Daemon 的浏览器级验收「尚未执行」，而归档的 `openspec/changes/archive/2026-10-16-frontend-sync-wiring/plan.md` 的「真机验收记录」逐条给出了 7 项实测结果（1–6 通过、第 7 项如实登记为待裁决并在后续变更 `ui-csp-inline-styles` 中闭合），`README.md` 的表述与它一致。

## Goals / Non-Goals

**Goals:**

- 让「解码边界受理哪些帧」由**协议资产**定界，并把它变成一道**固定样例驱动**的门禁：以后再有同类漂移，门禁会红，而不是等到某台机器上某个客户端静默拒收合法帧。
- 让本次发现的两种漂移在**编译期或门禁**里不可复现：类型加取值而运行期表没跟上 → 编译失败；运行期判定与协议资产不一致 → 用例失败。
- 后果面（游标推得动）有一条连接级用例，而不只是「函数返回 ok」。
- 三处过期文档与代码/记录一致，且口径只有一种。

**Non-Goals:**

- 不在客户端做全量 JSON Schema 校验：值域、视图内部形状仍归 `schemas/sync/v1/**` 与既有降级路径（`wire.ts` 开头已写明这条边界，本次不改）。
- 不因为修 `error` 就顺手放宽 `control.ping`/`control.pong` 或其它类型的信封判定。
- 不重构 `wire.ts` 的其它封闭词表；只有当门禁证明它们也在漂移时才动（见 Risks 的处理规则）。
- 不改 wire、不改 feature 协商、不改服务端行为、不新增前端功能。

## Decisions

**D1 运行期词表用 `EventOriginKind` 钉住，而不是再抄一份字面量。**

把 `EVENT_ORIGIN_KINDS` 的类型从 `Record<string, true>` 改为 `Record<EventOriginKind, true>`。它因此成为 `EventOriginKind` 的运行期投影，而后者已被 `contract.test.ts` 的编译期镜像钉在 schema 枚举上，于是形成 `schema enum → EventOriginKind → 运行期词表` 的闭合链：以后给类型加取值而漏改运行期表，`tsc` 直接失败。

- 备选：只补一个 `node: true`。否——这正是本次的成因，同样的漏改会再来一次。
- 备选：运行期从仓库根 `schemas/**` 读枚举。否——`contract.test.ts` 断言 `clients/app` 下不存在 schema 副本，且 `expo export` 的产物不携带仓库根资产；客户端在生产里拿不到那份文件。

**D2 node 的配对规则只在解码边界加一条，且只对 `node` 生效。**

`kind = "node"` 时要求 `nodeId` 存在且为 UUID、`deviceId` 为 `null`；其余四种取值**不新增任何**身份字段必填性。

- 依据：这是 §10.1 唯一的配对规则；「未协商 `core.event-origin.v1` 的连接不得收到 `kind = "node"`」保证这条收紧不会误伤冻结形状的接收方。
- 备选：要求 `kind = "device"` 必须带非空 `deviceId`。否——§10.1 与 §16.2 明文列为破坏性变更、不得顺手加，且会把 `valid/event-origin-legacy-shape.json` 误杀。
- 备选：把 `nodeId` 校验留给视图层。否——信封与判别字段是解码边界的职责；`protocol/event.ts` 的 `nodeId?: Uuid` 已经把这条形状契约放在协议类型层。

**D3 `error` 的信封判定改成「连接标识与连接序号同时出现或同时缺席」。**

两者同时出现时按既有 UUID / `decimalString` 形状校验；只出现一个时按信封违规拒收——与 schema 的 `oneOf` + `unevaluatedProperties: false` 判定一致（`preAuthBase` 不允许出现 `connectionId`，`postAuthBase` 两个都必填）。`control.ping`/`control.pong` 只保留在认证后集合。

- 备选：把 `error` 从认证后集合里直接删掉、不做任何信封判定。否——「只带 `connectionId` 不带 `connectionSequence`」这个中间形状会被静默放宽，而 schema 明确拒绝它。
- 备选：给每个消息 `type` 建一份「类型名 → 信封要求」的类型级映射，让它像 D1 一样在编译期钉住。否——`ErrorMessage` 本身就是两种信封的联合类型，条件类型无法把它归到一侧；其余类型今天都正确，而固定样例门禁对**每个 `type`** 都有正面覆盖，是更便宜的同等保障。

**D4 门禁是固定样例驱动的**正向**断言，不做「全部 valid 通过、全部 invalid 拒收」。**

新用例只断言「`valid` 且 schema 为 `message.schema.json` 的每条样例必须被受理」，另加两个 node 负例的定点原因断言。理由：实测 20/35 条 `invalid` 样例会被解码边界受理，而这是既定口径（值域归 schema 与降级路径）；要求全部 invalid 被拒等于要求客户端实现全量 JSON Schema 校验，属另一件事，会把这个门禁变成假需求。

- 落点：新建 `clients/app/src/sync-client/wire-fixtures.test.ts`（与解码器同目录，vitest 的 `include` 是 `src/**/*.test.ts`，无需改配置）。
- 依赖复用：从 `clients/app/src/protocol/manifest.ts` 导入 `readManifest`/`resolveFixtureCase`，路径解析用既有 `protocol/paths.ts` 的仓库根；**不经 `protocol` 的 barrel 导入**——`contract.test.ts` 断言该 barrel 的运行期导出里不含 `manifest`。
- 过滤条件写成可读的一条：`valid === true && schema.endsWith("message.schema.json")`。这同时排除了配对 HTTP 载荷与视图定义两类样例（它们本来就不是 WSS 消息，`type` 字段根本不存在）。

**D5 后果面用既有 harness 驱动一条连接级用例。**

复用 `clients/app/src/sync-client/testing/harness.ts` 的 `FakeSocketFactory` / `makeAuthenticated` / `makeEvent`，喂一条 `origin.kind = "node"` 的事件，断言它被提交进账本并发出 ACK（游标前进）。

- 备选：只断言解码返回受理。否——那证明不了本变更要修的可观察后果（见 Context 的「游标停滞」）。
- 落点：`clients/app/src/sync-client/` 既有用例文件（与 harness 同目录），沿用其既有前置（认证完成后再喂事件）。

**D6 文档口径以证据为准，改错的那一方。**

唯一事实源是归档 `2026-10-16-frontend-sync-wiring/plan.md` 的「真机验收记录」（同日 `ui-csp-inline-styles` 的 proposal 也引用了同一次验收）。据此：`docs/FRONTEND_DESIGN.md` 的 §4.1①、§9 `[实现现状]`、§9 上方修订记录行三处是过期的一方，按记录改写；`README.md` 与记录一致，**不改**。修订记录行是历史记录，但它的措辞断言了「本次变更登记为未执行」，与同一次变更当天的实测不符，因此补上同日的执行事实，而不是保留一个与证据相反的表述。

- §9 第 11 条的前置条件（`sync-path-browse-and-diff-delivery` 合入前不得声称已验收）同样过期，但**不在本次范围**（proposal 的非目标已登记），避免一次改动顺手扩大范围。

**D7 `crates/server/src/sync/mod.rs` 的修正只改模块文档、不改代码。**

- 已落地切片列表补 `event`（它今天声明在 `pub mod event;` 且实现与测试都在）；
- 删掉「尚未落地：事件投递腿」那段；
- 「过渡形态」段落改成与 `crates/server/src/sync/subscribe/mod.rs:52-57` 今天的自述一致：事件投递只走 `UseCases::sync_replay`，`Broker::replay` 的直连只剩保留窗口探测一处，不再写「任务 1.6 在途」。

理由：模块文档是别人接手时的第一入口，与实现相反的文档比没有文档更贵；而这三处都是「代码已经往前走、文档没跟」这一类。

## Risks / Trade-offs

| 风险 | 缓解 |
|---|---|
| 固定样例门禁可能立刻暴露**第三处**漂移（101 条里目前全绿除两条） | 这正是设计目的。处理规则写进 tasks：若某条有效样例因「与 schema 一致的合法形状」被拒，就按同一原则修解码边界；若发现的是**协议资产自身**的问题（schema/fixture 有误），停止该工作包并回到变更层讨论——那属于 wire 变更，超出本变更范围 |
| 收紧 node 配对规则造成误伤（把本该校验通过的形状拒了） | 只加 `node` 一条、依据 §10.1 唯一规则；冻结形状样例（`valid/event-origin-legacy-shape.json`）与既有 `wire.test.ts` 的「词表外取值被拒」负例都在门禁里兜底 |
| 编译期绑定让将来的合法扩展变成编译错误 | 这是期望效果：`EventOriginKind` 只在 schema 同步改时才动，而 `contract.test.ts` 已经要求「改类型必须同步 schema 枚举」 |
| 前端是独立 npm 工程，容易被漏跑 | 本次全部前端改动都在 `clients/app` 内；Project Verify 显式在该目录执行 `npm run check`，CI 的必需检查 `前端检查（clients/app）` 覆盖它 |
| 「只是注释」的文档改动也可能改错 | 判据逐句对照来源：`mod.rs` 的每条声明都要在 `crates/server/src/sync/` 找到对应实现；`FRONTEND_DESIGN.md` 的每句结论都要能在归档真机验收记录里找到对应项 |
| 门禁用例与 `contract.test.ts` 职责重叠（都读同一份 manifest） | 分工写清：`contract.test.ts` 管「样例 ↔ schema」的一致性（ajv 侧），新用例管「样例 → 解码边界」的受理面；两者都读仓库根同一份资产，不新建第二份清单 |

## Migration Plan

- **无数据迁移、无 DDL、无 wire 变更、无 feature 协商变更**：本变更只改客户端的消费侧判定与三处文档。
- **部署**：随前端产物发布。PWA 由 Daemon 同源托管，升级 Daemon 即同时换掉页面；未刷新的旧标签页在刷新前仍带旧判定——最坏情形是它继续拒收 `node` 事件（与今天完全一致），不是本次新引入的行为。
- **回滚**：回退前端产物即可；不涉及数据、不涉及服务端、不涉及 wire 兼容窗口。

## Open Questions

- 无。门禁若暴露第三处漂移，其处理规则已写在 Risks（它可能改变任务清单，因此不能作为可延后项挂在 Open Questions 里）。
