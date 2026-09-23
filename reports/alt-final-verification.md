# 替代验证(not-applicable 路径)与资源清理汇总

- 变更:`admin-state-persistence-v2`(schema `agentic`,E2E `mode = not-applicable`,附用户降级批准)
- 阶段:7.1 执行 / 7.2 汇总,责任人:主 Agent
- 目标:本地 `refs/heads/main` = `86f282bf545ea7839e961e09481d0784005ce420`(`git rev-parse`,2026-09-23)
- 验收 worktree:`D:/Project/acp-remote-main`(于 `main`,执行前 `git status --porcelain` 为空)
- 环境:rustc 1.98.1(`rust-toolchain.toml` 固定)、Node v24.19.0 / npm 12.0.2;`CARGO_TARGET_DIR` 指向主 worktree 的 `target/`(内容同一提交,用于复用构建缓存);不联网
- 原始输出:`reports/alt-7.1-run.log`、`reports/du1-main-verify.log`

## 7.1 逐项执行

### ① [PV4] `cargo test --locked -p core -p storage-sqlite --all-features`

- 退出码 `0`。`core` 78 passed;`storage-sqlite`:migration 7 passed / 1 ignored(生成器)、`admin_store` 28、`enum_coverage` 2,其余目标全 `ok`;无 0 用例目标、无全跳过。
- 关键用例(「保留性」的行为证明):`v1_fixture_upgrades_to_v2_and_preserves_rows`、`a_failed_upgrade_rolls_back_to_v1`、`second_open_of_an_upgraded_database_rewrites_nothing`、`v2_fixture_is_untouched_by_two_consecutive_starts`、`too_new_database_is_rejected_without_writing_rows`、`fresh_directory_is_created_at_version_two`。

### ② [PV1] `npm run verify`

- 在 `86f282b` 上执行,退出码 `0`(`npm run check` 十道合同门禁 + `cargo fmt --all -- --check` + `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` + `cargo test --locked --workspace --all-features` 全部子项无失败)。
- 证据复用:`reports/du1-main-verify.log`(6.7 在**同一提交**上执行;此后 `main` 未再前进,复用有效)。

### ③ [PV2] `node scripts/check-contract-drift.mjs`

- 退出码 `0`,输出:`§7 的 36 条 DDL 与 crates\storage-sqlite\src\migrate.rs 逐条一致;§5 的 15 个 trait / 87 个方法签名与 crates\core\src\ports.rs 一致`。

### ④ [PV3] `node scripts/check-crate-boundaries.mjs`

- 退出码 `0`,输出:`6 个 crate 的依赖方向与 §5 矩阵一致`;`core` 普通依赖闭包与 allow-list 逐项相等(29 项,不含 `ecdsa`/`hmac`/`signature`/`pkcs8`)。

### ⑤ v1 → v2 重放(`fixtures/storage/v2/from-v1.sqlite3`)

- 重放入口:`cargo test -p storage-sqlite --all-features --test migration -- --nocapture`,退出码 `0`(7 passed / 1 ignored)。该用例把夹具复制到临时目录后打开存储,走的就是 `migrate()` 的 v1→v2 升级路径。
- 夹具本体的 v1 状态(用 `bun:sqlite` 只读观察**副本**,夹具不动):`user_version = 1`;`owned_audit` 行 `audit_id = 1,2,5`(CHECK 无 `provider.configured`、有 `node.paired`);`sqlite_sequence`:`owned_audit.seq = 7`、`imported_audit.seq = 3`、`owned_event.seq = 3`、`owned_command.seq = 1`;`imported_audit` 2 行;`imported_import` 仍是 v1 形状(含 `export_id` 与 `UNIQUE(owner_node_id, export_id)`);无 `imported_import_export`、无 `owned_device`;`owned_session` 1 行、`owned_event` 3 行;`meta.server_epoch` 已钉死。
- 升级后的保留性断言(由上述用例断言,非空跑):序号不回退(`owned_audit` 插入得 `audit_id = 8`)、`audit_id` 与全部审计行保留、读视图 `head()` 与三条事件的 `event_payload` 保留、升级库与新建库的 6 张 `imported_*` 表列集合逐项相等、两张审计表 DDL 覆盖 `AuditAction::ALL` 每个取值、`imported_import` 迁移后无默认 grants(无可信来源不补齐,该 Import 保持不可用)。

## 7.2 汇总与资源清理

- 覆盖核对:`plan.md` 的 `not-applicable` 四项 `alternative_checks` 与本文件的 ①–⑤ 一一对应——① 变更 crate 行为/保留性、② 项目级 `npm run verify`、③ 契约漂移(§7/§5 与实现逐条一致)、④ 依赖边界与 `core` 闭包、⑤ 生产路径 v1→v2 重放与保留性断言。无一项被跳过或以「启动成功/退出码为 0」代替断言。
- 夹具只读:三份夹具在全部执行前后 SHA-256 不变——`empty.sqlite3` `c9c367e8…e5fad`、`from-v1.sqlite3` `c84ad51e…424f2`、`too-new.sqlite3` `45793273…04ec`;`git status --porcelain -- fixtures` 为空。
- 临时资源:重放观察使用 `target/alt-replay-*/` 下的副本,观察后即删除(已确认为不存在);测试自身的 SQLite 临时目录由用例释放;`CARGO_TARGET_DIR` 只做构建缓存复用(同一提交、无并行写入)。
- 结论:**替代验证四项全部通过**,E2E 本身保持 `NOT_APPLICABLE`;未发现需要提升为 FAIL/BLOCKED 的问题。
