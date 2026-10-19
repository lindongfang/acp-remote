# Design

## Context

- 参照物是 `prototypes/acp-remote-pwa.html` 的 `viewChat` / `streamHTML` 与相关 CSS；当前实现是 `clients/app` 的 `ConversationStream`、`ConversationTimeline`、`ConversationComposer`、`FileChangesPanel` 与 `app/session/[id].tsx`。
- 同步事件的 `payload.view` 是开放对象；`tool.call.*` 目前只有 `toolCallId` / `turnId` / `title` / `state`，工具参数与输出只存在于 `acp.rawJson`。
- 客户端时间线把同一 `toolCallId` 的多条记录归并成一张卡片，`StructuredRecord` 已携带可选 `acp` 原文。
- 共享外壳的根与内容区目前是文档流；对话页要像原型一样三栏独立滚动，需要一个只作用于对话路由的“填满视口”模式。
- 权威文档中的 `pwa-web-client` 规定连接状态只由设备胶囊表达、左栏会话行含状态/标题/时间；本次按已确认的“原型优先”决策更新这些条文。

## Goals / Non-Goals

**Goals:**

- 对话页的宽屏骨架、左栏行形态、会话头、输入区状态、工具详情与右栏呈现与原型一致。
- 工具详情通过**服务端类型化投影**提供，客户端不解析 `acp.rawJson` 生成结构化内容。
- 保持 ACP 原文逐字节保真、未知字段透传和既有安全口径。
- 合同投影与 UI 对齐在同一个 OpenSpec 变更里，按“合同先合”的顺序落地。

**Non-Goals:**

- 不实现移动原型的导航形态、PWA 安装、i18n。
- 不改目录页/目录详情页的呈现规则；只复用它们已有的共享组件与 token。
- 不把 diff、终端输出或未识别 content 塞进工具详情。
- 不引入新的协议版本或 feature 开关；新增字段是向后兼容的可选视图字段。

## Decisions

### D1 工具详情用可选 `toolDetail` 对象，按部分限流

`tool.call.*` 视图新增可选 `toolDetail`：

```json
{
  "input": "<ACP rawInput 的 JSON 值>",
  "output": "<ACP rawOutput 的 JSON 值>",
  "text": ["<文本 content 块>"],
  "truncatedParts": ["input" | "output" | "text"]
}
```

- 三个部分独立判定：`input` / `output` 各 32 KiB（按序列化后的 UTF-8 字节），`text` 合计 32 KiB；总 `toolDetail` 不超过 64 KiB。
- 超出上限或无法通过 `view` 的 ACPR-CJ1 值域（非整数浮点、超出 2^53−1 的整数、嵌套过深）的部分**整体缺席**，并在 `truncatedParts` 中登记；JSON 部分绝不发送被截断或无法保真的片段，避免客户端拿到无法持久化的值。
- ACP 没有提供的部分直接缺席；没有 `toolDetail` 时视图仍完整。
- `content` 里只取文本块；diff 与 terminal 不进入 `toolDetail`，继续由既有派生逻辑与 `terminal.output` 承担。
- 未识别的 content 类型不透传文本，原文仍可经 `acp.rawJson` 诊断查看。

替代方案：客户端解析 `acp.rawJson`。否决理由：会形成第二套 ACP 解码器，且违反“结构化展示不得靠解析原文补齐”的既有规则。

### D2 映射与归并：部分字段可以跨事件合并

- `agent-host` 的 `tool_call_view` / `tool_call_update_view` 从 ACP `ToolCall` / `ToolCallUpdate` 直接构造 `toolDetail`；`rawInput` / `rawOutput` 保留 JSON 值，`content` 只映射文本块。
- 客户端归并同一 `toolCallId` 的记录时，`input` / `output` / `text` **按部分取最新出现的值**，而不是整块取最新记录：`tool.call.started` 通常只有输入，`tool.call.completed` 才有输出；只取最新一条会丢掉输入。
- `truncatedParts` 取并集，确保任何一次事件中的截断都不会被后一条覆盖掉。
- 卡片保留最新记录的事件标识与 `acp` 原文，供“查看 ACP 原文”入口使用。

### D3 时间线卡片与诊断

- `ToolCallTimelineCard` 增加 `detail`（归一后的三部分与截断集合）与 `eventId`（最新记录），不把原始 `acp.rawJson` 放进卡片渲染路径。
- `ConversationTimeline` 的工具折叠块渲染参数/输出/文本，并提供一个按钮调用既有 `onOpenDiagnostics(eventId)`；诊断抽屉继续只把原文当不可信文本显示。
- 没有 `toolDetail` 时不渲染空的参数/输出区块。

### D4 对话路由的填满视口模式

- 共享外壳的模型增加一个由路由派生的 `contentFill`（仅 `/session/<id>` 为真）。
- `contentFill` 为真时，外壳根使用视口高度、内容区 `flex:1; minHeight:0; overflow:hidden`；其它路由保持现有文档流，避免目录页的滚动行为被一并改掉。
- 对话页自身 `flex:1; minHeight:0`，三栏各用一个 `ScrollView`（`minHeight:0`）独立滚动；会话头与内容来源条固定在中栏顶部，输入区固定在中栏底部。窄屏不启用三栏独立滚动，沿用页面/次级视图的单列滚动。

替代方案：整站改成固定高度外壳。否决理由：会改变目录页与详情页的滚动语义，超出本次范围。

### D5 中栏组合顺序

中栏自上而下：会话头 → 内容来源/连接提示条 → 时间线滚动区 → 输入区。输入区不再放在三栏之外；次级视图切换控件保留在三栏之外，保证中栏被藏起来时仍能切回。跳转胶囊仍固定在中栏/视口底部。

### D6 左栏紧凑行与会话内导航

- `SessionRow` 增加一个明确的紧凑变体（或对话页专用的呈现参数），只渲染状态圆点、标题、状态词与相对时间；目录页继续使用现有的五字段行，不把“Agent 名”从目录页移除。
- 左栏头部：目录名、带“路径”标签的路径、「‹ 目录」返回目录列表、「全部 ›」进入目录详情、带图标的搜索框。
- pager 文案改为“已显示 N / M”；空目录仍给出明确说明。

### D7 输入区三态

- 可输入：可编辑 + 发送。
- 进行中：只读 + 停止；仍在提示行说明“进行中 · 按钮变为停止”。
- 已关闭：只读 + 禁用的发送 + 关闭原因。
- 判定继续来自 `ConversationPageModel.input`，组件不新增推断。

### D8 视觉细节

- 状态胶囊补 `● / ⏸ / ✕ / ○` 字形；`waiting_input` 使用警告色。
- 代码卡按行拆分并渲染行号；复制仍复制原始代码，不带行号。
- 右栏差异行按 `kind` 加 `+` / `-` / 空格前缀，并保留 hunk 头；右栏清单变紧凑行，选中后显示详情。
- Agent 胶囊展开层补标题“本会话涉及的 Agent”和运行中圆点。
- 模型/模式弹层补“由 Agent 提供，下一轮生效”的说明。

### D9 权威文档口径

- 原型优先：更新 `pwa-web-client` 与 `docs/FRONTEND_DESIGN.md`，把对话页的内容来源提示允许按连接状态换文案、左栏紧凑行、输入区只读与中栏滚动写成新口径。
- 设备胶囊仍是唯一状态徽标；内容来源提示不得长成第二个徽标，这条边界保留。
- `docs/SYNC_PROTOCOL.md` §10.2/§14 补 `toolDetail` 字段、部分上限与截断语义；schema 与 fixtures 同步更新，`npm run check` 作为门禁。

## Risks / Trade-offs

- **协议资产漂移**：新增视图字段必须同时改 Rust 类型、JSON Schema、fixtures 与文档；漏一处会被 `npm run check` 拦下。缓解：合同工作包先合，再合 UI。
- **工具详情体积**：大输出可能挤占单条消息预算。缓解：按部分上限、整体上限与截断标记；不发送被截断的 JSON。
- **固定高度外壳的回归**：`contentFill` 只对对话路由生效，目录页不回退。缓解：外壳模型与三页 E2E 都断言滚动不变。
- **颜色语义**：内容来源条按状态换色可能与设备胶囊的档位被读成两个状态源。缓解：文案用说明句、形状用条状提示而非胶囊，并在 spec 中写死“不得呈现第二个状态徽标”。
- **原型与 spec 的后续漂移**：本次把原型口径写回文档；后续原型再改时需要同步更新，避免文档再次落后。

## Migration Plan

- `toolDetail` 是可选视图字段，旧客户端忽略它，新客户端在字段缺席时回退到现有 title/state；无需数据迁移。
- `truncatedParts` 与 `toolDetail` 同批上线；旧 fixtures 不需要改写，只新增覆盖三部分的 fixture。
- 回滚 UI 时只需回退客户端与文档；服务端多下发的可选字段对旧客户端无害。若需回滚协议字段，先回退 schema/fixtures 与映射，再回退客户端。

## Open Questions

（无。部分上限取 32 KiB / 整体 64 KiB，与现有差异行上限同一量级；若后续实测不足，再按新变更调整。）
