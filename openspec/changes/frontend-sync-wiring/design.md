<!-- 本变更的方案：接线取舍与实施路径。动机见 proposal.md，行为见 specs/。 -->

# Design

## Context

动机见 `proposal.md`。前置是 `sync-server-connection`、`sync-server-commands`、`sync-ui-hosting`（Daemon 侧完整且能同源托管页面）。这里只记决定方案形状的现状与约束。

**现状事实（本次已核实）**

| 事实 | 证据 |
|---|---|
| 连接地址缺 `/v1` | `clients/app/src/composition.ts` 的 `syncUrlFor` 生成 `wss://<origin>/sync` |
| 建立 socket 时不带子协议 | `clients/app/src/sync-client/socket.web.ts` 的 `new WebSocket(input.url)` 单参调用 |
| socket 工厂签名不含子协议 | `clients/app/src/sync-client/ports.ts` 的 `SocketFactory` 只收 `{ url, handlers }` |
| `approved` 分支只取 deviceId | `clients/app/src/composition.ts` 的 `poll` 返回 `{ kind: "approved", deviceId }` |
| 已配对记录需要主机公钥 | `ports.ts` 的 `PairedHostRecord { hostId, publicKeyBase64Url, canonicalOrigin }` |
| 配对页无输入入口 | `clients/app/app/pair.tsx` 只渲染 `PairingPanel` 的状态；`PairingPanel` 无 input |
| e2e 靠预写配对记录绕过真实配对 | `clients/app/e2e/page/support.tsx` 的模块注释明说「真实配对在运行时不可达」 |
| 前端源码注释把路由写作 `#/pair` | `clients/app/app/pair.tsx` 头注释与 `index.tsx` 头注释 |
| 二维码把内容放在片段 | `crates/server/src/local_admin/pairing.rs` 的 `pairing_url` 生成 `<origin>/pair#data=<base64url-json>` |
| 客户端已声明必需 feature 但不校验选中结果 | `connection.ts` 的 `REQUIRED_FEATURES` 只在 hello 里发送，`#onServerChallenge` 不比对 |

**约束**

- `AGENTS.md` §6：页面不能直接读写 WebSocket/IndexedDB/SecureStore/SQLite；平台能力只经端口与组合根装配。
- `AGENTS.md` §6：私钥不得存入 `localStorage`；浏览器安全能力不足只能进入显式开发模式。
- `docs/FRONTEND_DESIGN.md` §4.5：一个 PWA 身份只绑定一个 canonical origin；来源变化必须重新配对。
- 前端是独立 npm 工程：`cd clients/app && npm run check`（typecheck + vitest + `expo export`）是它的门禁，不在根 `npm run check` 链上。

## Goals / Non-Goals

**Goals**

- 四处接线缺口全部闭合，且每处都有可证伪的断言（不是「看起来能跑」）。
- 消解两处口径冲突（`/sync` vs `/sync/v1`；井号路由 vs 配对片段）。
- 验收证据来自真实浏览器 + 真实 Daemon，对假主机的用例降级为补充。

**Non-Goals**

- Rust 侧任何改动。
- 前端富内容渲染、主题、移动端断点、安装引导、i18n。
- 原生客户端（Android/iOS）；本变更只碰 Web/PWA 路径。

## Decisions

### D1：子协议标识从协议 crate 的机器登记取，不在前端硬编码

前端需要一个子协议字面量。它已在机器资产里登记（`compatibility/errors/v1/errors.json` 的 Sync 段 `subprotocol`），因此**唯一来源**是那份登记；前端通过既有的合同测试机制（`clients/app/src/protocol/contract.test.ts` 已读仓库根资产）断言自己用的值与登记一致。

**备选与否决**：直接在 `socket.web.ts` 写死字符串——会被将来「登记改了前端不知道」的漂移击穿；前端已有一个读根资产的契约测试文件，复用它成本最低。

**实现要点**：`SocketPort`/`SocketFactory` 的入参需要容纳「声明的子协议列表」，由组合根传入；`socket.web.ts` 把它作为 `new WebSocket(url, protocols)` 的第二个参数。**这属于端口签名变化**，必须同步改测试里的假 socket 与所有构造点（`clients/app/e2e/page/fake-host.ts` 记录了 `socketUrls`，需要一并记录子协议以便断言）。

### D2：`approved` 分支取主机材料，并让「缺材料」成为可区分失败

`statusResponse` 在已批准时**必须**同时给出设备元数据与主机身份材料（协议用条件结构强制）。前端目前只看 `device.deviceId`。

**决定**：`poll` 的已批准结果扩展为携带主机标识与主机公钥；配对页在拿到它之后才调用组合根写入已配对记录（`rememberPairedHost` 已存在，只是没有生产调用方）。缺少主机材料时**不进入已配对**，而是返回可区分的失败——因为已配对记录缺公钥会让后续每次连接都进「主机身份变化」阻断态，那比当场失败更难诊断。

**为什么不「先标记已配对、等连接时再补公钥」**：`verifyHostChallenge` 用已配对记录里的公钥验签（`ports.ts` 的 `HostIdentityPort`），没有公钥就没有可验证的信任根；补不出来，只能重配对。

### D3：v1 的二维码入口用「粘贴」而不是扫码

设计原型要求扫码，但本变更的取舍是：**v1 提供粘贴入口**（一个受控输入框 + 校验 + 提交），扫码作为可选增强。

**理由**：

1. 扫码在 Web 上需要 `getUserMedia` + 解码库，属新增依赖（`AGENTS.md` §8 要求逐项核对），且需要 secure context（同源 https 部署已满足，但开发环境未必）。
2. 「粘贴二维码内容」在 `PairingPanel` 的既有文案里已被提及（`awaiting_qr` 阶段的指引写「把摄像头对准电脑上的配对二维码，**或粘贴二维码内容**」）——说明该路径已被设计接受。
3. 配对是完成一次后长期不复用的动作（`docs/FRONTEND_DESIGN.md` §4.5：正常重连不要求重复扫码），不值得为它引入一个解码依赖。
4. 桌面浏览器调试路径也因此可用（没有摄像头也能配对）。

扫码可在后续变更按需补上，届时只要把「得到一段文本」这一接口接上同一个校验与认领流程。

### D4：路由与片段争用的取舍——**沿用片段承载配对内容，把路由从片段改为正常路径**

这是本变更唯一「需要判断」的地方，因为两条既有事实互相冲突：二维码把 JSON 放在 `#data=...`，而源码注释称路由是 `#/pair`。

**决定**：配对内容**必须**留在片段（协议规定秘密必须在 fragment，不进 query——`SYNC_PROTOCOL.md` §7.1：「秘密部分必须位于 fragment」）。因此**让路由走正常路径**（`/pair`、`/hosts`、`/dir/<alias>`、`/session/<id>`），片段只承载配对内容。这同时是 `sync-ui-hosting` 选「挂根目录」的另一收益：正常路径路由需要服务端的单页回退，而回退已就位。

**验证这一点是否成立**：Expo Router 在 Web 上默认用正常路径（`expo-router` 的 web 输出为 `output: "single"`，`app.json` 未开启片段路由）。源码注释里的 `#/pair` 与实际行为**不一致**——本变更需要**核实**（读构建产物或用 e2e 观察地址变化），并把注释与行为对齐；若实际是片段路由，则改为正常路径路由（Expo Router 支持），因为协议不允许把秘密挪出片段。

**为什么不反过来**：把配对内容挪到 query 会让秘密进服务器日志、Referer 与浏览器历史（协议明令禁止这一点）。**这条不可协商。**

### D5：补齐「必需 feature 未被选中」的处理

客户端在 hello 里声明了 `requiredFeatures`（ACK 与命令状态查询），但当前不校验服务端的 `selectedFeatures` 是否覆盖它们。协议要求服务端在必需 feature 未满足时返回 `protocol.feature_required`；客户端侧的正确行为是：**若服务端未选中必需 feature，进入可区分的阻断态**（而非照常使用一条缺能力的连接）。

**为什么纳入本变更**：它属「连接契约对齐」的同一族缺口（前端信任了未经验证的协商结果），且改动极小（在 `#onServerChallenge` 里加一次集合包含判定）。不纳入会让「必需 feature」这句话在前端成为空文。

### D6：既有 e2e 的登记必须更新，而不是保留

`clients/app/e2e/page/support.tsx` 明确登记「真实配对在运行时不可达……预置的是『已配对』这个起点」。本变更让配对可达，因此：

- **更新该登记**：真实配对入口可用后，预置路径要么改为走真实配对，要么明确保留为「多数用例的起点、配对本身由专门的用例覆盖」。
- **新增真实配对用例**：至少一条用例从「未配对」出发，经输入二维码内容、认领、批准、写入主机记录，进入已配对。该用例在真实浏览器里对**真实 Daemon**（经 `sync-ui-hosting` 的同源托管）执行——这是 `docs/WORKFLOW.md` §7 要求的可观察行为断言，不是启动成功。

## Risks / Trade-offs

- **[端口签名变化波及面]** `SocketFactory` 增加子协议入参会波及假 socket 与既有测试 → 所有构造点在同一变更内更新；测试里的假 socket 需记录子协议供断言（这样「有没有真的声明」可被证伪）。
- **[路由模式的核实成本]** 若 Expo Router 实际用片段路由，D4 需要改前端路由配置 → 核实放在实现第一步，改配置的代价已知且有限（正常路径路由是 Expo Router 的默认模式）。
- **[粘贴入口的可用性]** 用户要从电脑上复制二维码内容（而不是扫码）→ 与 `PairingPanel` 既有指引一致；此外配对二维码本身是 URL，用户也可以用「在手机上打开二维码里的链接」这一更强的路径，二者共用同一校验。
- **[内联样式与 CSP]** `sync-ui-hosting` 登记的冲突（Expo 产物含内联 `<style>`，基线 CSP 会拒它）会在本变更的浏览器验收时暴露为样式异常 → 若它阻断验收，在本变更内解决（把 reset 样式外置，或在前端设计文档里显式放行 `style-src` 并同步 CSP 基线）；**不得**在 Rust 侧放宽 CSP 掩盖。
- **[真实浏览器验收的环境依赖]** 端到端验收需要「同源 https 部署 + 真实 Daemon」在验收机器上可跑 → 若环境不可得，按 `docs/WORKFLOW.md` 如实登记为未验证项，MUST NOT 以假主机用例冒充；但本变更的**主验收**必须包含真实路径，否则本变更等于没验。

## Migration Plan

- **无存储与协议变更**：纯前端改动，无 migration。
- **部署顺序**：前提是 Daemon 侧三个变更已合入并可同源托管；否则本变更做完了也看不到效果。
- **回滚**：回滚即回到「页面能加载、连接失败」的状态（前三步仍有效）。
- **前端本地缓存**：已配对记录的键含规范化来源；本变更不改该键形状，因此不涉及缓存迁移。若某设备的记录是旧路径下写入的（无主机公钥），其表现是「连接时进入主机身份变化阻断态」——这与 D2 的判定一致，恢复路径是重新配对。

## Open Questions

- 扫码是否在后续变更补上（D3 已定 v1 用粘贴，扫码属可选增强）。
- 前端构建版本的载体（属 `sync-ui-hosting` 的 Open Question，不在本变更）。
