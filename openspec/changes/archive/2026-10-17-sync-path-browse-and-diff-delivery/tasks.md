# Tasks

## 1. 机器合同与协议 DTO（WP1）

- [x] 1.1 在 `compatibility/commands/v1/commands.json` 增加 `workspace.browse`（`kind: query`、`scope` 与命令同名、`grant: grant.observe`、`transport: ["sync"]`、`delivery: mvp`）并把它加进 `grants` 的 `grant.observe` 分组；判据：`node scripts/check-command-catalog.mjs` 通过（它断言命令名集合、grant 分组并集与两份文档表格三方相等）
- [x] 1.2 在 `compatibility/features/v1/features.json` 增加 `core.workspace-path.v1`、`core.workspace-browse.v1`、`core.file-diff.v1`（均为 `mvp`、`required: false`），并把 `core.session-create.v1` 的描述从 `{ workspaceAlias, agentId }` 修正为「别名或客户端提供的目录二者取一 + 可选建目录开关」；判据：`node scripts/check-features.mjs` 通过
- [x] 1.3 在 `schemas/sync/v1/command.schema.json` 增加 `workspace.browse` 的 `$defs.commandName` 取值、请求与结果的 `$defs`、并挂进 `$defs.command.body.oneOf` 与结果分支；字段只含父路径、条目（名称/绝对路径/是否已登记）与截断标记；判据：`node scripts/check-command-catalog.mjs` 通过（enum 与 `transport` 含 `sync` 的命令集合相等）
- [x] 1.4 在 `schemas/sync/v1/common.schema.json` 的 `workspaces` 元素上增加**可选**路径键（保持 `additionalProperties: false` 下的封闭形状，仅新增一个具名属性），并在 `schemas/sync/v1/event-views.schema.json` 的 `file.changed` 视图上增加可选的差异行键与不可用原因；判据：`node scripts/check-schema-fixtures.mjs` 通过且新夹具按声明通过校验
- [x] 1.5 在 `crates/sync-protocol/src/command.rs` 增加 `CommandName` 变体、`ALL` 列表与长度、`as_str`、`is_query`、`requires_session_id`/`requires_expected_version`、`CommandPayload` 变体、`parse_payload` 臂、`CommandResultPayload` 变体与 `parse_result` 臂；判据：`cargo test -p sync-protocol` 全绿（含 `CommandName::ALL` 的计数断言与 `schema_drift` 的 enum 对齐断言）
- [x] 1.6 在 `fixtures/sync/v1/valid/` 与 `invalid/` 增加 `workspace.browse` 的正例与反例（父路径为 `null`、绝对路径、相对路径、含 `..`、非目录），全部写进 `fixtures/sync/v1/manifest.json`（负例带 `expectedKeyword`），并同步 `crates/sync-protocol/tests/envelope_fixtures.rs` 的两个计数常量；判据：`node scripts/check-schema-fixtures.mjs` 与 `cargo test -p sync-protocol` 全绿
- [x] 1.7 在 `docs/SYNC_PROTOCOL.md` 更新 §5.2 的 feature 表（三条新 feature）、§9.1/§9.4 的目录资源字段与「路径只在目录资源出现」的口径、§10.2 与 §10.3 的 `file.changed` 描述（含差异行与省略）、§11.5 的命令表与新命令的载荷/结果/失败分类、§14 的新固定限额；判据：`node scripts/check-command-catalog.mjs` 与 `node scripts/check-features.mjs` 通过（两份文档表格与 registry 三方相等）

## 2. core：端口、用例与派生（WP2）

- [x] 2.1 在 `crates/core/src/ports.rs` 增加目录浏览端口（列举根、列举一层），并在 `docs/CORE_PORTS_AND_STORAGE.md` §5 的 ```rust 块与 §4 的用例子面表里逐签名同步；判据：`node scripts/check-contract-drift.mjs` 通过（§5 与 `ports.rs` 逐签名相等）
- [x] 2.2 在 `crates/core/src/use_cases.rs` 增加浏览用例，首个参数为 `actor`，形状校验沿用既有五步顺序（非绝对 → 含 `..` → 存在 → 是目录 → 规范化）且形状非法时不触碰文件系统；条目不返回文件、大小或时间戳，按确定的顺序排列并施加上限与截断标记；判据：`cargo test -p core` 全绿，且用例覆盖「相对路径不触碰文件系统」「不可达层返回不可用」「超限置截断标记」三条
- [x] 2.3 让目录资源的投影在协商 `core.workspace-path.v1` 时带上已登记目录的规范化路径（取值经 `strip_verbatim` 去掉平台前缀），未协商时该键缺席；判据：`cargo test -p core` 中快照投影用例覆盖「协商时带路径」「未协商时键缺席且对象仍通过封闭形状校验」
- [x] 2.4 在 `session.create` 的用例里实现显式建目录开关：开关与别名互斥、开关为真且目标不存在时先创建（含缺失父层）再启动 Agent、路径成分已是非目录或创建失败时以服务端不可用类错误拒绝、已创建目录在后续失败时不删除；判据：`cargo test -p core` 全绿并覆盖规格的四条新场景
- [x] 2.5 在 `crates/core/src/derive.rs` 从同一次类型化 Diff 派生结构化差异行（新增/删除/上下文），施加固定的行数与字节上限，超限时改为只带统计并标记差异行未下发与已登记原因，且不改写 ACP 原文的字节、摘要与长度；判据：`cargo test -p core` 全绿并覆盖「派生新增删除行」「超限省略」「原文逐字节不变」
- [x] 2.6 在 `crates/core/src/broker.rs` 的 `required_grant` 增加新命令的臂；判据：`node scripts/check-command-catalog.mjs` 通过（它解析该函数源码并断言键集合与 grant 与 registry 一致）
- [x] 2.7 同步 `docs/MODULE_ARCHITECTURE.md` 中受影响的职责描述；判据：`node scripts/check-crate-boundaries.mjs` 通过且文档描述的职责与实现一致（人工核对新增段落）

## 3. server：命令面与装配（WP3）

- [x] 3.1 在 `crates/server/src/sync/command/query.rs` 增加 `workspace.browse` 处理函数，并在 `mod.rs` 的 `on_query` 增加分支：未协商 feature 时在**触碰文件系统之前**返回 `protocol.feature_required`，形状非法返回参数类错误，不可达返回不可用类错误，结果超限返回 `resource.result_too_large` 而不截断；判据：`cargo test -p server` 的同步用例覆盖这四条
- [x] 3.2 在 `crates/server/src/sync/command/limits.rs` 的 `CommandFamily::of` 把新命令归入合适的命令族；判据：`cargo test -p server` 的命令族穷尽断言全绿
- [x] 3.3 在 `crates/app/src/daemon.rs` 装配新的目录浏览端口实现并接进 `server::sync` 的命令面；判据：`cargo test -p app` 的端到端用例能在真实 daemon + 真实 SQLite 上完成一次浏览
- [x] 3.4 验证「浏览不产生登记与审计副作用」：调用前后 workspace 登记表与审计表都不变；判据：`cargo test -p app` 的用例断言两表行数不变
- [x] 3.5 核对错误路径不泄漏路径：浏览与建目录失败时的错误 `details` 只含字段名或别名；判据：`cargo test -p app` 的负向断言，并在主检出用 `rg` 检索新增错误构造点确认不含路径取值

## 4. 安全边界与 ACP 交付档位（WP4）

- [x] 4.1 改写 `docs/SECURITY_DESIGN.md`：§3 的数据分类、§8 的「本地管理能力」清单、§10.2 的**命令表新增 `workspace.browse` 行**与目录校验口径、§10.3 里「Sync 面只能引用本机当前已登记的 workspace 别名」这句与 §10.2 自相矛盾的表述、§12.3 的路径口径（改为「只下发给协商了该能力的设备，且只含已登记目录」）、§14.1 的日志禁止项保持不变；判据：`node scripts/check-command-catalog.mjs` 通过（§10.2 表与命令集合相等），且人工核对无一处仍宣称「规范化路径不得出现在任何对端可见输出」
- [x] 4.2 同步 `docs/INITIAL_DESIGN.md` 中与路径登记、同步策略相关的表述；判据：人工核对两文档口径一致，且 `npm run check` 的文档引用检查通过
- [x] 4.3 新增一条 ADR（`docs/adr/`）：把「已登记目录路径」与「节点目录浏览」纳入已认证设备能力面的理由、被否决的备选（隐式建目录、一次返回整棵子树、把浏览并入 `session.create`）、限制（只读、单层、无文件元数据、不产生审计动作）与残余风险；判据：ADR 文件存在且被 `docs/SECURITY_DESIGN.md` 与 `docs/MODULE_ARCHITECTURE.md` 的引用检查覆盖（`npm run check` 的文档引用项）
- [x] 4.4 更新 `docs/ACP_COMPATIBILITY_MATRIX.md` 与 `compatibility/acp/v1/matrix.json`：`tool_content.diff` 的第一阶段交付从「统计 + 相对化路径」扩为「统计 + 相对化路径 + 有界逐行差异」，Layers 保持 PWA 为 `view_only`；判据：`node scripts/check-acp-compatibility.mjs` 通过，且文档与机器矩阵口径一致
- [x] 4.5 全仓一致性核对：检索「路径」「canonicalPath」相关的禁令表述，确认只剩「客户端尚未知道的路径不得泄漏」这一条口径，且新边界在 `SECURITY_DESIGN.md`、`INITIAL_DESIGN.md`、`SYNC_PROTOCOL.md`、`CORE_PORTS_AND_STORAGE.md`、`ACP_COMPATIBILITY_MATRIX.md` 与 ADR 六处一致；判据：把检索结果与逐处结论写进工作包报告，检视者据此复核

## Workflow follow-up

- 全部工作包合入并跑过 Project Verify 后，按 `docs/WORKFLOW.md` §6.2 走远端落地（分支、PR、六个必需检查）。
- 本变更合入后，变更 `pwa-prototype-parity` 才能进入浏览器级验收；两者不是同一个 PR。
