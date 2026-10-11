# PWA 实现缺口清单

> **这不是权威文档。** 它是「对照设计原型，当前还差什么」的现状记录，供后续写 openspec 变更提案时切分任务用。
> 产品行为、协议语义、安全模型仍以 `docs/` 下的权威文档为准；若本文件与权威文档冲突，以权威文档为准。
>
> 本轮更新：2026-10-11，依据当前分支实测（桌面原型 `prototypes/acp-remote-pwa.html`，
> 已实现代码在 `clients/app/`）。上一版写于前端工程存在之前（「无 `clients/` 目录」），
> 那部分已整体过时——**前端工程与逻辑层已落地，且 C1/C2/C3 合同也已经补齐**。

---

## 0. 结论速览

| 层 | 状态 | 证据 |
|---|---|---|
| 合同（目录字段 / Sync `session.create` / Agent 目录 / 快照范围） | **已落地** | `openspec/specs/sync-workspace-catalog`、`sync-session-create`、`sync-snapshot-scope`；`compatibility/commands/v1/commands.json` 的 `session.create` 已含 `transport: ["sync"]`；`schemas/sync/v1/common.schema.json` 的 `sessionSummary` 已有 `workspace` |
| 前端工程与逻辑层（配对状态机、连接 12 态、快照/重放、事件去重、命令幂等、目录模型、创建入口、对话读取与翻页、降级卡片与诊断、主机面板、imported 不落盘） | **已落地** | `clients/app/src/**`，含单测与浏览器 E2E（`clients/app/e2e/`） |
| 前端**视觉与交互层** | **基本未落地** | 组件是无样式语义标记（全仓无 CSS / `StyleSheet`）；对话页只读 |
| `server::sync` | **仍未落地** | `crates/server/src/transport/net/route.rs` 注释：未注册 path（含 `/sync/v1*`）一律 404 |
| `/ui` 静态资源托管 | **仍未落地** | 无任何注册的静态路由 |

一句话：**能算、能测的层都有了；用户能看见、能点的层基本还没有，且它连不上真机。**

---

## 1. 已实现（不要重做）

以下都已存在且有测试，不要在实现 UI 时重写其语义：

- 七层目录与依赖方向约束（`clients/app/src/layers.ts` + `layers.test.ts`）、组合根（`src/composition.ts`）。
- 设备身份：WebCrypto **不可导出** P-256 `CryptoKey` + IndexedDB（`src/platform/secure-storage.web.ts`）。
- 连接状态机 12 态、命令 6 态、`requestId` 复用与「仅服务端确认才呈现 uncertain」（`src/state/`）。
- Sync 客户端：握手、`subscribe`、ACK、增量重放、快照暂存与原子替换、`eventId` 去重（`src/sync-client/`）。
- 协议 DTO：严格消费 `schemas/sync/v1/**` 与 `fixtures/sync/v1/**`（`src/protocol/`）。
- 目录页与目录详情**数据模型**、两种空态、离线来源标注（`src/features/directory-model.ts`）。
- 会话创建入口：精确两键载荷、无权限说明、创建中/终态/uncertain（`src/features/create-session-model.ts`）。
- 对话页**只读**模型：消息文本、三态上下文条、翻页游标、降级事件、能力标记
  （`src/features/conversation-model.ts`、`src/components/ConversationStream.tsx`）。
- 降级卡片与诊断抽屉（原文不可用时不渲染占位、三类不支持可区分）。
- 主机与连接面板模型（Agent 列表来自快照、覆盖层缺席 = 状态未知）。
- 本地缓存边界：imported 不落盘、本节点摘要缓存 8 MiB / 30 天 / LRU（`src/platform/local-cache.web.ts`）。

前端目前**只 dispatch 两条命令**：`session.create`、`session.read`。其余命令名只存在于协议类型与测试里。

---

## 2. 对照原型的 UI 缺口

### 2.1 对话页（最大缺口）

原型的对话页是「左栏会话 / 中间流 + 输入区 / 右栏变更文件」三栏，实现只有一个只读的单栏列表。

| 原型有 | 实现现状 |
|---|---|
| 输入框 + 发送/停止按钮、`Enter` 发送 / `Shift+Enter` 换行、仅文本与附件不支持说明 | **完全没有**；`ConversationStream` 只输出 `data-can-send` 等标记，没有输入控件 |
| 发送与取消命令（`session.prompt` / `session.cancel`） | 前端无 dispatch |
| 权限请求卡（允许一次 / 始终允许 / 拒绝）、已被电脑处理的折叠块、`interaction.already_resolved` | 无（无 `interaction.*` dispatch） |
| 结构化专用卡：思考、工具调用（含参数/输出/退出码）、终端输出、plan、usage 折叠块 | 无；非文本消息只标一个「含结构化内容」 |
| 变更文件右栏 + diff 视图 + 与消息块双向联动 | 无 |
| 模型 / 模式选择器（`session.config.list|set`、`session.mode.list|set` 及对应 `*changed` 投影） | 无 |
| 上下文 chip 弹层 + ≥85% 警告条 | 只有 `ContextUsageBar` 的三态条 |
| Agent chip popover（本会话涉及的 Agent） | 无 |
| 左栏：同目录会话列表 + 目录内搜索 + 分页 + 新建入口 | 无 |
| Markdown 渲染、语法高亮、代码复制、链接/图片安全降级 | 无；消息按纯文本 `<p>` 渲染 |
| 「跳到你最近的输入」胶囊、待处理授权跳转条、连接状态条（正在追平 / 重连第 N 次） | 无 |

### 2.2 目录页与目录详情

- 搜索框（按目录名 / 会话标题）。
- 状态筛选分段（全部 / 运行中 / 待处理）。
- 每个目录内最多 3～5 条会话 + 「查看全部」+ 目录折叠。
- 详情页按月分档 + 月份分段 + 「当前显示 N / M」。
- 离线提示条与同步时间（「已同步 14:33」/「离线 · 2 分钟前同步」）。
- 目录页顶部的「这台设备没有创建会话的权限」提示条（实现只在详情页按钮上表达）。

### 2.3 顶栏 / 主机与连接

- 顶部 logo 与 hostchip（点击打开主机面板）。
- 开发模式横幅与「开发模式」徽标（`FRONTEND_DESIGN §4.4` 要求明文连接必须显著提示）。
- 帮助抽屉（ⓘ）。
- 主题切换（跟随系统 / 白天 / 黑夜）。
- 主机面板内容：设备名与类型、设备 ID、当前 scope 标签列表、主机指纹（含「指纹 = SHA-256(主机公钥)」说明）、本版本边界说明、「立即重连」/「重新配对」/「清除数据」二次确认。

### 2.4 配对页

- 二维码扫描/粘贴入口与落地处理。
- 6 位 SAS 人工核对、倒计时环、每 3 秒轮询提示。
- 步骤条（已认领 → 电脑已批准 → 建立安全连接）。
- 权限清单（本次申请的 scopes）、「接下来会发生什么」、防转发安全提示。
- 原型的 18 个状态（S0 / S2a–d / S3a–e / S4 / S5 / S6 / S7 / S8a / S8b / S9 / S10）；
  实现只覆盖 `idle / awaiting_qr / claiming / pending_confirmation / approved / rejected / expired / consumed / failed`。
- 完成总结卡与「进入会话列表」。

### 2.5 新建会话弹层

- 「将在你的电脑上启动一个 X 进程，工作目录 Y」副作用提示。
- 结构化失败原因（凭据缺失 / Agent 启动超时 + 脱敏摘要）。
- 已具备：Agent 选择、默认标记、无权限说明、创建中 / 终态 / 结果不确定。

### 2.6 全局

- **无任何样式**：无 CSS、无 Tailwind、无 `StyleSheet`，组件没有任何 `style`/`className`。
- 无 PWA `manifest.json`、service worker、安装引导、图标。
- 无响应式断点（移动端原型已画出，实现未做）。
- 无 i18n（原型为 zh-CN）。

---

## 3. 服务端缺口（PWA 连不上真机的原因）

### 3.1 `server::sync`（整个不存在）

`crates/server/src/` 只有 `local_admin` / `node_link` / `transport`。因此以下全部缺失：

- `/sync/v1` WebSocket 路由与 subprotocol `acp-remote.sync.v1.json`。
- 认证四消息（`client_hello` / `server_challenge` / `client_proof` / `authenticated`）与 `auth.*` 错误码。
- `subscribe` + 增量重放 + `caught_up` / `reset_required`。
- 快照组装（`sessions` / `workspaces` / `agents` 三类资源 + 分块 + `snapshotDigest`）；
  可参照 `crates/server/src/node_link/resource.rs`。
- 12 条 Sync 命令处理器 + 幂等记录 + `command.result` 终态（`docs/SYNC_PROTOCOL.md` §11.5）。
- 34 类事件视图投影（`payload.view`，`schemas/sync/v1/event-views.schema.json` 的 34 个 `$defs`）
  + `acp.rawJson` 装配 + `rawUnavailable`。
- `limits` 下发、限流与 4429、撤销传播、授权拒绝。
- `/sync/v1/pairing/claim` 与 `/sync/v1/pairing/status` 的 HTTP 处理器
  （`identity-auth` 状态机已有，缺 server 侧接线）。

### 3.2 `/ui` 静态资源托管

无静态路由，也没有「前端构建版本 ↔ Daemon」的可检查关联（`FRONTEND_DESIGN §4.3` 要求）。

### 3.3 本次不需要

- `server::acp_facade`：属 Zed 闭环，PWA 走 Sync。
- `node-link-client`：PWA 直连单个 Owner Node。

---

## 4. 与原型不一致、但已按权威文档决策（不是缺口）

| 项 | 原型的做法 | 实现的做法 | 依据 |
|---|---|---|---|
| 目录路径 | 详情页显示完整路径、目录页显示尾段 | 只显示展示名与别名，路由参数里也没有路径 | `FRONTEND_DESIGN §2.2/§4.1`、`SECURITY_DESIGN §12.3`、`pwa-web-client` 的 R15。原型的完整路径是**待决策项**，实现是对的 |
| 会话创建入口 | 原型 README 旧文案写「本版不提供创建会话入口」 | 已实现受限创建入口（v1 内） | `FRONTEND_DESIGN §2.2` 已改为受限入口，旧文案过时 |
| 远程会话 | 原型不涉及 | v1 不协商 `resource.remote-origin.v1` | PWA 直连 Owner，属已记录的产品边界 |
| 队列策略 | 原型用单按钮屏蔽了「运行中排队发送」 | 同样未暴露 | 两边一致，非缺口 |

---

## 5. 待决策 / 尚未设计

- 主题是否做成产品内开关（原型顶栏有按钮，但按 PWA 规范应跟随系统）。
- 移动端布局与响应式断点（移动原型已画，`FRONTEND_DESIGN §9` 列为验收项）。
- PWA 安装引导与 service worker 更新提示（`FRONTEND_DESIGN §4.4` 要求可安装）。
- 手机后台挂起与恢复体验（`INITIAL_DESIGN §16` 未验证项 5）。

---

## 6. 建议的落地顺序

```text
① 视觉与响应式层：把无样式语义标记换成真实 UI（先目录页、对话页）  ← 用户能看见的前提
② 对话页交互闭环：prompt / cancel / permission + 乐观状态与终态呈现
③ 对话页富内容：Markdown 渲染、结构化折叠块、变更文件与 diff
④ 模型 / 模式选择器 + Agent chip + 上下文弹层
⑤ server::sync + /ui                      ← 真正连上 Daemon 的前提
⑥ 配对页交互（扫码 / SAS / 倒计时 / 状态）与主机面板补全
⑦ PWA 安装、主题、移动端布局
```

说明：①②③ 可以先用 `clients/app/e2e/` 的 fake host 做视觉与交互验收，不必等 ⑤；
但**端到端可用**必须等 ⑤。
