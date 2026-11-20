# 历史轮次分页与实时衔接验收测试套件

本文由独立 `tester` 依据 [proposal.md](proposal.md)、[design.md](design.md)、增量规格 [history-navigation](specs/history-navigation/spec.md) 及分支基线 `4dd96797` 的真实代码树编写，定义变更 `deliver-history-navigation` 的编码前门禁与集成交付验收用例。正式验收运行日志保存在本地被 Git 忽略的 `testing/runs/deliver-history-navigation/tester/` 目录，精简逐例结论回填至 `plan.md` 的 `Verification Results` 章节。

## 共用执行条件与真实边界

- **版本快照与角色独立**：正式集成验收在与独立 `reviewer` 共享的同一本地分支 Git `commit SHA` 上执行（核对 `git rev-parse HEAD` 且受跟踪文件 `git status` 干净）；`tester` 不继承 `coder` 对话，不以开发自评、模拟推断或“检查已启动”冒充执行证据，零用例、仅启动或仅截图均不算通过。
- **隔离运行环境与资源清理**：所有节点、SQLite、TLS/WSS、浏览器与真实 Harness 测试均使用专属临时 `data_dir`（同时包含 `node.sqlite3` 与 `transcript-cache.sqlite3`）、操作系统分配的空闲回环端口、隔离 `CARGO_TARGET_DIR`（或只读复用同一 commit 的已构建产物）、隔离浏览器上下文与受控测试工作区；严禁读写用户真实默认数据目录、本机真实 `node.sqlite3`/`transcript-cache.sqlite3`、真实浏览器 `IndexedDB` 身份、`~/.codex` 等厂商私有目录或私人会话文件。用例通过、失败或中断时均须经 RAII 守卫与 Windows Job Object 终止本次拉起的节点、受控对端、浏览器及 Codex 完整子进程树，关闭全部数据库连接池（含 `TranscriptCacheStore::shutdown()`），并验证临时 `data_dir` 可在当前进程内立即无锁删除。
- **受控 ACP 对端（测试自有替身）**：L5 复用 `app/src/node/tests/transcript_cache_wss_cases.rs` 中真实节点 HTTPS/WSS + 受控 `controlled-acp-peer.mjs` 的写法，仅在测试代码内扩展配置：`replayTurns`（逐轮生成 `user_message_chunk`（`messageId` 可为字符串或缺省/`null`）、`agent_message_chunk`、可选 `tool_call`/`tool_call_update` 帧，每轮正文携带唯一标记 `U####`/`A####`/`T####`，四位轮号）、`preludeFrames`（首个用户帧前的前导帧）、`echoUserChunk`（实时 `session/prompt` 期间是否先发出该提示的 `user_message_chunk`）、`randomAgentMessageIds`（每次重放生成不同 `messageId`，证明不存在跨加载稳定标识）、`promptBarrier*`/`loadBarrier*` 文件屏障及 `failLoadMessage`。对端以 `ACP_PEER_LOG_FILE` 逐行记录 `pid`、方法名及断言所需的非敏感参数；子查询仅记录连接编号、请求编号、序号与字节数，在内存中比对正文和游标；“拉起子进程数”按日志中 `initialize` 事件的不同 `pid` 计数并与 Windows Job 内进程枚举交叉核对，“名额占用”以 `session.capacity.inspect` 结果为准。受控对端只证明平台合同，不得作为真实 Harness 通过证据。
- **L3/L4 确定性条件**：领域与存储层使用纯函数输入、注入式 `CacheRetentionClock` 与可控单调时钟、确定性随机种子和故障注入；SQLite 断言直接查询 `transcript-cache.sqlite3` 的 `transcript_cache_meta`、`transcript_sessions`、`transcript_records`、`transcript_turns` 与 `PRAGMA` 结果，并复用仓库既有 `sqlx::sqlite`（零新增依赖）。
- **L6 真实链路边界**：真实端到端必须连接真实 Windows 节点、内嵌生产前端的同源 Web 与已授权真实 Codex ACP 适配器子进程；缺少真实 Codex 可执行环境、认证凭据或配额时整例如实记为 `not-run` 并写明前置阻塞，不以受控对端结果替代，也不自动转 `LX`。受控对端的浏览器端到端单独作为平台合同验收（HN-027），不冒称真实 Harness 验收。
- **安全与不变量红线**：模型上下文始终以 Harness 原生恢复为权威，分页内容不得回灌模型；`node.sqlite3` 零 Schema 变更；游标与会话正文不得出现在日志、错误载荷或测试快照中；错误响应保持既有单键闭集信封，不附带详情。
- **用例范围说明**：HN-001、HN-002 为全部需求共用的工程与结构门禁，不单列场景；本变更全部场景均可由代码、协议探针、浏览器或真实 Harness 验证，不设 `LX` 用例。

所有 continue 请求均显式带请求方向与签发的游标；文中“更早”“更新”分别表示 older、newer。各页范围均按界面自上而下的轮次顺序书写。

## 需求关联

| 当前增量路径 | Requirement | Scenario | 用例 |
| --- | --- | --- | --- |
| specs/history-navigation/spec.md | 前端指定轮次分页 | 超过十轮 | HN-006、HN-010、HN-025、HN-027、HN-028 |
| specs/history-navigation/spec.md | 前端指定轮次分页 | 自定义及非法参数 | HN-003、HN-005、HN-010、HN-025、HN-027 |
| specs/history-navigation/spec.md | 前端指定轮次分页 | 超大一轮 | HN-013 |
| specs/history-navigation/spec.md | 前端指定轮次分页 | 原生只能整段读取 | HN-011、HN-025 |
| specs/history-navigation/spec.md | 前端指定轮次分页 | 重放或快照无法证明轮次 | HN-004、HN-024、HN-025 |
| specs/history-navigation/spec.md | 重放与展示快照分页的读取成本及缓存边界 | 命中有效本地展示快照离线翻页 | HN-009、HN-010、HN-023、HN-027、HN-028 |
| specs/history-navigation/spec.md | 重放与展示快照分页的读取成本及缓存边界 | 较早页超出内存且本地快照已被淘汰 | HN-011、HN-016 |
| specs/history-navigation/spec.md | 重放与展示快照分页的读取成本及缓存边界 | 重新重放会扰动原生工作 | HN-012 |
| specs/history-navigation/spec.md | 稳定游标与来源变化 | 翻页期间轻量恢复追加新轮次 | HN-009、HN-014 |
| specs/history-navigation/spec.md | 稳定游标与来源变化 | 显式原生同步或外部删改覆盖代际 | HN-008、HN-009、HN-015、HN-028 |
| specs/history-navigation/spec.md | 稳定游标与来源变化 | 跨范围游标 | HN-007、HN-019 |
| specs/history-navigation/spec.md | 缓存外目标重新定位 | 快照淘汰后无稳定标识且历史远大于内存窗口 | HN-006、HN-016 |
| specs/history-navigation/spec.md | 缓存外目标重新定位 | 重定位期间来源被压缩或改写 | HN-017、HN-026 |
| specs/history-navigation/spec.md | 缓存外目标重新定位 | 重定位失败或响应迟到 | HN-018、HN-026 |
| specs/history-navigation/spec.md | 手动打开子历史与权限隔离 | 折叠后重新展开 | HN-020、HN-026、HN-027 |
| specs/history-navigation/spec.md | 手动打开子历史与权限隔离 | 展开失败或撤销 | HN-020、HN-026 |
| specs/history-navigation/spec.md | 手动打开子历史与权限隔离 | 子目标缺少分页能力 | HN-020 |
| specs/history-navigation/spec.md | 独立只读子历史查询 | 有能力且根空闲 | HN-012、HN-016、HN-029 |
| specs/history-navigation/spec.md | 独立只读子历史查询 | 未证明能力或根忙 | HN-012、HN-020、HN-029 |
| specs/history-navigation/spec.md | 独立只读子历史查询 | 中途失去证明或响应错误 | HN-020、HN-029 |
| specs/history-navigation/spec.md | 原生历史与实时衔接 | 分页中的工具更新 | HN-022 |
| specs/history-navigation/spec.md | 原生历史与实时衔接 | 迟到读取响应 | HN-009、HN-022、HN-026 |
| specs/history-navigation/spec.md | 刷新重连与历史限制 | 外部客户端继续对话后显式刷新 | HN-015、HN-021、HN-028 |
| specs/history-navigation/spec.md | 刷新重连与历史限制 | 节点重启后读取无重放会话 | HN-021 |

## Cases

### HN-001 全仓工程检查、零新依赖、主库零表侵入与错误码合同不变

- 用例级别：L1
- 操作步骤：1) 在受测 commit 的隔离工作树根目录依次执行 `npm run check`、`npm run build:rust`、`npm run check:spec` 与 `git diff --check`；2) 执行 `git diff 4dd96797..HEAD -- app/Cargo.toml app/Cargo.lock package.json package-lock.json clients/web/package.json clients/web/package-lock.json vendor/`，核对依赖清单、锁文件与 `vendor/` 是否变化；3) 比对基线与受测提交中 `crate::storage::SCHEMA_VERSION` 常量，并分别以基线与受测二进制在两个全新临时 `data_dir` 初始化节点后导出 `node.sqlite3` 的 `SELECT type, name, sql FROM sqlite_master ORDER BY name` 全文；4) 运行 `npm --prefix clients/web test -- management-error-contract`，并以 `git diff 4dd96797..HEAD` 比对 `clients/web/tests/error-contract-vectors.ts`、`clients/web/src/management/error-messages.ts` 中 `ManagementErrorCode` 闭集与 Rust 管理错误码映射；5) 在 HN-010 至 HN-026 的运行日志与失败输出中检索本次签发的全部游标原文及 `U####`/`A####` 正文标记。
- 预期结果：第 1 步全部检查退出码为 0；第 2 步差异为空（零新增 Cargo 或 Web 运行时依赖、锁文件不变）；第 3 步 `SCHEMA_VERSION` 不变，两份 `sqlite_master` 全文逐字一致，`node.sqlite3` 中不存在任何 `transcript_*`、轮次索引或游标相关表与列；第 4 步 C50 向量 50 项全部通过，`ManagementErrorCode` 联合与中文文案表逐项不变，`history.page` 使用的 `invalid_request`、`invalid_cursor`、`stale_cursor`、`not_found`、`unsupported_capability`、`invalid_stage`、`session_capacity_full`、`invalid_target` 均为基线已有错误码；第 5 步命中次数为 0。

### HN-002 Web 模块行数预算、单向分层、零 innerHTML 与浏览器夹具不变

- 用例级别：L2
- 操作步骤：1) 统计 `clients/web/src/main.ts`、`clients/web/src/management/session-protocol.ts`、`clients/web/src/management/session-protocol/*.ts`（含新增 `history.ts`）、`clients/web/src/workbench/*.ts`（含新增 `history-page-controller.ts`）与 `clients/web/src/management/history-page-view.ts` 的逐文件行数；2) 对 `clients/web/src/**/*.ts` 构建导入有向图并执行 DFS 环路检测与单向分层核查（`protocol/views -> pairing/management -> workbench -> main.ts`）；3) 扫描 `clients/web/src/` 中 `innerHTML`/`outerHTML`/`insertAdjacentHTML` 出现次数与 `clients/web/package.json` 的 `dependencies` 字段；4) 在无 `#app`、无 `document` 的 Node 环境中直接 `import` `history-page-view.ts`、`history-page-controller.ts` 与 `session-protocol/history.ts`；5) 执行 `git diff 4dd96797..HEAD -- clients/web/src/management/session-protocol.ts` 检查 barrel 变更形态；6) 逐文件计算 `clients/web/tests/browser/independent-dom/FIXTURE-SHA256.md` 所列 6 个文件的字节数与小写 SHA-256。
- 预期结果：`main.ts <= 250` 行、`session-protocol.ts <= 120` 行，其余列出文件单文件 `<= 650` 行；导入图强连通环数为 0、跨层反向引用数为 0；HTML 字符串注入 API 出现次数为 0 且无运行时依赖；Node 导入不抛错、不访问 DOM、不产生顶层副作用；`session-protocol.ts` 的差异仅为新增导出行（无删除或改写既有导出）；6 个浏览器夹具字节数与摘要全部与 `FIXTURE-SHA256.md` 一致。

### HN-003 管理协议闭集解析：history.page 请求与 view.opened.page 元数据

- 用例级别：L2
- 操作步骤：1) 运行 `app/src/management_ws/session_protocol.rs` 单元测试解析 `history.page`：合法载荷覆盖 `target.kind = "target"` 与 `"source"`（含 `selection` 与 `nativeSessionId`）、有无 `child`、`turns` 缺省/`"1"`/`"50"`、`mode.kind = "first"`（仅 kind）、`"continue"`（`cursor`、必填 `direction = "older"|"newer"`）与 `"locate"`（`fromTurn`、`direction = "older"|"newer"`）；非法载荷覆盖 `turns` 为数字 `10`、`null`、`"0"`、`"-1"`、`"1.5"`、`"010"`、`"51"`、`" 10"`、`""`，`fromTurn` 为 `"0"`/`"01"`/数字，`direction = "up"`，`first` 携带 cursor/fromTurn/direction、`continue` 缺 direction 或携带 `fromTurn` 或 `locate` 携带 `cursor`，缺失 `mode`/`target`，各层级附加未知键，未知 `target.kind`，空 `child`；2) 解析 `view.open`/`view.open_source`：缺省 `page`、`page = {}`、`page = {"turns":"5"}`、`page = null`、`page` 含未知键；3) 运行 Web Vitest 校验 `session-protocol/history.ts` 的编码与 `parseSessionViewOpened` 对 `page` 的 `closedWithOptional` 解析：无 `page` 的旧载荷、全部字段合法的载荷（`source` 三值、`turnStatus` 两值、`order` 仅 ascending、`continuity` 两值、十进制字符串、`firstTurn`/`lastTurn` 为 `null`、`turns[]` 元素仅含 `turn` 与 `firstPosition`、游标为字符串或 `null`），以及未知键、非法枚举（含 order=descending）、数字型十进制、`turns[]` 元素含额外键的载荷；4) 将未携带 `page` 的 `view.open` 命令编码结果与基线 `4dd96797` 的编码结果逐字节比较。
- 预期结果：合法载荷在 Rust 与 Web 两侧往返一致，`turns` 缺省时解析结果为未指定（由节点按 10 处理）；全部非法载荷被 Rust 确定性拒绝为命令格式错误并在线上返回单键 `invalid_request`，不打开任何视图流、不返回内容；Web 解析器对非法 `page` 抛出 `SessionProtocolError`，对省略 `page` 的旧载荷正常接受；未携带 `page` 的 `view.open` 编码与基线逐字节相同。

### HN-004 可证明轮次分类与 TurnIndexer 规则

- 用例级别：L3
- 操作步骤：对协商版本下的原始 ACP 用户帧（含字符串 ID、缺省 ID 及 V1 null）、助手回应、工具和控制通知运行分类器，再将分类结果送入 TurnIndexer。验证 (a) 前导两条通知、用户 a、工具更新、用户 a、控制通知、用户 a、用户 b；(b) 无 ID 后接无 ID、有 ID 后接无 ID、无 ID 后接有 ID，各组分别直接相邻及隔着工具或控制通知；(c) 无 ID 用户、明确助手回应、无 ID 用户；(d) 仅通知或空输入；(e) 已有两个可证明轮后追加 (b) 的歧义序列。以固定种子生成 10 000 帧，对比逐帧和一次性扫描，检查领域模块依赖。
- 预期结果：(a) 首两条为前导记录，轮起点仅为下标 2、7；(b)、(d)、(e) 整代 unprovable，先前可证明的前缀也不再对外分页；(c) 得到起点 0、2 的两轮。增量与全量扫描的证明状态、有效起点一致，领域模块独立于原始 JSON 和 ACP 类型。

### HN-005 每页轮数校验与默认值

- 用例级别：L3
- 操作步骤：对 `app/src/domain/history_page.rs` 的轮数校验函数逐项输入：缺省、`"1"`、`"10"`、`"50"`、`"5"`（父请求）、`"3"`（子请求）、`"0"`、`"-1"`、`"1.5"`、`"1e1"`、`"010"`、`"51"`、`"18446744073709551616"`、`" 5"`、`"5 "`、全角数字 `"５"`、空字符串；随后以校验结果分别调用父目标与子目标的页区间规划（总轮数 25）。
- 预期结果：缺省得到 10；`"1"`、`"10"`、`"50"`、`"5"`、`"3"` 原值生效；其余输入全部返回对应 `invalid_request` 的校验错误，不截断、不夹取到 50、不改写为 10；父请求 5 轮得到 `[21..25]`，子请求 3 轮得到 `[1..3]`，两者互不影响。

### HN-006 页区间规划：父向更早页、子向更新页与重新定位区间

- 用例级别：L3
- 操作步骤：对纯函数页区间规划逐项断言：1) 父目标总轮数 25、每页 10：最新页、连续规划“更早”直至返回空游标、再从最早页连续“更新”；2) 子目标总轮数 25、每页 10：首页及连续“更新”，再连续“更早”；3) 父 25 轮每页 5、子 25 轮每页 3 的完整遍历；4) 总轮数 1000、每页 10：父从最新页遍历到最早页、子从首页遍历到末页；5) 定位：总轮数 1000、每页 10，`older` 自 `fromTurn` 500、5、1，`newer` 自 500、995、1000，`fromTurn` 为 1001；6) 记录每页的前导标记、`olderCursor`/`newerCursor` 是否存在。
- 预期结果：1) 父页依次为 `[16..25]`、`[6..15]`、`[1..5]`，第三页后无更早游标，反向依次回到 `[6..15]`、`[16..25]`；2) 子页依次为 `[1..10]`、`[11..20]`、`[21..25]`，末页无更新游标；3) 父页为 `[21..25]`…`[1..5]`，子页为 `[1..3]`…`[25..25]`；4) 父 100 页依次为 991–1000 至 1–10，子 100 页依次为 1–10 至 991–1000，各自并集恰为 1..1000 且无重复、无跨页拆轮；5) `older` 分别得到 `[491..500]`、`[1..5]`、`[1..1]`，`newer` 分别得到 `[500..509]`、`[995..1000]`、`[1000..1000]`，`fromTurn = 1001` 返回对应 `not_found` 的错误；6) 仅包含第 1 轮的页标记前导记录，边界页对应方向游标为空。

### HN-007 游标编解码与跨目标、子目标、代际严格校验

- 用例级别：L3
- 操作步骤：1) 用测试节点的持久 P-256 密钥签发并验证 v1 游标，对照专用域标签、完整载荷与返回字段（版本、方向、范围指纹、代际类型和令牌、边界轮次、每页轮数）。2) 对正确签名的内容制造截断、追加、非规范 base64url、替换签名，以及单独翻转每个字段的位；用测试密钥签发未知版本、非法方向及轮数 0/51 的载荷；另用其他节点密钥或其他域签名。3) 用有效游标分别改请求方向、来源、原生会话、子目标存在性或标识，交叉测试范围错误与代际错误。4) 按设计对目标种类、各标识规范长度前缀及子目标存在位计算 SHA-256 前 16 字节。固定来源，以 (native="ab", child="c") 和 ("a","bc") 检查字段边界；覆盖多字节标识与无子目标。5) 重启同一节点后再验原游标。
- 预期结果：往返逐字段一致，游标不含明文标识。畸形、篡改、无效签名、范围或方向不符均为 invalid_cursor；仅验签成功、请求方向和范围匹配后，代际类型或令牌不同才为 stale_cursor。各范围指纹与独立计算一致，相同拼接字节的不同字段划分得到不同指纹，交叉使用被拒绝。同一节点重启后正常使用原游标。

### HN-008 展示快照库 Schema v1 至 v2 迁移、故障原子性与回滚降级

- 用例级别：L4
- 操作步骤：1) 用基线 `4dd96797` 的 v1 Schema 在临时 `data_dir` 生成 `transcript-cache.sqlite3`，写入 3 个会话（含一个约 200 MiB 记录量的大会话），记录每会话 `record_count`、`byte_size`、`content_digest`、`last_updated_at_ms` 与全部 `raw_json` 的 SHA-256；2) 以受测 `TranscriptCacheStore` 打开并查询 `transcript_cache_meta`、`PRAGMA table_info(transcript_sessions)`、`transcript_turns` 的 DDL、`PRAGMA foreign_key_list(transcript_turns)` 与各会话新列取值；3) 注入迁移中途失败（例如在补列后、提交前令事务出错），随后检查数据库与节点状态并验证核心会话操作；4) 将 `schema_version` 改为 3 后启动受测节点；5) 在隔离工作树与独立 `CARGO_TARGET_DIR` 构建基线 `4dd96797` 二进制，以其打开已迁移的 v2 数据库并执行配对、提交输入与审批，记录数据库文件 SHA-256；6) 关闭节点后立即删除 `data_dir`。
- 预期结果：迁移后 `schema_version = 2`；`transcript_sessions` 新增 `generation_token`、`turn_state`（`CHECK` 限定 `pending`/`provable`/`unprovable`）、`turn_count` 三列，每个既有会话令牌为 16 字节且互不相同，`turn_state = 'pending'`、`turn_count = 0`；`transcript_turns` 带 `turn_index >= 1` 约束与对 `transcript_sessions` 的 `ON DELETE CASCADE` 外键，迁移后为 0 行（启动不扫描正文，大会话亦保持 `pending`）；全部记录字节与 `last_updated_at_ms` 等原有列不变；中途失败时数据库完整保持 v1（无半截列或表），节点进入仅实时降级并告警，执行、队列与审批照常；未来版本 3 与基线二进制打开 v2 均走 `SchemaMismatch` 仅实时降级，不修改数据库文件且身份、队列、审批正常；删除目录立即成功。

### HN-009 轮次索引懒构建、增量写入、换代令牌、纯读取不续期与级联擦除

- 用例级别：L4
- 操作步骤：使用注入时钟与 `TranscriptCacheStore`/`TranscriptCacheWriter`：1) 对 `pending` 的 25 轮会话（含 2 条前导帧）首次请求分页，记录分页前后 `transcript_sessions` 全部列与 `transcript_turns` 行，再次请求分页并比较 SQLite `total_changes()`；2) 对不含用户帧的会话首次分页；3) 通过写入器实时追加第 26、27 轮帧，然后重启存储（触发 `load_or_init_session_state` 重建）并追加第 28 轮；4) 以内容不同的重放调用 `replace_with_replay`，再以与当前内容完全相同的重放调用一次；5) 在 31 个模拟日内对同一会话执行 100 次纯分页读取后触发保留期清理；6) 分别经保留期清理、容量水位淘汰与 `purge_deleted_native_session` 删除三个已建索引会话后执行 `PRAGMA foreign_key_check`；7) 在对代际 g 进行懒构建扫描的同时并发执行 `replace_with_replay` 写入代际 g+1，并让旧代际的分页请求在换代后完成。
- 预期结果：1) 首次分页写入 25 行 `transcript_turns`，各 `first_ordinal` 等于各轮首个用户帧序号，`turn_state = 'provable'`、`turn_count = 25`，`last_updated_at_ms`、`last_synced_at_ms`、`content_digest`、`record_count`、`byte_size`、`generation_token` 均不变，第二次分页 `total_changes()` 增量为 0；2) `turn_state = 'unprovable'` 且 0 行；3) 第 26、27、28 轮各新增 1 行、序号正确、无重复，令牌不变，`turn_count = 28`；4) 内容不同时 `active_generation` 加 1、令牌变为新的 16 字节值、`transcript_turns` 仅剩新代际行且轮数按新内容重算，内容相同时代际、令牌、索引与 `last_updated_at_ms` 全部不变；5) 读取期间 `last_updated_at_ms` 不变，第 31 天该会话照常被清理；6) 三种擦除后对应键的 `transcript_turns` 行数均为 0 且外键检查为空；7) 最终不存在代际 g 的索引行，代际 g+1 的索引完整，旧代际请求未写入或覆盖任何新代际数据。

### HN-010 冷会话命中有效快照零子进程零名额翻页与选择租约保护

- 用例级别：L5
- 操作步骤：1) 启动真实节点与受控对端（`replay_capable`、`echoUserChunk = true`、`preludeFrames` 2 条），控制客户端 A 创建会话 S 并完成 25 轮，等待快照落盘；2) 只读客户端 B（从未接管）以 `view.open` 携带 `page = {"turns":"10"}` 打开 S，等待后台校准完成（重放内容与快照一致）并保存返回的 `olderCursor`；3) 经 `native.release` 释放 S 并重启节点，两客户端重连后记录对端日志 `initialize` 计数、`session.capacity.inspect` 与 `transcript_sessions` 行；4) B 以 `history.page`（`continue`）连续请求两次更早页，A 也以 B 共享的重启前游标和对应方向请求一次；5) B 以 `mode.kind = "first"`、`turns = "5"` 读最新静态页，再以 `turns = "0"` 请求；6) 将快照库容量上限配置为仅容纳当前快照，B 在收到页流首块后停止发送 credit，此时写入另一会话触发水位淘汰，随后关闭页流再次触发淘汰。
- 预期结果：第 2 步页元数据为 `order = "ascending"`、`totalTurns = "25"`、`firstTurn = "16"`、`lastTurn = "25"`；第 4 步依次返回 6–15 与 1–5 两页，`source = "transcript_cache"`、`liveTail = false`、`relocated = false`、`continuity = "same_generation"`，最后一页 `prelude = true` 且 `olderCursor = null`，流中原始记录恰为对应轮次（含前导帧）的全部帧并按来源顺序排列，`turns[]` 按正序给出每轮首条位置；重启前签发的游标在重启后仍有效；第 3 至 5 步期间对端新增 `initialize` 与 `session/load` 次数均为 0、已占用名额为 0、`last_updated_at_ms` 不变、S 保持未加载；`turns = "5"` 返回 21–25，`turns = "0"` 返回 `invalid_request` 且不打开流；第 6 步页流传输期间 S 不被淘汰，页流关闭后 S 可被淘汰。

### HN-011 缺快照经标准重放分页、快照淘汰后重新定位与满额限制

- 用例级别：L5
- 操作步骤：1) 在无任何快照的全新 `data_dir` 中，令受控对端（`replay_capable`，`replayTurns` 25 轮）暴露一个外部原生会话 U，客户端 B 经发现与 `target.register` 后，在主树身份未证时尝试打开 U 并记录进程与名额；再使用有可信根身份记录且无快照的会话 S，以 `view.open` 携带 `page = {"turns":"10"}` 打开；2) B 关闭视图、S 被释放后以返回的 `olderCursor` 请求更早页；3) 推进保留期时钟 31 天触发清理删除 S 的快照，B 以先前游标再请求一次，随后以 `locate`（`older`，`fromTurn = "15"`）请求；4) 再次删除快照，以 50 个受保护主会话占满名额后重复第 3 步的定位请求，随后释放 1 个名额重试。
- 预期结果：第 1 步 U 返回 `session_tree_unverified`，不启动加载进程、不新增容量名额、不返回正文；S 经一次 `session/load` 完成重放，页元数据 `source = "replay"`、返回 16–25，快照库新增 S 的完整代际与 25 行轮次索引；第 2 步返回 6–15，`source = "transcript_cache"` 且未再次拉起子进程；第 3 步旧游标返回 `stale_cursor` 且无内容，定位请求按容量规则重放一次后返回 6–15，`relocated = true`、`continuity = "unproven"`、`generationId` 与旧值不同，快照被重建；第 4 步满额时返回 `session_capacity_full`，不打开流、不返回部分页、不关闭任何受保护会话，释放名额后同一请求成功返回 6–15。

### HN-012 重放会扰动运行中原生工作时返回 invalid_stage 且不取消

- 用例级别：L5
- 操作步骤：1) 控制客户端 A 在根会话 R 上提交提示，受控对端以 `promptBarrier` 保持 R 处于运行中；2) 经测试夹具在 `node_session_parents` 记录 R 的子目标 C（C 无快照与当前子事件；当前连接已协商独立查询并备有 C 的 12 轮历史）；3) 只读客户端 B 请求 `history.page`（`target` 为 R、`child` 为 C、`mode.kind = "first"`）；4) 记录对端日志中的 `initialize`、`session/load`、`session/cancel`、独立子查询与 R 的输出流；5) 释放屏障令 R 正常结束，B 重复同一请求。
- 预期结果：第 3 步返回 `invalid_stage`，不打开流、不返回子正文；运行期间对端新增 `initialize`、`session/load`、`session/cancel` 及子查询次数均为 0，R 的输出流持续且最终正常 `end_turn`；第 5 步 R 空闲后同一请求经根树名额下的子目标读取成功返回 C 的首页，名额计数只体现根会话 1 个。

### HN-013 超大单轮按需分块、未加载部分标注与无输出截断

- 用例级别：L5
- 操作步骤：1) 受控对端为 25 轮会话的第 12 轮生成一条约 5 MiB 的工具输出（`tool_call_update` 内容），记录该原始帧的 SHA-256；2) 客户端 B 请求覆盖第 12 轮的页（父页 6–15），先只发放少量 credit，确认流暂停后再补足 credit 读完整页；3) 再次请求同一页并在接收到一半时断开 WSS，然后用 Web `SessionViewDisplay`/`HistoryPageView` 渲染该中断页；4) 重连后重新请求该页；5) 在已加载会话的实时连接视图中查看同一工具输出。
- 预期结果：该页仍为 6–15 且包含完整第 12 轮，不拆分到其他页；原始记录按 16 KiB 分块与积分流控交付，credit 不足时暂停而非截断，补足后重组字节与原帧 SHA-256 一致；中断的页在 Web 上明确标注存在未加载部分且不显示为完整页；重连后重读得到完整页；实时视图中的工具输出与原帧字节一致，不存在额外的内存输出截断。

### HN-014 翻页期间轻量恢复追加新轮次时游标保持有效并衔接最新页

- 用例级别：L5
- 操作步骤：1) 受控对端为 `replay_and_resume` 且 `echoUserChunk = true`，控制客户端 A 创建会话 S 并完成 25 轮后令 S 被回收；2) 只读客户端 B 以 `view.open` 携带 `page = {"turns":"10"}` 打开 S 的最新页并保持该视图，再以 `olderCursor` 读取 6–15，保存此时的 `olderCursor` 与 `newerCursor`；3) A 提交新输入，节点经 `session/resume`（非重放）恢复并完成第 26、27 轮；4) B 依次以第 2 步保存的 `olderCursor` 与 `newerCursor` 请求；5) B 抓取第 2 步保持的分页实时流，同时核对追加前已打开的静态页与普通非分页视图（两者均无 TurnStart）；6) 新客户端以 `view.open` 携带 `page = {"turns":"10"}` 打开 S。
- 预期结果：第 3 步对端只收到 `session/resume` 与新提示，无 `session/load`；第 4 步两游标均有效（无 `stale_cursor`），分别返回 1–5 与 16–25，内容与追加前快照中同范围的原始记录逐帧一致，`generationId` 未变；第 5 步最新页视图在第 25 轮之后分别收到第 26、27 轮的有序 ACPV TurnStart（轮号与首条 revision 一致），每个标记都先于该轮首条内容；逐帧核对随后追加的全部内容，无遗漏、无重复；第 6 步返回 18–27 且 `totalTurns = "27"`。

### HN-015 显式原生同步与外部删改换代后的旧游标失效与重新定位

- 用例级别：L5
- 操作步骤：1) 受控对端 `replay_capable`，会话 S 已有 25 轮快照，客户端 B 已读取 16–25 与 6–15 并保存指向 1–5 的 `olderCursor`；2) 以 `view.open`（`reason = "explicit_refresh"`）触发显式同步，对端重放内容与快照完全相同，B 随后使用保存的游标；3) 令 B 重新取得指向 1–5 的游标后，修改对端 `replayTurns`：删除原第 3、4 轮、改写原第 10 轮文本并在末尾追加外部新增 3 轮（共 26 轮），再次显式同步；4) B 使用旧游标请求，然后按期望坐标以 `locate`（`older`，`fromTurn = "5"`）请求；5) 以带加载屏障的冷打开触发后台校准，释放屏障后检查 `stream_rebound` 与 `view.rebind` 返回的页元数据；6) 查询快照库中 S 的代际、令牌、记录与索引。
- 预期结果：第 2 步 `generationId` 不变、旧游标有效并返回 1–5；第 3 步生成新代际；第 4 步旧游标返回 `stale_cursor` 且无内容，定位返回新代际的 1–5，`relocated = true`、`continuity = "unproven"`，内容为新历史第 1–5 轮（不再包含已删除的原第 3、4 轮或原第 10 轮旧文本）；第 5 步 `view.rebind` 在新实时代际上按相同轮数重新计算下限，返回 `lastTurn = "26"`、`firstTurn = "17"` 且最新页包含外部新增轮次；第 6 步旧代际记录与索引已删除、令牌已更换，不存在旧快照内容冒充最新同步历史。

### HN-016 快照淘汰后一千轮无稳定标识来源的逐页完整访问

- 用例级别：L5
- 操作步骤：1) 受控对端 `replay_capable`，`replayTurns` 1000 轮且全部用户帧无 `messageId` 且每两个用户帧之间均有明确助手回应，开启 `randomAgentMessageIds`；R 的根重放和当前连接已协商的 C 独立只读查询各提供可区分的 1000 轮；父子关系已登记，本地均无快照，根重放不投递 C 的事件；2) 父方向：客户端 B 以 `view.open` 携带 `page = {"turns":"10"}` 打开 R，随后反复以 `olderCursor` 请求；在收到 501–510 后令 R 释放并推进保留期时钟删除其快照，下一次以旧游标请求，收到 `stale_cursor` 后以 `locate`（`older`，`fromTurn = "500"`）请求，再继续以游标遍历至末页；3) 子方向：以 `child = C`、`mode.kind = "first"`开始，反复以 `newerCursor` 请求；在收到 491–500 后删除 C 的快照，收到 `stale_cursor` 后以 `locate`（`newer`，`fromTurn = "501"`）请求并遍历至末页；4) 统计每方向所有页的轮次标记、`relocated`/`continuity` 标志与对端根加载次数、子查询首批次数。
- 预期结果：父方向共 100 页依次为 991–1000 至 1–10，子方向共 100 页依次为 1–10 至 991–1000，每页恰含 10 个完整轮次且标记与轮号一致，各方向并集恰为 1..1000、无重复、无缺失；淘汰后旧游标返回 `stale_cursor`，重新定位页（491–500 与 501–510）标记 `relocated = true`、`continuity = "unproven"`，其余页为 `same_generation`；重定位后没有跳回各自首页；末页对应方向游标为 `null`；R 的 `session/load` 恰为 2 次；C 的查询首批恰为 2 次（首次与淘汰后重定位），子 `session/load` 为 0 次。

### HN-017 重定位期间来源被压缩或改写时的范围与 not_found

- 用例级别：L5
- 操作步骤：1) 会话 S 原有 1000 轮快照，客户端 B 正停留在 811–820 并持有 `olderCursor`；2) 删除 S 的快照，修改对端重放为压缩后的 600 轮（文本标记改为 `C####`）；3) B 以旧游标请求，收到 `stale_cursor` 后以 `locate`（`older`，`fromTurn = "810"`）请求；4) B 以 `locate`（`older`，`fromTurn = "500"`）请求；5) 将对端重放改为轮数不变但全部文本改写，再次删除快照并定位第 500 轮。
- 预期结果：第 3 步定位返回 `not_found`，不返回任何内容、不改为首页或其他页；第 4 步返回新历史的 491–500，正文均为 `C####` 标记，`totalTurns = "600"`、`relocated = true`、`continuity = "unproven"`；第 5 步返回改写后的新文本，不出现任何原快照文本或新旧拼接内容。

### HN-018 重定位期间断线、撤销、重放失败与并发定位的节点侧处理

- 用例级别：L5
- 操作步骤：在 S 的快照已删除、对端启用加载屏障的条件下分四组执行：1) B 发起 `locate` 后在重放未完成时断开 WSS，随后释放屏障；2) B 发起 `locate` 后由客户端 A 撤销 B 的设备信任，再释放屏障；3) 设置 `failLoadMessage` 后 B 发起 `locate`；4) B 先后发起 `locate` 第 500 轮（请求 1）与第 300 轮（请求 2），控制屏障使请求 2 的结果先完成、请求 1 的结果后完成，同时记录每个响应的 `generationId` 与流内容，期间 A 正常向另一会话提交输入；每组结束后检查对端日志、名额与快照库。
- 预期结果：1) 节点不向任何连接发布该请求的页或流，对端未收到 `session/cancel`，重放完成后名额按规则释放；2) B 的管理连接以 `4410` 关闭且收不到页，重放不被取消，A 的操作不受影响；3) 返回明确失败的单键错误，不打开流、不返回部分页，快照库无半截代际；4) 两个响应各自带有其读取时所属的 `generationId` 且流内容只属于该代际，快照库最终 `active_generation` 与令牌为最新值且未被较早请求回写，另一会话输入正常完成。

### HN-019 跨会话、跨原生范围与篡改游标明确失败且不泄漏内容

- 用例级别：L5
- 操作步骤：准备同一来源下的会话 S1、S2，以及另一来源下原生会话标识与 S1 相同的 S3，客户端 B 对三者均有读取信任；1) 用 S1 的游标请求 S2；2) 用 S1 的游标经 `target.kind = "source"` 请求 S3；3) 用 S1 的父游标附加 `child` 请求；4) 用子目标游标去掉 `child` 请求父目标；5) 篡改 S1 游标：翻转指纹中的 1 字节、截断、追加字节、把方向字段改为未定义值、改为另一合法方向但保留旧签名、把边界轮次改为另一个有效值；另用未经修改的 older 游标配合法定 newer 请求方向；6) 捕获每个响应帧与节点日志。
- 预期结果：全部请求返回单键 `{ "code": "invalid_cursor" }` 错误信封，不发出 `view.opened`、不打开流、不返回任何记录，响应中不含其他会话的标识或正文；节点日志与错误输出不包含游标原文或会话正文。

### HN-020 子目标分页：关系校验、独立分页、失败隔离、缺能力与展开不改变执行

- 用例级别：L5
- 操作步骤：经夹具记录根会话 R 的子目标 C1、C1 的子目标 C2、另一棵树的会话 U；预先由根重放自动投递 C1 的 10 轮与 C2 的 3 轮可证明历史并等到子页可读，各分支重置夹具；1) B 以 `child = C1`、`turns = "3"`、`mode.kind = "first"`请求，同时以 `turns = "5"` 请求 R 的父页，再以 `child = C2` 请求；2) 以 `child = U` 与不存在的 `child` 请求；3) 令 C1 无快照与当前子事件，在已协商的独立子查询上注入服务端错误，并发请求 C1 子页与 R 父页；4) 在仅支持根加载的当前连接为已证明关系的 C3 保持无快照、无当前子事件，另令自动投递的 C4 内容不含用户帧，分别请求子页并继续请求 R 父页；5) 在 B 读取子页流期间撤销 B；6) 恢复有读取权限的 B 与 C1 的可读子快照；R 运行提示期间，只读客户端 B 反复展开（打开子页流）与折叠（关闭流）C1 共 20 次；7) 折叠后令 C1 实时追加第 11、12 轮，再次展开 C1 首页并以 `newerCursor` 翻至末页，同时重读 R 的父页。
- 预期结果：1) C1 子页为 1–3、`order = "ascending"`，R 父页为 21–25，C2 可按后代关系读取，子首页 relocated=false、continuity=same_generation，父子分页互不影响；2) 返回 `invalid_target`，无子正文；3) 子页请求失败只返回该请求的错误，R 父页成功；4) C3 与 C4 返回 `unsupported_capability`（C4 不按消息条数切分），R 父页继续可用；5) B 的连接以 `4410` 关闭，未收到剩余子正文；6) 对端未收到来自展开/折叠的 `session/prompt` 或 `session/cancel`，R 正常完成，`session.control.inspect` 的 `controlEpoch` 与队列快照不变，名额只计根会话；7) 重新展开首页仍为 1–10，末页包含第 11、12 轮，R 父页的轮次范围与各轮 `firstPosition` 与折叠前一致。

### HN-021 无重放会话：重启后快照只读分页、外部增量边界与淘汰后限制

- 用例级别：L5
- 操作步骤：1) 受控对端为 `resume_only`（另以 `pure_memory` 重复一次），`echoUserChunk = true`，完成 25 轮后重启节点；B 以 `view.open` 携带 `page = {"turns":"10"}` 打开并以游标读取 6–15、1–5；2) 令外部客户端在对端侧追加内容后，B 触发显式同步刷新，再使用刷新前的游标；3) 推进保留期时钟删除快照后，B 重新打开会话并请求 `history.page`；4) 在第 3 步状态下由控制客户端提交新输入，并调用 `product.status` 与会话列表。
- 预期结果：1) 页元数据 `source = "transcript_cache"`，视图状态标明 `resume_only`/`pure_memory` 能力边界与后续恢复、外部完整性边界提示，对端 `initialize` 与 `session/load` 新增 0 次；2) 无 `session/load`，`generationId` 不变、旧游标仍有效，状态明显提示外部增量不可见，不伪造新增轮次；3) 打开结果说明历史无法从原生补回，`history.page` 返回 `unsupported_capability`，不返回伪造历史；4) `resume_only` 会话的新输入经 `session/resume` 正常受理执行（`pure_memory` 按既有规则明确不可继续），`product.status` 与会话列表正常，不出现整体接入失败。

### HN-022 分页中的工具原位更新与旧代际读取不覆盖新代际

- 用例级别：L5
- 操作步骤：1) 已加载会话 S 有 25 轮，B 以 `view.open` 携带 `page = {"turns":"10"}` 保持最新页视图；对端在第 25 轮发出 `tool_call`（`in_progress`）后发出同一 `toolCallId` 的 `tool_call_update`（`completed`）；2) 第 25 轮后，对端连续两次更新第 12 轮已有工具（最后 completed），并更新第 3 轮的另一工具；等待写入器落盘，关闭视图并释放 S，记录对端进程计数；3) B 以旧游标从冷快照读取 6–15，用测试屏障在读事务固定上界后暂停读取，经测试写入器同代提交第 12 轮工具的另一补丁，解除屏障；比较事务已固定的代际及最大 ordinal 与独立归并结果；4) 以独立 pending 快照重置夹具，B 对代际 g 发起首次（触发懒构建的）分页请求，并在其完成前触发显式同步生成代际 g+1，随后检查两个响应与快照库。
- 预期结果：1) 最新页中第 25 轮的工具条目原位更新（同一锚点与 `firstPosition`，版本递增），不新增条目，页轮次边界不变，快照在同一代际追加更新帧且令牌不变；2) 最新页不追加第 12 轮工具的更新；3) 冷页第 12 轮工具仅出现一次，在原锚点显示截至事务上界的 completed；相同页读内不混入随后提交的内容、第 3 轮工具或第 25 轮其他条目。只交付本页原始记录与所属锚点最终投影，新增进程、名额占用均为 0，读取不续期；4) 旧请求要么返回 `stale_cursor`、要么返回仅属于代际 g 的页，快照库中无代际 g 的索引残留，代际 g+1 的数据与令牌未被覆盖。

### HN-023 只读客户端与控制端独立翻页且不改变控制权与空闲计时

- 用例级别：L5
- 操作步骤：1) 控制客户端 A 接管已加载空闲会话 S（`controlEpoch = "1"`），只读客户端 B 不接管；以可控单调时钟记录 S 的最后活动时刻；2) A 以 `turns = "10"`、B 以 `turns = "5"` 交错执行共 50 次 `history.page` 请求，期间 B 也按 A 的签发方向及每页 10 轮使用 A 的游标；3) 推进时钟至距最后活动 29 分 59 秒与 30 分；4) 撤销 B 后用 B 原有游标请求（在新连接尝试中）。
- 预期结果：A 与 B 各自得到与其轮数一致的页，互不改变对方视图；B 使用 A 的游标得到同一目标的正确页（游标不绑定客户端）；全程 `controlEpoch` 保持 `"1"`、控制者仍为 A，队列快照不变，对端未收到 `session/prompt` 或 `session/cancel`；29 分 59 秒时未回收、30 分时按原计时回收（分页不刷新空闲计时）；撤销后 B 的连接以 `4410` 关闭，游标不能替代读取信任。

### HN-024 轮次不可证明时的完整视图、续读定位限制与会话操作不受影响

- 用例级别：L5
- 操作步骤：1) 分别运行无用户帧及含相邻匿名/混合用户块且无可靠分隔的两组代际。无用户帧组使用不发出任何用户帧的受控对端（实时 `echoUserChunk = false`，重放只含 `agent_message_chunk`）完成 25 次提示；2) B 以 `view.open` 携带 `page = {"turns":"10"}` 打开；3) B 以 `locate` 请求任意轮次；4) 在 Web 渲染该页；5) 控制客户端提交新输入；另在已打开分页实时流的可证明代际追加歧义序列。
- 预期结果：第 2 步返回全部原始记录（记录数等于快照 `record_count`），页元数据 `turnStatus = "unprovable"`、`firstTurn`/`lastTurn` 为 `null`、两个游标均为 `null`；第 3 步返回 `unsupported_capability`；第 4 步页面明确显示轮次分页限制且不显示翻页控件，不按消息条数分组；第 5 步输入正常受理执行；丧失轮次证明的分页实时流结束，Web 重新打开完整视图，撤销旧分页分组及翻页控件。

### HN-025 Web 历史页视图：父子页内正序分组、轮数输入与来源提示

- 用例级别：L5
- 操作步骤：在 Vitest DOM 环境挂载 `HistoryPageView` 与 `HistoryPageController`（宿主端口为测试替身），输入构造的 `view.opened.page` 元数据与 `ACPV` 帧：1) 父页 `order = "ascending"`、`turns[]` 为第 16–25 轮；2) 子页 `order = "ascending"`、第 1–10 轮；3) 含前导记录的父页 1–5；4) 依次在轮数输入框填入 `"5"`、清空、`"0"`、`"-1"`、`"1.5"`、`"51"` 后点击翻页；5) 分别渲染 `source = "replay"`、`"transcript_cache"`、`"live"` 的页；6) 渲染 `turnStatus = "unprovable"` 的页；7) 用 `Timeline` 新增的位置区间过滤渲染区间外条目；8) 保持父最新页，依次输入第 26、27 轮的 ACPV TurnStart 与各轮具唯一标记的内容，在适用重投路径重复同一代际及 revision 的标记；全程监视 `innerHTML` 赋值。
- 预期结果：1) DOM 轮次分组顺序为 16、17…25，每组内条目按 `firstPosition` 来源顺序排列，各条目归属与 `turns[].firstPosition` 边界一致；2) 分组顺序为 1…10；3) 前导记录与第 1 轮同页，位于该轮分组之前且带前导标识；4) `"5"` 发送 `turns: "5"`，清空时请求不携带 `turns` 键，其余输入不发送请求并显示参数错误；5) 仅 `source = "replay"` 显示首次加载可能遍历全部历史的提示；6) 显示轮次分页限制且无翻页控件；7) 区间外条目不渲染；8) DOM 顺序为 16…25、26、27，各轮独立分组，原第 16–25 轮节点不变，26、27 每组恰出现一次且正文标记无丢失、无重复、无串组；更早/更新按钮可用性与对应游标是否为 `null` 一致；`innerHTML` 赋值次数为 0。

### HN-026 Web 控制器：自动重新定位、not_found 旧视图、迟到响应丢弃与子容器折叠展开

- 用例级别：L5
- 操作步骤：在 Vitest DOM 环境以测试替身宿主端口驱动 `HistoryPageController`：1) 当前父页 6–15，点击“更早”后宿主返回 `stale_cursor`，再返回 `relocated = true`、`continuity = "unproven"` 的 1–5；对“更新”方向重复；2) 定位请求返回 `not_found`；3) 连续发起尝试 1 与尝试 2，先返回尝试 2 的页、后返回尝试 1 的页；4) 请求挂起期间宿主报告断线；5) 展开子容器（首页 1–10）后折叠，再重新展开，期间记录父页各分组 DOM 节点引用；6) 子页请求返回 `invalid_target` 或撤销关闭；7) 宿主推送不属于当前目标的响应。
- 预期结果：1) 控制器自动发出 `locate`，“更早”时 `fromTurn = "5"`、`direction = "older"`，“更新”时 `fromTurn = "16"`、`direction = "newer"`，重新定位页替换（而非追加）原分页视图并显示“已在最新同步代际中重新定位，与之前页面的连续性未经证明”，不请求首页；2) 保留当前页并标记为旧视图，提示目标轮次在最新原生历史中已不存在，不跳转；3) 显示尝试 2 的页，尝试 1 的响应被丢弃且其流被释放（发送关闭），DOM 不变；4) 不渲染新页，保留并标记旧视图；5) 折叠时关闭子页流，重新展开重新请求子目标首页（两次都是独立 first，无 cursor/fromTurn/direction，relocated=false、continuity=same_generation）并正序渲染，父页分组 DOM 节点引用全等不变；6) 仅在该子容器显示限制，不出现子正文，父页内容不变；7) 被忽略且不改变 DOM。

### HN-027 真实节点、同源 Web 与受控 ACP 对端的浏览器翻页端到端验收

- 用例级别：L6
- 操作步骤：构建内嵌最新生产前端的 `acp-remote.exe`，在隔离 `data_dir` 与随机端口启动，隔离浏览器上下文经真实配对入网，使用 `clients/web/tests/e2e/` 既有浏览器驱动方式（不新增依赖），对端为受控 ACP（25 轮父会话含前导帧，根重放自动投递已登记子目标的 12 轮可证明事件，未协商独立查询）：1) 打开父会话并逐次点击“更早”至末页；2) 将每页轮数改为 5 后刷新最新页，再输入 0；3) 恢复有效父每页轮数后，在历史面板“子会话历史”入口手动输入子目标标识展开、翻至下一页、折叠后再展开；4) 释放会话后继续翻页并记录对端日志；5) 第一客户端停留在旧代父页，记录旧更早游标与期望坐标；修改对端重放内容，由第二个已配对客户端显式刷新并确认换代。第一客户端保持原页、不刷新，点击“更早”，记录请求与响应顺序；6) 停止节点并清理进程树与临时目录。
- 预期结果：1) 页面依次显示 16–25、6–15、1–5 三页，每页轮次分组自上而下正序，最后一页含前导记录；2) 显示 21–25，输入 0 时提示参数错误且未发送请求；3) 子会话依次显示 1–10、11–12 且组内正序，重新展开回到 1–10，父页内容与分组不变；4) 释放后翻页期间对端新增 `initialize` 为 0；5) 第一客户端先按旧游标及 older 续读收到 stale_cursor，继而自动按原期望坐标 locate，新页 relocated=true、continuity=unproven；页面显示“已在最新同步代际中重新定位，与之前页面的连续性未经证明”且展示新内容；6) 无残留子进程、端口占用或文件句柄。

### HN-028 真实 Windows 节点、同源 Web 与真实 Codex 的历史分页端到端闭环

- 用例级别：L6
- 操作步骤：在隔离 `data_dir`、随机端口与受控 Codex 隔离会话目录下启动真实 `acp-remote.exe`（内嵌生产前端），同源 Web 完成配对并绑定已授权真实 Codex ACP 适配器（缺可执行环境、凭据或配额时整例记 `not-run` 并记录原因）：1) 在 Web 创建会话并完成至少 12 轮真实对话（含一次真实工具调用），以每页 5 轮查看最新页并记录页元数据；2) 释放会话并重启节点，打开该会话后在后台校准完成、会话再次释放后逐页点击“更早”，同时枚举 Job 内与系统中的 Codex 进程；3) 使用外部独立 Codex 客户端向同一原生会话追加一轮真实对话后退出；4) 在 Web 点击显式刷新后再点击“更早”；5) 停止节点并核查进程树、端口与临时目录。
- 预期结果：1) 页元数据 `turnStatus = "provable"`，最新页为最新 5 轮且自上而下正序分组；2) 翻页依次覆盖全部轮次直至第 1 轮，翻页期间新增 Codex 进程数为 0；3) 外部追加成功；4) 刷新后最新页包含外部新增轮次，随后“更早”从刷新后的新游标取得新代际历史页；5) 无残留 Codex 子进程或端口占用，临时目录可立即删除。

### HN-029 当前连接协商的只读子历史查询、分批验证与根树准入

- 用例级别：L5
- 操作步骤：分别在隔离环境执行以下五组检查。
  1. 通过真实节点和管理 WSS 准备来源 S、空闲根 R、关系已证明的孩子 C 和兄弟 D、两个已授权的只读客户端。C 无快照且无当前子事件，R 有可读父页。分别运行 ACP v1 和 v2 对端，核对双方的命名空间键、版本及设计指定的初始化元数据位置。只接受设计固定方法，输入 12 轮带工具与未知字段的原始子历史，使单帧跨 16 KiB 块边界并切开 UTF-8 字符。两个客户端同时展开 C，暂停首批并查看容量，再释放后逐页读取。内存中核对请求的 rootSessionId、sessionId、首次空游标、续批游标及保存的逐帧原始字节。
  2. 在各自全新连接依次测试能力缺省、仅根 loadSession、仅子取消、声明在错误位置、版本未知、仅方法名提示、重连后未重新确认能力；再让已匹配对端返回 method-not-found，同一连接重试。另在无扩展连接分别提供有效子快照与根重放自动投递的足够子事件后展开。
  3. 以屏障分别令根提示先获准、查询先登记。分别在下一批发送前和最终发布前撤销权限、移除父子关系、断开来源、切换来源配置/身份或连接代际、切换读取代际，放行迟到响应。分组测试一个观察者离开、最后一个观察者离开和节点关闭。检查原生方法计数、WSS 输出、暂存、租约及下一次合法根工作。
  4. 每组重置夹具，分别注入错误 JSON-RPC 关联 ID（推进无进展超时）、响应根或子 ID 错配、更新帧属 D 或协议版本不匹配、非 session/update、空或变化的 snapshotId、sequence 非规范或从 1 开始/跳号/重复、重复游标、非末批无字节进展、非法 base64url、块长超过 16 KiB、非法 UTF-8、末帧未以 LF 完结、服务端错误、提前 EOF 与进展超时。另测首批为空且结束。并行检查父页以及另一来源同名 R/C 的子页。
  5. 在最终发布路径设置屏障，使用存储夹具预置相同内容或不同内容的可读子代际，记录令牌、保留时间、轮次索引与正文摘要，推进保留时钟后放行。分别注入事务失败和较新读取代际抢先完成。比对发布前后各项以及翻页后的保留时间；以原文标记、原始响应和游标扫描产品日志、错误及测试快照。
- 预期结果：逐组核对协议、数据库与根树状态。
  1. 两个版本下，同一根、孩子和连接代际只有一条查询链，sequence 从字符串 `"0"` 连续增长；两个客户端的 C 首页均为 1–10，续页为 11–12，均正序。命中快照的续页新增查询为零。原始帧字节和顺序一致，页标识为 C，Web 只见平台分页游标。根树始终只计一个名额，针对 C 的 prompt、load、resume、cancel、close 调用均为零。
  2. 无匹配能力且无数据时返回 `unsupported_capability`，不发探测请求。已声明却拒绝方法的连接仅第一次调用，其后直接报告限制。快照和自动投递两个分支均给出 C 页，扩展请求为零；只有根加载成功却无子数据时不能打开 C。父页及根执行持续可用。
  3. 根工作先获准时返回 `invalid_stage`，查询先登记时原生查询与受保护工作没有重叠。单个观察者退出不影响另一读者；失去证明、读取代际过期或最后一个观察者退出后停止后续请求，迟到响应不发布，原生取消次数为零。撤权按既有关闭合同执行。查询任务有界退出，暂存清空，读取租约释放，随后合法工作能取得准入；关机后临时目录可立即删除。
  4. 每个错误分支均不发布部分子快照，无子正文外泄，父页、其他来源及已有可读代际完整。空历史能结束传输，无虚构轮次，依既有不可证明轮次规则返回分页限制。
  5. 发布前只可见上一完整代际；持有效发布权且摘要相同时保留原令牌与保留时间，内容改变时正文、索引、新令牌一次生效。事务失败或旧代迟到保留先前可读代际，读取不延长保留时间。日志、错误和测试快照中敏感标记命中数为零。正向结果属于受控对端合同证据，真实 Harness 未协商时仅能验证负向。
- 共享查询中的撤权复核：分别在续批之前和完整发布之前暂停查询，同一子目标由 A 首发、B 加入。只撤销 A，验证 A 收到 4410 后再无正文，B 在原查询上获得完整页，首批只发送一次、共用根租约最终只释放一次。再分别撤销最后一个观察者、使所有观察者共用的关系证明失效，两种情况均停止续批并丢弃未发布暂存。
