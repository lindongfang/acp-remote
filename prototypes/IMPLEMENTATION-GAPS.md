# PWA 实现缺口清单

> **这不是权威文档。** 它是「对照设计原型，当前还差什么」的现状记录，供后续写 openspec 变更提案时切分任务用。
> 产品行为、协议语义、安全模型仍以 `docs/` 下的权威文档为准；若本文件与权威文档冲突，以权威文档为准。
>
> 本轮更新：2026-10-17，依据当前分支实测（桌面原型 `prototypes/acp-remote-pwa.html`，实现在 `clients/app/`、
> `crates/`），把变更 `pwa-prototype-parity`（前端：共享外壳、三页重写、创建与文件夹浏览、四个裸标记组件迁基座）
> 与变更 `sync-path-browse-and-diff-delivery`（合同：目录路径下发、`workspace.browse`、`session.create` 的建目录开关、
> 结构化差异行）落地后的现状写回。
> 上一版写于 2026-10-12，那时**前端视觉与交互层只有目录页与目录详情页有样式**，对话页是两栏只读、配对页与 `hosts`
> 页壳还是裸标记，四条命令在协议里存在而前端不 dispatch。**那一版 §2.1/§2.2 的绝大多数条目属于「实现落后于原型」，
> 今天已经落地**；本版把它们改记为已对齐，只留仍然成立的缺口。

---

## 0. 结论速览

| 层 | 状态 | 证据 |
|---|---|---|
| 合同（目录字段 / Sync `session.create` / Agent 目录 / 快照范围 / **目录路径下发 / 目录浏览 / 结构化差异行**） | **已落地** | `openspec/specs/` 的 `sync-workspace-catalog`、`sync-session-create`、`sync-snapshot-scope`、`sync-event-projection`、`core-derived-events`、`workspace-resolution`，以及变更 `sync-path-browse-and-diff-delivery` 的增量规格（其中的 `sync-workspace-browse` 是本变更新增的能力，尚未并入 `openspec/specs/`）；`compatibility/commands/v1/commands.json`（14 条命令，13 条走 sync 传输，含 `workspace.browse` 且归 `grant.observe`）；`compatibility/features/v1/features.json`（含 `core.workspace-browse.v1`/`core.file-diff.v1`；目录条目的 `path` 始终随快照下发，`core.workspace-path.v1` 已不在机器登记表里）；`schemas/sync/v1/**` 与 `fixtures/sync/v1/**` |
| 前端工程与逻辑层（配对状态机、连接 12 态、快照/重放、事件去重、命令幂等、目录模型、创建链路、对话读取与翻页、降级卡片与诊断、主机面板、imported 不落盘） | **已落地** | `clients/app/src/**`，含单测与浏览器 E2E（`clients/app/e2e/page/checks/`） |
| 前端**视觉与交互层** | **已落地** | 共享外壳（`src/components/shell/`，装在 `app/_layout.tsx`；`/pair` 与 `/node-link/pair` 隐藏全局顶栏、不绘制页头与返回——桌面原型本来就没有页头，移动原型的「标题栏 + 主题按钮」是登记的有意偏离）+ 目录页、目录详情、对话页、`hosts` 与配对路由 + 创建与文件夹弹层（`CreateSessionSheet.tsx`、`DirectoryPicker.tsx`）；`src/components/` 下**已没有**不带 `StyleSheet` 的组件（含 `ContextUsageBar`、`DiagnosticsDrawer`、`DegradedEventCard`、`PairingPanel` 四个原裸标记组件） |
| `server::sync` | **已落地**（认证、订阅与投递、快照、配对 HTTP 与业务命令面，含 `workspace.browse`、路径下发与 `diffLines` 投影）；**残留见 §3.1** | `crates/server/src/sync/**`（`conn`/`subscribe`/`snapshot`/`event`/`command`/`pairing`/`router`） |
| 前端静态资源托管（原 `/ui` 项） | **已落地** | Daemon **根路径**同源托管、产物嵌入二进制、发布构建缺产物拒绝启动（`server::transport::net::ui`、`crates/server/build.rs`）；`docs/FRONTEND_DESIGN.md §4.3` |
| 真实浏览器 + 真实 Daemon 的端到端 | **已完成**（2026-10-16 逐条实测） | `README.md` 的「仓库当前状态」；记录见归档的 `openspec/changes/archive/2026-10-16-frontend-sync-wiring/plan.md` 的「真机验收记录」；`docs/FRONTEND_DESIGN.md §9` 已同步为「已执行」 |

一句话：**原型里的四个页面、共享外壳与创建链路都已经落进 `clients/app`；今天剩下的缺口不在三个页面本身**——它们在
①主机面板的信息/动作集合；②移动原型那一套导航形态（已登记的有意偏离）；③PWA 安装与 i18n
（已登记为非目标）；④服务端两处**如实降级**（`session.read` 的三类明细、imported 事件回源投递）与 `acp_facade`、
`node-link-client` 两个尚未开始的切片。

---

## 1. 已实现（不要重做）

以下都已存在且有测试，不要在实现 UI 时重写其语义：

- 七层目录与依赖方向约束（`clients/app/src/layers.ts` + `layers.test.ts`）、组合根（`src/composition.ts`）。
- 设备身份：WebCrypto **不可导出** P-256 `CryptoKey` + IndexedDB（`src/platform/secure-storage.web.ts`）。
- 连接状态机 12 态、命令 7 态（`draft`/`submitting`/`accepted`/`completed`/`failed`/`rejected`/`uncertain`）、
  `requestId` 复用与「仅服务端确认才呈现 uncertain」（`src/state/`）。
- Sync 客户端：握手、`subscribe`、ACK、增量重放、快照暂存与原子替换、`eventId` 去重（`src/sync-client/`）。
- 协议 DTO：严格消费 `schemas/sync/v1/**` 与 `fixtures/sync/v1/**`（`src/protocol/`）。
- 目录页与目录详情**数据模型**、两种空态、离线来源标注（`src/features/directory-model.ts`）。
- 会话创建链路：精确三键载荷（`agentId` + 目录引用 + 可选的 `createIfMissing`）、创建中/终态/结果不确定、
  目录浏览状态（`src/features/create-session-model.ts`、`src/features/client-store.ts`）。
- 对话页：消息与结构化条目的同一条时间线、三态上下文条、翻页游标、降级事件、能力标记、左栏会话列表与
  右栏差异行（`src/features/conversation-model.ts`、`src/domain/timeline.ts`）。
- 共享外壳与主题三态（跟随系统 / 白天 / 黑夜，持久化为客户端 UI 偏好）、开发模式横幅与徽标、帮助抽屉、toast
  （`src/components/shell/`、`app/_layout.tsx`）。
- 降级卡片与诊断抽屉（原文不可用时不渲染占位、三类不支持可区分）。
- 主机与连接面板模型（Agent 列表来自快照、覆盖层缺席 = 状态未知）。
- 本地缓存边界：imported 不落盘、本节点摘要缓存 8 MiB / 30 天 / LRU（`src/platform/local-cache.web.ts`）。

前端今天 dispatch **10 条命令**：`session.create`、`session.read`、`session.mode.list`、`session.prompt`、
`session.cancel`、`permission.resolve`、`elicitation.respond`、`session.config.set`、`session.mode.set`、
`workspace.browse`（`src/features/client-store.ts`）。其余命令名只存在于协议类型与测试里；目录与 Agent 目录
改由快照供数，因此**不**需要 `session.list` / `session.config.list` 这两次往返。

---

## 2. 对照原型的 UI 缺口

### 2.1 对话页（已对齐）

上一版这里是最大缺口：实现只有一条只读单栏列表，原型是三栏 + 输入区 + 变更文件右栏。本变更把三栏骨架、
输入区组合与时间线条目形态一次补齐，逐项对照如下（状态按 `pwa-prototype-parity` 落地后的代码核对）。

| 原型有 | 实现落点 |
|---|---|
| 宽屏三栏（300 / 1fr / 360），窄屏把两侧降为次级视图 | **已实现**：`components/ConversationStream.tsx`；窄屏的次级视图扩到四项（`all` / `timeline` / `sessions` / `files`，见 `features/conversation-model.ts`），四列内容**始终在 DOM**，两形态的 `data-*` 集合相同 |
| 左栏：目录上下文（展示名 + 逐字路径 + 「全部 ›」）+ 目录内搜索 + 同目录会话列表 + 「已显示 N / M」+ 加载更多 + 吸底新建会话 | **已实现**：搜索只影响左栏、不改中栏；当前会话没有目录归属时明确说明而不是隐藏中栏 |
| 中栏会话头：标题、状态徽标、Agent 名、Agent 胶囊 popover（覆盖层缺席呈现「状态未知」）、连接状态条 | **已实现**：胶囊与主机面板共用同一份连接事件覆盖层；缺席**不**写成「已断开」 |
| 输入区：上下文胶囊（进度条 + 占比 + 已用/上限，未上报显示「—」）+ ≥85% 警告条 + 模型/模式胶囊与弹层 + 输入行三态（可输入 / 进行中 / 已关闭）+ 发送 / 停止 + 提示行（`Enter` 发送 · `Shift+Enter` 换行 · 附件不受支持） | **已实现**：`components/ConversationComposer.tsx`、`ConversationSettingsPanel.tsx`；不可提交时回车不发命令 |
| 「跳到你最近的输入」胶囊、待处理交互跳转条 | **已实现**：没有用户输入时不呈现 |
| 时间线条目形态：用户消息靠右气泡、Agent 气泡走 Markdown、代码卡（语言条 + 复制）、运行中指示、失败条 | **已实现**：`components/ConversationTimeline.tsx`、`Markdown.tsx`、`CodeBlock.tsx`；代码复制与链接点选**已接线**（此前组件给了按钮而页面没接） |
| 结构化折叠块（思考 / 工具调用 / 终端输出 / plan）：默认展开值由条目状态决定、显式操作只影响本条、带文件标识的条目选中时与右栏联动 | **已实现**：`expandedEntries` 只记显式覆盖值；`selectedChangeId` 由页面持有，时间线条目与右栏清单**共用同一份**（原型 `A.selFile` 的双向高亮） |
| 权限请求卡三按钮 + 「已被电脑处理」的折叠块 | **已实现**：`InteractionAnswering.tsx` 的三按钮形态（底色按严重度分档，可选项只来自该请求自身的下发内容）；非 `answerable` 的几条边界各有可区分说明（仅从历史读到 ⇒ 不可操作并说明原因、已被其它设备处理 ⇒ 说明不是本设备所为） |
| 变更文件右栏：逐行差异（新增 / 删除 / 上下文）+ 「查看 ACP 原文」诊断入口 | **已实现**：`components/FileChangesPanel.tsx` 渲染 `file.changed` 视图的 `diffLines`；没有结构化来源时呈现 `diffUnavailable` 的说明，**不解析** `acp.rawJson` |

**修正一处旧记法**：上一版把原型的「usage 折叠块」记成时间线上的专用卡。原型的时间线折叠块只有
think / tool / perm 三类，上下文用量是**输入区的胶囊 + 弹层**（`ctxChip` / `ctxWarn`）；它已由上面的
`ConversationComposer` 与 `ContextUsageBar` 覆盖，不另立缺口。

**仍未对齐（一条，且不在客户端）**：Node Link 面的**结构化历史**——本版尚未实现（`docs/NODE_LINK_PROTOCOL.md`
§12.7 与 §17 登记的已知缺口）。经 Node Link 展示的 imported 会话在客户端侧对这部分**显式降级**，不得让用户误以为
该会话没有结构化内容。

### 2.2 目录页与目录详情（已对齐）

原型（`acp-remote-pwa.html` 的 `viewDirs` / `viewDirDetail`）与实现（`components/DirectoryList.tsx`、
`DirectoryCard.tsx`、`SessionRow.tsx`、`DirectoryDetail.tsx`、`EmptyState.tsx`）已逐项一致。

上一版在本节列出的缺口（搜索框、状态筛选分段、目录内联会话行 + 「查看全部」+ 折叠、详情页按月分档、离线提示条与
同步时间、目录页权限提示条、卡片「最近活动时间」、卡片上的「⊕ 新建」、卡片头部可进入详情页、0 会话死路、卡片体
0 会话提示行）**已全部不再成立**：其中大部分已落地，另有两项是被权威决策**移除**或**改形**（目录页权限提示条随
设备权限层删除；「离线提示条」改为在页面顶部保留「来源 + 同步时刻」标注，属 `pwa-prototype-parity` 登记的「来源与
同步时刻标注」一项有意偏离）。需要按新口径记住的是
**原型这一轮自己也改了组织方式**，因此下列变更不是「实现落后于原型」，而是两侧同时改：

- **移除**月份分档、月份切换、目录内搜索与「当前显示 N / M」，改为**按时间倒序平铺 + 每页 10 条 + 页码翻页**
  （非法页码就地纠正、只有一页不呈现翻页、翻页后回到页首）。
- **移除**目录项上的「最近活动时间」；目录项与详情页改为原型的时间口径——会话时间统一走**相对文案**
  （刚刚 / N 分钟前 / 日期，UTC 固定切片），时钟由视图模型注入。
- 卡片头改为原型的槽位（折叠箭头、目录图标、展示名、同名时的路径尾段、`N 个` 计数胶囊、运行中/待处理徽标、
  弹性空白、`⊕ 新建`、`查看全部 ›`）；卡体改为**缩进树**，会话行改为五字段（状态圆点、标题、空标题斜体
  「未命名会话」、状态词、Agent、相对时间）。
- **目录标识不再出现在界面上**：`directoryId` 只作路由与命令引用的内部标识；展示名由服务端下发的 `path` 尾段派生，
  两个尾段相同时以**更多路径段**消歧，尾段唯一时不呈现任何额外路径片段。目录页搜索只匹配展示名与会话标题。
- 「这台设备没有创建会话的权限」提示条与 `A.canCreate` 门控**已删除**：设备方向没有授权维度，入口不按权限门控
  （`docs/FRONTEND_DESIGN.md` §2.2/§9）；反回潮断言见 `r15-directory-list.test.ts`、`r16-create-session.test.ts`。
- **详情页页头与对话页左栏同构**（次级「目录」按钮返回目录页 / 目录名 + 会话数 / 路径）：实现已按原型改为
  描边次级按钮 + 21px 目录名 + 会话数胶囊 + 「路径」等宽值；会话行直接铺在页面上，不再套一层带边框的卡片。
- **目录页的视觉与行为细节已补齐**：卡片始终单列堆叠（不再随宽屏并排）；卡片头自成一栏（`panel-2` 底 +
  下边框）；折叠箭头不带底色块；`⊕ 新建` 是淡主色底+主色字，`查看全部 ›` 是弱化文字；顶部 `⊕ 新建`
  是实心主色底白字；搜索框带 🔍 图标且最大宽度 420。卡内会话行按搜索/筛选收窄，超过 5 行时卡体底部
  再给一个「查看全部 ›」；工具条在所有状态都保留搜索、状态筛选与右上角创建入口，「一个目录都没有」的
  空态另在说明块里给一个常规尺寸的创建入口，标题改为「还没有可用的目录」。

**实现优于原型且保留的两处**：0 会话的目录同样能进详情页（原型只有「查看全部」且只在有会话时给出）；空态下同时
列出目录项，使空态指引说的「在下面选一个目录」真的存在（本机一个目录都没有时仍只呈现空态）。

### 2.3 顶栏 / 主机与连接（部分缺口）

已落地（`components/shell/`：顶栏 logo + 设备胶囊 + 开发模式徽标 + 主题开关 + 帮助入口、开发模式横幅、帮助抽屉
+ 遮罩 + toast；`HostConnectionPanel.tsx`：Agent 列表来自快照、连接状态**只**由设备胶囊表达、节点级降级事件的
出口）。设备胶囊的三档色与「需要注意 / 阻断」两档追加短词、阻断态从胶囊进主机与连接面板都已接上。

**仍缺（原型有、实现没有）**：

- 主机与连接面板的**信息集合**：主机指纹（含「指纹 = SHA-256(主机公钥)」说明）、「本版本边界」说明段、
  设备名与客户端类型。今天面板给的是地址、主机 ID、本机设备 ID 与 Agent 列表。
- 面板的**三个动作**：「立即重连」、「重新配对」、「清除数据」及其二次确认。重新配对今天经配对路由
  （`/pair`）本身进入，面板里没有入口；平台层的清除能力（`secure-storage` / `local-cache`）已存在，只是没有界面。

### 2.4 配对页（实现已对齐；桌面原型有一处未落地调整）

已落地：配对页只接受**地址片段**作为入口（打开带 `#data=` 的配对地址即进入流程，读到片段后立即清除；`/pair`
的 sync 载荷据此自动开始认领，`/node-link/pair` 的 node-link 载荷只呈现对端信息、不发起 claim/status），页内
没有输入框、粘贴入口、提交按钮或「重新扫码」，也不绘制页头与页内返回，全局顶栏仍不呈现；「安全上下文 → 来源
一致 → 载荷类型」三道前置校验（`pairing-origin-secure-context`）、两条密码学校验（用配对载荷里的主机公钥验
`hostProof`；已批准响应的公钥与载荷逐字节比对）、已批准后落盘 `paired-host/<canonicalOrigin>` 并触发连接、
失败原因的封闭词表；页面阶段覆盖 `reading / idle / claiming / pending_confirmation / approved / rejected /
expired / consumed / node_link_scanned / failed`。配对进行中按载荷的 `expiresAt` 显示真实剩余有效期 `m:ss`，
归零或打开即过期进入「地址已过期」并停止轮询；审批通过后约 1.6 秒自动替换到目录页（路由 `/`），不以连接已在线为前提，
连接装配失败仍进目录、由设备胶囊表达断开与重连。失败页九类固定文案（被拒绝、已过期、已被使用、无法识别、
来源不一致、非安全上下文、请求未能完成、载荷类型与入口不匹配、没有载荷）都没有按钮，不渲染后端 `message`；
其中**没有载荷**（直接访问或刷新后片段已清）与**载荷类型与入口不匹配**是原型未画到的两条分支，各有专用标题
与说明，且都不发任何请求。配对页复用当前主题调色板，但页内不提供主题切换。

实现仍按「浏览器里打开配对 URL 即开始」工作：进入页面就是居中的配对中状态（转圈 + 剩余有效期倒计时，原型按 5 分钟展示，
每 2 秒轮询审批），页面没有输入框、顶部返回入口、步骤条或右侧说明；审批通过后自动进入目录，未通过 / 过期 / 已用 /
无法识别 / 来源不一致 / 非安全上下文 / 请求失败只显示原因，失败页不提供进入目录的按钮。原型的 18 个陈列态、6 位 SAS
人工核对与「这台设备申请了这些权限」清单都已删除——它们不是产品行为（设备方向没有授权维度）。

**桌面原型现在按 `openspec/changes/pairing-code-web-entry/` 画了配对码入口，实现尚未跟进。** `acp-remote-pwa.html` 在
`/pair` 无 fragment 时显示 8 位 Crockford 配对码输入（终端显示为 `XXXX-XXXX`）；S0 不显示数字倒计时，只显示静态提示
“配对码 5 分钟内有效”，lookup 成功后开始用服务端 `expiresAt` 倒计时。提交后经 `looking_up` →
`claiming` → `pending_confirmation` 进入既有审批流程，lookup/claim 不重新计时。失败面新增固定呈现「配对码不对或已过期，
请重新生成配对码」，页内不提供重试按钮。这目前只是评审用原型：主文档（`FRONTEND_DESIGN.md` / `SYNC_PROTOCOL.md`）仍规定
页内没有输入框、地址片段是唯一入口，`pairing-model` 也仍只有 fragment 入口。落地时需要先完成 lookup 端点、配对码派生
与内存查找、统一失败与限流、前端 `awaiting_code` / `looking_up` / `code_invalid` 阶段，并写回权威文档；不能只加输入框。

**注意**：`pairing-code-web-entry` 的已确认决策是「移动端只使用二维码，不提供输码入口」。移动原型仍保留配对码入口
（2×4 格），但 S0 已与桌面一致：不显示数字倒计时、只显示静态“配对码 5 分钟内有效”，lookup 成功后才开始倒计时；
入口本身仍与产品决策不一致，属于原型遗留。若要让移动原型与产品决策一致，需要单独把这一块回退成二维码入口。

上一版在此处登记的三条缺口**已全部落地**，不再是缺口：

- **已批准后自动进入目录**：实现按原型显示短暂过渡并在约 1.6 秒后以替换历史条目的方式进入目录页；`/node-link/pair`
  复用同一主体但不跳转、不显示倒计时。
- **页面入口形态**：页内粘贴入口与「重新扫码」已删除，地址片段是唯一入口；没有载荷时按入口类型给不同标题
  （`NO_PAYLOAD_COPY`）——`/pair` 呈现「这个地址里没有配对信息」，`/node-link/pair` 呈现「这个地址里没有节点
  配对信息」，两者都指引用电脑重新生成的地址打开，而不是等待用户粘贴。
- **倒计时**：`expiresAt` 已投影到页面模型并按截止时间每秒刷新；归零即停止轮询并进入过期页，打开时已过期则
  一个请求都不发。这是真实剩余有效期，不是固定从页面打开数 300 秒。

主机指纹与「完成总结卡」不属于配对页：配对页没有凭据输入，实现也只在主机面板给出地址、主机 ID、本机
设备 ID 与 Agent 列表。若要展示指纹，应补在主机面板（见 §2.3），不要在配对页加回凭据类信息。

### 2.5 新建会话弹层与文件夹浏览（已对齐）

已落地（`components/CreateSessionSheet.tsx`、`DirectoryPicker.tsx`、`features/create-session-model.ts`）：

- 目录块的两种形态：已知目录 → 「目录 / 路径」两行；顶部入口 → 「选择文件夹」控件 + 已选路径 + 说明。
- 「选择文件夹」弹层：面包屑（首段「此电脑」）+ 上一级 + 逐层列表（已登记目录标注）+ 新建文件夹 + 页脚
  （当前路径 / 取消 / 选择此文件夹，此电脑层禁用确认）；路径逐字来自服务端，客户端不拼接、不规范化、不反查父层。
- 服务端未提供浏览能力（未协商 `core.workspace-browse.v1`）时**根本不发命令**，弹层明确说明不可用并保留
  「选已登记目录」这条路——不呈现一个必然失败的手工输入。
- 副作用提示（「将在你的电脑上启动一个 Agent 进程」）、结构化失败原因（按错误码给出可区分文案与脱敏说明）、
  创建中 / 终态 / 结果不确定（不确定态不随时间或重连自动消失，只能由用户处理）。

**有意差异（不是缺口）**：原型为「在未登记目录下新建的会话」画了一个 `unsorted` 目录项；本客户端**不造**这个
分组——未指定目录的会话不计入任何目录项，提示文案也不承诺它存在（`create-session-model.ts` 的文件头）。

### 2.6 全局

- **样式基座已覆盖全部组件与四个页面路由**：`clients/app` 的 `src/` 与 `app/` 下仍无 `.css` 文件、无 Tailwind、无
  `className`（原型自带的 CSS 不进实现）；样式走 `react-native-web` 的 `StyleSheet`，基座是
  `src/components/theme.ts` 的浅/深两套 token 与尺度（含新增的断点、字重、层级与动效四类）。四个原裸标记组件
  （`ContextUsageBar`、`DiagnosticsDrawer`、`DegradedEventCard`、`PairingPanel`）已迁入基座且 `data-*` 集合不变；
  根布局只装配共享外壳，`/node-link/pair` 复用 `/pair` 的入口主体（两者因此不自建样式）。
- **响应式**：断点收敛为 `theme.ts` 的 `breakpoints.wide`（720），目录页、目录详情页与对话页都读它；宽屏用
  原型的多栏形态，窄屏收成单列 + 次级视图切换，两形态信息集合相同。**移动原型自己那套导航形态**（底部 tab、
  `☰` push 页、底部全屏 sheet、56px 会话行）没有实现——这是 `pwa-prototype-parity` 登记的有意偏离，不是遗漏。
- 无 PWA `manifest.json`、service worker、安装引导、图标（该变更明列为非目标）。
- 无 i18n（原型为 zh-CN）。
- 原型自带的「原型控制台」不是产品 UI，不移植。

---

## 3. 服务端缺口

### 3.1 `server::sync`（已落地，残留见下）

`crates/server/src/sync/` 已包含 `conn`（四消息握手、来源绑定、单设备替换、撤销传播、心跳与静默超时、close code
映射、连接注册表与有界出站账本）、`subscribe`（cursor 判定、增量重放与扇出 barrier、累计 ACK）、`snapshot`
（清单资源组装、分块与 `snapshotDigest`）、`event`（事件视图投影与 `payload.view` 装配）、`command`（13 条 Sync
命令的五态终态、`(actor, requestId)` 幂等、`expectedVersion` 版本冲突、终态观察者）与 `pairing`（两条配对
HTTP），由组合根 `crates/app/src/daemon.rs` 的 `register_sync_paths` 接线。本轮另随合同落地：快照 `workspaces`
条目的路径键、`workspace.browse` 的逐层列举、`file.changed` 视图的结构化差异行投影。

**仍缺（如实登记，不写作已实现）**：

- `session.read` 的 `turns` / `pending_interactions` / `capabilities` 三类明细与 imported 会话的明细一律回
  `capability.unsupported_by_broker`（`docs/SYNC_PROTOCOL.md` §9.4）。客户端 MUST NOT 把它渲染成「没有内容」或
  「已到最早一条」，要按「本期服务端不提供这一类」说明原因；`messages` / `config_options` / `history` 三类不受影响。
- **imported 事件回源投递**仍是计划条款（`docs/SYNC_PROTOCOL.md` §9 注记）。
- **Owner 可达性**今天由 `NodeLinkReachability` 读入站注册表得出，是**近似值**，真实出站数据源未落地
  （`README.md` 的「仓库当前状态」）。
- `server::acp_facade` 仍未开始（属 Zed 闭环，PWA 走 Sync）；`node-link-client`（切片 6）仍未开始。

### 3.2 前端静态资源托管（已落地，不再是缺口）

Daemon 在**根路径**同源托管 PWA 产物：入口文档、单页应用回退、按扩展名的内容类型映射，CSP 与四个安全响应头
由响应头下发；协议端点是 router 的优先路径，静态面只是 fallback。产物以 `rust-embed` 嵌入二进制，**发布构建缺
前端产物即拒绝启动**；构建版本可经 `daemon.status` 的 `uiBuildVersion` 查看（`docs/FRONTEND_DESIGN.md` §4.3）。
注意历史措辞的差别：早先设计的目标路径是 `/ui`，实绩是**根路径**。

### 3.3 本次不需要

- `server::acp_facade`：属 Zed 闭环，PWA 走 Sync。
- `node-link-client`：PWA 直连单个 Owner Node。

---

## 4. 与原型不一致、但已按权威文档决策（不是缺口）

| 项 | 原型的做法 | 实现的做法 | 依据 |
|---|---|---|---|
| 目录路径 | 详情页与对话页左栏显示完整路径，目录页显示尾段 | **与原型同向**：快照 `workspaces` 条目始终下发 `directoryId` 与 `path`，详情页与左栏呈现路径；但只呈现**服务端下发的值**，客户端不得拼接、规范化或反查；目录标识不呈现为界面文本 | `docs/SYNC_PROTOCOL.md` §9.4/§11.5、`docs/SECURITY_DESIGN.md` §12.3、`docs/FRONTEND_DESIGN.md` §4.1。上一版此行的「原型的完整路径是待决策项」已由 `sync-path-browse-and-diff-delivery` **结项** |
| 未登记目录下新建的会话 | 原型画了一个 `unsorted` 目录项 | 不造这个分组：未指定目录的会话不计入任何目录项 | `docs/FRONTEND_DESIGN.md` §4.1（目录项只来自服务端下发的目录） |
| 创建入口的权限门控 | 原型有「这台设备没有创建会话的权限，需要电脑端授予」的降级态与 `A.canCreate` 开关 | 入口**不按权限门控**；页面不出现该文案 | `docs/FRONTEND_DESIGN.md` §2.2/§9（设备方向没有授权维度） |
| 远程（imported）会话 | 原型不涉及 | 事件 `origin` 与 `origin.online_changed` 已落地；但**明细**（`turns`/`pending_interactions`/`capabilities` 与 imported 明细）本期回 `capability.unsupported_by_broker` | `docs/SYNC_PROTOCOL.md` §9.4、`docs/FRONTEND_DESIGN.md` §4.2/§6 |
| 会话创建入口 | 原型 README 旧文案写「本版不提供创建会话入口」 | 已实现受限创建入口（目录引用取已登记目录标识 `directoryId` 或浏览选定的层，二者之一） | `docs/FRONTEND_DESIGN.md` §2.2 已改为受限入口，旧文案过时 |
| 队列策略 | 原型用单按钮屏蔽了「运行中排队发送」 | 同样未暴露 | 两边一致，非缺口 |

---

## 5. 待决策 / 尚未设计

本轮已结项一条：**主题是否做成产品内开关**——三态（跟随系统 / 白天 / 黑夜）已落地为客户端 UI 偏好
（`docs/FRONTEND_DESIGN.md` §4.1），跟随系统是缺省值，不再是待决策项。

- **移动端布局的收口**：响应式已落地（720px 断点 + 次级视图），但移动原型自己那套导航形态（底部 tab / push 页 /
  底部 sheet / 56px 触达）要不要做、做成哪一档，仍未决策（`docs/FRONTEND_DESIGN.md` §9 第 11 条只登记了「与原型
  一致、差异只剩有意偏离」）。
- PWA 安装引导与 service worker 更新提示（`docs/FRONTEND_DESIGN.md` §4.4 要求可安装；本轮变更明列为非目标）。
- 手机后台挂起与恢复体验（`docs/INITIAL_DESIGN.md` §16 未验证项 5）。
- PWA WebCrypto P-256 / IndexedDB 的实际浏览器支持矩阵与验收结果，以及 §11 里其余仍未收口的暂缓项
  （`docs/FRONTEND_DESIGN.md` §11）。
- ~~**权威文档之间的一处口径冲突待核对**：`README.md` 与 `docs/FRONTEND_DESIGN.md §9` 对真实 Daemon 的浏览器级
  端到端验收是否已执行的口径不一致。~~ 2026-10-17 复核：`FRONTEND_DESIGN.md §9` 已写明该验收**已执行**（2026-10-16
  逐条实测，记录见归档的 `frontend-sync-wiring/plan.md`），本文件此处不再登记冲突。

---

## 6. 剩余工作的建议顺序

```text
① 主机与连接面板补信息与动作：指纹与说明、本版本边界、立即重连 / 重新配对 / 清除数据（二次确认）
② 服务端如实降级的三项收口：session.read 三类明细、imported 事件回源投递、Owner 可达性的真实数据源
③ Node Link 面的结构化历史（协议侧既有缺口，客户端已对 imported 显式降级）
④ PWA 安装（manifest / service worker）、i18n、移动原型那套导航形态            ← 均已被登记为非目标
```

说明：①—④ 都是**增量**，不再有「必须先等某个前置变更」的阻塞——三页与外壳已经落地（配对页的入口、倒计时、
通过后自动进目录与失败页呈现也已对齐原型；移动原型的页头与主题按钮是 §0 表登记的有意偏离），`server::sync` 与根路径静态托管也已落地，真实 Daemon 的浏览器级
端到端验收已记录为完成（出处见 §0 与 §5）。①② 的判据仍以 `docs/` 的权威文档与各变更的 specs 为准，本文件
只记「还剩什么」。
