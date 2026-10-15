<!-- 本变更的方案：接口形状、接缝与取舍。动机见 proposal.md，行为见 specs/。 -->

# Design

## Context

动机见 `proposal.md`。这里只记决定方案形状的现状与约束。

**已就绪、必须复用而不是重写的东西**

| 资产 | 位置 | 复用方式 |
|---|---|---|
| Sync wire DTO（18 消息类型、34 视图、12 命令、配对载荷） | `crates/sync-protocol`、`schemas/sync/v1/` | 已冻结，`server::sync` 只消费，不新增 wire 类型 |
| 设备/节点握手状态机 | `identity-auth` 的 `Authority::hello`/`verify_proof` | 已支持 `ConnectionKind::SyncDevice`，**状态机本身不改** |
| 快照分块与摘要算法 | `server::node_link::resource`（`snapshot_digest`，`resource.rs:1181`） | 算法逐字相同，Sync 侧复用同一口径 |
| 连接注册表、出站队列、限流、心跳 | `server::node_link::conn::{registry,limits}` | 形态可照抄（Sync 的限额数值与消息类型不同，不能直接复用类型） |
| 配对状态机与投影 | `identity-auth` 的 `begin_pairing`/`verify_claim`/`pairing_status` | 与 Node Link 同源；Sync 只是换 domain 与载荷 DTO |
| core 读面 | `UseCases::{replay, read_session, list_sessions, devices, device, peer_key, consume_pairing}` | 大部分够用，三处需补（见 D3） |
| 事件扇出样板 | `app::compose` 的 `ForkedPublisher` + `NodeLinkPublisher`（mpsc，1024 队列） | 加一支 Sync 分支 |

**约束**

- `AGENTS.md` §5：`server::sync` 是入站适配器，只调用 `core::use_cases`，不得 import `storage-sqlite`，不得与 `server::node_link` 互相调用。
- `AGENTS.md` §6：Sync 与 Node Link 是不同边界，**不得复用对方的 wire DTO**。
- 目录/会话摘要一律只出 `alias` 与 `displayName`，规范化路径绝不进入对端可见输出（`SECURITY_DESIGN.md` §12.3）。
- `node-link-listener` 规范当前要求 `/sync/v1` 返回 404（该要求由本变更改写）。

## Goals / Non-Goals

**Goals**

- 让已配对的浏览器设备经真实 listener 完成「配对 → 认证 → 订阅 → 快照 → ACK」全链路，协议级可脚本化验证。
- 把 Sync 需要的 core 读面缺口一次补清，使第 2 个变更（命令族）只需消费、不必再动 core 的读取路径。
- 事件投递的背压与隔离做到「慢客户端不阻塞 Broker 或其他连接」。

**Non-Goals**

- 命令族与事件视图投影（第 2 个变更）、静态资源托管（第 3 个，页面挂**根目录**、资源**嵌入二进制**，均为用户 2026-10-12 裁定）、前端接线（独立变更）。
- 设备 ACK 的持久化：§9.5 已明确恢复位置以客户端提交的 cursor 为准，连接期内存态足够（**因此本变更不推进文件格式版本**）。
- 为同步面引入任何新 crate 或新第三方依赖。

## Decisions

### D1：`Origin` 由接入层透传，服务端自行判定来源

**问题**：§3.2 要求「服务端必须校验 `Origin`、允许的 `Host` 和 TLS 部署配置，不能信任客户端 JSON 中自报的 Origin」，而 device proof transcript 的第 6 个字段就是 `canonicalOrigin` —— 双方必须对同一取值达成一致才可能验签通过。今天 `route.rs:433` 把连接交给处理器时只传 `(WsConnection, PeerInfo)`，`PeerInfo` 里没有任何请求头。

**决定**：给 `WsHandler::handle` 增加一个「已校验的请求来源」参数，取值由升级请求的 `Origin` 头规范化而来；同时保留 `None`（非浏览器对端无此头）。

**备选与否决理由**

- *让 handler 自己从 `WsConnection` 读头*：`WsConnection` 由 `axum::extract::ws::WebSocket` 拥有，升级后拿不到原始请求头，需要把 `HeaderMap` 单独存下来 —— 与显式参数等价但更绕。
- *把来源写进 `PeerInfo`*：`PeerInfo` 的语义是「对端网络地址与转发头是否可信」，来源是协议层概念，混进去会让两个不相关的关注点共用一处。
- *从 `Host` 头推导来源*：`Host` 由 `HostPolicy` 校验、语义是「请求打到哪个虚拟主机」，而 `Origin` 表达「页面从哪个来源加载」。二者在反代场景下可以合法地不同，混用会误拒。

**归一化口径**：小写 scheme、浏览器规范化 hostname、显式有效 port，省略默认端口（`https:443`/`http:80`），不含 path/query/fragment 与尾部 `/`。判定「与设备绑定是否一致」由 `identity-auth` 的绑定检查完成（`check_binding` 已实现该比较），适配层只负责**如实取得**，不重复实现比较规则。

### D2：Sync 设备握手读面照 `node_link_handshake_view` 的形状补

握手完成前不存在已验证主体，`node.hello` 里的自报身份同理。Node Link 已有 `UseCases::node_link_handshake_view(access_node)`（`use_cases.rs:811`）作为「无 actor 的窄只读入口」。Sync 侧需要同形的入口，回传：该 deviceId 的信任行、公钥、最近一次配对、本机 head 与 epoch。理由：把「自报 id → 当次持久事实快照」的收窄放在 core，适配层只做协议映射，避免适配层自己拼信任读取（那会绕过 core 的失败关闭与审计）。

**为什么不直接放宽 `device()`/`peer_key()`**：它们挂在 `require_local()` 下（`use_cases.rs:1537-1542`），那条守卫同时保护 `devices()`、`profiles()`、`workspaces()` 等管理读面。放宽它等于同时打开一批本机管理面，风险面远大于需求。新增窄入口是唯一不动既有授权边界的做法。

### D3：三处 core 接缝，不引入第四处

| 接缝 | 形式 | 理由 |
|---|---|---|
| Sync 设备握手读面 | 新增无 actor 窄入口（D2） | 握手前无主体 |
| 设备认证收尾 | 复用 `consume_pairing`（已支持 `Actor::Device`）+ 仿 `record_node_connected` 的 `last_seen_at` 推进 | Node Link 已有同形收尾；`owned_device.last_seen_at` 列已存在，只前进语义已冻结 |
| 设备可见会话清单 | 新增「自有 + 已导入合并」读面 | `SessionStore::list` 只返回 owned，`RemoteDeliveryStore::list_sessions` 只返回 imported，快照的 `sessions` 资源必须两者合并后再按 scope 过滤 |

**合并读面为什么必须在 core 而不是适配层**：授权判定（`Broker::authorize`/`node_allowed`）在 core；若适配层分别调两个 store 再自己合并，会形成第二条可见性判定路径，与 `MODULE_ARCHITECTURE.md` §5 的「单一可见性判定点」原则冲突。

**过滤口径**：本地自有会话按设备 scope 过滤（`session.list`）；已导入会话额外要求协商 `resource.remote-origin.v1`。归组与 `workspace` 引用沿用既有投影规则（别名取持久化值，MUST NOT 由路径反查）。

### D4：事件扇出按设备 scope 过滤，背压分两级

`EventPublisher::publish` 是同步的 fire-and-forget（`ports.rs:1178`），订阅者注册表在适配层。Node Link 的做法是 `NodeLinkPublisher` 把事件丢进容量 1024 的 mpsc，由连接侧消费。

Sync 侧照同一形态，但多两件事：

1. **按 scope 过滤发生在投递时**，不在提交时——同一事件对不同设备可见性不同，`publish` 一次广播给所有 Sync 连接的任务，各自按该连接的 scope 与已协商 feature 决定是否下发。
2. **背压分两级**（§14）：单连接待发送队列达到高水位（8 MiB 或 2000 条，先到者触发）时先**停读新的重放批次**；持续过慢则发 `resource.backpressure` 并断开。断开后由客户端 cursor 重放，**绝不丢弃事件、绝不阻塞 Broker**。

**为什么不给每个设备建持久队列**：那会把「权威事件日志在 SQLite」变成「权威在内存队列」，违反 `AGENTS.md` §7 的持久化规则与 `docs/SYNC_PROTOCOL.md` §2.3 的投递语义。队列只是投递优化，事实来源始终是存储层。

### D5：单设备替换与撤销传播共用一个连接注册表

`(hostId, deviceId)` → 连接句柄的有界映射。两条规则都落在它上面：新连接认证成功后查表、对旧连接发 `4411`；设备撤销提交后按 deviceId 查出全部连接、发 `4410`。

**关键约束**（抄 `conn/registry.rs` 的既有结论）：注册表**不是事实来源**。撤销判定以持久化记录为准，进程内缓存不得让已撤销设备继续可用；因此撤销路径必须是「先提交持久状态、再扫连接表」，且超时未送达也要保证后续认证被拒。

**替代方案**：不做注册表、让每条连接自己轮询设备状态 —— 延迟不可控且要额外定时任务，否决。

### D6：配对 HTTP 与 Node Link 配对端点同形但各自独立

两条 pairing 路径（`/sync/v1/pairing/*` 与 `/node-link/v1/pairing/*`）在结构上高度相似：原子校验、恒定 200 的 status、四个安全响应头、固定限流、确认只走本机入口。但 `AGENTS.md` §6 禁止两侧复用 wire DTO，且两侧的 domain 与载荷形状不同（Sync 是 `pairing-proof`/`pairing-status`，Node Link 是各自的 domain）。因此**结构照搬、类型各自定义**：安全头常量与限流数值口径可以一致，DTO 与校验入口不复用。

配对确认路径复用既有的本机 CLI（`device.pair.*`）：`/sync/v1/pairing/*` 只负责 claim 与 status，`approved` 之后由 CLI 完成信任创建。这与 Node Link 完全对称，**不新增本地管理方法**。

### D7：`sync.*` 三个配置键接线

`config.rs` 已有 `Config::unwired` 机制与「接线后不得再报未接线」的自检（`config.rs:1076-1088`）。本变更把三个键从解析变成消费，**必须同时把它们从 `unwired` 清单移除**，否则启动日志会谎报。取值口径：只下调（高于协议默认值取默认值）。

### D8：文档冲突按 wire 权威裁定

`FRONTEND_DESIGN.md` §4.3 与 `INITIAL_DESIGN.md` §6.3 把端点写作 `/sync`，与 `SYNC_PROTOCOL.md` §3.1 的 `/sync/v1` 冲突。按 `AGENTS.md` §9「wire 以协议文档为准」，本变更改那两处文档；前端连接路径的修改属独立变更（用户裁定），但**该冲突必须在本变更内消解**，否则下一批会以错的路径实现。

## Risks / Trade-offs

- **[单设备替换的竞态]** 两条连接并发完成认证时可能都判定自己是最新 → 用一个「注册 + 判定 + 关闭旧连接」的临界区，按注册顺序取最后一条为准；重复投递由客户端按 `eventId` 去重兜底（§8.5 已声明该窗口存在）。
- **[Origin 透传改变既有签名]** `WsHandler::handle` 是既有 trait，Node Link 也实现它 → 两个实现必须在同一变更内一起改，不能只改 Sync 侧；改动小但必须一次到位。
- **[合并读面的性能]** 自有 + 已导入两路合并会让快照组装多一次查询 → 快照本就在一致性读视图内取，且只随会话条数增长（明细不进快照已冻结）；如成为瓶颈，优化点在存储层的单查询，不在适配层加缓存。
- **[高水位停读的观感]** 停读新的重放批次会让慢客户端「看起来卡住」 → 协议已规定先发 `resource.backpressure` 再断开，客户端据此提示并重连；不得为了让界面「动起来」而丢事件。
- **[撤销传播的送达保证]** 关闭是请求而非暴力断开（沿用 Node Link 结论），若对端不读则送达失败 → 持久状态已提交，因此**即使关闭失败，后续认证与命令仍被拒**；这是正确性的一阶保证，送达只是体验。
- **[fake 客户端脱离真实浏览器]** 协议级 E2E 用脚本化客户端，无法覆盖浏览器的 WebCrypto/IndexedDB 行为 → 前端 E2E（48/48）已覆盖客户端一侧，两侧在同一批合同与固定向量上各自独立判定；真正的浏览器级证据留到第 3、4 个变更。
- **[Host 与 origin authority 的端口口径不一致]** `identity_auth::Authority::check_binding` 的 `SyncDevice` 分支要求 `binding.host` 与 `origin.authority()` **大小写不敏感相等**（`authority()` 含显式端口），而接入层的 `HostPolicy` 归一化**忽略端口**。因此绑定了显式端口、且反代把 Host 改写成无端口形态的部署会在握手层失配 → 实现时（WP3）必须按 identity 层期望的形态传 `Host`，或要求 `daemon.public_origin` 不带显式端口；这条属部署接线，不属接入层缺陷。
- **[非 https 来源由适配层拒绝，不是接入层]** 接入层如实产出它收到的形态（含 `http://…`）。`CanonicalOrigin::parse` 只接受 `https://` 前缀，因此 WP3 必须自行把非 https 来源映射为 `auth.origin_mismatch`，**不得**期望接入层替它拒绝。

## Migration Plan

- **schema 无变更**：不新增表、不新增列、不推进文件格式版本，因此**无 migration、无回滚数据风险**。
- **部署顺序**：`/sync/v1` 从 404 变为可用，是纯增量；Node Link 与本地管理通道行为不变。回滚即回到 `/sync/v1` 404，旧客户端（尚不存在）不受影响。
- **配置**：新增对 `sync.*` 三个键的消费不影响未配置它们的部署（取协议默认值）。
- **文档回滚点**：`node-link-listener` 规范被改写，回滚该变更时需一并回滚该规范文件。

## Open Questions

- 无。方案中所有会影响 specs、approach 或任务拆分的判断都已在本文件内定；剩余的可延后项（例如合并读面是否需要存储层单查询优化）都不改变本变更的行为与工作包划分。托管位置与嵌入方式已由用户 2026-10-12 裁定（根目录 + 嵌入二进制），它们属第 3 个变更，不影响本变更。
