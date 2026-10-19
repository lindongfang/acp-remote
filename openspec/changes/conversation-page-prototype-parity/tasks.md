# Tasks

## 1. 合同投影（WP1）

- [x] 1.1 在 `crates/sync-protocol/src/views.rs` 为 `tool.call.started/updated/completed` 增加可选 `toolDetail`（`input` / `output` / `text` / `truncatedParts`）及类型定义。（完成判据：三个 view 类型可序列化，未知字段仍由 `ExtraFields` 透传，`cargo test -p sync-protocol` 通过）
- [x] 1.2 在 `crates/agent-host/src/mapper.rs` 从 ACP `rawInput` / `rawOutput` / 文本 content 构造 `toolDetail`；只映射文本块，diff 与 terminal 不进入；按 32 KiB/部分、64 KiB/整体截断，并拒绝无法通过 ACPR-CJ1 值域的值，登记 `truncatedParts`。（完成判据：mapper 单测覆盖有输入、有输出、超限、无详情、未知 content、非整数浮点与大整数六种输入）
- [x] 1.3 在 `schemas/sync/v1/event-views.schema.json` 为三个 `tool.call.*` 定义增加 `toolDetail` 属性与 `truncatedParts` 枚举。（完成判据：schema 校验三个新 fixture 通过、旧 fixture 不因新增可选字段失败）
- [x] 1.4 更新 `fixtures/sync/v1/valid/view-tool-call-*.json`（至少一条带 `input`、一条带 `output`、一条带 `truncatedParts`），必要时更新 `fixtures/sync/v1/manifest.json`。（完成判据：fixture 与 schema 一一对应，`node scripts/check-schema-fixtures.mjs` 通过）
- [x] 1.5 更新 `docs/SYNC_PROTOCOL.md` §10.2 的字段表与 §14 上限表，写明 `toolDetail` 各部分的语义、截断与“不发送被截断 JSON”。（完成判据：§10.2/§14 均出现新字段与 32 KiB/64 KiB 取值，文档链接检查通过）
- [x] 1.6 运行 `npm run check` 与 `cargo test -p agent-host -p sync-protocol`。（完成判据：两条命令全绿，无合同漂移告警）

## 2. 客户端模型（WP2）

- [x] 2.1 在 `clients/app/src/protocol/` 增加 `toolDetail` 的可选类型与严格解析，保持未知字段不丢失。（完成判据：protocol 单测覆盖缺字段、完整字段与未知字段）
- [x] 2.2 在 `clients/app/src/domain/timeline.ts` 扩展 `ToolCallTimelineCard`：`detail`（input/output/text/truncatedParts）与最新记录 `eventId`；从 `StructuredRecord.view` 读取，不解析 `acp.rawJson`。（完成判据：domain 单测覆盖三部分读取与缺字段）
- [x] 2.3 在 `clients/app/src/features/conversation-timeline.ts` 归并同一 `toolCallId` 的记录时为各部分组成取最新非缺席值、`truncatedParts` 取并集，并把原文/事件标识留给诊断入口。（完成判据：单测构造 started(输入)+updated(输出) 断言两部分都在；构造两次截断断言并集）
- [x] 2.4 更新 `clients/app/src/features/conversation-timeline.test.ts` 与 `clients/app/src/domain/domain.test.ts`。（完成判据：新增用例覆盖跨事件合并、截断并集、无详情不产生空字段；`npx vitest run src/domain src/features/conversation-timeline.test.ts` 通过）
- [x] 2.5 运行 `cd clients/app && npm run typecheck`。（完成判据：类型检查通过，无新增 `any` 或宿主类型逃逸）

## 3. 外框与左栏（WP3）

- [x] 3.1 在 `clients/app/src/components/shell/shell-model.ts` 增加仅对话路由为真的 `contentFill`；在 `ShellView.tsx` 按该值给外壳根与内容区设置视口高度/`minHeight:0`/隐藏溢出。（完成判据：`shell-view.test.ts` 断言 `/session/x` 为真、目录页为假，且 `data-*` 可观察）
- [x] 3.2 重排 `ConversationStream.tsx`：会话头 → 内容来源条 → 三栏（左/中/右）→ 输入区放入中栏；三栏各自使用 `ScrollView` 独立滚动；次级视图切换仍留在三栏之外。（完成判据：组件测试断言输入区位于 `conversationColumn=timeline` 内、三栏各有滚动容器）
- [x] 3.3 左栏改为紧凑会话行：状态圆点 + 标题 + 状态词 + 相对时间，不显示 Agent 名/路径/行分隔线；目录页继续使用五字段行。（完成判据：`conversation-page.test.ts` 断言左栏行不含 Agent 名，目录页行仍含 Agent 名）
- [x] 3.4 左栏头部补「路径」标签、搜索图标、「‹ 目录」返回目录列表与「全部 ›」进入详情；pager 文案改为「已显示 N / M」。（完成判据：组件测试断言两个入口分别触发 `onOpenDirectoryList` / `onOpenDirectory`，文案逐字匹配）
- [x] 3.5 会话头改为单行，状态胶囊补 `● / ⏸ / ✕ / ○` 字形，`waiting_input` 使用警告色；Agent 胶囊靠右。（完成判据：`StatusPill` 与 `conversation-page.test.ts` 断言字形、色调类别与单行结构）
- [x] 3.6 更新 `conversation-page.test.ts`、`conversation-responsive.test.ts`、`shell-view.test.ts` 与 `app/session/[id].tsx` 的接线。（完成判据：新增用例覆盖返回目录、紧凑行、单行头、`contentFill`；`npx vitest run ...` 通过）

## 4. 中栏内容、输入区与右栏（WP4）

- [x] 4.1 `ConversationTimeline.tsx` 的工具折叠块渲染 `detail.input` / `detail.output` / `detail.text`；`truncatedParts` 非空时显示对应截断说明；无 `detail` 时不渲染空区块。（完成判据：`conversation-timeline-view.test.ts` 断言三部分文本、截断说明与无详情时无空占位）
- [x] 4.2 工具折叠块增加“查看 ACP 原文”按钮，调用既有 `onOpenDiagnostics(eventId)`；原文仍只在诊断抽屉按不可信文本呈现。（完成判据：组件测试断言按钮触发回调且渲染树里不出现解析后的结构化补造内容）
- [x] 4.3 `ConversationComposer.tsx` / `conversation-input-model.ts` 落实三态：进行中只读且只呈现「停止」，可输入呈现发送，已关闭说明原因；提示行说明进行中按钮变化。（完成判据：`conversation-composer.test.ts` 与 `conversation-submission.test.ts` 断言进行中无发送、只读、回车不发命令）
- [x] 4.4 `ConversationSettingsPanel.tsx` 的模型/模式弹层补“由 Agent 提供，下一轮生效”的说明。（完成判据：`conversation-settings-view.test.ts` 断言两处说明文案存在）
- [x] 4.5 `CodeBlock.tsx` 渲染行号，复制内容不含行号；`FileChangesPanel.tsx` 改为紧凑清单 + 选中详情，差异行按 kind 加 `+` / `-` / 空格前缀；当前 `DiffLineKind` 没有 hunk 类型，因此不伪造 hunk 头，模型提供时再保留。（完成判据：`codeblock.test.ts` 与 `file-changes-panel.test.ts` 断言行号、复制文本与单前缀；e2e 断言不出现双前缀）
- [x] 4.6 运行 `cd clients/app && npm run typecheck && npx vitest run`（相关文件）。（完成判据：本组测试全绿，既有 `data-*` 断言未被删改）

## 5. 浏览器 E2E（WP5）

- [x] 5.1 更新 `clients/app/e2e/page/checks/conversation-content.ts` 或新增检查：宽屏三栏可见、输入区在中栏、滚动时间线时左右栏不动。（完成判据：检查在真实页面断言 `conversationColumn` 与输入区父子关系、滚动容器 `scrollTop`）
- [x] 5.2 增加左栏检查：紧凑行字段集合、`‹ 目录` 到目录列表、`全部 ›` 到目录详情。（完成判据：真实点击后路由与页面标题可达，行内文本不含 Agent 名）
- [x] 5.3 增加输入区检查：进行中输入框 `readOnly`、无发送按钮、有停止按钮、回车不发命令。（完成判据：断言 DOM 属性与假主机命令计数不变）
- [x] 5.4 增加工具详情检查：输入/输出可见、截断说明可见、原文按钮打开诊断抽屉。（完成判据：真实事件帧投递后展开折叠块，断言文本与抽屉内容）
- [x] 5.5 增加右栏/代码检查：差异单前缀（不出现双前缀）、代码行号。（完成判据：真实 `file.changed` 与 Markdown 代码块渲染后断言；hunk 头因模型无 hunk 类型不伪造）
- [x] 5.6 在 `clients/app/e2e/page/index.tsx` 注册新检查集，运行 `npm run typecheck:e2e && node e2e/run-e2e.mjs`。（完成判据：全部检查通过，新增用例在输出中可见）

## 6. 文档写回（WP6）

- [x] 6.1 更新 `docs/FRONTEND_DESIGN.md` §4.1：对话页三栏独立滚动、输入区在中栏、内容来源提示位置与连接态文案、左栏紧凑行与两个目录入口、进行中只读、工具详情与右栏差异前缀。（完成判据：文档段落与最终实现逐条对应，无未落地措辞）
- [x] 6.2 更新 `prototypes/IMPLEMENTATION-GAPS.md`：把对话页从“已对齐”改为本次落地的事实，登记仍保留的有意偏离（如有）。（完成判据：不再声称与原型存在本次已修复的差异）
- [x] 6.3 运行 `npm run check:docs` 与 `npm run check:openspec`。（完成判据：两条命令通过）
