# 独立持久化展示快照库与混合自适应历史恢复测试套件

本文由独立 `tester` 依据 [proposal.md](proposal.md)、[design.md](design.md)、增量规格 [session-transcript-cache](specs/session-transcript-cache/spec.md)、[structured-history](specs/structured-history/spec.md)、[native-session-recovery](specs/native-session-recovery/spec.md)、[session-resource-policy](specs/session-resource-policy/spec.md) 及仓库真实源码架构编写，定义变更 `deliver-session-transcript-cache` 的编码前门禁与集成交付验收用例。正式验收运行日志保存在本地被 Git 忽略的 `testing/runs/deliver-session-transcript-cache/tester/` 目录，精简逐例结论回填至同目录 `plan.md`。

## 共用执行条件与真实边界

- **版本快照与独立角色边界**：正式集成验收必须在与独立 `reviewer` 共享的同一本地分支 Git `commit SHA`（核对 `git rev-parse HEAD` 且受跟踪文件 `git status` 干净）上执行；`tester` 会话独立于 `coder` 开发对话，不修改受跟踪源码或测试夹具，不以开发自评或模拟推断冒充执行证据。
- **隔离运行环境、资源保护与进程树清理**：所有节点、SQLite、TLS/WSS、Web 与真实 Harness 测试均使用专属临时 `data_dir`、独立空闲本地端口、隔离 `CARGO_TARGET_DIR`（或只读复用同 commit 已构建产物）及受控测试工作区，严禁读写用户真实默认目录、本机真实 `node.sqlite3`、真实浏览器 `IndexedDB` 私钥或私人会话文件。无论测试通过、失败或中断，退出前必须通过 RAII 守卫与 Windows Job Object 完整终止受管子进程树并关闭全部数据库连接句柄，验证临时 `data_dir` 可在 Windows 下立即无锁删除。
- **真实 Harness 与受控替身边界**：L1/L2 静态与工程门禁直接扫描真实源码树并运行标准构建脚本；L3/L4 领域与存储层使用可控单调时钟、确定性字节预算，并复用仓库既有带补丁 `sqlx::sqlite`（`vendor/sqlx-sqlite`，零新增 Cargo 依赖）通过指向 `<data_dir>/transcript-cache.sqlite3` 的物理独立连接池与确定性连接池/worker 关闭（`TranscriptCacheStore::shutdown()`），验证 30 天滑动过期、10 GiB 水位淘汰、Windows ACL、`secure_delete` 覆写与故障注入；L5 协议与前后端集成基于真实 Rust 节点（HTTPS/WSS + `node.sqlite3` + `transcript-cache.sqlite3`）、受控标准 ACP 对端（分别覆盖 `replay_capable`、`resume_only`、`pure_memory`、满额占用、历史压缩与原生报错）及真实 Web 控制器/DOM 时间线验证；L6 端到端闭环必须连接真实 Windows 节点、同源 Web 与已授权的真实 Codex ACP 适配器子进程，严禁以模拟替身冒充真实 Harness 通过。若缺少真实 Codex 认证或可执行环境，如实记为 `not-run` 并说明前置阻塞，绝不自动转为 `LX`。本变更全部场景均可由代码、协议探针、浏览器与真实 Harness 验证，不设 `LX` 用例。
- **核心不变量与安全红线**：模型执行上下文恢复始终以 Harness 原生 `session/resume` 或 `session/load` 为唯一权威，严禁将平台展示快照、命令、审批或审计拼接文本重发给 Harness，严禁扫描 `~/.codex`、`~/.pi` 等厂商私有磁盘目录；`node.sqlite3`（`SCHEMA_VERSION = 15`）保持零聊天正文表与零锁耦合；Web 模块保持单文件 `<= 650` 行（`main.ts <= 250` 行）、零运行时第三方依赖、零 `innerHTML`，且 `clients/web/tests/browser/independent-dom/FIXTURE-SHA256.md` 所列 6 个独立浏览器夹具字节级不变。

## 需求关联

| 当前增量路径 | Requirement | Scenario | 用例 |
| --- | --- | --- | --- |
| specs/session-transcript-cache/spec.md | 物理隔离的持久化展示快照库与上下文权威解耦 | 正常记录并在节点重启后首屏秒开 | STC-001、STC-003、STC-005、STC-017 |
| specs/session-transcript-cache/spec.md | 物理隔离的持久化展示快照库与上下文权威解耦 | 快照库故障不阻断执行与严格审批 | STC-004 |
| specs/session-transcript-cache/spec.md | 按 Harness 恢复与重放能力混合自适应读取 | 支持重放会话打开时先秒开本地快照再自动拉取补齐外部更新 | STC-002、STC-005、STC-016、STC-017 |
| specs/session-transcript-cache/spec.md | 按 Harness 恢复与重放能力混合自适应读取 | 容量满额或原生暂不可达时后台自动同步优雅降级 | STC-002、STC-006、STC-016 |
| specs/session-transcript-cache/spec.md | 按 Harness 恢复与重放能力混合自适应读取 | 仅支持无重放恢复的会话跨回收继续对话 | STC-007 |
| specs/session-transcript-cache/spec.md | 按 Harness 恢复与重放能力混合自适应读取 | 纯内存无恢复能力会话结束后保留只读历史 | STC-008、STC-016 |
| specs/session-transcript-cache/spec.md | 默认三十天滑动保留与十吉字节上限清理 | 三十天内有新对话滑动续期而超期自动清理 | STC-001、STC-011 |
| specs/session-transcript-cache/spec.md | 默认三十天滑动保留与十吉字节上限清理 | 容量超过十吉字节时优先淘汰最旧非活跃快照 | STC-001、STC-012 |
| specs/session-transcript-cache/spec.md | 默认三十天滑动保留与十吉字节上限清理 | 原生会话删除同步擦除本地快照 | STC-013 |
| specs/structured-history/spec.md | 原生会话与历史为权威 | 清除缓存后查看 | STC-009 |
| specs/structured-history/spec.md | 原生会话与历史为权威 | 原生历史压缩或删除 | STC-009、STC-016 |
| specs/structured-history/spec.md | 通用标准重放与可丢弃视图 | 只有标准全量重放 | STC-005、STC-016、STC-017 |
| specs/structured-history/spec.md | 通用标准重放与可丢弃视图 | 无重放能力仍可接入 | STC-007、STC-008 |
| specs/structured-history/spec.md | 当前连接视图由节点共享管理 | 浏览器刷新或手机接入 | STC-003、STC-005 |
| specs/structured-history/spec.md | 当前连接视图由节点共享管理 | 正常回收后重新查看 | STC-005、STC-006、STC-017 |
| specs/structured-history/spec.md | 当前连接视图由节点共享管理 | 读取撤销 | STC-010 |
| specs/native-session-recovery/spec.md | 原生绑定与按需恢复 | 原生状态缺失 | STC-008 |
| specs/native-session-recovery/spec.md | 恢复与历史读取分离 | 无重放恢复 | STC-007 |
| specs/native-session-recovery/spec.md | 恢复与历史读取分离 | 已准入连接选择无重放恢复 | STC-007、STC-016 |
| specs/native-session-recovery/spec.md | 恢复与历史读取分离 | 恢复重放旧消息 | STC-002、STC-009、STC-016 |
| specs/session-resource-policy/spec.md | 默认五十个主会话名额 | 最后一个名额并发分配 | STC-014 |
| specs/session-resource-policy/spec.md | 默认五十个主会话名额 | 满额时打开命中本地快照的冷会话优雅降级 | STC-006、STC-014 |
| specs/session-resource-policy/spec.md | 默认五十个主会话名额 | 子会话不单独管理 | STC-014 |
| specs/session-resource-policy/spec.md | 安全空闲三十分钟回收 | 工作结束后重新计时 | STC-015 |
| specs/session-resource-policy/spec.md | 安全空闲三十分钟回收 | 页面持续查看 | STC-015 |
| specs/session-resource-policy/spec.md | 安全空闲三十分钟回收 | 历史重放刷新计时 | STC-015 |
| specs/session-resource-policy/spec.md | 安全空闲三十分钟回收 | 最近重放仍可容量淘汰 | STC-015 |

## Cases

### STC-001 全仓工程构建、配置解析契约与主库零表侵入验证

- 用例级别：L1
- 操作步骤：1) 在隔离工作树根目录依次执行 `npm run check`、`npm run build:rust`、`npm run check:spec` 与 `git diff --check`；2) 执行 `cargo test --manifest-path app/Cargo.toml config` 验证 `acp-remote.toml` 的 `[transcript_cache]` 解析：覆盖省略 `[transcript_cache]` 默认填充（`retention_days = 30`、`max_bytes = 10_737_418_240`）、显式合法正整数配置，以及 `retention_days = 0`、`max_bytes = 0`、负数、毫秒换算溢出与未声明未知键（`#[serde(deny_unknown_fields)]`）的拒绝路径；3) 核查 `app/src/session_storage/schema.rs` 与节点初始化后的 `node.sqlite3` `sqlite_master` 表清单，确认主控制库 `SCHEMA_VERSION` 保持 `15` 且未混入任何 `transcript_*` 聊天快照表。
- 预期结果：全仓 TypeScript 类型检查、Vitest、Vite 构建、Rust `fmt`/`clippy`/单测/二进制构建、`check:spec` 与空白字符检查全部以退出码 0 通过；缺省 `[transcript_cache]` 与既有配置 100% 兼容且默认启用 30 天与 10 GiB，非法数值或未知配置键确定性返回 `AppError::Config`；核心控制库 `node.sqlite3` 表结构零新增、零改动。

### STC-002 Web 模块行数上限、单向无环依赖、协议可选闭合原语与浏览器夹具不变性验证

- 用例级别：L2
- 操作步骤：1) 执行静态检查脚本统计 `clients/web/src/main.ts`、`clients/web/src/management/session-protocol.ts`、`clients/web/src/workbench/*.ts`（含 `shared-view-controller.ts`、`source-session-controller.ts`）、`clients/web/src/management/session-protocol/*.ts`、`clients/web/src/management/transcript-cache-banner.ts` 与 `clients/web/src/management/source-session-view.ts` 的逐文件总行数，扫描 `clients/web/src/` 是否存在 `innerHTML` 或 `clients/web/package.json` 运行时第三方依赖，并对 `clients/web/src/**/*.ts` 运行有向图 DFS 环路检测与严格单向分层核查（`protocol/views -> pairing/management -> workbench -> main.ts`），在无 `#app` 的 Node 环境下直接 `import` 全部 `workbench/*.ts` 与 `clients/web/src/management/transcript-cache-banner.ts` 模块；2) 执行协议单测校验 `closedWithOptional` 与 `parseSessionViewOpened` / `readSessionViewStatus`：分别传入省略全部快照扩展字段的旧载荷、携带合法 `transcriptSource`/`syncState`/`capabilityBoundary`/`continuable`/`cacheWarning` 的载荷、以及混入未声明未知键或非法枚举值的载荷；3) 逐文件比对 `clients/web/tests/browser/independent-dom/FIXTURE-SHA256.md` 所列 6 个浏览器夹具文件的字节数与小写 SHA-256 摘要。
- 预期结果：`main.ts <= 250` 行、`session-protocol.ts <= 120` 行，全部 `clients/web/src/workbench/*.ts`、`clients/web/src/management/session-protocol/*.ts`、`clients/web/src/management/transcript-cache-banner.ts` 与 `clients/web/src/management/source-session-view.ts` 单文件严格 `<= 650` 行，模块依赖图强连通环数为 `0`、违反单向分层 `protocol/views -> pairing/management -> workbench -> main.ts` 的跨层反向引用数为 `0`、Node 导入零 DOM 副作用、`innerHTML` 与运行时依赖数为 `0`；`closedWithOptional` 100% 兼容旧对端省略可选键并严格拒绝任何未知键或非法字段类型；`FIXTURE-SHA256.md` 所列 6 个浏览器夹具文件的字节长度与 SHA-256 摘要 100% 保持不变。

### STC-003 独立展示快照库物理隔离、Windows 独占 ACL、句柄关闭与结构化投影保真验证

- 用例级别：L4
- 操作步骤：1) 在隔离临时 `data_dir` 初始化节点与 `TranscriptCacheStore`，检查 `data_dir` 下 `transcript-cache.sqlite3`（含 `-wal`、`-shm`）与 `node.sqlite3` 的物理文件分离，查询 SQLite `PRAGMA journal_mode`、`PRAGMA foreign_keys` 与 `PRAGMA secure_delete`，在 Windows 下核验 `transcript-cache.sqlite3` 及其附属文件的当前进程 owner 独占 SDDL ACL，并注入指向外部文件的符号链接/重解析点验证 `DataDirectory` 拒绝行为；2) 分别通过协商为 `V1`（`protocol_version = 1`）与 `V2`（`protocol_version = 2`）两类协议版本的标准 ACP 会话流写入多轮用户/助手消息、思考块、同一工具 9 槽字段多次进度更新与晚完成状态（含 `V1` 与 `V2` 在 `tool_call` / `tool_call_content_chunk`、`state_update`、`_meta: null` 及 `messageId: null` 上的差异化载荷）、子步骤原始结构引用、未知结构载荷以及包含脚本标签/事件属性/双向控制字符的不可信原始 JSON，等待落盘后核查 `transcript_sessions.protocol_version`（`1` 对应 `V1`、`2` 对应 `V2`）持久化值，随后释放内存视图并重启节点（无存活 `Connection`），从 `transcript-cache.sqlite3` 流式装载快照代际并通过离线 `Version` 感知的 `AcpViewProjector` + `IndexedJournal` 与前端 `RawJsonDocument` 还原；3) 执行 `OwnedNodeSessionServices::shutdown` / `NodeCloseOperation`（调用 `TranscriptCacheStore::shutdown()`）停止节点，立即在当前进程内调用 `std::fs::remove_dir_all(&data_dir)`。
- 预期结果：`transcript-cache.sqlite3` 独立于 `node.sqlite3` 创建且启用 `WAL`、`foreign_keys = ON`、`secure_delete = ON`，Windows 文件 ACL 仅授权当前进程 owner 且符号链接/重解析点被明确拒绝（`AppError::UnsafeDataDirectoryEntry`）；`transcript_sessions` 准确持久化 `protocol_version`（`V1` 为 `1`、`V2` 为 `2`），节点重启后经离线 `Version` 感知 `AcpViewProjector` 在 `V1` 与 `V2` 两类协议会话上还原的轮次顺序、思考、工具首次位置原位最终状态、版本差异化字段投影、子步骤引用及原始 JSON 证据均与各自回收前实时连接视图完全保真一致，不可信主动内容仅安全惰性展示而零执行；节点关闭时异步写队列封存、后台任务 join 且物理独立的 `sqlx::sqlite` 连接池与 worker 线程经 `TranscriptCacheStore::shutdown()` 确定性关闭释放全部句柄，临时目录立即无锁删除成功。

### STC-004 展示快照库磁盘故障旁路隔离与非阻断告警降级验证

- 用例级别：L5
- 操作步骤：1) 在隔离 `data_dir` 启动真实节点与受控标准 ACP 对端，建立会话并进入活跃 `Run` 执行，同时让对端发出需严格权限审批的工具请求；2) 分别在节点启动打开损坏的 `transcript-cache.sqlite3` 文件头、运行时阻塞/只读锁死快照库底层写入或触发 SQLite 磁盘满 I/O 错误的场景下，继续推进实时 `session/update` 输出、客户端提交严格审批裁决（注册与提交决策）、新请求去重入队、设备信任校验及容量查询；3) 观察客户端管理连接收到的视图状态记录与横幅状态。
- 预期结果：展示快照库初始化损坏或运行时写盘失败绝不升级为控制面 `SqliteStoreError` 致命错误，节点不会崩溃或中断会话；协调器切换为 `CacheHealth::DegradedWriteWarning` 并向客户端实时推送 `cacheWarning = true` 持久化降级告警，同时内存实时 `SharedViews`、在途 `Run` 输出、严格权限审批裁决、命令去重、客户端信任门禁与 50 会话容量账本全部正常完成。

### STC-005 冷会话首屏秒开本地快照、后台单飞标准重放自动校准与多端共享视图验证

- 用例级别：L5
- 操作步骤：1) 使用支持标准重放（`standard_replay = true`）的受控 ACP 对端产生多轮对话与工具完成记录，确认写入 `transcript-cache.sqlite3` 后执行空闲回收（并覆盖零子进程重启节点的分支）；2) 在外部绕过节点内存向该底层原生会话追加新轮次，并对受控对端的 `session/load` 重放设置可控响应屏障（延迟返回重放流）；先构造 `selection` 中携带过期 `profileRevision` 或不匹配 `workspaceId`/`harnessId` 的负向请求分别调用 `view.open_source` 与 `view.open`；3) 使用合法配置在 Web 打开该未加载会话（同时覆盖 `view.open` 目标视图与 `view.open_source` 来源只读视图），记录收到 `view.opened` 及首屏 `ACPV` 快照 `SnapshotEnd` 的时刻、此时 `Authority` 上已验证 `NativeSourceId` 的登记状态、`SessionCapacity` 已占用名额数与子进程重放状态；随后并发发起第二个客户端打开同一冷会话，再释放对端重放屏障完成标准重放；4) 在会话已处于活跃加载态时，模拟浏览器刷新及第二台已授权手机/观察端接入读取同一会话，并核查测试全程是否访问 `~/.codex`、`~/.pi` 等厂商私有目录。
- 预期结果：第 2 步携带过期 `profileRevision` 或不匹配 `workspaceId`/`harnessId` 的 `view.open_source`（及 `view.open`）请求在披露 `transcript-cache.sqlite3` 中任何冷快照之前即被纯元数据级 `ReadConfigurationPort::resolve_source_read` / `resolve_read` 明确拒绝（零冷快照泄露）；第 3 步合法冷会话打开在占用 `0` 个主会话名额、启动 `0` 个子进程的前提下将已核验的 `NativeSourceId` 注册至 `Authority`，在对端重放屏障尚未释放、无需等待 Harness 子进程完成重放前，客户端已立即收到 `view.opened`（`transcriptSource = "transcript_cache"`、`syncState = "revalidating"`）并秒开渲染完整本地历史快照且不刷新 30 分钟空闲计时；后台校准按 `NativeSessionKey` 单飞合并（并发打开仅发起一次原生标准重放），重放完成后原子更新 `transcript-cache.sqlite3` 新代际并发出 `stream_rebound`，前端无感补齐外部新增对话且状态更新为 `syncState = "synced"`，后续同连接浏览与分页直接复用当前视图；第 4 步已活跃加载会话直接返回内存共享视图（`transcriptSource = "live"`），浏览器刷新与多设备观察端内容一致且不重复触发原生重放，全程零私有目录文件读取、零快照文本回灌模型。

### STC-006 五十主会话满额与原生不可达时冷会话快照优雅降级及手动重试验证

- 用例级别：L5
- 操作步骤：1) 在节点加载 50 个处于活跃运行或待决权限审批保护态的主会话占满全部名额，准备第 51 个已回收但 `transcript-cache.sqlite3` 快照有效的支持重放冷会话，客户端打开该第 51 个会话；2) 另在名额充足条件下，将受控 ACP 对端置为启动失败/传输断开（原生暂不可达），打开另一命中本地有效快照的冷会话；3) 释放 1 个受保护主会话名额（或恢复对端可达性），在页面点击降级横幅中的 `"重试同步"` 按钮触发 `reason = "explicit_refresh"`。
- 预期结果：在 50 名额全被受保护会话占满或原生暂不可达两种场景下，首屏均立即秒开并持续正常展示本地展示快照而不报错白屏、不关闭已建立的快照读取流、不强占任何受保护忙碌会话名额；视图通过 `transcript_cache_state` 准确呈现 `syncState = "degraded_capacity_full"` 或 `"degraded_unreachable"` 轻提示及 `"重试同步"` 入口；当名额释放或对端恢复后点击手动重试，成功按容量规则完成标准重放并平滑切换至最新同步视图（`syncState = "synced"`）。

### STC-007 无重放 `session/resume` 跨回收上下文恢复衔接与轻量准入验证

- 用例级别：L5
- 操作步骤：1) 配置仅支持 `session/resume` 而不支持标准重放（`!standard_replay && resume`）的受控 ACP 对端，经平台完成多轮对话后触发 30 分钟空闲回收：先仅打开查看该会话，核查对端是否收到 `session/resume`；再由主控端提交新轮次输入触发上下文恢复，捕获节点发送给对端的全部 ACP 请求载荷，观察恢复后页面历史与新输出的衔接；另测试本地快照已过期清除后的同条件 `session/resume` 恢复；2) 配置同时支持 `standard_replay` 与 `session/resume` 的对端，先在 50 名额满额下打开会话进入未完成重放同步的快照降级态，随后释放 1 个名额并由主控端采用轻量 `session/resume` 恢复继续输入，最后手动触发 `explicit_refresh` 重放校准。
- 预期结果：第 1 步纯查看 `resume_only` 冷会话直接展示本地快照并标注 `"展示平台已记录历史（外部终端直接产生的变更不可见）"` 且受保留策略约束，不因纯查看调用 `session/resume`；提交新输入时节点仅向 Harness 发送原生 `session/resume` 与本次新输入（严禁将本地快照正文拼接重灌给模型），未过期本地快照自动作为前序历史段（`transcriptSource = "hybrid_resumed"`）与新轮次实时 `session/update` 连续呈现在同一视图且同步追加落盘，零历史白屏；若本地快照已过期清除，模型上下文仍可恢复但页面明确提示历史缺失；第 2 步轻量 `session/resume` 快速恢复可执行态并衔接本地快照，诚实标注尚未同步外部最新原生变更而不谎称已全量同步，且用户点击手动重试可正常完成全量重放校准。

### STC-008 纯内存无恢复能力会话只读历史兜底与原生状态缺失拒绝执行验证

- 用例级别：L5
- 操作步骤：1) 配置既不支持标准重放也不支持 `session/resume`（`!standard_replay && !resume`）的纯内存受控 ACP 对端，在平台产生对话后关闭对端连接并重启节点，再次打开该会话查看并尝试发起继续执行操作；另清除其过期快照后再次打开查看；2) 配置曾记录本地有效快照的会话，使 Harness 侧对应的原生会话状态缺失（`session/resume` 与 `session/load` 均返回原生会话不存在/无法恢复错误），在 Web 打开该会话并尝试提交新输入或触发恢复，同时查阅管理审计与本地快照面板。
- 预期结果：第 1 步纯内存会话在连接结束或节点重启后打开时，正常完整渲染未过期的本地只读历史快照，同时明确标注 `capabilityBoundary = "pure_memory"`、`continuable = false`（连接结束后已无法继续恢复执行），过期清除后则明确提示历史无法补回；第 2 步原生状态缺失时，新输入与恢复请求明确失败拒绝（`DispatchError::ProtocolRejected`），绝不静默创建替代新会话、绝不用平台展示快照伪造模型上下文，节点将快照标记为 `native_recovery_state = "native_unrecoverable"`，管理审计与未过期本地快照仍可按各自只读边界查阅并醒目标注原生会话不可恢复。

### STC-009 缓存清除后原生权威重建、历史压缩删改同步与重放旧条目代际更新验证

- 用例级别：L5
- 操作步骤：1) 将某支持标准重放会话的本地展示快照置为已到期清除（或首次发现外部新建会话），客户端打开该会话：分别验证原生重放成功与原生重放失败（且 `node.sqlite3` 中存有历史管理命令、队列、审批与安全审计记录）两类响应；2) 对已有本地 5 轮快照的冷会话，让对端在后台自动同步（及手动重试同步）时分别返回压缩/删改后的 3 轮权威历史、以及返回原生会话已被外部删除错误；3) 在恢复或标准重放中让对端再次发出包含重复正文的旧条目（分别覆盖具备可靠跨加载原生稳定标识与缺少跨加载稳定标识两类情况），并交错迟到的旧读取代际数据包。
- 预期结果：第 1 步缓存缺失但重放成功时按容量规则拉取原生历史并重建 `transcript-cache.sqlite3` 快照；重放失败时明确返回读取错误，严禁从 `node.sqlite3` 命令、队列、审批或审计表拼凑伪造聊天历史；第 2 步原生发生历史压缩或删改时，节点原子替换本地快照代际并刷新视图为最新 3 轮权威现状，原生已删除时失效可执行状态并标注限制，绝不用过期旧快照掩盖最新原生现状；第 3 步重放再次发出旧条目时严格按可靠来源标识与读取代际更新视图及快照（无法证明连续性时整代替换并说明限制），不按消息文字去重、不把旧历史当作新回复追加，且旧代际迟到包不覆盖新视图。

### STC-010 客户端信任撤销对实时共享视图与本地展示快照的统一即时阻断验证

- 用例级别：L5
- 操作步骤：1) 配对并授权两个客户端 `Client A` 与 `Client B`，准备一个正在输出实时流的活跃会话（`SharedViews`）与一个已空闲回收但命中 `transcript-cache.sqlite3` 的冷会话（`CachedSharedViews`）；2) 让 `Client B` 分别处于已打开冷会话快照分块读取中途、等待 `wait_change` / `stream_rebound` 重绑中途，随后由 `Client A` 撤销 `Client B` 的设备信任；3) 撤销后观察 `Client B` 在途流状态，并使用 `Client B` 再次尝试调用 `view.open`、`view.open_source`、本地读取接口及发送视图 credit。
- 预期结果：`Client B` 信任被撤销瞬间，`SocketViewAuthority` 与 `LocalViewAuthority` 在 `TrustStore::client_gate` 保护下立即阻断其实时共享视图与本地展示快照的分块发送、`prepare`、`wait_change`、`rebind` 及新打开请求，统一返回 `ViewError::Revoked` 并以 `4410` 关闭其管理连接；绝不因命中离线快照缓存或此前曾受理过输入而绕过当前权限校验。

### STC-011 三十天滑动保留期内容摘要防虚刷与超期非活跃快照自动清理验证

- 用例级别：L3
- 操作步骤：1) 使用注入可控时钟初始化 `TranscriptCacheStore` 与清理协调器（默认 `retention_days = 30`，即 `2_592_000_000 ms`），在 `t = 0` 写入会话 A 与会话 B 的展示快照并记录两者的初始 `content_digest` 与 `last_updated_at_ms`；2) 在第 10 天与第 25 天多次打开查看会话 B 的冷快照，并触发会话 B 的后台标准重放校准（对端返回与原快照完全一致的规范化 `raw_json` 序列，即 `new_digest == content_digest`），核查会话 B 的 `last_updated_at_ms` 与 `last_synced_at_ms`；3) 在第 20 天向会话 A 写入新对话（或让标准重放返回新增内容使 `new_digest != content_digest`）；4) 将时钟推进至第 31 天（会话 A 距上次实质更新 11 天，会话 B 距上次实质更新 31 天且处于未加载空闲态），触发清理检查（覆盖启动检查、周期定时扫描与写后触发），同时检查受控 Harness 侧会话 B 的原生数据。
- 预期结果：纯打开读取快照以及 `new_digest == content_digest` 的无变更后台重放仅更新 `last_synced_at_ms`，严禁修改 `last_updated_at_ms`（零虚刷续期）；有实质新对话或内容变更的会话 A 成功刷新 `last_updated_at_ms` 滑动续期；第 31 天清理执行后，超期空闲的会话 B 在 `transcript_sessions` 与 `transcript_records` 中的记录被彻底清除，会话 A 的快照完整保留，且节点未向 Harness 发送任何原生删除指令、原生会话数据毫发无损。

### STC-012 十吉字节容量上限 LRU 整会话淘汰与活跃加载及在途读取租约保护验证

- 用例级别：L4
- 操作步骤：1) 验证默认配置 `max_bytes = 10_737_418_240`（10 GiB）常量与累计账本精度，并在确定性容量预算下向 `transcript-cache.sqlite3` 依次写入均在 30 天有效期内的会话 X（最旧 `last_updated_at_ms = 1000`）、会话 Z（次旧 `last_updated_at_ms = 2000`）、会话 Y（第三旧 `last_updated_at_ms = 3000`）与会话 W（最新 `last_updated_at_ms = 4000`）；2) 令最旧会话 X 在 `NativeCoordinator` 中持有活跃主会话加载槽，令会话 Z 持有正在分块传输快照的活跃读取租约（`SelectionLeases > 0`），而会话 Y 处于未加载且无在途读取的空闲态；3) 写入新快照使 `SUM(byte_size) > max_bytes` 触发水位淘汰，检查各会话留存状态；随后释放会话 Z 的读取租约并再次触发超限淘汰。
- 预期结果：`ProtectedSessionsPort` 准确识别并保护已加载活跃会话 X 与存在在途分块读取租约的会话 Z，按 `last_updated_at_ms ASC` 跳过受保护会话，整会话原子删除最旧非活跃空闲会话 Y（级联清除其全部 `transcript_records`，不产生半截残缺会话），使总 `byte_size` 降回 `max_bytes` 以内；会话 Z 租约释放后若再次超限则按序正常纳入候选淘汰，全过程不影响任何原生会话权威文件。

### STC-013 原生会话确认删除同步擦除快照与 `secure_delete` 零明文残留验证

- 用例级别：L4
- 操作步骤：1) 向 `transcript-cache.sqlite3` 写入包含高熵唯一特征字符串（如 `UNIQUE_TRANSCRIPT_SECRET_MARKER_9F8A7B`）的多轮会话快照并完成 WAL 落盘与内存缓存代际装载；2) 调用原生删除联动端口 `purge_deleted_native_session(&NativeSessionKey)` 模拟有权用户完成该原生会话删除确认；3) 立即查询 `transcript_sessions` 与 `transcript_records`、尝试通过缓存协调器重新打开该会话快照，并直接以二进制方式扫描磁盘上的 `transcript-cache.sqlite3`（及 `-wal` Checkpoint 后的数据文件）是否仍包含上述唯一特征字节序列。
- 预期结果：`purge_deleted_native_session` 在同一事务内立即同步删除目标 `(source_id, native_session_id)` 的全部元数据与记录行，同时立即封存并移除内存中任何已装载的该会话缓存代际（不等 30 天到期），后续缓存读取立即返回未命中；得益于 `PRAGMA secure_delete = ON` 与 WAL 同步覆写，磁盘数据库文件字节扫描中唯一特征明文序列出现次数严格为 `0`。

### STC-014 五十主会话最后一额并发准入与子会话归属主执行树不单独计数验证

- 用例级别：L5
- 操作步骤：1) 在节点准备 49 个已占用的不同主会话名额，随后并发发起两个不同新主会话的实际加载请求（覆盖“新建/恢复主会话”与“打开未加载冷会话触发后台标准重放 `ReadCapacityAdmission`”并发竞争第 50 个名额，同时并发打开第 3 个命中本地快照的冷会话验证首屏阶段）；2) 在已加载的第 50 个主会话内模拟创建、运行并结束多个子会话（sub-agent / child session），全程监测主会话容量计数、超时计时器与淘汰候选列表。
- 预期结果：并发竞争最后一个名额时至多一个实际原生加载请求直接取得第 50 个名额，已加载加在途预留的唯一主会话总数绝不超过 `50`（首屏直接读取本地展示快照不占名额且不被阻塞）；主会话下创建或结束任意数量子会话时，主会话占用名额保持恒定不变，平台绝不单独对子会话设置名额、超时计时或容量淘汰动作。

### STC-015 三十分钟安全空闲回收、纯快照查看不保活、实际重放刷新计时与重放候选可淘汰验证

- 用例级别：L5
- 操作步骤：1) **工作结束后重计**：让长任务在最后一段输出文字出现很久之后才结束已知执行与有效审批进入可回收状态，核查进入可回收当刻、`29:59` 与 `30:00` 扫描周期的回收行为；2) **页面持续查看与快照翻页不保活**：持续停留在已安全空闲会话页面、发送连接心跳并翻阅内存视图与独立展示快照库（期间不触发任何新的协议重放或原生活动），观察满 30 分钟时的回收结果及回收后的页面可读性；3) **实际历史重放刷新计时**：在空闲会话接近 `29:00` 时通过打开冷会话后台自动同步（及手动重试同步/缓存缺失加载）实际接收包含旧历史时间戳的标准重放流，核查计时起点是否按本次接收时刻重置以及旧执行/旧审批是否复活；4) **最近重放仍可容量淘汰**：在 50 个名额已满且其中 49 个会话受活跃任务保护、仅 1 个候选刚完成打开时的后台自动重放（无执行、无有效审批、无受保护后台工作）时，请求新建/加载第 51 个主会话。
- 预期结果：1) 长任务结束后严格从进入可回收状态时刻起重新计算 30 分钟，不因最后文字很早而立即回收；2) 单纯停留页面、心跳与只读内存/独立展示快照库翻页绝不刷新空闲计时，满 30 分钟准时发起回收并更新为空闲态，回收后页面继续依托本地有效展示快照保持历史完整可读；3) 实际接收协议重放按本次节点接收时刻重新计算 30 分钟（持续重放可继续延后超时），但不产生新执行保护也不复活历史旧审批；4) 刚完成后台自动重放的无活动空闲候选在容量满额时正常作为淘汰目标被请求关闭并腾出名额，绝不因后台自动重放堵死 50 会话容量池。

### STC-016 Web 离屏暂存跨代际无闪烁前缀补齐、压缩裁剪、阅读滚动保持与降级横幅交互验证

- 用例级别：L5
- 操作步骤：1) 在 Vitest / DOM 环境挂载 `SharedViewController`、`SourceSessionController`、`SessionViewDisplay`、`clients/web/src/management/source-session-view.ts`、`Timeline` 与 `clients/web/src/management/transcript-cache-banner.ts`，打开命中本地快照的会话流 `stream-1`（初始段 `segmentId = Seg_1`）渲染前 4 轮卡片，设置窗口阅读滚动位置（如 `window.scrollY = 420`）、在卡片 2 建立文本选区并记录前 4 张 `<li class="acp-timeline__item">` 的 DOM 对象引用；2) 向 `stream-1` 推送 `stream_rebound` 触发重绑至具有不同 `segmentId`（`Seg_1 -> Seg_2`）的后台标准重放新流 `stream-2`：在 `stream-2` 分块传输期间（尚未收到 `snapshot_end`）检查 `#session-view-cache > [aria-label='共享会话视图']` 的节点数量与当前可见卡片，随后发送 `stream-2` 的 `snapshot_end` 触发跨流代际对账（覆盖“前 4 轮 `(anchorId, firstPosition, kind)` + 解析字段内容 `bodyKey` 一致且末尾新增第 5、6 轮外部对话”以及“原生压缩删改为仅剩 3 轮”两个分支）；3) 依次推送 `transcript_cache_state` 的 `revalidating`、`synced`、`degraded_capacity_full`、`degraded_unreachable`、`resume_only`、`pure_memory`、`native_unrecoverable` 与 `cacheWarning = true` 状态记录，点击降级横幅中的 `"重试同步"` 按钮。
- 预期结果：跨流重绑期间离屏暂存器在后台组装新代际，DOM 中始终保持唯一一个 `[aria-label='共享会话视图']` `<section>`（零双视图挂载闪烁）；`snapshot_end` 到达时跨流代际对账将主 `Timeline` 跨不同 `segmentId`（`Seg_1 -> Seg_2`）原位重绑（更新 `dataset.segmentId = "Seg_2"` 与 `dataset.streamId = "stream-2"`），并按 `(anchorId, firstPosition, kind)` + 解析后字段内容（`bodyKey`）匹配未变更前序条目：前缀一致追加分支下既有前 4 张 `<li>` DOM 节点引用（`===`）、`window.scrollY` 阅读位置与文本选区严格保持全等不变（零销毁重建），第 5、6 轮平滑追加至 `<ol class="acp-timeline__items">` 末尾；原生压缩删改分支下变更卡片原位更新且已压缩的尾部旧卡片被精确裁剪移除，不残留旧快照内容；各类状态横幅与可继续性边界提示准确渲染，点击 `"重试同步"` 准确以 `reason = "explicit_refresh"` 发起刷新。

### STC-017 真实 Windows 节点、真实 Codex Harness 与 Web 秒开快照及外部对话无感补齐端到端闭环验证

- 用例级别：L6
- 操作步骤：1) 在专属隔离临时 `data_dir`、独立空闲端口与受控 Codex 隔离会话目录下启动真实 Windows `acp-remote` 节点（HTTPS/WSS + `node.sqlite3` + `transcript-cache.sqlite3` + Windows Job Object），通过同源 Web 完成设备配对并绑定已授权真实 Codex ACP 适配器；2) 在 Web 创建会话并执行包含真实工具调用的多轮对话，核验 `transcript-cache.sqlite3` 落盘记录与 Windows 独占 ACL，随后显式释放该会话并重启 Windows 节点进程；3) 绕过当前节点活跃内存，使用外部独立 Codex 会话客户端向同一底层 `native_session_id` 追加一轮真实对话并退出外部客户端；4) 在 Web 重新打开该未加载冷会话，采集首屏本地快照渲染耗时、后台自动触发真实 Codex 标准重放的协议轨迹、重放完成前后的时间线 DOM 节点身份与末尾补齐内容；5) 正常停止节点并核查 Windows 子进程树、监听端口及临时目录删除情况。
- 预期结果：真实链路端到端全绿：节点重启后打开冷会话首屏立即渲染此前保存的多轮对话与工具最终状态（`transcriptSource = "transcript_cache"`，无需等待真实 Codex 子进程完成重放即可阅读）；与此同时节点无需用户手动点击即自动在后台拉起真实 Codex 标准重放，完成后前端保持既有前序卡片 DOM 与滚动位置不变、在页面末尾无感补齐外部新增的真实对话轮次并收敛至 `syncState = "synced"`；节点退出后无任何残留 `codex` 子进程或端口占用，临时 `data_dir` 内 `node.sqlite3` 与 `transcript-cache.sqlite3` 句柄完全释放并可立即清理删除。
