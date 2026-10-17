# Tasks

## 1. core 与 storage（WP1）

- [x] 1.1 新增目录值对象与 id 派生（`directoryId = hex(sha256(canonical_path)[..16])`），并写「同一目录的大小写差异与符号链接/ junction 写法得到同一个 id」用例；判据：`cargo test -p core` 中该用例通过。
- [x] 1.2 把「解析目录来源」从别名改为目录 id：id 必须来自本节点已登记目录，解析结果仍是规范化绝对路径；判据：`cargo test -p core` 中「未登记 id 报参数类错误」「已登记但路径失败报不可用」两条用例通过。
- [x] 1.3 会话创建成功路径上与目录 upsert 同事务提交（`INSERT ... ON CONFLICT DO NOTHING`），失败/不确定不登记；判据：注入后端失败的用例断言目录表无新增行。
- [x] 1.4 storage 迁移 v8→v9：新建 `owned_directory`、把 `owned_workspace` 与既有会话归属回填、`workspace_alias` 重命名为 `workspace_directory_id`；判据：`cargo test -p storage-sqlite` 中「v8 库升级后历史会话获得 directory_id」「升级失败整体回滚」「连续两次打开 schema 文本相同」三条用例通过。
- [x] 1.5 迁移用例覆盖「原先未分组但 `workspace_cwd` 非空」的历史会话被归组；判据：该用例断言其 `workspace_directory_id` 非空且等于由路径派生的 id。

## 2. 协议与入站面（WP2）

- [x] 2.1 快照 `workspaces` 条目改为 `{ directoryId, path }`，会话摘要 `workspace` 改为 `{ directoryId } | null`；判据：`cargo test -p server` 中快照投影用例逐字段断言新形状且不含 `alias`/`displayName`。
- [x] 2.2 `session.create` 载荷：`directoryId` 与 `workspacePath` 二选一，`createIfMissing` 与 `directoryId` 互斥；判据：`cargo test -p server` 中三条拒绝用例（两者同给、未知键、id 不存在）通过且零副作用。
- [x] 2.3 Node Link `session.create` 改用 `directoryId`，仍拒绝任何路径字段与未登记 id；判据：`cargo test -p server` 中「路径字段被拒」「未登记 id 被拒」用例通过。
- [x] 2.4 删除 `local.workspace.select` 方法与错误映射；判据：`cargo test -p server` 中该方法返回 `local.unsupported` 的用例通过。
- [x] 2.5 合同资产对齐：`schemas/sync/v1`（`workspaceCatalogEntry`/`workspaceRef`/`session.create`）、`schemas/node-link/v1`、`compatibility/features/v1/features.json` 撤销 `core.workspace-path.v1`、`fixtures/**/v1/**`；判据：`npm run check` 中 schema/fixture/feature 三项通过。

## 3. CLI（WP3）

- [x] 3.1 删除 `workspace` 子命令组与参数校验，保留其它命令不变；判据：`cargo test -p app` 中「`workspace` 不再被接受（未知子命令退出码非零）」用例通过，且 `acp-remote --help` 输出无 `workspace`。
- [x] 3.2 核对删除后不残留引用（help 文案、映射表常量、错误分支）；判据：`grep -rn "workspace select" crates/app` 无命中，`cargo clippy -p app --all-targets -- -D warnings` 通过。

## 4. 前端（WP4）

- [x] 4.1 目录页/详情页/左栏改用 `directoryId` + `path`：展示名取路径尾段、同名用更多段消歧、路由 `/dir/<directoryId>`；判据：`cd clients/app && npm run check` 中视图模型用例与 `routing` 相关用例通过。
- [x] 4.2 「目录已不存在」态：目录项与详情页可区分呈现，历史会话可打开，新建入口禁用并给出原因；判据：新增组件用例断言该态的三个可观察点。
- [x] 4.3 空态文案更新为「还没有任何目录被会话使用过」，且不再提示去电脑端登记；判据：对应用例断言文案不含「登记」引导。
- [x] 4.4 创建弹层的目录块只呈现 `path`（不再有别名/展示名概念），浏览选定后提交 `workspacePath`；判据：`npm run check` 全绿。

## 5. 文档（WP5）

- [x] 5.1 `docs/SYNC_PROTOCOL.md`：§9.4 条目形状、会话摘要引用、`session.create` 载荷、feature 表撤销 `core.workspace-path.v1`；判据：文档内不再出现 `workspaceAlias` 与 `core.workspace-path.v1`（`grep` 零命中）。
- [x] 5.2 `docs/NODE_LINK_PROTOCOL.md` 与 `docs/CORE_PORTS_AND_STORAGE.md`：远程建会话的目录引用、目录表与归属列、迁移版本；判据：`npm run check:docs` 通过且表格与 schema 一致。
- [x] 5.3 `docs/FRONTEND_DESIGN.md`、`docs/LOCAL_ADMIN_PROTOCOL.md`（§5.8 删除 `workspace select`）、`docs/CONFIG_REFERENCE.md`、`README.md`：删除登记入口并写实「目录自动出现 / 目录删除后历史可读」；判据：`npm run check:docs` 通过。
- [x] 5.4 文档引用门禁：`npm run check` 全绿；判据：命令输出。

## Workflow follow-up

- 按 `docs/WORKFLOW.md` §6 走 PR 与六个必需检查。
- 归档前确认 `openspec/specs/**` 已同步并再次跑 `npm run check`。
