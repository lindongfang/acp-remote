# 独立持久化展示快照库与混合自适应历史恢复任务清单

## 1. 独立展示快照存储引擎、配置契约与生命周期清理（WP1）

- [x] 1.1 WP1 在 `app/src/config.rs` 与 `app/src/data_dir.rs` 扩展可选 `[transcript_cache]` 配置解析（默认 `retention_days = 30`、`max_bytes = 10 GiB`，拒绝 `0`/溢出/未知键）及 `transcript-cache.sqlite3` 独立路径与 Windows 独占 ACL 保护，并在 `app/src/adapters/acp/shared_view_projection.rs` 提供基于协议版本（`V1`/`V2`）的离线 `AcpViewProjector` 构造；完成判据：`cargo test --manifest-path app/Cargo.toml --locked config`、`data_dir` ACL/符号链接拒绝与离线 `V1`/`V2` 投影单测全绿，且 `node.sqlite3`（`SCHEMA_VERSION = 15`）表结构零改动（覆盖 `STC-001`、`STC-003`）。
- [x] 1.2 WP1 复用既有 `sqlx::sqlite` 创建 `app/src/transcript_cache/` 模块实现 `TranscriptCacheStore` 与异步非阻塞写入/清理器（`WAL` + `foreign_keys = ON` + `secure_delete = ON`、`protocol_version` 持久化、规范化 `content_digest` 防虚刷、30 天滑动过期清理、10 GiB 跳过活跃/在途租约 LRU 整会话淘汰、`purge_deleted_native_session` 即时擦除及确定性 `shutdown()`），并编写存储实现文档 `docs/implementation/transcript-cache-storage.md`；完成判据：`STC-003`、`STC-011`、`STC-012`、`STC-013` 存储与独立池关闭后立即 `remove_dir_all` 单测全部通过。

## 2. 混合自适应读取、后台单飞校准与上下文恢复协调（WP2）

- [x] 2.1 WP2 将 `TranscriptCacheStore` 接入 `OwnedNodeSessionServices` 启动与确定性关闭序列（`node.rs`、`node/lifecycle.rs`、`node/session_services.rs`），在 `SharedViewCapture` / `SharedViews` 与 `NativeViewService`（`view.open` / `view.open_source`）打通实时/重放旁路落盘、故障告警降级（`CacheHealth::DegradedWriteWarning`，绝不阻断任务执行与严格审批）、客户端撤销统一 `client_gate` 门禁，以及冷启动纯元数据配置准入（`resolve_read` / `resolve_source_read` + `Authority` 来源注册，拒绝错误/过期 `selection`）后的零名额首屏秒开与按 `NativeSessionKey` 单飞后台标准重放自动校准（含 50 满额/不可达优雅降级与 `explicit_refresh` 手动重试）；完成判据：核心状态机完成 Red Check，`STC-003`、`STC-004`、`STC-005`、`STC-006`、`STC-010`、`STC-014`、`STC-015` 节点测试通过。
- [x] 2.2 WP2 在 `NativeCoordinator` / `NativeHistoryBridge` 与 `session_commands.rs` 完成 `resume_only` 无重放上下文恢复衔接（仅调原生 `session/resume` 零快照文本回灌，预载 `hybrid_resumed` 前序快照）、满额后轻量 `session/resume` 准入、`pure_memory` 与原生状态缺失（`native_unrecoverable`）只读兜底、缓存缺失/压缩删改权威同步，编写协调层文档 `docs/implementation/transcript-cache.md` 及 L5/L6 自动化验收测试代码；完成判据：`STC-007`、`STC-008`、`STC-009` 集成测试及 `cargo clippy --manifest-path app/Cargo.toml --all-targets --locked -- -D warnings` 全绿。

## 3. Web 协议扩展、跨段离屏暂存无闪烁前缀补齐与状态交互（WP3）

- [x] 3.1 WP3 在 `clients/web/src/management/session-protocol/primitives.ts` 与 `view-and-approval.ts` 实现 `closedWithOptional` 及 `SessionViewOpened.status` 可选字段解析，在 `session-view-status.ts` 支持 `transcript_cache_state` 状态记录，并在 `ProjectionCollection`、`ViewContentStore`、`Timeline` 与 `SessionViewDisplay` 实现跨流/跨 `segmentId` 重绑离屏暂存、原位主 `Timeline` 与 `dataset.streamId` 切换、前缀一致零跳动末尾追加及压缩删改裁剪；完成判据：`npm --prefix clients/web run check` 与协议/跨段时间线对账单测全绿，`FIXTURE-SHA256.md` 6 个浏览器夹具字节级不变（覆盖 `STC-002`、`STC-016`）。
- [x] 3.2 WP3 在 `management` 层新建 `clients/web/src/management/transcript-cache-banner.ts` 供 `session-view-display.ts` 与 `source-session-view.ts` 同层复用，并在 `shared-view-controller.ts` 与 `source-session-controller.ts` 薄委托接入秒开校准状态、满额/不可达降级轻提示、`"重试同步"` 按钮（`explicit_refresh`）、无重放/纯内存/原生不可恢复边界提示与写盘告警，同步更新 `docs/implementation/web-workbench.md`；完成判据：全部 `workbench/*.ts`、`session-protocol/*.ts`、`management/transcript-cache-banner.ts` 与 `management/source-session-view.ts` 严格 `<= 650` 行、零循环依赖与零跨层反向引用、Node 导入零 DOM 副作用，且 `npm run check`、`npm run build:rust`、`npm run check:spec` 全部退出码 0 通过（覆盖 `STC-001`、`STC-002`、`STC-016`、`STC-017`）。

## 4. 集成验收与交付检视门禁（WP4）

- [x] 4.1 WP4 在完成 `WP1`–`WP3` 并将业务代码、单元/验收测试与文档提交为干净的本地分支集成 `commit SHA` 后，并行执行独立 `tester` 正式验收（`STC-001`–`STC-017`）与独立 `reviewer` 交付检视；完成判据：`PV1`–`PV5` 完成核验、所有阻断问题闭环、`plan.md` 中 `Code Review` 集成交付门禁登记为 `passed` 且 `Verification Results` 完成真实证据回填。

## Workflow follow-up

- 归档在同一本地提交内按原子顺序执行：① 将 `specs/` 下 4 份增量规范同步合并至 `openspec/specs/session-transcript-cache/spec.md`、`structured-history/spec.md`、`native-session-recovery/spec.md`、`session-resource-policy/spec.md` → ② 将变更目录移入 `openspec/changes/archive/YYYY-MM-DD-deliver-session-transcript-cache/` 并补齐内部指向变更外相对链接的 `../` 层级，同步更新 `docs/architecture/capability-map.md` 引用 → ③ 将 `STC-001`–`STC-017` 用例 `### <ID>` 正文合入主归属套件 `testing/suites/session-transcript-cache/tests.md` 并按归档日期追加 `Source definitions`，同时更新 `structured-history`、`native-session-recovery`、`session-resource-policy` 长期套件的四列 `## 需求关联` 表引用对应 `STC-*` ID（关联套件不重复拷贝正文、不追加 `Source definitions`） → ④ 运行 `npm run check:spec` 校验通过后提交。
