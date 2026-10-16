<!-- 记录动机、范围、能力与影响；行为写 specs，方案写 design，协作写 plan。 -->

## Why

配对这一刻是整个系统里**唯一建立长期信任**的时刻。三个问题都集中在这里，本变更一起改。

**一、设备侧对主机的验证只做了一半（漏做，不是设计如此）。** 协议要求设备做三件事，前端**一件都没做**：

| # | 协议要求 | 依据 | 前端现状（实测） |
|---|---|---|---|
| 1 | 用二维码里固定的主机公钥验证 claim 响应的 `hostProof` | `docs/SYNC_PROTOCOL.md:448`「PWA 用二维码中固定的 Host public key 验证」 | claim 处理只读 `pairingRequestId` 就返回，`hostProof` 从未被使用 |
| 2 | `approved` 里的主机公钥必须等于二维码里那把 | 规范未写，但**必要**：`statusResponse` 里**没有任何主机签名字段** | 只比了 `hostId`，公钥只查长度 65，然后原样落盘 |
| 3 | 双方各自算、都显示 SAS | `SYNC_PROTOCOL.md:451`、`SECURITY_DESIGN.md:241`、`identity-pairing` 规格 | `clients/app/src` 全仓 `sas` 零命中 |

第 1、2 条缺失意味着：**设备此后长期使用的那把主机公钥是没人核对过的**——而之后每次 WSS 连接都用它验签。为什么当年漏了：这两条只写在协议文档里，`FRONTEND_DESIGN.md` 与 `pwa-web-client` 规格**都没有**这条要求，前端的权威链里根本没有它。

**二、SAS 在本项目的流程里没有独立的安全职责。** 本协议的信任锚是「二维码/URL **带外**携带主机公钥 + 配对秘密 + 认领的 HMAC 绑定」——SAS 在教科书里的职责（公钥走网络交换时防中间人）已被这条锚承担。逐条核对：认领侧的替换被 HMAC 挡住；「设备最后存下的主机公钥对不对」只有比对能查（SAS 查不到，它算的是二维码里那把）；URL 整条被换与配对秘密泄露这两种最现实的情形，SAS **完全无效**（前者真主机那次配对未被认领、终端上根本没有数字可对照；后者拿到秘密的人能算出同一个数字）。它剩下的是"不依赖代码正确性的人眼检查点"——纵深，不是安全依据。

**三、设备权限层与实际使用方式错配，而且当前默认值是个坑。** `requestedPacks`/`requestedScopes` 允许为空（不点名即零授权），而 `pack.create-session` **不进任何预设**——照常理走完配对的人连上了却开不了会话，还要回电脑上再授一次。同时用户的实际部署是"自己的设备连自己的节点"，权限层带来的表达力（只读设备、按设备降级）没有被用上，而它带来的组合与界面分支是持续的维护面。

**为什么现在做**：①② 之后，配对与命令面的实绩已经稳定；此时重做配对模型，代价最低，而且它是"真实 Agent 端到端可用"之后紧接着要收口的一环。

## What Changes

### 一、验证层：删掉 SAS，补上两件密码学校验

- 设备配对与节点配对的 **SAS 全部删除**：不再派生、不再在配对状态里返回、不再在确认页展示、CLI 不再有 `--sas`；两套 transcript domain（`acp-remote/pairing-sas/v1`、`acp-remote/node-link-pairing-sas/v1`）与它们的固定向量一并删除。
- 设备（PWA）SHALL 用二维码里固定的主机公钥验证 claim 响应的 `hostProof`；验签失败 MUST 明确失败并停止配对流程，MUST NOT 继续轮询或写入任何主机记录。
- 设备 SHALL 在 `approved` 响应上校验其主机公钥与二维码里那把**逐字节相同**（因为状态响应没有主机签名）；不一致 MUST 明确失败，MUST NOT 落盘。
- `SECURITY_DESIGN.md` SHALL 写明"为什么本协议不需要 SAS"（锚走带外 + 认领 HMAC），并按 `AGENTS.md` §9 修订 **ADR-0001**。

### 二、授权层：删除设备权限层

- 设备配对 MUST NOT 再授予 scope：不再有 `pack.*`/`preset.*` 的展开与 `pack.*`/`preset.*` 出现在 wire 或记录中；配对确认页 MUST NOT 再列权限集合；设备记录不再带 scope 集合。
- 逐命令的**设备 scope 授权判定删除**：通过认证的设备可执行任何命令（`authorization.scope_denied` 在设备侧不再由 scope 产生）。
- 前端 SHALL 删除 scope 门控（"需要电脑端授予"的降级态与其说明一并删除）。
- **保留**：设备认证（每连接挑战-应答）、配对确认必须由**本机用户**完成（"发起配对 + 按一次 y"）、设备撤销立即生效、速率与各项上限。
- **保留**：跨节点的 `grant.*`／Export `scopes`／Access 侧 imports grants——那是"向哪个节点导出什么"的边界，不是设备权限层。

### 三、目录：Sync 面允许客户端指定目录

- Sync 面的 `session.create` SHALL 接受客户端提供的目录（替代/补充 `workspaceAlias`），由本机做既有的绝对路径、存在性、目录与 `canonicalize` 校验；未登记目录 MUST NOT 因此被接受为"别名"。
- Node Link 面 MUST 维持"请求不得携带任意 Owner 路径"（`node-link-owner-server` 的现有要求不变）。
- `workspace-resolution` 的路径口径 SHALL 由"规范化路径不得出现在任何对端可见输出中"改为"不得泄漏**客户端尚未知道的**宿主路径"（客户端自己提议的路径不算泄漏）。

### 四、验收

- 浏览器级：设备**拒绝**被篡改的 claim 响应（换 `hostProof`）与被替换主机公钥的 `approved` 响应；正常流程仍能完成配对并进入 online。
- 配对后**不做任何授权设置**即可在目录里创建会话。
- CLI：配对确认只展示名称、指纹与过期时间；不再有 `--sas`，非交互只用指纹逐字校验。
- 机器合同：`compatibility/transcripts/v1/transcripts.json` 少两个 SAS domain，`npm run check` 全绿。

**本次不包含**：

- Node Link 的 `grant.*`／Export 边界（保留）。
- 配对仪式本身（不做"发起即批准"，保持启动配对 + 按一次 y）。
- Node Link 的目录限制（保留"不得携带任意 Owner 路径"）。
- 命令级归因日志（由 `session-create-settle-order` 交付，含"谁在控制"的主体字段）。

## Capabilities

### New Capabilities

（无新增能力。L1/L3 两条新要求归入既有的 `pwa-web-client`。）

### Modified Capabilities

- `pwa-web-client`: 新增「用二维码公钥验 claim 响应的主机证明」「approved 的主机公钥必须与二维码一致」；删除创建入口与目录内入口的 scope 门控及其说明。
- `identity-pairing`: 删除「SAS 派生双方独立」整条；配对请求集合拆开（设备侧不再有 scope，节点侧保留 grant）；落定不再提交设备 scope 集合。
- `identity-handshake`: 登记表去掉两个 SAS domain 的固定向量；认证输出的 scope/grant 措辞收窄到设备不再有 scope。
- `sync-pairing-http`: 「配对确认只能由本机入口完成」去掉"分配初始 scope"；status 的已批准响应不再返回授予的 scope（保留身份材料）。
- `sync-device-auth`: 设备撤销/信任变更的措辞去掉「scope 缩减」这一维度。
- `sync-command-handling`: 删除「逐命令的设备 scope 授权与零副作用拒绝」这一维度（保留幂等、版本冲突、五态与零副作用拒绝的其余来源）。
- `sync-session-create`: 载荷形状放开目录、删除双层授权与设备级授权语义，引用失败的错误分类随之调整。
- `sync-workspace-catalog`: 目录资源的授权过滤维度删除（不再按设备 scope 过滤）。
- `workspace-resolution`: 解析输入归属（别名不再是唯一来源）与路径不泄漏口径。
- `sync-event-delivery`: 事件投递与设备可见会话清单不再按设备 scope 过滤。
- `scope-expansion`: 授权词汇只剩跨节点导出授权（`grant.*`），删除设备授权包与配对预设两类展开。
- `local-admin-methods`: `device.pair.*` 不再返回/提交 SAS 与 scopes。
- `cli-commands`: 配对确认仪式去掉 SAS 展示与 `--sas`，去掉设备侧 scopes 的展示与提交。

## Impact

- **权威文档**：`docs/AGENTS.md` §4 的安全不变量（「每个业务命令都要根据已认证设备及其 scope 授权」整条改写）、`docs/SECURITY_DESIGN.md`（§10 授权表、§9.5 的 scope 缩减传播、§:241 配对确认页展示项、威胁模型里补"为什么不需要 SAS"）、`docs/adr/0001-pwa-identity-and-secure-transport.md`（按 §9 修订）、`docs/SYNC_PROTOCOL.md`（§6.3 域表、§7.2、§11.3、§11.5）、`docs/NODE_LINK_PROTOCOL.md`（§9.4 节点 SAS）、`docs/IDENTITY_AND_AUTH_CONTRACT.md`（授权展开与 SAS 派生两处）、`docs/LOCAL_ADMIN_PROTOCOL.md`（`sas`/`scopes` 字段与配对方法表）、`docs/FRONTEND_DESIGN.md`（配对链路与创建入口门控）、`docs/CONFIG_REFERENCE.md`（若配置面受影响）。
- **机器资产**：`compatibility/transcripts/v1/transcripts.json`、`fixtures/sync/v1/transcripts/pairing-sas.json`、节点侧对应 fixture、`compatibility/commands/v1/commands.json`（`packs`/`presets` 与 `pack.create-session`）、`schemas/sync/v1/pairing.schema.json`（`scopes` 字段）、`schemas/local-admin/v1/envelope.schema.json`、相应 `scripts/check-*.mjs` 的判据。
- **代码**：`identity-auth`（SAS 派生与装配、授权展开、PACKS 镜像）、`core`（逐命令授权判定、设备记录的 scope 集合）、`storage-sqlite`（设备记录的 scope 列与其读写）、`server`（两个面的配对与授权路径、local_admin 字段）、`app`（CLI 配对仪式与 `--sas`）、`clients/app`（验签、公钥比对、删门控）。
- **验收边界**：主判据是浏览器级"拒绝被篡改的配对响应 + 无授权设置即可创建会话"，以及 `npm run check` 全绿。**风险如实登记**：这是仓库第一次改写 `AGENTS.md` §4 的安全不变量，文档与机器资产改动面最大；`scopes` 列与其约束的处置（删列走 migration，还是保留为不再写入的历史列）在 design 里定。
