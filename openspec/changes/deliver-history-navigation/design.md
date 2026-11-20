# 历史轮次分页与实时衔接技术设计

## Context

动机与用户确认依据见 [proposal.md](proposal.md)，行为规则见 [history-navigation](specs/history-navigation/spec.md) 增量规格及 `openspec/specs/` 中 `structured-history`、`session-transcript-cache`、`session-resource-policy`、`session-control-takeover` 长期规格。实现基线为分支 `v2` 提交 `4dd96797`：

- 节点已具备物理隔离展示快照库 `transcript-cache.sqlite3`（`TRANSCRIPT_CACHE_SCHEMA_VERSION = 1`，表 `transcript_sessions` / `transcript_records`，按 `ordinal` 保存原始 `session/update` 帧），以及冷会话秒开 + 后台单飞重放校准（`app/src/transcript_cache/`、`app/src/node/native_views.rs`）。
- 共享视图通过 `ACPV` 分块流交付 `OriginalRaw` / `ItemProjection` / `Status` / `SnapshotEnd` 记录（`app/src/application/shared_views/`），离线快照通过 `SharedViewOwner::open_cached_target` 构造只读视图。
- 主树父子关系记录在 `node.sqlite3` 的 `node_session_parents`，子目标原生读取由 `NativeCoordinator::ensure_child_read` 归属根会话名额。
- Web 端由 `SessionViewDisplay` + `Timeline` 渲染单一流，`view.opened.status` 已通过 `closedWithOptional` 支持可选字段。

当前没有任何轮次概念：冷会话打开时整份快照一次性流式下发，长历史无法按轮次分页、无法在快照淘汰或原生同步后定位目标轮次。

## Goals / Non-Goals

**Goals:**
- 在本地有效展示快照、当前连接内存视图与标准重放三类来源上统一按可证明轮次分页（前端传每页轮数，默认 10，上限 50）；父会话先读最新页、子目标先读最早页，页内均从旧到新；一轮不跨页。
- 命中有效快照的翻页零子进程、零名额、不刷新 30 分钟空闲计时与 30 天保留期；缺快照时沿既有容量规则重放。
- 游标绑定目标、方向、轮次边界与快照/读取代际；代际变化明确失效并支持按轮次坐标重新定位，不回首页、不伪称无损续读。
- 最新页衔接实时增量；迟到响应不覆盖新页。
- 子目标分页协议、节点关系校验与可嵌入的 Web 子分页容器。

**Non-Goals:**
- 原生按页接口、跨加载稳定条目 ID、永久全量索引或永久聊天档案。
- 在 `node.sqlite3` 写入任何聊天正文或轮次索引；修改控制权、审批、队列语义。
- 委派步骤卡片的识别与渲染（由 `agent-session-tree` 交付，本变更只提供其挂载所需的子分页容器与协议）。
- 新增 Cargo 或 Web 运行时依赖；新增管理错误码。

## Decisions

### 1. 可证明轮次判定（纯领域规则）

新增 `app/src/domain/history_page.rs` 中的 `TurnIndexer`，输入为逐帧分类结果，不接触原始 JSON；分类在适配层 `app/src/adapters/acp/turn_classify.rs` 依据协商版本读取 `params.update.sessionUpdate` 与 `messageId` 完成：

适配器区分用户帧、助手回应和旁路通知。首个用户帧建立候选轮；不同非空用户消息 ID，或助手回应后出现的新用户消息，提供新轮证据。同一 ID 续块跨工具更新或控制通知仍归原轮。连续无 ID 或有无 ID 混合且缺乏可靠分隔，无法证明上一轮结束；整代标为 unprovable，撤销分页并保留完整原始视图。无用户帧亦如此。首轮前的记录随第 1 轮展示，标记 prelude。增量与快照扫描共用分类状态机。

轮次序号是同一代际内从 1 开始的正序展示坐标，不是原生身份。

### 2. 展示快照库 Schema v2 与轮次索引

`TRANSCRIPT_CACHE_SCHEMA_VERSION` 升至 `2`：

- `transcript_sessions` 新增 `generation_token BLOB NOT NULL DEFAULT X''`（每个快照代际 16 字节随机令牌）、`turn_state TEXT NOT NULL DEFAULT 'pending' CHECK(turn_state IN ('pending','provable','unprovable'))`、`turn_count INTEGER NOT NULL DEFAULT 0`。
- 新表 `transcript_turns(source_id, native_session_id, generation, turn_index INTEGER CHECK(turn_index >= 1), first_ordinal INTEGER, PRIMARY KEY(source_id, native_session_id, generation, turn_index))`，外键 `ON DELETE CASCADE` 关联 `transcript_sessions`；轮次结束序号由下一轮 `first_ordinal - 1` 或代际最大 `ordinal` 推导。
- v1 → v2 迁移在单事务内只做 DDL：补列、建表、为既有会话写入新随机令牌并置 `turn_state = 'pending'`，`schema_version = 2`；不在启动时扫描正文。`pending` 会话在首次分页请求时扫描一次当前代际并持久化索引。
- `TranscriptCacheWriter` 在 `SessionWriterState` 中维护增量 `TurnIndexer` 状态（`load_or_init_session_state` 现有全量扫描时同步重建），新轮起点即插入 `transcript_turns` 行并更新 `turn_count`；`replace_with_replay` 写新代际时同时写入轮次索引与新令牌；`new_digest == old_digest` 时保持代际与令牌不变（内容相同，旧游标仍有效）。
- 纯读取（索引懒构建除外）不修改 `last_updated_at_ms`；懒构建只写索引列与 `transcript_turns`，同样不刷新保留期。清除、淘汰与原生删除擦除随级联删除索引。`node.sqlite3` 零改动。

### 3. 页来源选择与资源边界

对一次分页请求，节点按以下顺序选取同一代际的来源（`app/src/node/history_pages.rs`）：

1. **展示快照库**：会话当前已加载时先 `flush` 写入队列，再读取当前代际；有效未过期即用，`source = "transcript_cache"`。零子进程、零名额、不观测活动、不续期。
2. **当前连接内存视图**：快照库降级、缺行或该目标从未持久化时，读取活跃目标当前代际的内存原始记录（`TargetSnapshot.raw`），按同一 `TurnIndexer` 计算，代际为该目标的 `ViewEpoch`，`source = "live"`。
3. **标准重放**：前两者均不可用且对端支持标准重放时，沿既有读取准入（`admit_socket` / `admit_socket_source`；子目标转 §8 的独立查询或接收父连接自动投递的子事件）申请名额并完成重放，随后从新快照代际（或其内存视图）分页，`source = "replay"`。根会话存在已知运行工作且需要重新加载时，拒绝本次重放并返回 `invalid_stage`，不取消、不新建写入进程；满额返回 `session_capacity_full`。

分页前统一完成既有纯元数据门禁（`verify_cold_target_read` / `verify_cold_source_read` 与客户端信任 `client_gate`），失败时在披露任何快照内容前拒绝。分页读取只要求读取信任，不要求也不改变会话写控制权。被流式传输的页视图通过既有选择租约登记为受保护会话，期间不被保留期或容量清理淘汰。

### 4. 管理协议

**最新页（复用 `view.open` / `view.open_source`）**：payload 新增可选 `page` 对象 `{ "turns"?: "<1..50 十进制字符串>" }`。缺省 `page` 时行为与现状完全一致（独立浏览器夹具不变）；携带 `page` 时走与现状相同的已加载/冷快照+后台校准/准入三分支，并以“最新 N 轮中最早一轮的首条记录”为下限（floor）构造视图。冷快照分支用下限之后的帧构造离线视图；后台校准完成触发 `stream_rebound` 后，`view.rebind` 在新的实时代际上按相同 N 重新计算下限并返回新的页元数据。

**静态历史页（新命令 `history.page`）**：

```json
{
  "target": { "kind": "target", "targetId": "<Id128>" }
          | { "kind": "source", "selection": { ... }, "nativeSessionId": "<text>" },
  "child": "<nativeSessionId>",            // 可选，必须为父目标根树内已记录的后代
  "turns": "10",                           // 可选，十进制字符串 1..50，缺省 10
  "mode": { "kind": "first" } // 首次静态读取（含子首页）
        | { "kind": "continue", "cursor": "<opaque>", "direction": "older" | "newer" }
        | { "kind": "locate", "fromTurn": "<十进制 ≥1>", "direction": "older" | "newer" }
}
```

`continue` 的请求方向须与签发方向相同；first 为普通首页，relocated=false、continuity=same_generation；`locate` 以本代际正序坐标 `fromTurn` 为锚：`older` 返回以 `fromTurn` 为最新一轮的 N 轮，`newer` 返回以 `fromTurn` 为最早一轮的 N 轮。静态页不带实时尾部、不触发后台校准、不参与 `stream_rebound`。

**`view.opened.page`（两类响应共用，可选字段，`closedWithOptional` 解析）**：

| 字段 | 说明 |
| --- | --- |
| `generationId` | 本页所属代际的不透明标识（快照令牌或 `ViewEpoch` 编码） |
| `source` | `transcript_cache` / `live` / `replay` |
| `turnStatus` | `provable` / `unprovable` |
| `order` | `ascending`（父子页内均从旧到新） |
| `totalTurns`、`firstTurn`、`lastTurn` | 十进制字符串；不可证明时 `firstTurn`/`lastTurn` 为 `null` |
| `turns` | `[{ "turn": "16", "firstPosition": "<revision>" }, ...]`，按正序列出页内每轮首条记录在本流中的位置，供前端分组 |
| `prelude` | 页内是否含前导记录 |
| `olderCursor`、`newerCursor` | 不透明字符串或 `null` |
| `relocated` | 本页是否由 `locate` 产生 |
| `continuity` | `same_generation` / `unproven` |
| `liveTail` | 是否继续接收实时增量（仅已加载会话的最新页为 `true`） |

**游标**：不透明 base64url 字节串，格式 v1 = 版本 | 方向 | 目标指纹（`SHA-256(目标种类及各标识字段的规范长度前缀编码，含可选子目标存在位)` 前 16 字节，不含明文会话标识）| 代际类型与令牌 | 边界轮次 | 每页轮数 | 节点签名。以现有持久节点 P-256 签名能力对专用域标签和完整载荷签名，先验签再核对请求方向及范围，篡改返回 invalid_cursor。节点逐字段严格校验：格式错误或目标/子目标/方向不符返回 `invalid_cursor` 且不返回任何内容；代际不符返回 `stale_cursor`。游标不是授权凭证，每次请求重新执行完整读取门禁。

**错误码**：全部复用既有闭集——`invalid_request`（轮数为零、负数、非整数、超上限、字段缺失或多余）、`invalid_cursor`、`stale_cursor`、`not_found`（定位坐标超出本代际实际范围）、`unsupported_capability`（轮次不可证明或目标无任何可分页来源）、`invalid_stage`（重载会扰动运行工作）、`session_capacity_full`、`invalid_target`、撤销沿用 `4410` 关闭。管理错误码 50 项合同不变。

### 5. 页面顺序与大轮次

- 父目标：首屏为最新 N 轮；“更早”方向朝第 1 轮翻页；页内从旧到新（25 轮时首屏为第 16 至 25 轮，最新轮在底部），每轮内部条目保持来源顺序。子目标：从第 1 轮起正序分页，页内各轮正序展示。
- 一页总是包含完整轮次；单轮超大工具输出仍属同一页，原始记录沿用 `ACPV` 16 KiB 分块与积分流控按需交付，流中断时页面标注未加载部分，不标为完整页。
- 首轮前导记录在该轮分组之前按来源顺序显示。前端依据 `page.turns[].firstPosition` 将投影条目（`ItemProjection.firstPosition`）划入所属轮次分组；`Timeline` 新增可选位置区间过滤，`HistoryPageView` 为每轮渲染一个过滤后的时间线分组并按 `order` 排列。

### 6. 最新页的实时尾部（读取下限）

`ViewReadService` 新增可选读取下限 `floor`（首条记录 revision）：快照与尾部阶段跳过 revision 低于下限的原始记录，以及首次位置低于下限的锚点及其后续版本。语义为“本页只显示首次出现在下限之后的条目”；冷读旧页在同一 SQLite 读事务中固定代际及最大 ordinal，按页内原始范围选取锚点，再流式扫描其后的同代帧，按现有结构身份归并指向这些锚点的补丁；仅交付该页原始记录及所属锚点最终投影，后到更新不产生新的页内条目。字段引用及各条追加证据若指向页外记录，分页快照在发送所属锚点投影前以 ACPV 数据记录 FieldBytes（种类 6）交付其原始 JSON 值。记录头 recordId 是原始记录 ID，正文为 8 字节大端无符号 byteStart 和紧随其后的原始字节，byteEnd 为 byteStart 加原始字节数（不含前缀）。整条记录按现有分块偏移、长度、结束位及 credit 规则传输。Web 按流、记录 ID 和完整半开区间匹配引用，保留原始词法及证据顺序，仅将切片用于字段显示和检查。此路径最坏需扫描同代后缀，保持零子进程且不续期。仅分页实时流在首个新轮内容记录前发送有序 ACPV TurnStart（轮号、首条 revision），前端按流代际和 revision 幂等创建末尾分组；失去可证明性时终止分页流并重新打开完整视图。同一可证明代际内既有轮次边界只追加不改写，因此更早方向游标在新轮追加后继续有效，新轮衔接在最新页。

### 7. 重新定位与迟到响应

- 前端为每次分页请求保存“期望目标坐标”（被请求页最早或最晚一轮的正序坐标）。收到 `stale_cursor` 时自动以该坐标发起 `locate`；响应 `relocated = true`、`continuity = "unproven"` 时只替换分页视图并显示“已在最新同步代际中重新定位，与之前页面的连续性未经证明”。
- 收到 `not_found` 时明确提示目标轮次在最新原生历史中已不存在，保留当前页并标记为旧视图，不跳回首页。
- 前端以单调尝试计数丢弃迟到响应并释放其流；节点侧在重放未完成、断线或权限撤销时不发布页，也不取消原生执行。

### 8. 子目标分页

`child` 必须满足：父目标解析出的根会话与 `node_session_parents` 记录的子路径根一致（`SessionTreeRepository::resolve_session_tree`）；否则返回 `invalid_target` 且不披露子正文。子目标来源顺序同 §3：子快照 → 子内存视图 → 根树名额下已明确协商的只读子历史查询；`ensure_child_read` 本身只建立子视图归属。子目标无独立控制锁，读取仅需读取信任；子目标失败只影响对应容器。Web 提供可嵌入的 `HistoryPageView`（正序）与历史面板中的“子会话历史”最小入口（输入已记录的子原生会话标识后展开/折叠），委派步骤卡片内挂载由 `agent-session-tree` 复用该组件。折叠即关闭子页流；重新展开重新请求子目标第一页，父页边界不变。

#### 独立子历史查询的协商与读取

新增项目定义的可选 ACP 扩展。Codex ACP 公布的标准合同通过根重放投递子事件，未提供独立读取子目标的请求；它继续走已有快照和自动投递路径。扩展仅适用于明确实现本合同的 Harness。[ACP v1 扩展机制](https://agentclientprotocol.com/protocol/v1/extensibility)、[ACP v2 扩展机制](https://agentclientprotocol.com/protocol/v2/extensibility)、[Codex ACP 子会话合同](https://github.com/agentclientprotocol/codex-acp/blob/main/docs/subagent-sessions.md)。

- **协商**。以仓库命名空间固定键 `io.github.lindongfang.acp-remote/child-history`，固定方法 `_io.github.lindongfang.acp-remote/child-history`。客户端在初始化能力元数据中声明 `{"version":1}`。v1 读取 initialize.result.agentCapabilities._meta；v2 读取 initialize.result.capabilities._meta。两端版本匹配且连接、配置、身份及协商协议版本仍为原代际时可发送请求。忽略未知版本和方法名提示。禁止用品牌、配置开关、普通 loadSession、子 cancel 或过往连接记录补足证明。已声明却返回 method-not-found 时本连接停用该能力。
- **身份和归属**。普通管理分页命令不变。冷子目标先按既有顺序尝试展示快照和可证明的子事件，再由根树准入所有者串行登记只读查询。每批发送前和最终发布前核对读取权限、来源、根和子关系、连接代际及工作保护。准入及登记与新工作互斥，根忙返回 invalid_stage。根树唯一名额覆盖查询，不新建子会话资源。查的是指定子 ID，响应也必须与该根及子目标匹配；不得以根重放结束证明子历史完整。
- **传输**。查询请求携带 rootSessionId、sessionId、cursor（首次为 null）。每个 JSON-RPC 成功响应返回同一根及子 ID、非空 snapshotId、从 0 开始的连续十进制字符串 sequence、chunk（最长 16 KiB 原始字节的无填充 base64url）、nextCursor（不透明字符串或 null）。chunk 按顺序拼接为按来源排序的 UTF-8 NDJSON，逐行保存完整原始 ACP session/update，含原 JSON 字节；每帧以 LF 结束，字符串内部换行须转义。允许单帧跨块，不按轮数截断。nextCursor 非空时必须有字节进展；不接受重复游标、越序、跨代、非法字节或未结束帧；空历史仅允许首块为空且直接结束。游标、正文及原始响应不入日志。子查询成功返回的分页来源标记为 replay，表示本次从原生历史读取。单条请求经连接原 JSON-RPC ID 关联，子查询游标不暴露给 Web。
- **发布与退出**。同一根下同一子目标与连接代际共享单次查询。按块解码，逐帧复核 method、原生子 ID 与版本，以隔离的暂存记录构建快照。对 changed digest 以原子事务替换子快照及轮次索引；相同内容保留原快照令牌与保留计时。分页读取和查询结果以子身份返回。服务端错误、提前 EOF、进展超时或共同的来源、关系、连接代际证明失效时丢弃暂存，终止全部观察并释放所有者。查询任务归根资源所有，分别持有各观察者的读取授权；每批前及发布前剔除已撤销或离开的观察者。撤销发起者 A 时立即以 4410 关闭 A 并阻断后续正文，仍获授权的 B 继续同一查询；只有最后一人失权或退出才因读者消失丢弃暂存并停止后续请求。共用租约只释放一次；关闭顺序为停止发请求、等待或有界取消查询任务、清理暂存、释放根名额租约。不能用 session/cancel 停止读取。能力缺失仅限制子视图。真实 Harness 未协商时只能验证负向，受控实现的正向证据单独登记。


### 9. Web 分层与模块预算

- `clients/web/src/management/session-protocol/history.ts`：`history.page` payload 编码、`page` 元数据闭集解析（`session-protocol.ts` barrel 仅追加导出）。
- `clients/web/src/management/history-page-view.ts`：分页栏、轮次分组、重新定位/限制提示，零 `innerHTML`、零顶层 DOM 副作用。
- `clients/web/src/workbench/history-page-controller.ts`：页状态、尝试计数、游标与自动重新定位、轮数输入校验、子容器生命周期；通过宿主端口接入 `management-workbench.ts`。
- 全部 `workbench/*.ts` 与 `session-protocol/*.ts` 单文件 `<= 650` 行、`session-protocol.ts <= 120` 行、`main.ts <= 250` 行，单向分层无环；`FIXTURE-SHA256.md` 所列 6 个浏览器夹具保持不变。

### 10. 模块归属（Rust）

| 位置 | 职责 |
| --- | --- |
| `app/src/domain/history_page.rs` | `TurnIndexer`、轮数校验、页区间规划、游标编解码（纯函数） |
| `app/src/adapters/acp/turn_classify.rs` | 版本感知的逐帧轮次分类 |
| `app/src/transcript_cache/schema.rs`、`turns.rs`、`writer.rs`、`store.rs` | Schema v2 迁移、轮次索引写入/懒构建、按轮次区间读取帧 |
| `app/src/application/shared_views/reader.rs` | 读取下限 |
| `app/src/node/history_pages.rs`、`app/src/node/native_views.rs` | 来源选择、门禁、页视图构造、最新页下限接线 |
| `app/src/management_ws/session_protocol.rs`、`session_commands.rs` | `history.page` 与 `page` 字段解析、响应 JSON |

## Risks / Trade-offs

- [连续匿名用户分块缺少可靠边界] → 整代转为不可证明，提供完整视图并标明分页限制。
- [读取下限改动共享视图读取器的积分与快照结束不变量] → 下限仅在构造时固定、只做过滤；以既有共享视图开发/独立套件全量回归，并新增下限边界与尾部追加用例。
- [懒构建索引在超大会话首次分页时扫描较慢] → 只扫描单个会话当前代际一次并持久化，不在启动时阻塞；扫描期间不持有快照库写锁之外的锁。
- [快照在翻页过程中被淘汰或原生同步换代] → 游标令牌失效返回 `stale_cursor`，前端自动重新定位并明确连续性未经证明。
- [实时尾部中旧锚点的跨轮更新被最新页过滤] → 语义明确为“首次出现位置归属轮次”，旧页重新读取时呈现，避免伪造跨页合并。

## Migration Plan

- 展示快照库在节点启动打开时由 v1 原子迁移至 v2（仅 DDL 与令牌回填，正文不改）；索引按会话懒构建。核心控制库 `node.sqlite3` 零 Schema 变更、零新依赖。
- 回滚：旧版本二进制遇到 `schema_version = 2` 时按既有 `SchemaMismatch` 路径进入仅实时降级模式，不影响身份、队列、审批与原生会话；如需恢复快照功能，可删除可丢弃的 `transcript-cache.sqlite3` 由旧版本重建。
- 管理协议仅新增可选字段与新命令，未携带 `page` 的旧客户端行为不变。

另一客户端重读并发布不同内容时，向仍获授权的旧实时分页读者送达失效状态。Web 保留已显示正文，并提供手动重新打开入口；重复通知、先前读取的迟到正文或已离开目标的结果不能覆盖当前视图。读者点击后以当前目标和原页大小重新打开；同内容重读继续使用已有尾流。通知本身不授权 view.rebind，撤权仍按原清理路径处理。
