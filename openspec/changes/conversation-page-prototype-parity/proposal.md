# Proposal

## Why

对话页的数据语义已经对齐，但视觉与交互骨架仍明显落后于原型 `prototypes/acp-remote-pwa.html`：输入区横跨三栏底部而不是住在中栏、左栏复用目录页的五字段会话行、会话头两行且标题过大、进行中仍可输入并同时显示发送与停止、工具调用折叠块没有参数/输出、右栏每项一张完整卡片。原型是产品呈现的参照，这些差异让对话页看起来不像同一套产品，也让“进行中不能提交”等状态口径出现两种表达。

同时，工具调用的 `rawInput` / `rawOutput` / 文本内容块只存在于 ACP 原文里，同步投影后的 `tool.call.*` view 没有这些字段，客户端无法在不解析 `acp.rawJson` 的前提下按原型展示工具详情。本次一并把投影补齐，避免前端变成第二个 ACP 解码器。

## What Changes

### 合同投影

- `tool.call.started` / `tool.call.updated` / `tool.call.completed` 的公开 view 增加工具详情字段：`rawInput`、`rawOutput` 与文本类 `content` 块；每项带字节上限，超出时以截断标记说明，不静默丢弃。
- diff 与终端内容仍走现有独立卡片，不在工具详情里重复。
- `acp.rawJson` 的往返保真不变；“查看 ACP 原文”入口复用现有原文诊断，不由客户端解析原文生成结构化详情。
- 同步 JSON Schema、fixtures、Rust view 类型、agent-host 映射与客户端协议类型同步更新。

### 对话页呈现与交互

- **宽屏三栏**：左栏（目录上下文 + 同目录会话）、中栏（会话头、连接/内容来源条、时间线、输入区）、右栏（变更文件）；三栏各自滚动，输入区吸在中栏底部。窄屏继续用次级视图切换，两形态信息集合相同。
- **左栏**：紧凑会话行（状态圆点 + 标题 + 状态词 + 相对时间，不再显示 Agent 名与行分隔线）；增加“‹ 目录”返回目录列表，同时保留“全部 ›”进入目录详情；路径行带“路径”标签；搜索框带图标；pager 文案改为“已显示 N / M”。
- **会话头**：单行布局，标题、状态胶囊、Agent 名、弹性空白、Agent 胶囊；状态胶囊补回 `● / ⏸ / ✕ / ○` 字形并按原型配色（等待输入使用警告色）。
- **连接与内容来源条**：合并为会话头下方的一条，按连接状态换文案与色调；非在线时给出真实的上次同步时刻。设备胶囊仍是顶栏唯一的连接状态徽标，本条的语义是内容来源提示。相应修改 `pwa-web-client` 的规范。
- **时间线**：工具调用折叠块展示参数、输出与文本内容块，并提供“查看 ACP 原文”；代码卡补行号；保留既有默认展开、显式覆盖与文件联动规则。
- **输入区**：进行中只读且只呈现“停止”；可输入、进行中、已关闭三态的说明与原型一致；模型/模式弹层补“下一轮生效”说明。
- **右栏**：紧凑变更清单 + 选中项详情；差异行补 `+` / `-` / 空格前缀与 hunk 头。
- **文档**：更新 `docs/FRONTEND_DESIGN.md`、`openspec/specs/pwa-web-client/spec.md`、`docs/SYNC_PROTOCOL.md`（工具详情字段）、`prototypes/IMPLEMENTATION-GAPS.md` 与相关合同资产。

## Capabilities

### New Capabilities

（无）

### Modified Capabilities

- `sync-event-projection`: `tool.call.*` 事件视图新增工具详情字段、上限与截断语义。
- `pwa-web-client`: 对话页的三栏布局、左栏会话行与目录入口、会话头、连接/内容来源条、输入区状态、工具详情与右栏差异呈现。

## Impact

- Rust：`crates/agent-host/src/mapper.rs`、`crates/sync-protocol/src/views.rs`。
- 合同资产：`schemas/sync/v1/event-views.schema.json`、`fixtures/sync/v1/**`、`compatibility/**`（如涉及封闭词表）、`npm run check` 合同漂移门禁。
- 客户端：`clients/app/src/protocol/**`、`clients/app/src/domain/timeline.ts`、`clients/app/src/features/conversation-model.ts`、`clients/app/src/components/**`、`clients/app/app/session/[id].tsx`。
- 文档：`docs/SYNC_PROTOCOL.md`、`docs/FRONTEND_DESIGN.md`、`openspec/specs/pwa-web-client/spec.md`、`prototypes/IMPLEMENTATION-GAPS.md`。
- 测试：Rust view/fixture 测试、客户端 domain/feature/component 测试、`clients/app/e2e/page/checks/` 的对话页浏览器检查。

## Non-Goals

- 不实现移动原型的导航形态（底部 tab、`☰` push 页、底部 sheet、56px 触达）。
- 不实现 PWA 安装、service worker、i18n。
- 不改变 ACP wire 语义、不解析 `acp.rawJson` 生成结构化详情、不把 diff/终端内容重复塞进工具详情。
- 不新增 Node Link 能力，不改目录页/目录详情页的既有呈现规则。
- 不实现原型的演示数据（固定“2 分钟前”、固定 Agent 列表）；所有随时间与环境变化的值来自真实状态。

## Success Criteria

- 对话页在宽屏呈现三栏且输入区位于中栏底部；窄屏以「对话 / 会话 / 变更文件」三种次级视图呈现，信息集合与宽屏相同（「全部」只属宽屏，避免窄屏三栏被均分挤压）。
- 上游八个决策（工具详情走投影、连接条合并、单变更打包、真实值、紧凑左栏行、进行中只读、视觉细节全选、合同先合）全部落地，无静默缩减。
- 新增工具详情字段通过 `npm run check` 的 schema/fixture/合同漂移校验；Rust 与客户端测试覆盖截断、缺失与未知字段保真。
- 真实浏览器 E2E 断言对话页的关键结构、进行中只读、工具详情与差异前缀。
