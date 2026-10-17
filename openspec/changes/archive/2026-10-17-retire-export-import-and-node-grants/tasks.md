# Tasks

## 0. 按新基线补写重叠 delta（WP0，A/B 归档后执行）

- [x] 0.1 `specs/node-link-owner-server/spec.md`：catalog 不按 Export 过滤、命令不做 grant 判定、`session.create` 只剩 `agentId` + `directoryId`。判据：`openspec validate --strict` 通过、场景无丢失。
- [x] 0.2 `specs/local-admin-methods/spec.md`：删除 Export/Import 方法族。判据：同上。
- [x] 0.3 `specs/cli-commands/spec.md`：删除六个子命令与 `--grant`/`--export-id`/`--display-name`。判据：同上。
- [x] 0.4 `specs/storage-schema-v2-migration/spec.md`：v11 删表/删列/imported 重建。判据：同上。
- [x] 0.5 `specs/sync-snapshot-scope/spec.md` 与 `specs/pwa-web-client/spec.md`：imported 资源去 Export 维度（若主规格确有相关要求）。判据：同上。

## 1. core 与 identity（WP1）

- [x] 1.1 删除 Export/Import 用例与端口参数；判据：`cargo test -p core` 全绿且 `grep -rn "ExportId\|ImportId" crates/core/src` 只出现在删除后的注释/测试夹具中。
- [x] 1.2 删除 `grant.*` 展开与 `Authority` 的 grant 集合；节点认证事实只携带主体标识；判据：`cargo test -p identity-auth` 全绿，`grep -rn "grant\." crates/identity-auth/src` 无命中。
- [x] 1.3 删除 `broker::required_grant` 镜像；判据：`grep -rn "required_grant" crates` 无命中，`cargo test -p core` 全绿。

## 2. 协议资产与 DTO（WP2）

- [x] 2.1 `compatibility/commands/v1/commands.json` 删除 `grants` 与命令的 grant 归属；判据：`npm run check` 中命令目录一致性通过。
- [x] 2.2 `schemas/sync/v1`、`schemas/node-link/v1` 删除 Export/Import 与 grant/export 字段；`fixtures/**/v1/**` 同步；判据：`npm run check` 中 schema/fixture/feature 三项通过。
- [x] 2.3 协议 crate 的 DTO 删除对应类型与枚举取值；判据：`cargo test -p sync-protocol -p node-link-protocol` 全绿。

## 3. 存储 v11（WP3）

- [x] 3.1 删除 `owned_export`、`imported_import`、`imported_import_export`；删除 `owned_node.grants_json`/`export_ids_json`；判据：`cargo test -p storage-sqlite` 中「v10→v11 删表删列且既有信任与会话不变」通过。
- [x] 3.2 重建 `imported_session`/`imported_delivery_index`/`imported_command_ref`（去 `export_id`），迁移时按 `(owner_node_id, session_id)` 合并重复行并保留最新 cursor；判据：合并用例断言无孤儿索引、`local_sequence` 不回退。
- [x] 3.3 升级失败整体回滚与连续两次打开 schema 文本不变；判据：两条既有模式的用例通过。

## 4. 入站面（WP4）

- [x] 4.1 删除 `local.export.manage` 方法族；判据：`cargo test -p server` 中对应方法返回 `local.unsupported`。
- [x] 4.2 Node Link catalog 取消 Export 过滤；判据：协议级用例断言已配对节点收到全部资源。
- [x] 4.3 命令面移除 grant 判定，保留上限、幂等与终态；判据：`cargo test -p app --test node_link_e2e` 通过，且用例覆盖「无 grant 即可建会话」与「撤销即时拒绝」。
- [x] 4.4 `node pair` 删除三个参数、节点名取自配置；判据：`cargo test -p server` 中对应用例通过。

## 5. CLI（WP5）

- [x] 5.1 删除 `export`/`import` 命令组与三个参数；判据：`cargo test -p app` 与 `acp-remote --help` 断言。
- [x] 5.2 帮助与错误文案无残留；判据：`grep -rn "export \|import " crates/app/src/cli*` 无命中。

## 6. 文档（WP6）

- [x] 6.1 `docs/SECURITY_DESIGN.md`：写入「配对即完全信任」的信任边界与威胁模型、保留的非授权边界；判据：`npm run check:docs` 通过且该节可被引用。
- [x] 6.2 `docs/NODE_LINK_PROTOCOL.md`、`docs/SYNC_PROTOCOL.md`、`docs/CORE_PORTS_AND_STORAGE.md`、`docs/LOCAL_ADMIN_PROTOCOL.md`、`docs/CONFIG_REFERENCE.md` §9、`README.md`；判据：`npm run check` 全绿（含文档引用与合同漂移）。
- [x] 6.3 升级说明：v11 迁移不可逆、配对含义变化；判据：文档含该说明。

## Workflow follow-up

- 依赖 A → B → C 的串行归档。
- 按 `docs/WORKFLOW.md` §6 走 PR 与六个必需检查。
