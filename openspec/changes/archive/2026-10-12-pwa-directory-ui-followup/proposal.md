<!-- 记录动机、范围、能力与影响；行为写 specs，方案写 design，协作写 plan。 -->

## Why

变更 `pwa-directory-ui`（已合入 `main`：`6a11291`，归档于 `2026-10-12-pwa-directory-ui`）把目录页与目录详情页做成了可用界面，但**留下一条功能死路**，而且上一轮"这两页完成了吗"的回答漏了它——这里先更正：

**在一台新机器上（电脑已登记 workspace、但一条会话都还没有），用户无法创建第一个会话。**

- `src/domain/directory.ts` 的 `buildDirectoryViews` 在"没有任何目录有会话"时返回 `{ kind: "empty", empty: "directories_without_sessions" }`，`DirectoryList` 在这个分支**只渲染空态**、不渲染任何目录卡片；
- 而进入目录详情页的唯一入口是卡片里的「查看全部 N 个」，它在 `counts.total === 0` 时为 `null`（`DirectoryList.tsx` 的 `onOpenAll`）；
- 结果：空态文案说"在下面选一个目录进入详情页，然后在目录内创建一个会话"，**但下面没有任何东西**——指引指向一个不存在的列表，详情页（以及它唯一的创建入口）不可达。

同一条死路在**部分为空**时也存在：只要有别的目录有会话，0 会话的目录会作为卡片出现，但那张卡片的标题不可点、「查看全部」为 `null`，因此同样进不去。现有浏览器用例把这条行为**锁住**了（R15-11 断言"0 条会话的目录项内不呈现查看全部入口"，只做对照、没有要求存在别的入口）。

除了这条死路，另有原型有、实现没有的两处呈现（`prototypes/acp-remote-pwa.html` 的 `viewDirs`）：目录卡片头部的**「最近 <活动时间>」**（`dwhen`）与卡片上的**「⊕ 新建」**快捷入口。`docs/FRONTEND_DESIGN.md` §9 第 1 条把该原型定为这两页的交互口径来源，因此这两项属于口径差距。

最后是验证口径的三处空白：①「点击查看全部进入详情页」与「返回目录页后查询词仍在」两个 spec 场景**没有浏览器用例**——`clients/app/e2e/page/router-stub.tsx` 的 `Stack` 直接调用 `current.component()`，路由组件的 hooks 落在 `Stack` 自己的 fiber 上，跨路由切换组件类型时 hooks 槽位错配并抛错（既有测试替身缺陷，非产品缺陷）；②目录页的"无创建会话权限"提示条只有组件级断言；③`composition-root.ts`、`r15-directory.ts`、`r16-create.ts`、`r18-degradation.ts` 里仍有 6 处依赖 RNW 标签映射的选择器（`li[…]`、`h2`、`p`、`pre[…]`）。

**用户要求**：先补齐目录页与目录详情页与原型之间的差距，并把上面两个 spec 场景变成真正的浏览器用例。

**硬约束**：

- 不改协议 wire、`schemas/`、`fixtures/`、任何 Rust crate、`server::sync`。
- 不新增第三方依赖；样式仍用既有 `src/components/theme.ts`（RNW 原语 + token）。
- 既有 `data-*` 属性名与取值，除本变更**显式声明**的命名调整外，一律保留；`[data-directory-alias]` 仍必须每个目录项恰好出现一次。
- 两页的判定仍全部落在 `src/features/` 的纯函数里，页面没有业务分支。

**成功判据**：新机器（已登记目录、零会话）上能从目录页进入任一目录并创建第一个会话；0 会话的目录卡片在"部分为空"时同样可达；目录卡片呈现最近活动时间与目录级创建入口；「查看全部→详情页」「返回目录页查询词仍在」「无权限提示条」三条有真浏览器断言；`clients/app` 的 `npm run check` 与 `node e2e/run-e2e.mjs` 全绿。

## What Changes

**一、修掉"进不去、建不起来"的死路（本变更的主项）**

- `DirectoryPageModel` 的 `empty` 分支带上已登记的目录（`no_directories` 时为空数组），`DirectoryList` 在两种空态下都**照常呈现空态标记与文案**，并在有目录时**同时列出目录卡片**。这样空态指引"在下面选一个目录"第一次成为事实。
- 目录卡片的**标题本身成为进入详情页的入口**（无论该目录有多少会话），不再只依赖「查看全部 N 个」。
- 于是 0 会话的目录在两种情形下都可达，详情页的创建入口也随之可达。

**二、补齐原型的两个呈现项**

- 目录卡片呈现**最近活动时间**（该目录下会话 `updatedAt` 的最大值；无会话时不呈现），格式与同步时刻一致（UTC 确定性切片，可逐字断言）。
- 目录卡片呈现**「⊕ 新建」**：受 `session.create` scope 门控，点击后在该目录打开创建弹层——复用既有 `CreateSessionSheet` 与同一条 `session.create` 命令，不新增命令、不新增载荷字段。

**三、把验证缺口补上**

- 修 `e2e/page/router-stub.tsx` 的跨路由渲染（按路由挂载组件、切换时重挂），使「点击查看全部进入详情页」与「返回目录页后查询词仍在」成为真浏览器用例。
- 新增目录页"无创建权限提示条"的浏览器用例；因本变更的行为改动更新被影响的既有断言（如 R15-11 的"0 会话目录项无入口"）。
- 把剩余 6 处依赖标签映射的选择器去标签化；`EmptyState` 与 `CreateSessionSheet` 的标题/指引补显式 `data-*` 锚点（只加属性，不改文案与契约）。

**四、规格与文档**

- `openspec/specs/pwa-web-client/spec.md` 新增 4 条 Requirement：无会话时仍列出目录并提供进入详情页的入口；目录项提供进入详情页的入口（与会话数无关）；目录页呈现最近活动时间；目录项提供目录内创建入口。
- `docs/FRONTEND_DESIGN.md` §4.1 补上面四项的呈现口径（§9.1 已把原型定为口径来源，这里把偏离与新增写实）；`prototypes/IMPLEMENTATION-GAPS.md` 的对照表同步一行。

**本次不包含**：

- 对话页、配对页、`app/hosts.tsx`、`DiagnosticsDrawer`、`ConversationStream` 的样式迁移与交互。
- 顶栏 / hostchip / 开发模式横幅 / 主题开关（全局壳）。
- PWA manifest、service worker、图标、i18n。
- 新依赖；`client-store` 的连接/命令语义；协议与合同资产（`schemas/`、`fixtures/`、`compatibility/`）。
- 原型的路径行（`docs/FRONTEND_DESIGN.md` §2.2/§4.1 与 `SECURITY_DESIGN.md` §12.3 明令禁止，现有负向断言盯着它）。
- `server::sync` 与 `/ui` 静态托管（前端连真机仍缺它，但不是本变更的事）。

## Capabilities

### New Capabilities

无。本次改动落在既有 `pwa-web-client` 能力内（目录页与目录详情页的呈现与可达性）。

### Modified Capabilities

- `pwa-web-client`: 新增 4 条 Requirement——「无会话时仍列出目录并可进入详情」「目录项提供进入详情页的入口且与会话数无关」「目录页呈现最近活动时间」「目录项提供目录内创建入口」。

## Impact

- **前端代码**：`clients/app/src/features/directory-model.ts`（empty 分支带目录、最近活动时间）、`clients/app/src/components/DirectoryCard.tsx`（标题入口、最近活动、新建按钮）、`DirectoryList.tsx`（empty 分支渲染列表、接线新回调）、`EmptyState.tsx` 与 `CreateSessionSheet.tsx`（补 `data-*` 锚点）、`clients/app/app/index.tsx`（挂载创建弹层与 `sheetOpen` 状态、传目录级新建回调）。
- **测试**：`clients/app/src/**` 下的模型/组件单测；`clients/app/e2e/page/router-stub.tsx`（跨路由渲染修复）、`e2e/page/checks/{r15-directory,r15-directory-detail,r16-create,r18-degradation,composition-root}.ts` 的选择器与断言更新、新增用例。
- **依赖**：不新增任何依赖。
- **合同资产**：`schemas/`、`fixtures/`、`compatibility/` 一行不改。
- **权威文档**：`docs/FRONTEND_DESIGN.md` §4.1（新增四项的呈现口径）。`docs/MODULE_ARCHITECTURE.md`、`docs/SYNC_PROTOCOL.md`、`docs/SECURITY_DESIGN.md`、`docs/CORE_PORTS_AND_STORAGE.md` 无需改动。
- **风险**：`router-stub` 的修复需要真实调查（上一轮实测里"直接改成 `createElement`"会让详情路由停在 `RuntimeUnavailable`，原因未查明）；若确实无法安全修复，本变更**不**声称覆盖那两条场景，须在检视时显式登记为未覆盖项。另外目录卡片的「查看全部」与标题都会进入同一个页面，需要明确两者的动作属性命名，避免 e2e 出现"同值双入口"的歧义。
