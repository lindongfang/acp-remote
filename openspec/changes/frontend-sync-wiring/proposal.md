<!-- 记录动机、范围、能力与影响；行为写 specs，方案写 design，协作写 plan。 -->

## Why

前三个变更做完后，Daemon 侧已经完整：Sync 端点（配对、认证、订阅、快照、命令、事件）与同源静态托管都在。但**浏览器仍然连不上**——前端这一侧有四处与已冻结合同不符的接线缺口，每一处都足以让连接失败：

| # | 现状 | 依据 | 后果 |
|---|---|---|---|
| 1 | 连接地址是 `wss://<origin>/sync` | 前端 `composition.ts` 的 `syncUrlFor` | wire 权威规定端点为 `/sync/v1`，连到 404 |
| 2 | 建立 WebSocket 时**不声明** subprotocol | 前端 `sync-client/socket.web.ts` 的 `new WebSocket(url)` | 服务端要求声明并选中 `acp-remote.sync.v1.json`，否则拒绝升级 |
| 3 | 配对成功后**丢弃**服务端返回的主机公钥 | 前端 `composition.ts` 的 `approved` 分支只取 `device.deviceId` | 该公钥是下一次连接验 `hostProof` 的唯一依据，缺它必然进入身份变化阻断态 |
| 4 | 配对页**没有**任何二维码输入入口 | 前端 `app/pair.tsx` 与 `PairingPanel` 只渲染状态 | 用户无法完成配对；e2e 只能预写配对记录绕过 |

因此这是「PWA 真正可用」的最后一块。本变更同时消解两处文档与实现的口径冲突（`/sync` vs `/sync/v1`；配对页注释里的井号路由与二维码 `#data=` 争用同一片段位置）。

## What Changes

**一、连接契约对齐（缺口 1、2）**

- 前端 SHALL 按 wire 权威的端点连 `/sync/v1`；`docs/FRONTEND_DESIGN.md` §4.3 与 `docs/INITIAL_DESIGN.md` §6.3 中把端点写作 `/sync` 的表述已由 `sync-server-connection` 修正，本变更确保实现与之一致。
- 前端 SHALL 在 WebSocket 升级请求中声明服务端要求的 subprotocol，使服务端可以选中它；子协议标识 SHALL 与机器登记一致，MUST NOT 硬编码第二份。
- 连接被拒（未选中 subprotocol、协议版本不兼容）时 SHALL 进入可区分的阻断态并给出可读原因，MUST NOT 静默重连。

**二、配对链路闭合（缺口 3、4）**

- 配对状态为已批准时 SHALL 取用响应中的设备元数据**与主机身份材料**，并据此写入已配对主机记录（含主机公钥），使后续连接能验证主机证明。
- 配对页 SHALL 提供二维码内容的输入入口（扫码或粘贴），并 SHALL 在读取后按协议要求清除地址片段中的配对秘密；秘密 MUST NOT 进入日志、localStorage 或可导出配置。
- 配对完成后 SHALL 能进入已配对状态并触发连接，MUST NOT 停在等待扫码。

**三、路由与片段的争用消解**

- 配对二维码形如 `https://<origin>/pair#data=<base64url-json>`，而前端源码注释把路由写作 `#/pair`——同一个片段位置被两种用途争用。本变更 SHALL 明确取舍（见 design）并保证：二维码里的 `data` 能被读到、清除片段后仍停留在配对页、其余路由的正常路径可达。

**四、前端自检**

- 上述四项 SHALL 有可执行的验证：连接地址与 subprotocol 的断言（可证伪）、`approved` 分支写入主机公钥的断言、配对页输入入口到「已配对」的端到端断言。
- 既有 e2e 里「真实配对在运行时不可达」的登记与绕过路径（预写配对记录）SHALL 在本变更后更新：要么改为走真实配对入口，要么保留并说明剩余边界。

**本次不包含**：

- Rust 侧任何改动（Daemon 已就绪）。
- 内联样式与 CSP 的冲突（由 `sync-ui-hosting` 登记，若在本变更内解决则一并处理——见 design 的取舍）。
- 前端富内容渲染、主题、移动端断点、安装引导（属 `prototypes/IMPLEMENTATION-GAPS.md` §5 的待决策项，不在本批）。

## Capabilities

### New Capabilities

（无新增能力。本变更只让既有能力真正可用。）

### Modified Capabilities

- `pwa-web-client`: 「连接状态是互斥状态机」的阻断态成因增加「服务端未选中约定子协议」；新增「连接端点与子协议与 wire 权威一致」「配对完成后写入主机身份材料」「配对页提供二维码输入入口并清除片段秘密」「路由与配对片段的争用取舍」四条要求。这些是既有能力的**行为变化**（此前实现与规范不符，现改为与规范一致），因此按修改既有能力处理。

## Impact

- **前端**：`clients/app/src/composition.ts`（连接地址、subprotocol 传递、`approved` 分支）、`clients/app/src/sync-client/socket.web.ts` 与 `ports.ts`（socket 工厂签名需容纳 subprotocol）、`clients/app/src/sync-client/connection.ts`（若需校验服务端是否选中必需 feature）、`clients/app/src/features/pairing-model.ts` 与 `clients/app/app/pair.tsx` 与 `components/PairingPanel.tsx`（二维码输入入口）、`clients/app/e2e/**`（断言与既有登记更新）、`clients/app/app.json`（若井号路由的取舍需要在构建配置上体现）。
- **Rust**：无改动。
- **依赖**：若引入扫码库则新增前端依赖；否则以「粘贴二维码内容」为 v1 入口（见 design 的取舍）。
- **机器合同**：无改动（`schemas/`、`fixtures/`、`compatibility/` 不变）。
- **权威文档**：`docs/FRONTEND_DESIGN.md`（§4.1 的「主机连接与首次配对」实绩、§4.5 的 PWA 身份边界、§5 状态机、§9 验收标准）、`docs/SECURITY_DESIGN.md`（若配对秘密的清理口径需要写实）、`README.md`（前端现状）。
- **验收边界**：本变更是**浏览器级端到端可用**的收口——其验收必须在真实浏览器里对真实 Daemon（经同源托管）完成配对、加载目录与会话、并至少完成一次连接与快照，而不是对假主机。
