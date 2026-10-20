# `conversation-area-complete` 双真实节点（E5）验收证据

本文件只登记**实际执行并观察到的结果**。结论先行：

- **E5 未通过**。两个真实 Daemon 完成配对、认证、catalog 交换和 imported 无正文交付；
  Owner 离线/重连/撤销在链路上可观察。
- **Access 本地客户端看不到任何远程正文**：imported 会话的 `session.read` 返回
  `session.not_found`，imported 事件也不进入 Sync 扇出（浏览器收不到 `event` 帧）。因此
  “远程读取/命令/交互/表单/附件”没有一条能在 Access 页面上闭环。
- **`tasks.md` 的 7.2 不能勾选**。未执行项与残余缺口见第 10、11 节。

## 1. 环境与分层

| 项目 | 实际值 |
| --- | --- |
| 操作系统 | Windows `10.0.26300.0`（win32 x64） |
| 被测二进制 | 主检出 `D:/Project/acp-remote/target/debug/acp-remote.exe`，构建于 2026-10-20 10:31（+0800），SHA-256 `ebd71a6f1edd12fc0a52e3cb87ec2d68947538b9dca208a6a4079497657967ae` |
| 前端构建 | `D:/Project/acp-remote/clients/app/dist/build-version.json`：`0.0.0+71badcd-dirty`，`builtAt = 2026-10-20T02:26:17.902Z`（二进制 10:31 晚于 dist，静态资源按真实产物嵌入） |
| 源码基线 | `71badcd` + 工作区改动；隔离工作区 `D:/Data/omp-wt/t9ff347cd6/m` 与主检出内容一致（忽略行尾差异），本轮只新增本文件 |
| Owner Daemon | 独立数据目录 `D:/tmp/acpr-wp7dual/owner/data`，`https://127.0.0.1:18901`，`bind = 127.0.0.1`，`tls.mode = "direct"` |
| Access Daemon | 独立数据目录 `D:/tmp/acpr-wp7dual/access/data`，`https://127.0.0.1:18902`，`bind = 127.0.0.1`，`tls.mode = "direct"` |
| 出站信任 | Access 配置 `node_link.access.trust_roots = ["D:/tmp/acpr-wp7dual/owner/cert.pem"]`（只追加该自签 Owner 证书；Access 仍使用系统根） |
| 浏览器 | 真实 Chromium 内核 Microsoft Edge `155.0.4283.62`，`--headless=new --ignore-certificate-errors`，CDP 驱动真实 PWA 路由与真实 DOM |
| ACP 子进程 | **可控测试夹具** `agent.mjs`（LF-JSON ACP；场景 `full` / `interact`），**不是**真实 Codex / Oh My Pi；E7 不在本文件范围 |
| 配对 | `node pair --mode access`（真实 CLI，HTTPS claim/status）；Owner 本机确认经本地管理通道 `node.pair.confirm`（原因见 §3） |
| 证据形态 | DOM 文本/`data-*` 属性、CDP WebSocket 帧、Daemon JSON 日志、SQLite 查询、磁盘扫描；**未采集截图** |

“两个真实 Daemon”指两个独立进程、独立 SQLite/身份/日志/监听端口，经真实 WSS + TLS 互相认证；
Agent 事件由可控 ACP 子进程真实产生，经 Owner 的 ACP 适配器、Broker、SQLite、Node Link 后到达 Access。
它不替代“真实 Agent 能力”的结论（E7）。

原始材料位于 `D:/tmp/acpr-wp7dual/`（不在仓库内）：

- `driver/dual.mjs`、`pair-admin.mjs`、`owner-create*.mjs`、`live-s2b.mjs`、`access-final.mjs`、`revoke.mjs` 等；
- `evidence/*.json`（`access-final.json`、`live-s2b.json`、`offline-view.json`、`reconnect-live` 相关、`revoke.json` 等）；
- `owner/daemon.log`、`access/daemon.log`、两棵 SQLite 数据库。

## 2. 启动与隔离

两个 Daemon 以不同配置、不同数据目录、不同端口启动；`daemon status` 分别返回：

```text
owner: nodeId=e404231c-9b62-81ec-8301-8f5d5fc8a35f listen=["127.0.0.1:18901"]
access: nodeId=939d2100-e0bc-814b-b9dc-79bcc02ae9bf listen=["127.0.0.1:18902"]
```

TLS 证书为自签叶子证书（`CN=127.0.0.1`、`SAN=IP:127.0.0.1`、`CA:FALSE`）。
第一次使用 `CA:TRUE` 的自签证书时 Access claim 被 rustls 拒绝：

```text
local.remote_error: node-link transport failure: invalid peer certificate:
Other(OtherError(CaUsedAsEndEntity))
```

这是测试证书形态问题，不是产品缺陷；换成 `CA:FALSE` 叶子证书后配对成功。两个 Daemon 均为
`dev_mode.enabled = true`（仅测试环境），日志明确打印非安全部署警告。

## 3. 配对与信任

执行的真实路径：

1. Owner 管理面 `node.pair.begin { mode: "owner", pairingUrl: null }` 返回一次性
   `pairingId = 7090a40f-58b5-4e0c-88a9-8b8d0e2c68f1` 与 `https://127.0.0.1:18901/node-link/pair#data=…`；
2. Access 真实 CLI：

   ```text
   acp-remote --config D:/tmp/acpr-wp7dual/access/config.toml node pair --mode access --pairing-url <owner pairing URL>
   ```

   该命令经 HTTPS 执行 claim，并在 `status` 上等待 Owner 本机确认；
3. Owner `node.pair.status` 观察到已认领：

   ```json
   {"state":"pending_confirmation",
    "peerNodeId":"939d2100-e0bc-814b-b9dc-79bcc02ae9bf",
    "peerPublicKeyFingerprint":"21e8a9a1b55b4ab79b1759c1183d07ad171af06d55a465c44c342da2efc44ef3"}
   ```

4. Owner 本机 `node.pair.confirm { pairingId }` 返回
   `{"nodeId":"939d2100-e0bc-814b-b9dc-79bcc02ae9bf","confirmedAt":"2026-10-20T02:39:50.153Z"}`；
5. Access 随后一次 `node.pair.status` 返回 `state = "approved"`，日志写入 Owner 信任记录并建立出站连接：

   ```text
   app::access: {"event":"daemon.access_owner_paired","node":"e404231c-…","ts":"2026-10-20T02:40:24.630Z"}
   Owner: node_link.pairing_consumed (7090a40f-…)
   Owner: node_link.authenticated (access_node_id=939d2100-…, ts=02:40:24.659)
   Owner: node_link.catalog_subscribe (ts=02:40:24.659)
   ```

最终两侧 `node.list`：

```json
Owner:  [{"kind":"access","nodeId":"939d2100-…","state":"paired",
          "nodePublicKeyFingerprint":"21e8a9a1b55b4ab79b1759c1183d07ad171af06d55a465c44c342da2efc44ef3"}]
Access: [{"kind":"owner","nodeId":"e404231c-…","state":"paired",
          "nodePublicKeyFingerprint":"de14f3cd7057bb1bcc2584119b542e531b618861a1e294f30fdf0803600309ad",
          "ownerEndpoint":"wss://127.0.0.1:18901/node-link/v1"}]
Access: daemon.status.links = [{"nodeId":"e404231c-…","state":"connected"}]
```

**观察**：配对与双向身份校验通过；Owner 记录的是 Access 指纹，Access 记录的是 Owner 指纹，逐字对应。

**测试自动化限制（不是产品结论）**：

- Owner 交互式 CLI 在无 TTY 的管道下明确拒绝：

  ```text
  acp-remote: 非交互场景必须显式传入 `--fingerprint`（不得无参数自动确认）
  {"code":"local.invalid_request",…}
  exit=1
  ```

  用 `winpty` 取得 TTY 后能拿到 pairing URL，但向 Owner CLI 写入 `y\n` 没有触发确认（20 秒内
  `pending_confirmation` 不变）。因此 Owner 确认改走同一个本地管理方法 `node.pair.confirm`；
  Access 侧仍是真实 `node pair --mode access` CLI。
- Access CLI 在 `winpty` 包装下未观察到退出码；随后同一 `node.pair.status` 调用完成 approved 落账。
  配对是否成功以管理面状态、SQLite 信任行与 Owner 认证日志为准，不以 shell 退出码为准。

## 4. Catalog/import

Owner 上创建了 3 个真实会话（`agent.full` × 2、`agent.interact` × 1）。Access 重启后经
`catalog.subscribe` 收到 catalog，并写入 `imported_session`：

```text
c6356f69-5bec-4555-bf66-c9a74c395763  agent=full      state=idle
06531bc8-309d-4b8c-bafe-d6800130558f  agent=full      state=idle
67b3b100-f309-4c71-bd75-420830d2dccf  agent=interact  state=running
```

对应 Owner `owned_session` 三行一致。Access 数据库的 imported 表只有无正文列：

```text
imported_session(owner_node_id, session_id, title, agent_id, agent_name, state, version,
                 created_at, last_origin_epoch, last_origin_sequence,
                 acked_origin_epoch, acked_origin_sequence, next_local_sequence,
                 attachment_id, attachment_generation, updated_at)
imported_delivery_index(owner_node_id, session_id, origin_event_id, origin_epoch,
                        origin_sequence, local_sequence, event_type, payload_digest, received_at)
```

**消费者可见结果：失败。** Access 首页的会话列表始终为 0：

```text
CASE home {"sessions":0,
"text":"…从第一个会话开始\n\n创建会话后，这里会按目录整理你的工作。…"}
```

即使数据库中已有 3 行 imported，Access 首页仍为空。会话卡只存在于“目录”分组模型里，
而 imported 会话没有目录归属，页面给出：

```text
这个会话没有目录归属：它没有可用的目录引用，因此这里列不出同目录会话。
```

直接访问 `/session/<id>` 能显示标题/Agent/状态（来自 catalog/summary），但正文读取失败：

```text
session.not_found：the referenced session is not available
```

Access 日志对应：

```json
{"code":"session.not_found","command":"session.read",
 "event":"sync.command_rejected","device_id":"b949240a-…","ts":"2026-10-20T02:43:05.888Z"}
```

根因在读取面：`Broker::read_session` 只查本机 owned 视图
（`crates/core/src/broker.rs:2562` 的 `store.read_view().read_session(query)`），imported 行没有
`owned_session` 行，因此返回 `NotFound`；`Sync` 的 `session.read` 也没有为 imported 构造降级页。
这同时意味着“imported 会话在本设备没有正文”被实现成**读取错误**，而不是一份带显式说明的空页。

另一个可观察缺口：`origin_epoch` 在 catalog 导入时为 `null`（catalog 只携带无正文元数据）。
在第一条 imported 事件到达前，该会话不会出现在 `session.list` 的 imported 分支
（`imported_summary` 要求 `origin_epoch`），页面甚至把它当作本机会话：曾观察到
`0 个 Agent`、输入区显示「可输入」且没有来源提示；该状态下本轮没有执行提交，
因此“提交会被服务端怎样处理”未取证。达到第一条事件后，页面才显示
`这是来源为其它节点的会话…`。

## 5. 远程正文/流式

Owner 侧真实 Agent 事件（`turn.*`、`agent.thought.delta`、`agent.message.delta`、工具/终端/
计划）经 Node Link 到达 Access；Access 确实提交了无正文收据并累计 ACK：

```text
S1 c635…：last_origin_sequence=59，acked_origin_sequence=59，
          imported_delivery_index 中 local_sequence 1..19
S2 0653…：last_origin_sequence=78，acked_origin_sequence=78
```

但 **Access 浏览器收不到任何 `event` 帧**。在一次“页面已打开、Owner 继续发 prompt”的实测中，
通过 CDP 记录的 Access PWA WebSocket 帧类型为：

```text
["auth.server_challenge","auth.authenticated","sync.reset_required",
 "sync.snapshot_begin","sync.snapshot_chunk","sync.snapshot_chunk","sync.snapshot_chunk",
 "sync.snapshot_end","sync.caught_up","auth.server_challenge","auth.authenticated",
 "auth.server_challenge","auth.authenticated","sync.reset_required","command.result",
 "sync.snapshot_begin","sync.snapshot_chunk","sync.snapshot_chunk","sync.snapshot_chunk",
 "sync.snapshot_end","sync.caught_up","auth.server_challenge","auth.authenticated"]
```

没有 `"type":"event"`；页面时间线 `entries = 0`、`messages = []`，并持续显示
`session.not_found`。同步对照：同一时间 Owner 侧会话已经显示新的 Agent 文本。

根因在 Sync 扇出：`crates/server/src/sync/subscribe/fanout.rs:64` 的实现只投递
`CommittedDelivery::Owned`，imported 分支直接 `return`，代码注释写明“其回源投递本次不做
（任务 4.9 的登记口径）”。因此：

- imported 事件可以落 Access 的 `imported_delivery_index` 并 ACK；
- 但它们不会进入 Access 本地 Sync 的实时广播，也不会进入 `session.read`；
- 浏览器既看不到实时正文，也看不到历史正文。

这是 **E5 的核心失败**，不是浏览器或夹具问题。

## 6. 远程创建/命令/交互/表单/附件

### 6.1 远程创建：失败

Access 首页的“新建会话”面板：

```json
CASE create_sheet {"agents":0,"agentNames":[],
"text":"…新建会话…选择 Agent\n这台电脑还没有配置 Agent，请先在电脑上完成配置。\n将在你的电脑上启动一个 Agent 进程。…"}
```

Access 没有本地 Agent，也没有把 Owner catalog 的 `full`/`interact` 投影成可创建选项；
点击创建只会走 Access 本地 `session.create`（本机 Agent 进程），不能创建 Owner 上的会话。
仓库中也没有 `server::acp_facade`（`crates/server/src/lib.rs` 明确尚未落地），因此没有
“本地客户端 → Access → Owner 的 session.create”入口。

### 6.2 远程命令：失败

imported 会话的输入区被明确禁用：

```text
data-submit-enabled = "false"
“来源为其它节点的会话不能提交（本设备只承载元数据）”
“草稿只在内存中保留、尚未提交：来源为其它节点的会话不落盘。”
```

本轮对该页面点击发送后，草稿文本 `E5-ACCESS-MUST-NOT-SEND` 仍留在输入框，页面没有提交错误、
没有派发记录（该用例未抓取帧层细节，不断言帧层行为）。可见结果是**显式不可用**，符合
“不伪造可提交”；但 E5 要求的“远程命令闭环”没有实现。

### 6.3 有效交互/表单：失败

Owner 的 `interact` 会话真实产生了一个 pending permission（Owner 页显示
`data-interaction-id`、`写入配置文件`、`allow-once`/`reject-once`）。Access 的同一会话页面：

```json
{"header":"未命名会话\n● 运行中\ninteract\n0 个 Agent ▾",
 "error":true,
 "structured":"unavailable",
 "interactions":0,
 "messages":0}
```

与此同时 Access 数据库确实收到了该会话的 3 条 imported 事件：

```text
origin_sequence=5  permission.resolved
origin_sequence=6  command.completed
origin_sequence=7  elicitation.requested
```

没有 `permission.requested`（它在 Access 附着前发生，历史事件不会由 null cursor 补齐），
且收到的 `elicitation.requested` 也没有变成页面卡片。原因同 §5：imported 事件不进入 Sync 扇出。
因此 Access 侧无法看到、也无法回答授权或表单；“有效交互/表单闭环”未执行并通过。

### 6.4 远程附件：未执行（被同一缺口阻塞）

- Access 的 `data/attachments` 为空（0 个文件），测试结束时整个 Access 目录只有 23 个文件；
- imported 会话的输入区不可提交，无法从浏览器选择图片/文件/音频并发出上传；
- Access 侧有 Rust API `AccessManager::open_attachment`，但没有任何本地客户端/管理方法暴露它；
- 因此“远程附件上传、Owner 校验、Access 不落盘”只具备代码路径，没有本轮可执行的消费者入口。

## 7. 重连与重放

### 7.1 Owner 离线

停止 Owner 后，Access `daemon.status.links[0].state` 变为 `connecting`（随后在撤销场景为
`offline`），Access 页面显示明确不可用：

```json
{"originOffline":"来源「e404231c-9b62-81ec-8301-8f5d5fc8a35f」当前离线",
 "structuredHistory":"unavailable",
 "structuredHistoryText":"来源节点当前不可达，无法获取结构化历史；这不代表该会话没有结构化内容。",
 "submitEnabled":"false"}
```

刷新后同样保持该状态（页面没有把离线误报成空历史）。
但正文读取仍同时显示 `session.not_found`，说明“明确不可用”只覆盖了结构化历史提示，
没有覆盖 imported 正文读取本身。

### 7.2 Owner 恢复

重新启动 Owner 后，Access 日志重新认证并恢复 catalog/资源订阅：

```text
Owner:  node_link.authenticated (connection_id=13868796-…, ts=02:48:36.967Z)
Access: daemon.access_link_started (02:48:36.9xx)
Access: daemon.status.links = [{"state":"connected"}]
```

### 7.3 断线期间事件重放（投递层通过）

为了制造真实缺口，执行顺序为：

1. 停止 Access Daemon，记录 S2 的 `acked_origin_sequence = 59`；
2. 在 Owner 上对 S2 发送新 prompt，Owner `owned_event` 增长到 `origin_sequence = 78`；
3. 启动 Access Daemon；它按持久化 ACK 游标重新 `resource.attach` + `resource.subscribe`；
4. 查询 Access 数据库：

```text
S2: last_origin_sequence=78, acked_origin_sequence=78
imported_delivery_index: 92 行 / 92 个不同 origin_event_id（无重复）
S2 覆盖 origin_sequence 22..78；缺失 63、66、74
```

缺失的 63/66/74 在 Owner `owned_event` 中均为 `agent.message.delta` 且
`compacted_into = 139`（已并入同一 `agent.message.completed`）；这是增量压缩的正常结果，
不是丢事件。对应的最终消息事件（76）与 turn/command 终态（75/77/78）都已交付。
因此**游标重放与重复抑制在 Access 投递层通过**。

限制：重放后的正文不会进入 Access 本地 Sync 客户端（§5），浏览器仍看不到这些事件；
“重连重放闭环”只在数据库/ACK 层成立，消费者层未闭环。

## 8. 撤销

在 Owner 上执行 `node.revoke`：

```json
{"nodeId":"939d2100-e0bc-814b-b9dc-79bcc02ae9bf","revokedAt":"2026-10-20T02:55:56.838Z"}
```

结果：

```text
Owner node.list:  state="revoked", revokedAt=02:55:56.838Z
Access links:     state="offline"
Access 页面:       "来源「e404231c-…」当前离线"，submitEnabled=false
```

**部分通过**：Owner 侧撤销、Access 出站连接停止与页面不可用都成立。
但 Access 本地 `owned_node` 记录仍是 `state="paired"`、`revokedAt=null`：
Access 没有把 Owner 的撤销写回本地信任行（`node.trust.revoked` 只停止 supervisor）。
本轮没有重启 Access 验证它是否会在启动后再次尝试已撤销的 Owner；该行为未执行。

## 9. Access 磁盘/日志无远程正文

测试结束后停止两个 Daemon，再对 Access 数据目录、日志与 stdout/stderr 做二进制安全扫描：

```text
if grep -r -a -q -E "E5-(REMOTE-BODY|LIVE|S2|DEBUG|FRAME|INTERACT)|开始处理：" D:/tmp/acpr-wp7dual/access
then echo BODY-FOUND; else echo CLEAN; fi
→ CLEAN
```

扫描覆盖：

- `acp-remote.sqlite3`、`-wal`、`-shm`（95 条 imported_delivery_index，只有 `payload_digest`）；
- `daemon.log`（带 SQL 语句与命令元数据的 debug 日志）；
- `data/keystore`、`data/attachments`（0 个文件）、证书与 stdout/stderr。

SQLite 的 imported 表没有 payload/body/content/result 列（`imported_session.title` 恒为 `null`，
`imported_delivery_index` 只有摘要）；页面显示的 Agent 文本未出现在任何 Access 文件中。
**该项通过**。注意：这只是本轮的静态扫描，不等价于对所有崩溃转储/页面缓存的证明。

## 10. E5 结论与 7.2 映射

| E5 判据 | 结果 | 可观察证据 |
| --- | --- | --- |
| 两个真实 Daemon、不同数据目录/端口 | 通过 | 两个 `daemon status`、独立 SQLite/日志 |
| Access `node pair --mode access` + Owner 信任 | 通过 | 指纹逐字匹配、`node.list` 双方 `paired`、Owner 认证日志 |
| catalog/import | 失败 | 3 行 `imported_session` 写入成功；Access 首页 0 会话、imported 无目录归属 |
| 远程创建 | 失败 | Access 创建面板 0 个 Agent；无 `acp_facade` |
| 远程读取/历史 | 失败 | `session.read` → `session.not_found`；浏览器 0 条消息/历史 |
| 远程命令/流式 | 失败 | imported 输入禁用；即使 import 有事件，Sync 也不发 `event` 帧 |
| 有效交互/表单 | 失败/未执行 | Owner 有 pending permission；Access 0 卡片，虽有 `elicitation.requested` 入库 |
| 远程附件上传/读取 | 未执行（阻塞） | 输入禁用、无本地转发入口、Access attachments 为空 |
| 重连与重放 | 投递层通过，消费者层失败 | ACK 59→78、95 个唯一 event id、无重复；浏览器看不到重放正文 |
| Owner 离线明确不可用 | 通过 | `originOffline`、结构化历史说明、发送禁用 |
| 撤销 | 部分通过 | Owner `revoked`、Access link `offline`；Access 本地信任行仍 `paired` |
| Access 磁盘/日志无远程正文 | 通过 | 二进制安全扫描 CLEAN；imported 表无正文列 |

**E5：未通过。** `tasks.md` 7.2 的完成判据是“远程输入、附件、审批/表单、流式与历史均有真实证据”；
本轮只有配对、无正文投递、离线/重连/撤销和磁盘扫描有证据，远程输入/附件/审批/表单/流式/历史
均无消费者可见的成功证据。因此 **7.2 不勾选**。

## 11. 未执行项与残余缺口

未执行（不是通过）：

1. 通过 Access 本地客户端创建 Owner 会话；
2. 通过 Access 读取 Owner 历史/正文（当前为 `session.not_found`，不是显式降级页）；
3. 通过 Access 提交 prompt、取消、模式/配置变更；
4. 通过 Access 回答 permission / elicitation 表单（页面没有卡片）；
5. 通过 Access 上传图片/文件/已有音频到 Owner，以及 Owner 侧附件读取；
6. 浏览器端展示 imported 实时事件或重放事件（数据库已 ACK，界面无内容）；
7. Access 重启后对已撤销 Owner 的重连行为；
8. 真实 Codex / Oh My Pi（E7）、单节点 E1–E4/E6 重跑、PV7/`cargo test`（本轮不在范围）。

已确认的实现缺口（供后续修复认领）：

- `crates/server/src/sync/subscribe/fanout.rs` 只投递 `CommittedDelivery::Owned`，imported 事件
  不进入 Sync 实时流；
- `Broker::read_session` 只有 owned 读面，imported 会话返回 `session.not_found`；
- catalog 导入不填 `origin_epoch`，导致无事件 imported 会话从 `session.list` 消失并被页面误判为本机；
- imported 会话没有目录归属，Access 首页无法分组展示；
- `server::acp_facade` 未落地，Access 本地客户端没有远程 mutation 入口；
- Access 收到 Owner `node.trust.revoked` 后只停 supervisor，不把本地信任行标记为 revoked。

## 12. 复现命令（要点）

```text
# 1. 启动两个 Daemon（各自配置/数据目录/端口）
acp-remote --config D:/tmp/acpr-wp7dual/owner/config.toml daemon start
acp-remote --config D:/tmp/acpr-wp7dual/access/config.toml daemon start
acp-remote --config D:/tmp/acpr-wp7dual/owner/config.toml daemon status

# 2. Owner 生成配对 URL（管理面或交互式 CLI），Access 执行真实 claim
acp-remote --config D:/tmp/acpr-wp7dual/access/config.toml \
  node pair --mode access --pairing-url '<owner pairing URL>'
# Owner 本机确认：local_admin node.pair.confirm { pairingId }

# 3. 浏览器（CDP/headless Edge）分别配对 Owner/Access 设备，创建 Owner 会话并发送 prompt
node D:/tmp/acpr-wp7dual/driver/owner-create.mjs
node D:/tmp/acpr-wp7dual/driver/access-final.mjs
node D:/tmp/acpr-wp7dual/driver/live-s2b.mjs
node D:/tmp/acpr-wp7dual/driver/offline-view.mjs
node D:/tmp/acpr-wp7dual/driver/revoke.mjs

# 4. 扫描 Access 无远程正文
grep -r -a -q -E "E5-(REMOTE-BODY|LIVE|S2|DEBUG|FRAME|INTERACT)|开始处理：" D:/tmp/acpr-wp7dual/access
```

`live-s2b` 中 Owner 页面与 Access 页面同时打开；Access 侧通过 CDP
`Network.webSocketFrameReceived` 记录帧，确认没有 `event` 帧。

## 13. 现场清理

- 两个 Daemon 均以 `daemon stop` 停止，输出 `daemon 已停止（单实例锁已释放）`；
- 停止后 18901/18902 无 LISTENING（仅 TIME_WAIT）；
- 所有 headless Edge 由 CDP `Browser.close` 关闭；按命令行匹配 `acpr-wp7-e5` 的
  `msedge.exe` 进程数为 0；
- 数据、日志、驱动与原始证据保留在 `D:/tmp/acpr-wp7dual/`；
- 隔离工作区只新增本文件，未改产品代码、未改 `openspec` tasks、未提交。

# 第 9 轮（E5 定向复测）：Owner 离线时 imported 读取分类正确

## 14. 第 9 轮结论先行

| E5 判据 | 结果 | 可观察证据 |
| --- | --- | --- |
| Access 在线读取 imported 会话 | **通过** | Access 本地 WSS 的 `session.read` 为 `completed`、2 条消息且含 Owner 回复正文；页面显示同样两条消息（§16） |
| Owner 离线时读取显式不可用 | **通过** | 同一 `session.read` 变为 `rejected`，`error.code=resource.remote_unavailable`、`error.details.reason=owner_offline`（§17） |
| 不再误报 `session.not_found` | **通过** | 页面正文与 wire 错误都不含 `session.not_found`；页面显示「来源「…」当前离线」（§18） |
| 页面来源离线 | **通过** | `[data-origin-offline="true"]` 文本与 header 均为「来源「5a775826-…」当前离线」；发送禁用 |
| 重连恢复 | **通过（链路层）** | 重启 Owner 后 Access link 回到 `connected`（本轮未重跑正文读取） |
| 本轮未覆盖 | 见 §19 | 远程 mutation/附件/审批表单、Access 磁盘扫描本轮未重跑 |

本轮的修复目标（离线读取先判链路、返回 `resource.remote_unavailable` + `owner_offline`）**通过**。

## 15. 环境与两个真实节点（第 9 轮）

| 项目 | Owner | Access |
| --- | --- | --- |
| Origin | `https://127.0.0.1:18931` | `https://127.0.0.1:18932` |
| 数据目录 | `D:/tmp/acpr-wp7final/e5/owner/data` | `D:/tmp/acpr-wp7final/e5/access/data` |
| nodeId | `5a775826-027e-88b2-9251-0ac2d286c6f8` | `073d3891-9d91-85f5-91bc-876561fb0edc` |
| 二进制 | `D:/Project/acp-remote/target/debug/acp-remote.exe` SHA-256 `7a8f3533…24ca83`，2026-10-20 11:48（+0800） | 同左 |
| 前端产物 | `0.0.0+71badcd-dirty`，`builtAt=2026-10-20T03:47:57.571Z` | 同左 |
| 浏览器 | 真实 headless Edge（CDP），Owner 页面创建会话 | 真实 headless Edge（CDP）读取 imported 会话 |

配对（管理面，真实双向认证）：

```text
Owner node.pair.begin  pairingId=74d01903-34d6-43ef-b78c-c6b8649d8fd6
Access node.pair.begin mode=access  → pending_confirmation
Owner node.pair.confirm → confirmedAt=2026-10-20T03:54:59.670Z
Owner node.list: access 073d3891… state=paired, fingerprint=65684f8c…
Access node.list: owner 5a775826… state=paired, fingerprint=03bd2dc1…
```

Owner 侧用可控 ACP 夹具 `--scenario full`（不是真实 Codex）创建会话
`2b4e99c2-1541-4b60-b871-71b5a786c29f` 并发送 `E5-FINAL-REMOTE-1792468522203`；Owner 回复正文包含
「处理完成。全部完成。」。

导入观察：会话创建后 30 秒内 `imported_session` 没有出现；Owner 重启并重新握手后（catalog
resubscribe，`catalog_revision=12`）该行才写入 Access。本轮随后的读取复测使用该已导入行；实时
catalog push 的延迟未进一步定位，不据此宣称缺陷。

## 16. 在线：Access → Owner 的 imported 读取 completed

Access 页面打开 `/session/2b4e99c2-…` 后，Access 本地 WSS（`wss://127.0.0.1:18932/sync/v1`）收到：

```json
{"type":"command.result","body":{
  "requestId":"c80ce8ee-ecf9-41f9-9eb3-d973a7b79f1a",
  "command":"session.read",
  "status":"completed",
  "acceptedAt":"2026-10-20T03:58:46.465Z",
  "error":null,
  "result":{"sessionId":"2b4e99c2-1541-4b60-b871-71b5a786c29f",
            "resources":{"messages":[ … 2 条 … ]},
            "hasEarlier":false,"historyHasEarlier":false}}}
```

页面（同一时刻）：

```text
notFound=false
messages=["Agent 先分析请求，再检查文件。 03:55",
          "Agent 开始处理：E5-FINAL-REMOTE-1792468522203。处理完成。全部完成。 03:55"]
header="未命名会话 ○ 空闲 full 0 个 Agent ▾"
originOffline=[]
submitEnabled=false   ← imported 会话不可提交（既有设计），本项不作为失败
```

另用同一连接直接发一次 `session.read`（`include=["messages"]`）复核：`status=completed`、
`messageCount=2`、消息包含本轮标记。

## 17. Owner 离线：`resource.remote_unavailable` + `owner_offline`

停止 Owner（`daemon stop`）后，Access link 进入 `connecting`（非 `connected`）；随后同一 imported
会话的 `session.read` 返回：

```json
{"type":"command.result","body":{
  "requestId":"bb21353f-23e9-4d97-9856-90d91d534793",
  "command":"session.read",
  "status":"rejected",
  "error":{"code":"resource.remote_unavailable",
           "message":"the owner node is not reachable",
           "details":{"reason":"owner_offline"}}}}
```

页面重新加载后由应用自己发出的 `session.read` 也是同一条
`resource.remote_unavailable` + `details.reason=owner_offline`（同一 `command.result` 结构），没有出现
`session.not_found`、空历史或 `internal.unavailable`。

## 18. 页面表现

离线后的真实页面：

```text
header: 未命名会话 ○ 空闲 full 来源「5a775826-027e-88b2-9251-0ac2d286c6f8」当前离线 0 个 Agent ▾
[data-origin-offline="true"]: 来源「5a775826-027e-88b2-9251-0ac2d286c6f8」当前离线
正文: … resource.remote_unavailable：the owner node is not reachable（可重试）
      来源节点当前不可达，无法获取结构化历史；这不代表该会话没有结构化内容。 …
      来源为其它节点的会话不能提交（本设备…）
notFound=false
submitEnabled=false
```

`notFound=false` 且正文不含 `session.not_found`，说明页面没有把「来源离线」误报成「会话不存在」。

重启 Owner 后 Access link 恢复 `connected`；本轮未再次执行正文读取，重连后的读取恢复未在本轮取得
可观察证据。

## 19. 回归测试与未执行项

新增 Rust 用例（已写入，隔离工作区未执行）：

1. `crates/server/src/sync/command/mod.rs`
   `owner_offline_maps_to_resource_remote_unavailable`：`PortError::Unavailable(OwnerOffline |
   RemoteUnavailable)` 都映射成 `resource.remote_unavailable`，固定说明为
   `the owner node is not reachable`，`details` 恰为 `{"reason":"owner_offline"}`，且不等于
   `session.not_found`。
2. `crates/server/src/session_read.rs`
   `imported_owner_offline_is_forwarded_without_becoming_not_found`：本机 `read_session` 返回
   `NotFound` 时，imported 回源的 `OwnerOffline` 以 `SessionReadFault::Port` 原样上抛，不被吞成空页。

执行命令（主检出）：

```text
cargo test --locked -p server owner_offline
cargo test --locked -p server imported_owner_offline
```

本隔离工作区没有 `target/`，未执行；通过与否由主检出验证。

未执行（不得计为通过）：

- Access 远程 mutation/附件/审批与表单闭环；
- Access 数据目录无远程正文的二进制扫描（沿用第 1 轮结论，本轮未重跑）；
- Owner 恢复后的再次正文读取（只观察到 link `connected`）；
- 实时 catalog push 的导入延迟定位；
- E1–E4/E6/E7/E8 与 PV7（本轮不在范围）。

## 20. 现场清理

- Owner 与 Access 均以 `daemon stop` 停止；端口 18931/18932 无 LISTENING；
- 所有 headless Edge 由 CDP `Browser.close` 关闭；
- 数据、日志、驱动与原始证据保留在 `D:/tmp/acpr-wp7final/e5/`；
- 隔离工作区只新增/修改测试与证据文件，未改产品代码、未改 `openspec` tasks、未提交。
