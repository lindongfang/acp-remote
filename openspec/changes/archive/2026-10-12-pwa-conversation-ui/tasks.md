# Tasks

## 1. 协议合同（WP1）

- [x] 1.1 在 `schemas/sync/v1/sync.schema.json` 新增历史条目定义，在 `schemas/sync/v1/command.schema.json` 的 `include` 枚举加 `history`、请求加 `beforeHistory`、结果加 `history` 与 `historyHasEarlier`；完成判据：`node scripts/check-schema-fixtures.mjs` 通过，且新定义被 `sessionReadResult` 正确引用
- [x] 1.2 同步 Rust 镜像 `crates/sync-protocol/src/{sync,command,views}.rs`；完成判据：主检出 `cargo test -p sync-protocol` 的 `schema_drift` 双向门禁通过
- [x] 1.3 在 `fixtures/sync/v1/` 新增一条正例（含历史条目与可缺席原文）与一条负例（快照 chunk 携带历史明细资源，必须被拒），并登记 `fixtures/sync/v1/manifest.json`；完成判据：`npm run check:schemas` 通过，负例的 `expectedKeyword` 指向真正拦下它的约束
- [x] 1.4 登记 feature `core.session-history.v1` 于 `compatibility/features/v1/features.json`，并同步 `docs/SYNC_PROTOCOL.md` §5.2 feature 表、§9.4 明细资源集合（五类→六类）、§11.5 的 `session.read` 行；完成判据：`npm run check:features` 与 `npm run check:docs` 通过
- [x] 1.5 在 `docs/SYNC_PROTOCOL.md` 写明进入历史条目的 17 类事件与不进入的 17 类、以及「已按保留策略压缩的事件不产生条目（因此思考内容不承诺出现在历史里）」；完成判据：文档集合与 schema/`views.rs` 的登记逐条一致，且 `npm run check:docs` 通过

## 2. core 与 storage 读路径（WP2）

- [x] 2.1 端口层：`HistoryInclude` 增加历史开关，会话历史读的元素携带持久化正文（复用既有值对象，不新增类型）；完成判据：`npm run check:drift` 通过（`docs/CORE_PORTS_AND_STORAGE.md` §5 的 rust 块已同步）
- [x] 2.2 storage 实现：一次查询取行时一并取视图与 ACP 两列，按会话内序号分页并给出页标记，跳过已压缩行；完成判据：新增单测覆盖「不重不漏」「同一游标重复读结果相同」「已压缩行不出现」「原文不可用时带已登记原因」，且断言只查库一次
- [x] 2.3 用例层与 fake 读视图随端口形状更新；完成判据：`cargo test -p core -p storage-sqlite` 通过，且检视确认没有为迁就形状而放宽类型或跳过断言
- [x] 2.4 若实现期确认需要新索引，走既有版本化 migration 并同步 §7 SQL 块；若无需要，在 `docs/CORE_PORTS_AND_STORAGE.md` 说明「本次不改 §7」的理由；完成判据：`npm run check:drift` 与 `cargo test -p storage-sqlite` 通过

## 3. 客户端协议与状态层（WP3）

- [x] 3.1 `clients/app/src/protocol/**` 镜像新资源与分页字段；完成判据：`clients/app` 的 `src/protocol/contract.test.ts` 通过（逐条从 schema 读 `required`/`enum` 比对，不复制常量）
- [x] 3.2 呈现覆盖集合与判定：与协议类型词表彻底分开，未命中即产出降级卡片；完成判据：新增单测证明「协议已知但覆盖未命中」的抽样类型产出降级卡片，且覆盖命中时不再产生降级卡片
- [x] 3.3 时间线模型：消息与结构化条目合并、按标识去重、历史与实时走同一渲染入口；完成判据：单测覆盖「同一标识实时与历史并存时只呈现一处」
- [x] 3.4 实时尾巴：按消息标识累加并在读取结果到达后替换，压缩事件到达时作废；完成判据：单测覆盖「先增量后读取」序列不产生重复或残留
- [x] 3.5 输入、授权与设置的视图模型：只对亲历请求可应答、历史待处理项不可操作、关闭会话不可提交、来源离线时草稿只在内存；完成判据：单测逐一覆盖上述四条并断言「未提交」标记不被改写为已发送
- [x] 3.6 `session.origin.online_changed` 归并：只改可达性标注，不动摘要其余字段；完成判据：单测证明标注随事件更新，且其余摘要字段逐字段不变

## 4. 依赖与富文本渲染（WP4）

- [x] 4.1 可用性探针：安装候选 Markdown 与高亮依赖后跑 `npm ci`、`npx tsc --noEmit`、`npx expo export --platform web`；完成判据：报告给出所选版本、peer 警告原文与三条命令结果；不通过则按 design D9 依次降级并把最终选择写回 `design.md` D9
- [x] 4.2 新增 `clients/app/src/components/Markdown.tsx` 与 `CodeBlock.tsx`：渲染成 RN 视图、可关闭原始 HTML、不引用任何远端资源、链接与图片按安全口径降级、代码块可复制；完成判据：组件测试断言原始 HTML 与脚本不产生元素、渲染输出里不含远端地址
- [x] 4.3 两者样式走既有 `theme.ts` token；完成判据：组件测试在浅/深两套调色板下渲染并比对属性集合，且渲染路径无硬编码色值

## 5. 对话页呈现层（WP5）

- [x] 5.1 时间线视图：消息与结构化条目在同一容器按时间呈现，各类卡片（思考、工具调用、终端输出、计划、权限、文件改动、降级）各有唯一 `data-*` 锚点；完成判据：组件测试断言同容器、锚点唯一，且每个锚点在渲染输出里恰好出现一次
- [x] 5.2 输入区（仅文本、附件不支持说明、可输入/进行中/已关闭三态）、授权卡（亲历可应答、历史不可操作、已被其它设备处理）、模型与模式（切换中、下一轮生效、未提供）；完成判据：组件测试逐状态断言属性与实际呈现的按钮集合
- [x] 5.3 变更文件右栏：行数统计、相对化展示路径、诊断入口，且**不含**差异正文；完成判据：组件测试断言输出里没有差异行元素、含诊断入口，且未解析 ACP 原文
- [x] 5.4 宽窄两形态：沿用既有断点，窄屏把右栏降为可切换次级视图且信息集合相同；完成判据：两形态渲染的属性集合比对单测通过（集合相等）
- [x] 5.5 路由接线 `clients/app/app/session/[id].tsx`：页面只取模型、渲染组件、把交互上抛，无业务分支；完成判据：借用主检出 `node_modules` 的 `renderToStaticMarkup` 给出页面级渲染输出，且页面文件内不含判定逻辑
- [x] 5.6 主机/连接面呈现**节点级**降级条目（无 `sessionId` 的事件，如 `device.revoked`）：渲染 `HostPanelModel.nodeDegraded`，每条带唯一 `data-*` 锚点并可按 `eventId` 打开既有诊断抽屉；完成判据：组件测试断言节点级条目出现在渲染输出里（且不混进会话时间线的降级列表），并可经 `openDiagnostics(eventId)` 打开——该面是「呈现覆盖由客户端自持」对节点级事件的承载者（WP3 的检视 finding：状态层已可达，缺的是呈现接线）

## 6. 浏览器用例（WP6）

- [x] 6.1 新增用例覆盖 `plan.md` 的 Main E2E 九条可观察行为（时间线同源、重开仍见历史条目、输入与终态、授权应答边界、模型与模式、右栏无差异正文、来源离线降级、未覆盖事件产出降级卡片、富文本不可信）；完成判据：`node e2e/run-e2e.mjs` 全绿，且每条断言在报告里给出所用锚点
- [x] 6.2 既有用例回归适配（含 `pwa-directory-ui-followup` 合入后的目录页与降级用例）；完成判据：既有用例全部仍为 PASS，没有删除或弱化任何断言
- [x] 6.3 判别力证据：逐条说明新用例在对应实现回退时会红的机制（临时回退验证或给出机制论证）；完成判据：报告逐条给出证据，检视方能据此独立复核

## 7. 权威文档（WP7）

- [x] 7.1 更新 `docs/FRONTEND_DESIGN.md`：§4.1 补对话页口径（时间线与结构化卡片、输入与取消、授权应答、模型与模式、右栏边界、宽窄两形态），§6 明确「呈现覆盖由客户端自持」；完成判据：`npm run check:docs` 通过，且报告给出改动前后段落
- [x] 7.2 在 `docs/NODE_LINK_PROTOCOL.md` 登记「Node Link 面无结构化历史」这一已知缺口，并同步 `prototypes/IMPLEMENTATION-GAPS.md` 的对照表；完成判据：措辞明确写成尚未实现（不得写成已有能力），检视确认与 specs 措辞一致

## Workflow follow-up

- 按 `docs/WORKFLOW.md` §6 合入与远端落地：为这一批 patch 开分支、推远端、`gh pr create`、等六个必需检查全绿后 `gh pr merge --squash`；提交信息按 `commitlint.config.mjs` 手写（scope 是模块边界，不是目录名）。
- 合入后归档变更并同步增量 specs 到 `openspec/specs/`，确认 `npm run check` 全绿。
