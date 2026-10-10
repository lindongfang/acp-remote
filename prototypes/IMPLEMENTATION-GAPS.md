# PWA 实现缺口清单

> **这不是权威文档。** 它是从设计原型反推出的「要实现还差什么」，供后续写 openspec 变更提案时切分任务用。
> 产品行为、协议语义、安全模型仍以 `docs/` 下的权威文档为准；若本文件与权威文档冲突，以权威文档为准。
>
> 依据：分支 `feat/pwa-prototype` 上的两个原型（`acp-remote-pwa.html` 桌面端、`acp-remote-pwa-mobile.html` 移动端）
> 与当前仓库源码实测。凡是「已确认不存在」的地方都给了文件与行号，便于复核。

---

## 0. 结论速览

| 层 | 状态 | 规模 | PWA v1 是否需要 |
|---|---|---|---|
| A 前端工程 | **完全不存在**（无 `clients/` 目录） | 大 | 需要 |
| B `server::sync` | **完全不存在**，`/sync/v1*` 当前一律 404 | 最大 | 需要 |
| C 合同缺口（目录字段 / Sync `session.create` / Agent 列表 / 路径下发） | 协议层没有对应能力 | 中～大 | C1 需要；C2/C3 属 v1.1 |
| D 存储与身份基础 | **大部分已就位**（cursor、原文保全、配对状态机、Sync DTO） | — | — |
| E 已设计但服务端未支撑 | 缺投影/命令实现 | 小～中 | 需要 |
| G 流程（文档回写、提案、验收） | 未开始 | — | 需要 |

**最重要的一条好消息**：`owned_event` 表已经具备 cursor / 重放 / ACP 原文保全所需的全部字段，
`identity-auth` 的配对状态机与 challenge-response、`sync-protocol` 的 18 类消息与 33 类事件视图 DTO 也都已落地。
所以最大的两块工作（B1、A1–A7）主要是**接线与前端落地**，不是从零建设。

---

## 1. A 前端工程（100% 缺失）

`clients/` 目录不存在；原型之外没有任何前端代码。目标结构见 `docs/FRONTEND_DESIGN.md §3`。

| # | 缺什么 | 备注 |
|---|---|---|
| A1 | 工程脚手架：TS + Expo/React Native 通用工程、Expo Router、`platform/` 适配层 | 首个交付只构建 Web/PWA |
| A2 | **PWA 设备身份**：WebCrypto 生成**不可导出** P-256 `CryptoKey` + IndexedDB 持久化 | 配对与 `/sync/v1/pairing/claim` 的前提；`FRONTEND_DESIGN §4.5` |
| A3 | **Sync Client**：连接、challenge-response 认证、`subscribe`、ACK、增量重放、快照暂存与原子替换、`eventId` 去重、`requestId` 幂等重试 | `SYNC_PROTOCOL §8–§11` |
| A4 | 状态机：连接 9 态、命令 6 态 | 原型已实现这些判定逻辑，可直接对照迁移 |
| A5 | 页面：配对 / 目录 / 目录详情 / 对话 / 变更文件 / 降级卡片 / 诊断抽屉 / 主机与连接面板 | 对应原型四条路由 + 浮层 |
| A6 | 协议 DTO 层：消费 `schemas/sync/v1/**` 与 `fixtures/sync/v1/**`，与 Rust 共用同一份合同 | `AGENTS.md §8` 要求两端同源 |
| A7 | 测试：fixture 契约测试、状态机、幂等重试、浏览器 E2E、敏感信息不落盘 | `FRONTEND_DESIGN §10` |

---

## 2. B 服务端缺失模块

`crates/server/src/` 目前只有 `local_admin` / `node_link` / `transport`。**没有 `sync`，也没有 `acp_facade`。**

### B1 `server::sync`（PWA v1 的硬前置）

证据：`crates/server/src/transport/net/route.rs:5` 的模块注释明确写着
「只有注册过的 path 有服务：未注册的 path（含 `/sync/v1*`，`server::sync` 尚未落地）一律 404」，
对应测试见 `crates/server/src/transport/net/tests.rs:301`。

| # | 子项 | 说明 |
|---|---|---|
| B1.1 | `/sync/v1` WebSocket 路由 + subprotocol `acp-remote.sync.v1.json` | 现只有 node-link 的路由 |
| B1.2 | 认证四消息 `client_hello` / `server_challenge` / `client_proof` / `authenticated` | 15 秒超时、单设备单连接替换（4411）、`auth.*` 错误码 |
| B1.3 | `subscribe` + 增量重放 + `caught_up` + `reset_required` | cursor 校验四种 `reason`；`META_SERVER_EPOCH` 已就位（`migrate.rs:30`） |
| B1.4 | **快照组装**：6 类资源 + 分块 + `snapshotDigest` | 可参考 `crates/server/src/node_link/resource.rs` 的既有做法 |
| B1.5 | **11 条命令**处理器 + 幂等记录 + `command.result` 终态 | `SYNC_PROTOCOL §11.5` |
| B1.6 | **33 类事件投影**（`payload.view`）+ `acp.rawJson` 装配 + `rawUnavailable` | 存储字段已就位（见 §4） |
| B1.7 | `limits` 下发（3 项可调）+ 认证前固定上限 + 限流与 4429 | `CONFIG_REFERENCE §2` |
| B1.8 | 撤销传播（设备撤销 → 关连接）、授权拒绝 | `SECURITY_DESIGN §9.5` |
| B1.9 | `/sync/v1/pairing/claim` 与 `/sync/v1/pairing/status` 的 HTTP 处理器 | `identity-auth` 状态机已有，缺 server 侧接线 |

### B2 `/ui` 静态资源托管

现在没有任何静态资源托管路由，也没有「前端构建版本 ↔ Daemon」的可检查关联（`FRONTEND_DESIGN §4.3` 要求）。

### B3 / B4 本次不需要

| # | 缺什么 | 为什么 PWA v1 不需要 |
|---|---|---|
| B3 | `server::acp_facade` | 属 Zed 闭环（切片 6），PWA 走 Sync |
| B4 | `node-link-client` | PWA 直连单个 Owner Node，不经 Access 转发 |

---

## 3. C 合同级缺口（先改合同，再写代码）

| # | 缺口 | 证据 | 阻断的原型功能 | 要动的文件 |
|---|---|---|---|---|
| **C1** | **会话不携带目录** | `crates/storage-sqlite/src/migrate.rs:53` 的 `owned_session` 无目录列；`crates/core/src/model/session.rs:307` 的 `SessionSummary` 九个字段无 workspace；`schemas/sync/v1/common.schema.json#/$defs/sessionSummary` 是 `additionalProperties: false` | 目录页、目录详情、新建会话、创建弹层 | ① `migrate.rs` 加列 + v2→v3 migration；② `CORE_PORTS_AND_STORAGE.md §7` 表结构（`check:drift` 逐字比对）+ `§9.6` 投影；③ `common.schema.json` 加可选字段；④ `SYNC_PROTOCOL.md §10.3` 值对象 + `§9.4` 快照 item；⑤ fixture |
| **C2** | **Sync 侧没有 `session.create`** | `compatibility/commands/v1/commands.json` 中 `session.create` 的 `transport` 仅 `["node_link"]`；`SYNC_PROTOCOL §16.2` 要求新能力必须新 feature | 目录行 `⊕ 新建`、详情页新建 | ① 新 feature ID（`compatibility/features/v1/features.json` + 两份协议文档 + `check:features`）；② 命令目录加 `transport: ["sync"]`；③ 两个协议 schema 的命令枚举；④ `SYNC_PROTOCOL §11.5` 与 `SECURITY_DESIGN §10.2` 表格；⑤ `core::broker::required_grant` 镜像（`check:commands` 四处一致）；⑥ 新 scope/pack |
| **C3** | **PWA 无从得知本机 Agent 列表** | 现有命令集无查询类 Agent 列表命令 | 新建会话的 Agent 选择器 | 同 C2 的四处一致链路 + 新 scope |
| **C4** | **目录绝对路径是否下发，口径不一致** | 原型：详情页显示完整路径，目录页显示尾段；而 `SECURITY_DESIGN §12.3` 规定「路径与规范化结果**不得出现在对端可见的任何输出里**」，`CORE_PORTS_AND_STORAGE.md §5.1` 规定「`canonical_path` 只出现在该调用入参里」，同文 §11 结论为「原始路径……**不得经 wire 传输**」 | 目录详情页页头 | **产品决策**：要么改上述两份权威文档并加约束（仅本机 owned 会话、设备需独立授权），要么把详情页收回为尾段形态 |
| C5 | v1 不协商 `resource.remote-origin.v1` | 协议已支持该 feature；不协商即拿不到远程资源 | PWA 直连 Owner，不存在远程会话 | 属**产品边界记录**：需写进 `FRONTEND_DESIGN §2.2`，无需改协议 |

---

## 4. D 已有基础（只需接线，不要重做）

| 已具备 | 位置 |
|---|---|
| 事件序号、因果、原文保全、保留期、delta 压缩标记 | `crates/storage-sqlite/src/migrate.rs:79` `owned_event`：`global_sequence` / `session_sequence` / `origin_epoch` / `origin_sequence` / `event_id` / `kind` / `policy` / `causation` / `payload_json` / `payload_digest` / `acp_raw_json` / `acp_byte_length` / `acp_sha256` / `acp_raw_unavailable_reason` / `created_at` / `expires_at` / `compacted_into` |
| cursor 的 epoch | `migrate.rs:30` `META_SERVER_EPOCH` |
| 配对状态机、challenge-response、12 个 transcript domain、授权词表展开 | `crates/identity-auth/`（已落地） |
| 会话/turn/事件/交互/命令 的存储与读视图、附件内容寻址 | `crates/storage-sqlite/`（`SessionStore` / `ReadView` / `RemoteDeliveryStore` / `AttachmentStore`） |
| ACP wire DTO、`RawDocument` 原文承载、11 种 `session/update` 判别子 | `crates/acp-protocol/`（已落地） |
| 本机 Agent 子进程、能力门控、权限/elicitation 转发、Windows Job Object | `crates/agent-host/`（已落地） |
| Sync 18 类消息 + 33 类事件视图的类型化 DTO | `crates/sync-protocol/`（已落地） |
| node-link 侧的资源快照/重放/ACK/命令管线（可作为 Sync 快照实现的参照） | `crates/server/src/node_link/resource.rs` |
| 共享 listener、TLS、Host 边界、`/ui` 之外的静态路由框架 | `crates/server/src/transport/net/` |

---

## 5. E 原型已画、服务端尚未支撑

| 缺什么 | 依赖 | 原型现状 |
|---|---|---|
| 终端输出卡（流式 + 截断标识） | B1.6 投影 + 单条容量策略 | 未画 |
| **elicitation 动态表单**（按 schema 生成控件、submit/decline/cancel、schema 超限降级） | B1.5 `elicitation.respond` | 未画 |
| plan 卡片、slash 命令插入、usage 呈现 | B1.6 三类事件投影 | 未画 |
| **命令终态卡片**（completed / failed / **uncertain**） | B1.5 幂等 + 终态事件 | 未画（uncertain 必须显眼且不被自动消解） |
| 队列策略入口（`sessions.queue_policy` 默认 `queue`） | 服务端已支持 | 原型用单按钮屏蔽了「运行中排队发送」 |
| 「跳到你最近的输入」胶囊 | 纯前端 | 已画 |

---

## 6. F 尚未设计

| # | 缺什么 | 说明 |
|---|---|---|
| F1 | 无权限设备的「不显示新建入口」态 | 桌面端有，移动端未补 |
| F2 | PWA 安装引导、service worker 更新提示 | `FRONTEND_DESIGN §4.4` 要求可安装 |
| F3 | 移动端原生能力适配（SecureStore / 相机扫码 / SQLite） | 延后到原生端 |
| F4 | 手机后台挂起与恢复体验 | `INITIAL_DESIGN §16` 未验证项 5 |

---

## 7. G 流程（必须做，但不是代码）

| # | 缺什么 | 依据 |
|---|---|---|
| G1 | **设计结论回写** `docs/FRONTEND_DESIGN.md` | 原型已定的数十个交互，目前一条都没进权威文档 |
| G2 | **openspec 变更提案**（C1 / C2 / C3 各自涉及 core、storage、sync、compat 四处合同） | `AGENTS.md §8` |
| G3 | 把 `FRONTEND_DESIGN §9` 的 10 条验收标准做成可执行检查 | `FRONTEND_DESIGN §9` |

---

## 8. 原型功能 → 依赖（提案时按这张表切分）

| 原型位置 | 功能 | 依赖 |
|---|---|---|
| 配对页 | 二维码落地、SAS 比对、18 个状态 | B1.9、B1.2、A2 |
| 目录页 | 目录分组、每目录 3～5 条、重名显示路径尾段 | **C1**、B1.4、B1.5（`session.list`） |
| 目录页 | `⊕ 新建会话` → 选 Agent | **C2 + C3**、B1.5 |
| 目录详情页 | 按月分档、搜索、筛选 | C1、B1.4 |
| 对话页 | 消息流（Markdown/代码高亮） | B1.6、B1.3 |
| 对话页 | 思考/工具/审批折叠块 | B1.6、B1.5（`permission.resolve`） |
| 对话页 | 上下文占比 | B1.6（`session.usage.changed`） |
| 对话页 | 变更文件 + 双向联动 | B1.6（`file.changed`） |
| 对话页 | 降级卡片（未知事件/图片未获取/原文未同步） | 存储已具备 raw 字段；需 B1.6 装配 |
| 对话页 | 诊断抽屉 | B1.6 + A3 |
| 顶栏 | 主机与连接面板、9 态阻断 | B1.2/B1.3、`auth.*` |
| 顶栏 | 主题切换 | 纯前端 |

---

## 9. 建议的落地顺序

```text
① 合同 C1（目录字段）        ← 目录页一切的前提
    ② server::sync 最小闭环   ← 认证 + session.list + 快照 + 事件 + prompt/cancel + permission
 （前端可用 fixtures 并行起步，不必等 ②）
    ③ 前端工程 A1–A7 逐步对齐原型
    ④ 合同 C2 + C3            ← v1.1，含安全评审（设备可远程启动本机进程）
    ⑤ C4 路径口径决策         ← 改合同 或 收回完整路径
```

依赖关系上的两点提醒：

1. **C1 必须早于 B1.4**：快照的 `sessions` 资源与 `session.list` 结果都要带目录，否则前端第一版就得为「没有目录的会话」写一套兼容分支。
2. **C2/C3 建议独立变更**：它们把 PWA 的能力边界从「查看与交互已有会话」扩大到「远程启动本机进程」，安全评审与实现应分开。

---

## 10. 需要你先决策的四件事

| # | 决策点 | 现状 | 影响 |
|---|---|---|---|
| 1 | C4：目录绝对路径下不下发 | 原型两处不一致 | 详情页形态 + 是否要改两份权威文档 |
| 2 | C2：设备侧创建会话是否进 v1 | 原型已画出入口 | 决定要不要做安全评审与新 feature |
| 3 | 队列策略是否保留入口 | 原型单按钮屏蔽了排队发送 | `sessions.queue_policy` 默认 `queue` 会暂时用不上 |
| 4 | 主题是否做成产品内开关 | 原型顶栏有按钮，但按 PWA 规范应跟随系统 | 决定这行代码留不留 |