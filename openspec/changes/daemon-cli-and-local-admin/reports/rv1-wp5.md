# RV1-WP5 · 独立文档检视报告（WP5：切片 4 状态写回与陈旧陈述清理）

> 本文件由 reviewer 子 Agent 的输出工件原样落盘（运行 c34b49c7-64f9-49fc-b95a-00474fcea615），
> 主 Agent 未改动其结论；后续处置见 verification.md 的 Review Findings 与 RV2 复核。

```yaml
review_id: RV1-WP5
review_type: branch
stage: work-package review
work_package: WP5
base_revision: 2791f8b
target_revision: f39dda9
result: FAIL (F1/F2 BLOCK: 两处事实错误; F3-F6 P2)
agent: reviewer (结构只读, deepseek/deepseek-flash, fresh context)
```

## Review

**result: FAIL**（WP5 文档收口**未通过**：两处新写入/未清理的事实陈述与权威文档及实现直接冲突；均为 ≤1 行文本修复，不涉及代码或门禁行为）

判定依据：本轮唯一交付物是「文档陈述必须与实际仓库事实一致」。在 f39dda9 实际写入的文本里发现两条**事实错误**（关闭顺序、§5 表下注记对自身矩阵的描述），其中一条（关闭顺序）在同一变更的 `CORE_PORTS_AND_STORAGE.md` §7.5 第 4 条里被**明确改正过**，说明是「改了一处、漏了两处」的清理不彻底，而非各有依据的口径差异。

- **Correct（已核对通过）**
  - members 陈述与 `cargo metadata` 的 12 个成员逐行一致：`README.md:9-22`（12 行）、`AGENTS.md:89-101`（12 个已落地 + `node-link-client` 待落地）、`docs/MODULE_ARCHITECTURE.md:105-117`/`:127`/`:152`；`vendor/windows-local-ipc` 确实不在 members（metadata 成员表无它，`server` 对它是 path 依赖），`node-link-client` 在 `crates/` 下不存在。
  - 「已落地/未落地」边界**无过度声称**：`server::sync`/`server::node_link`/`server::acp_facade`、`node-link-client`、前端在所有被改文件中一律写成未落地（`README.md:21-24,30`、`MODULE_ARCHITECTURE.md:4,127,225,352,373`、`AGENTS.md:100-101,264`、`DEVELOPMENT_PLAN.md:14,17,24`、`CORE_PORTS_AND_STORAGE.md:11,1312`），且每条已落地陈述都带「仅切片 4 范围 / 本切片只落地两条路径」限定。
  - `nodeId` 派生式与实现**逐字一致**（见清单 4）。
  - `daemon.instance_lock` 的实现状态注记与实现一致（见清单 5）。
  - `LOCAL_ADMIN_PROTOCOL.md:133-135` 的 §3.1 实现状态注记与 `server::transport::local` 的 `CloseReason::FacadeUnavailable` 行为一致，无需改动（handoff §5.3 的判断成立）。
  - PV1/PV2 证据可读且落在 target 版本：`reports/wp4wp5-final-verify.log:1`（`@ f39dda93…`）与 `:1279`/`:1282`（`EXIT(npm run verify)=0`、`EXIT(check-crate-boundaries)=0`），全文无 `FAILED`/`error[`；`:30` 报告 `crate boundaries OK: 12 个 crate …（12 个已在矩阵登记）`。
- **Fixed**：无。本轮为只读检视，未修改任何文件（工具集无写能力）。

### 核对清单逐条结论

**1. members 陈述 → 一致（除 F2 的子句）**
- README crate 表 12 行 = metadata 12 个成员（`acp-protocol`/`acpr-transcript`/`acpr-wire`/`agent-host`/`app`/`core`/`identity-auth`/`identity-keystore`/`node-link-protocol`/`server`/`storage-sqlite`/`sync-protocol`），逐名可对上。
- `MODULE_ARCHITECTURE.md:127` 「members 就是本节除 `node-link-client` 之外的十二个，§5 的列是这十二个再加 `vendor/windows-local-ipc`」：§5 表头（`:464`）13 列 = 12 成员 + `windows-local-ipc` ✓；行 14 行 = 13 + `node-link-client` ✓。
- `MODULE_ARCHITECTURE.md:152`（「切片 4 落地后是十二个」）✓；`AGENTS.md` §4 表 ✓；`README.md:24`（`node-link-client` 属切片 6、尚不存在）✓。

**2. 「已落地/未落地」边界 → 无过度声称、无漏更（逐句结论）**
- 过度声称：`README.md:21-22`（server/app 限定「本切片只落地两条路径」）、`README.md:30`（列出两个合同例外 `import.add`/`node pair --mode access`，两者在实现中可证：`crates/server/src/local_admin/router.rs`、`crates/app/tests/cli_commands.rs:667-687`）、`MODULE_ARCHITECTURE.md:225`、`CORE_PORTS_AND_STORAGE.md:11,1312`、`DEVELOPMENT_PLAN.md:14,17` —— 均未越界。
- 漏更：未发现把 `server`/`app`/`storage-sqlite` 写成未落地；`COI_RE_PORTS/§11` 与 README 都明确写「已随切片 4 落地」。唯一边界观察：`LOCAL_ADMIN_PROTOCOL.md:3` 状态行仍写「切片 4 …实现中」（见 F6，属 WP5 写范围外）。
- 前端、`node-link-client`、三个 `server::` 平级模块在全文（含 `AGENTS.md:264`、`CORE_PORTS_AND_STORAGE.md:11`）一致写成未落地 ✓。

**3. §5 说明段与门禁注释 vs 门禁实际行为 → 基本准确，但一处自相矛盾（F2、F3）**
- 门禁实际行为（`scripts/check-crate-boundaries.mjs:148-160`）：成员依赖不在**列**集合里 → 硬失败；`row` 缺失时 `if (!row) continue` → 只做列成员判定，**行缺席确实静默**；末行 `registered.length` 只**上报**行登记数（`:234-236`）。§5 注记「会因『某成员的依赖不在列的集合里』硬失败，但对行缺席是静默的」✓ 逐字成立。
- 但同一条注记的 `windows-local-ipc` 子句与实际矩阵矛盾（F2），且门禁文件内的**行内注释**仍写旧口径（F3）。

**4. `nodeId` 派生式 → 与 `crates/app/src/identity.rs` 逐字一致**
- 域分离串：`NODE_ID_DOMAIN = b"acp-remote/node-id/v1"`（`identity.rs:30`）＝文档 §7 第 7 条的 `"acp-remote/node-id/v1"` 的 UTF-8 字节 ✓。
- 输入与拼接：`hasher.update(NODE_ID_DOMAIN); hasher.update(public_key.as_bytes())`（`:111-113`）＝「域分离前缀 ‖ 65 字节 SEC1 未压缩公钥，直接拼接、无长度前缀」✓（端口签名亦注明 `public_key` 返回 65 字节 SEC1，`IDENTITY_AND_AUTH_CONTRACT.md:407`）。
- 取前 16 字节 + RFC 4122 置位：`digest[..16]`、`bytes[6]=(b&0x0f)|0x80`、`bytes[8]=(b&0x3f)|0x80`（`:115-119`）＝「version nibble = 8、variant = 10xx」✓。
- 形状与校验：小写带连字符 hex 36 字符 + `NodeId::new(&text)`（`:121-147`）＝「带连字符的小写 canonical UUID，由 `core::model::NodeId` 做最终形状校验」✓（测试 `the_derived_node_id_is_a_canonical_uuid_and_bound_to_the_key` 断言 36 字符、`8-4-4-4-12`、version/variant、不同密钥不同 id）。
- 无独立持久记录、轮换即换 `nodeId`、不改 wire：文档三条与 `identity.rs:5-15` 模块注释一致 ✓；文档「实现位置是组合根（`app::identity`），不在 §7 的端口签名上」✓（本条不改 §7 任何签名）。
- 结论：**无一方有错**，文档无需按实现修正；未发现需要改实现的地方（且实现只读、不得改）。

**5. CLI/配置说明 → `instance_lock` 正确；另有 1 个键存在未标注的落差 + 1 处措辞（F4）**
- `daemon.instance_lock`：`crates/app/src/config.rs:297-311`（`"file"` 放行、`"ipc"` → `unwired.push("daemon.instance_lock(ipc)")`、其它值失败关闭）、`crates/app/src/daemon.rs:745-750`（debug 级 `daemon.config_unwired`，消息「该配置键本切片不消费：已解析但不生效」）；文档 `CONFIG_REFERENCE.md:49` 三处（只接线 `file`／被解析但不生效／debug 级 `daemon.config_unwired`）逐条成立 ✓，且「不静默换实现」与实现注释口径一致。
- 其他本切片消费/校验的键：`daemon.local_admin.endpoint`（F4）；`storage.flush_interval_ms` 措辞（F4 末段）；其余只在启动时被记为未接线的键（`daemon.listen`、`allowed_hosts`、`trusted_proxies`、`tls.*`、`sync.*`、`node_link.*`、`sessions.*`、`terminal.*`、`dev_mode.allow_plaintext`，见 `config.rs:275-383` 与测试 `:1025-1038`）**与各自文档一致**（它们属尚不存在的 adapter，已在模块/协议文档按切片登记），无需补逐键注记。

**6. 陈旧陈述清理（f39dda9 四处）→ 语义正确，但清理不彻底**
- `CORE_PORTS_AND_STORAGE.md:11`（开头修订注记）：只把 Daemon/CLI + `server` 本地适配器 + `app` 写成已落地，并列明「仍未实现的是 `server::sync`/`server::node_link`/`server::acp_facade` 与 `node-link-client`」 → 正确 ✓，未夸大切片 4 范围。
- `CORE_PORTS_AND_STORAGE.md:1312`（§11 末段）：同上，且加了「不得把这些存储测试通过解释为……端到端可用」的限定 → 正确 ✓。
- `AGENTS.md:264`（§9 括注）：只豁免未落地者、明确 `server` 本地管理与 `app` 已落地 → 正确 ✓。
- 门禁文件头注释（`scripts/check-crate-boundaries.mjs:12`）：与行为一致 ✓，但同文件行内注释未同步（F3）。
- **引入了新的不一致**：`DEVELOPMENT_PLAN.md:24` 的「本切片**切片** 4」重复措辞；且它改了 §3 却漏掉同文 §4 末句（`:77` 仍写切片 1「剩余工作是 Daemon/CLI 接线与端到端验收」）→ F5。
- 另有一处**未被本轮清理覆盖**的同类陈旧句：`MODULE_ARCHITECTURE.md:383`（与 README 新增文本同一错误顺序）→ F1。

**7. 可归档性 → 不能判「可归档」**
- 仍存在「把未实现/未生效写成已存在」的措辞：`CONFIG_REFERENCE.md:51` 的显式 endpoint 取值（F4，AGENTS.md §10 禁止「把尚未实现写成已存在」的同类）＋ 两处事实错误的关闭顺序（F1）。
- §10 映射表覆盖检查：本变更触及的权威面（crate/模块职责与依赖方向、core 端口/DDL 注记、配置键说明、身份合同、CLI↔方法映射的历史改动、`local-admin` 词表/fixture）都已在同一变更或既有提交中同步；本变更**未新增门禁**，因此 `package.json`/`README.md`「合同检查」/ci.yml 注释的四处同步义务不适用 ✓。
- 但 `tasks.md` 的 2.19/2.20 复选框仍未勾选（handoff §5.7 声明不在写范围、由主 Agent 维护）：归档/`all_done` 语义另需主 Agent 处理，属我无法验证的范围。

### 发现清单

**RV1-WP5-F1（P1，BLOCK）关闭顺序陈述错误，且与同一变更刚改正的权威口径及实现相反**
- 位置：`README.md:22`（本 diff **新增**文本）；`docs/MODULE_ARCHITECTURE.md:383`（同文件、同 crate 的职责段，本 diff 未清理）。
- 事实：两处写「停周期任务 → 停接入层 → 停 Agent → `wal_checkpoint(TRUNCATE)`」。
- 权威/实现证据：`docs/CORE_PORTS_AND_STORAGE.md:1220`（f39dda9 同一变更改写）＝「停接入层 → 取消周期任务与信号监听 → 停 Agent → `wal_checkpoint(TRUNCATE)` 并清理 endpoint/释放单实例锁」，并显式写「周期任务的取消位置……**不是**『先于接入层』」；`docs/SECURITY_DESIGN.md:358`＝「关闭时按顺序停止接入、取消任务、关闭 Agent、刷新存储并清理进程树」；`docs/LOCAL_ADMIN_PROTOCOL.md:593` 同口径；只读实现佐证 `crates/app/src/daemon.rs:15-17`、`:902`（日志文案「停接入层 → 取消后台任务 → 停止 Agent → 刷新存储 → 释放锁」）、`:1022-1071`（编号 1→5 的实际次序）。`CORE_PORTS_AND_STORAGE.md:1221`（§7.5 第 5 条）还要求 §4.10 的后台任务清单与本口径一致。
- 影响：README 是仓库状态的首屏入口；该句是**行为性**事实错误，并与同一变更内的改正自相矛盾。WP5 交付物的判据正是「文档陈述与实际一致」，因此构成交付条件不成立。
- 最小修复：`README.md:22` 改为「daemon 的启动/关闭序列（停接入层 → 取消周期任务 → 停 Agent → 刷新存储 → 释放锁；见 `SECURITY_DESIGN.md` §12.1）」或直接删除括号内细节；`MODULE_ARCHITECTURE.md:383` 改为同一顺序。

**RV1-WP5-F2（P1，BLOCK）§5 表下注记声称 `windows-local-ipc`「没有行」，实际矩阵有该行**
- 位置：`docs/MODULE_ARCHITECTURE.md:481`（本 diff 整段重写的文本）。
- 事实：注记写「`windows-local-ipc` 只作为『列』出现、**没有行**」。
- 证据：同文件 `:479` 的 §5 矩阵最后一行就是 `| windows-local-ipc |  |  |  |  |  |  |  |  |  |  |  |  | — |`（除自身格外全空白）；门禁会为它建立空的 row map（`scripts/check-crate-boundaries.mjs:55-67`）。
- 影响：注记与其正上方 2 行的表格直接矛盾；未来编辑者可能据此认为「无需维护该行」而漏改。属本轮判据「注释/说明不得声称某行不存在而实际存在」。
- 最小修复：把该子句改成「`windows-local-ipc` 是矩阵列（因 `server` 依赖它），同时在矩阵里也有一行（全空白、自身格 `—`）；它永远不是 workspace 成员，其依赖面不进门禁判定（§3.1）」。

**RV1-WP5-F3（P2）门禁脚本行内注释仍为旧口径**
- 位置：`scripts/check-crate-boundaries.mjs:49-52`（`readDependencyMatrix` 内的注释；本轮 diff 只改了该文件的**文件头**注释 `:12`）。
- 事实：注释写「§5 矩阵当前有 **9 个**『列』…… `node-link-client` / `storage-sqlite` / `server` / `app` **仍只作为「行」存在**」，与实际 13 列（`storage-sqlite`/`server`/`app` 既行又列）及同文件新写的文件头 `:12`（「已加入 `members` 的每个 crate 都必须能在 §5 矩阵里找到对应行与列（切片 4 起 `storage-sqlite`/`server`/`app` 三者都既有行也有列）」）矛盾。
- 影响：行为正确（列取自表头、行取自表格），仅注释误导；但这是 `verification.md` 的 Check Plan Changes 明确划入 WP5 写范围的修复项（RV1-WP1-F2），因此是**范围项未完成**（handoff §5.1 已如实登记为残余）。
- 最小修复：把注释改为「每一行都要收：§5 矩阵当前有 13 个『列』（12 个成员 + `windows-local-ipc`）；`node-link-client` 仍只作为列外行」。
- 备注（同源、不必单独改）：文件头 `:12` 的「必须能找到行与列」是**规范要求**，门禁对「行缺失」只上报不硬失败（`:155` 的 `if (!row) continue`）；建议在同句补「行缺席只上报、不硬失败」，与 §5 注记完全对齐。

**RV1-WP5-F4（P2）`daemon.local_admin.endpoint` 的说明与实现不符（唯一一个「肯定式陈述落空」的配置键）**
- 位置：`docs/CONFIG_REFERENCE.md:51`。
- 事实：文档写「显式值只用于测试或路径冲突排查」，读起来是**当前可用**的运维手段。
- 证据：`crates/app/src/config.rs:240-256` 对任何非 `auto` 的显式值直接失败关闭（`ConfigError::Invalid`，detail「`daemon.local_admin.endpoint` 在本切片只支持 `auto`」），并写明「静默按 `auto` 启动会让管理通道落在与用户配置不同的位置，因此失败关闭」；目标版本日志中该断言已执行：`reports/wp4wp5-final-verify.log:395`（`an_explicit_endpoint_configuration_is_refused_before_any_side_effect ... ok`）。
- 影响：与 WP5 刚为 `daemon.instance_lock` 补的实现状态注记属**同一类**（文档给了取值/用法，本切片实际不可用），但未被标注；`AGENTS.md` §10 要求未实现的设计不得写成已存在。
- 最小修复：在该行末尾补一句与 `instance_lock` 同风格的注记，例如「**当前切片只接受 `auto`**：显式值在启动配置解析阶段即以 `ConfigError` 失败关闭（`app` 配置层），待 `server::transport::local` 提供显式路径入口后再放开」。
- 同条目的次级项（P2，措辞，交调度者决定）：`CONFIG_REFERENCE.md:110` 把 `storage.flush_interval_ms` 描述为「流式事件的批量落盘**间隔**」，而权威定义是 broker 的 **delta 合并提交窗口**、且明确「不是存储刷盘开关」（`CORE_PORTS_AND_STORAGE.md:725`、`:1205`；实现侧 `crates/app/src/config.rs:169-171` 与任务 2.25 的收口口径）。建议改为「同一会话短窗口内 delta 合并为一次 `commit` 的窗口（`CORE_PORTS_AND_STORAGE.md` §6 第 10 条）」。

**RV1-WP5-F5（P2）`DEVELOPMENT_PLAN.md` 措辞与漏改**
- 位置：`docs/DEVELOPMENT_PLAN.md:24`（f39dda9 改写）与 `:77`（未改）。
- 事实：`:24` 写「**本切片切片 4** 已完成 Daemon/CLI 接线与本地管理通道……」（「本切片」重复，且删掉了原句的「端到端验收」剩余项，而同文 `:17` 仍写「本切片剩余的是端到端验收」，两处口径不再一致）；`:77` 仍写切片 1「剩余工作是 Daemon/CLI 接线与端到端验收」，与 `:14`/`:17`/`:24` 已改正的事实相反。
- 最小修复：`:24` 改为「切片 4 已完成 Daemon/CLI 接线与本地管理通道（`server` 的本地适配器与 `app`）；本切片剩余的是端到端验收与 `server::sync`/`server::node_link`/`server::acp_facade`/`node-link-client`」；`:77` 的剩余工作改为「端到端验收」。

**RV1-WP5-F6（P2，WP5 写范围外的同类陈旧句）`LOCAL_ADMIN_PROTOCOL.md:3`**
- 事实：状态行仍写「切片 4（`daemon-cli-and-local-admin`）实现中」，而该通道（`server::transport::local` + `server::local_admin` + `app` 客户端）在 target 上已实现并通过 PV1/PV3/PV4。
- 影响：低（未说「未落地」，只是进度措辞）；`plan.md` WP5 写范围不含该文件（属 WP1 口径），故**不建议**在本次 WP5 修复中越界处理，交主 Agent／6.4 的 DU1 reviewer 决定。
- 最小修复：把「实现中」改为「本地通道部分已落地、`server::acp_facade` 待切片 6」（与 `:133` 注记自洽）。

### 需调度者执行的命令清单（我无 shell）

1. 修复 F1/F2/F3/F5（纯文本，≤6 行）后：`npm run check`（`check:docs` 会重新解析 379+ 相对链接与 4546+ `§` 引用，`check:boundaries`/`check:drift`/`check:agentic` 同跑），期望 `EXIT=0`。**若只改文档**，Rust 侧结论仍可沿用（`crates/**` 自 `82d376d` 未变，见 `reports/wp5-verify.log` 头部与 handoff §4.2），无需重跑 `npm run verify`；一旦有任何 `crates/**` 改动，[PV1]/[PV2] 必须重跑并重登 SHA。
2. 复核 diff 工件保真：`git diff 2791f8b..f39dda9 --stat -- docs README.md AGENTS.md scripts`（应为 8 个文件、249 行，与 `reports/rv-input-wp5.diff.log` 一致）。
3. 核验 §5 注记的历史子句：`git log -S"storage-sqlite" --oneline -- docs/MODULE_ARCHITECTURE.md Cargo.toml`（确认「`storage-sqlite` 列与 `crates/app` 进入 members 同一改动」是否属实）。
4. `git status --porcelain @f39dda9`（复核任务书「worktree 干净」的前提；我无法执行）。
5. 归档前：`tasks.md` 2.19/2.20 复选框与 `verification.md` 的 RV1（3.10）行由主 Agent 维护；本报告不代替 `workflow check`。

### 未覆盖 / 无法验证（明确声明）

1. 无 `git`/`cargo`/`npm`：所有结论基于「diff 工件 + metadata 工件 + 只读文件内容」。聚合测试计数（82 个目标 / 730 passed / 2 ignored）**未逐行重算**；我只验证了 `EXIT` 行、无 `FAILED`/`error[`、无 `warning` 级失败信号。
2. diff 工件可能不完整（未含 `openspec/changes/**`、`crates/**`、`schemas/**`、`fixtures/**`、`compatibility/**`、`Cargo.toml`、`deny.toml` 等）：我无法证明 `docs/README.md/AGENTS.md/scripts` 之外没有需要同步的权威文档改动；`tasks.md`/`verification.md` 的状态未复核。
3. §5 注记里「`storage-sqlite` 的列已随 `crates/app` 进入 members 的同一改动补上」依赖 base 之前的历史，需命令 3 才能判定（不影响 F1/F2 结论）。
4. `crates/**` 仅作为核对文档陈述的**只读证据**阅读；不产出任何代码侧发现（WP4 归 RV1-WP4）。
5. CI-only 判定（`cargo-deny`、`gitleaks`）任何本地运行都不覆盖，仍未验证。
6. 未复核 `.gitignore` 下 gitignored 日志的产出来源（只核对其内部一致性与工件/文件内容的相符性）。

### handoff_index

```yaml
task_id: "3.10"
role: reviewer
phase: work-package review
stage: pre-delivery（WP5 文档收口，交付前）
target_revision: f39dda93c7a60dfe77cb3c268303d68190d3fa04
evidence_type: REVIEW
evidence_id: RV1
report_path: openspec/changes/daemon-cli-and-local-admin/reports/rv1-wp5.md
result: FAIL
evidence_status: NEW
applicability_basis: "独立 review（不继承实现对话）在隔离子 Agent 中完成：只读工具集，读入 rv-input-wp5.diff.log（`git diff 2791f8b..f39dda9 -- docs README.md AGENTS.md scripts`）、rv-input-wp5-metadata.log（cargo metadata/cargo tree 客观事实）、wp4wp5-final-verify.log（target 上 EXIT=0）与 target 工作树文件；发现 F1/F2（P1，BLOCK，均为 ≤1 行文本修复）与 F3–F6（P2）。TARGET 收敛所需：F1/F2 修复后重跑 `npm run check`；无 crates/** 改动时 Rust 侧 [PV1]/[PV2] 结论沿用。"
source_evidence: NOT_APPLICABLE
```