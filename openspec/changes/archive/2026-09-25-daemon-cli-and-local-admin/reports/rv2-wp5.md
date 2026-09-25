# RV2-WP5 · 复核检视报告（WP5 修复后复核，作用于候选 90c816a）

> 由 reviewer 子 Agent 输出工件原样落盘（运行 60063f3c），主 Agent 未改动结论。
> 复核对象：RV1-WP5 的 F1–F6（修复见 2.27 / b4f7f81）。

I have completed the recheck. All required artifacts were read (RV2 diff, metadata facts, RV1-WP5, wp427 handoff, plan/verification/tasks records) and each F-item was verified against the **target-version** files, the authoritative docs, and `crates/app` implementation.

## Review

**result: PASS**（RV1-WP5 的 F1–F6 全部已解决；无阻塞项。新发现 2 条 P2 措辞/指向类问题，均不影响行为或判据，建议合并前顺手修）

Merge verdict: **OK with notes**

### 逐项复核（F1–F6）

**RV1-WP5-F1（关闭顺序事实错误 — BLOCK）→ 已解决**
- target 位置：`README.md:22`、`docs/MODULE_ARCHITECTURE.md:384`。
- README:22 现为「停接入层 → 取消周期任务 → 停 Agent → `wal_checkpoint(TRUNCATE)` → 释放锁；详见 `SECURITY_DESIGN.md` §12.1」；§4.10 为「停接入层（排空在途连接至宽限上限）→ 取消周期任务与信号监听 → 停 Agent → `wal_checkpoint(TRUNCATE)` → 清理 endpoint/释放锁」。
- 判据：与 `docs/SECURITY_DESIGN.md:353-358`（§12.1「按顺序停止接入、取消任务、关闭 Agent、刷新存储并清理进程树」）、`docs/LOCAL_ADMIN_PROTOCOL.md:593`、`docs/CORE_PORTS_AND_STORAGE.md:1220`（§7.5 第 4 条，本变更刚改写并明写「不是先于接入层」）逐条一致；只读实现佐证 `crates/app/src/daemon.rs:15-16`（模块头）、`:902`（关闭日志文案）、`:1007-1077`（编号 1→5 的实际次序：停接入/drop endpoint → drain → cancel_all → 停 Agent → checkpoint → 清 endpoint/释放锁）、`crates/app/tests/daemon_lifecycle.rs:429`（R13 断言注释同序）。
- 残留：同一句新增的章节指向错误，见 RV2-WP5-F1。

**RV1-WP5-F2（§5 注记称 `windows-local-ipc` 没有行 — BLOCK）→ 已解决**
- target 位置：`docs/MODULE_ARCHITECTURE.md:482`。
- 现文：「`windows-local-ipc` 是矩阵的**列**（因为 `server` 依赖它），在矩阵里**也有一行**（该行除自身格外全空白…）；…其依赖面不进门禁判定，因此那一行只是占位、不需要维护」。
- 判据：矩阵确有该行（`:480`，仅自格 `—`）；列确在其表头（`:465`，13 列）；门禁只遍历 workspace 成员（`scripts/check-crate-boundaries.mjs:141-160`），`vendor/windows-local-ipc` 在 `Cargo.toml` 的 `workspace.exclude`（members 12 个，与 metadata 工件一致），故其行确实不参与判定 → 陈述成立。
- 残留：括注「没有任何依赖方」措辞，见 RV2-WP5-F2。

**RV1-WP5-F3（门禁脚本行内注释旧口径 — P2）→ 已解决**
- target 位置：`scripts/check-crate-boundaries.mjs:49-52`（本轮 diff 改的就是这三行）+ 文件头 `:12`。
- 现文：13 个列（12 成员 + `vendor/windows-local-ipc`）、`node-link-client` 仍只作为列外行、并新增「行缺席只上报、不硬失败（下面的 `if (!row) continue`）」。
- 判据：13 列由表头逐人数得（`:465`）；`node-link-client` 只在行集合中、无人依赖它（`:474` 行全是空白集合外的 ✓）；`if (!row) continue` 实存于 `:149`，与注记口径一致 → 注释与行为、与文档三者自洽。

**RV1-WP5-F4（endpoint 未标注 auto-only；`flush_interval_ms` 描述 — P2）→ 已解决**
- `docs/CONFIG_REFERENCE.md:51` 末尾新增「**当前切片只接受 `auto`**：显式值在配置解析阶段即以 `ConfigError` 失败关闭（`crates/app/src/config.rs`），待 `server::transport::local` 提供显式路径入口后再放开」——与 `crates/app/src/config.rs:246-259`（非 `auto` → `ConfigError::Invalid`，detail「在本切片只支持 `auto`」，注释理由「静默按 auto 启动会让管理通道落在与用户配置不同的位置」）逐字对应，且与 `LOCAL_ADMIN_PROTOCOL.md` §2.1 不冲突（该节未声称显式路径可用）。
- `docs/CONFIG_REFERENCE.md:110` 改为「同一会话短窗口内 delta 合并为一次 `commit` 的窗口（…由组合根定时器枚举非终态会话并逐个驱动 `Broker::pump`；§6 第 10 条；**不是**存储刷盘间隔）」——与 `CORE_PORTS_AND_STORAGE.md:725`（§6 第 10 条）、`:1205`（§7.5 表）、`crates/app/src/config.rs:169-176`（doc comment「它不是存储刷盘开关」）一致；实现佐证 `crates/app/src/daemon.rs`（`spawn_merge_window`：`tokio::time::interval(flush_interval_ms)` → `sessions()`（`MERGE_WINDOW_STATES`）→ `broker.pump`）。

**RV1-WP5-F5（`DEVELOPMENT_PLAN.md` 重复措辞 + 漏改旧句 — P2）→ 已解决**
- `docs/DEVELOPMENT_PLAN.md:24` 现为「切片 4 已完成 Daemon/CLI 接线与本地管理通道…本切片剩余的是**端到端验收**（`server::sync`/`server::node_link`/`server::acp_facade` 与 `node-link-client` 属后续切片）」（重复的「本切片切片 4」已消失）；`:77` 现为「Daemon/CLI 接线也已随切片 4 落地，剩余工作是端到端验收」，与 `:16`/`:17` 口径一致。

**RV1-WP5-F6（`LOCAL_ADMIN_PROTOCOL.md` 状态行「实现中」— P2）→ 已解决**
- `docs/LOCAL_ADMIN_PROTOCOL.md:3` 现为「切片 4 的**本地通道部分已落地**（`server::transport::local` + `server::local_admin` + `app`），`server::acp_facade` 待切片 6」——与 §3.1 实现状态注记（`:133-135`）及 `MODULE_ARCHITECTURE.md:353` 自洽。

### 无新引入的不一致（第 2 项要求）
- 关闭顺序/周期任务：文档 ↔ §12.1/§7.1/§7.5 ↔ `daemon.rs` ↔ `daemon_lifecycle.rs:429` 四处一致。
- §5 注记：13 列 = metadata 的 12 个成员 + `vendor/windows-local-ipc`；§3 的 12 个成员陈述与 metadata 工件的 12 行逐名一致；`node-link-client` 仍不在 `crates/`（`find` 无此目录、矩阵无列）。
- 未发现新增过度声称：`README.md:21-24/30`、`MODULE_ARCHITECTURE.md:3/128/226/353/375`、`DEVELOPMENT_PLAN.md:14/24`、`CORE_PORTS_AND_STORAGE.md:11/1312`、`LOCAL_ADMIN_PROTOCOL.md:3` 全部把 `server::sync`/`server::node_link`/`server::acp_facade`、`node-link-client`、前端写成未落地，且已落地陈述都带「仅切片 4 范围/两条路径」限定与两处合同例外（`import.add` → `local.unavailable`、`node pair --mode access` → `local.unsupported`）。

### 第 3 项：`plan.md` 证据路径
- `wp4-app-cli.log` 的 17 处（`:105/113/482/490/498/505/513/520/528/536/543/551/559/566/574/582` + PV4 行 `:676`）已全部改为 `reports/wp4b-cli.log`；`plan.md` 内该字符串**零命中**。
- 全仓残余命中仅在写范围外的历史/登记工件，且都是「描述缺陷本身」或「记录修复」：`reports/rv1-wp4.md:81/85`、`reports/wp427-handoff.md`、`tasks.md:46`（2.27 任务书正文）、`verification.md:68/125/159`；`tasks.md:24-26`（2.15–2.17 原任务定义）现已读 `reports/wp4b-cli.log`。
- PV4 证据列（`plan.md:676`）四条路径指向**真实文件**（`openspec/changes/daemon-cli-and-local-admin/reports/` 下 `wp4-app-daemon.log`、`wp4b-cli.log`、`wp425.log`、`wp426.log` 均存在）。注意：它们按 `.gitignore` 的 `*.log` 与 `openspec/changes/**/reports/**/*.log` 策略**不入库**，因此「存在」是工作区事实、不是提交内容（项目既定策略，非缺陷）。

### 新发现

**RV2-WP5-F1（P2，建议合并前修，1 词）** — `MODULE_ARCHITECTURE.md:384` 的新增括注把关闭顺序的权威指到了错的章节号
- 位置：`docs/MODULE_ARCHITECTURE.md:384`…「（与 `CORE_PORTS_AND_STORAGE.md` §7.1 第 4 条、`SECURITY_DESIGN.md` §12.1 一致）」（该括注为 F1 修复时**新增**）。
- 证据：`CORE_PORTS_AND_STORAGE.md` §7.1（`:757-766`）的第 4 条是 `:763`「**权限**：Unix 目录 `0700`、数据库/WAL/`-shm`/附件 `0600`」，与关闭顺序无关；关闭顺序条目在 **§7.5 第 4 条**（`:1220`）。同小节 `:379` 已正确写「时间与顺序判据见 `CORE_PORTS_AND_STORAGE.md` **§7.5**」，可见是笔误而非口径差异。
- 影响：读者按该指向核对关闭顺序会读到权限条款；`check:docs` 只解析相对链接与 `§` 是否可归因，不会判小节号。
- 最小修复：把 `§7.1 第 4 条` 改为 `§7.5 第 4 条`（或直接删掉该指向，只保留 `SECURITY_DESIGN.md` §12.1）。
- 同源提示（不在本 diff 内、不构成新引入）：`crates/app/src/daemon.rs:660` 注释同样写「（§7.1 第 4 条）」；`verification.md:115`、`reports/rv1-wp4.md:67` 亦沿用该指向。已改写前的 §7.5 第 4 条原文末尾正带「（§7.1）」（见 `reports/rv-input-wp5.diff.log:95`），这很可能是这处指针漂移的来源——若要统一，建议由主 Agent 一次决定「§7.5 第 4 条 + §12.1」为正式指向。

**RV2-WP5-F2（P2，措辞）** — §5 新注记括注的术语与同表事实相反
- 位置：`docs/MODULE_ARCHITECTURE.md:482`：「在矩阵里**也有一行**（该行除自身格外全空白——**没有任何依赖方**）」。
- 证据：本段自己定义「列 = 可被依赖的对象，行 = 发起方」，行空白表示「该 crate 不依赖任何对象」；而「没有任何依赖方」表示「无人依赖它」，与同表 `:478` 的 `server` 行在 `windows-local-ipc` 列上有 `✓` 直接矛盾（该列恰有一个依赖方）。
- 最小修复：改为「——它自己不依赖任何 crate」或「（该行全空白，表示它没有依赖对象）」。

### 需调度者执行的命令（我无 shell）
1. `npm run check` @ `90c816a`（**必跑**）：候选比最后一次门禁留证晚一个提交——`reports/wp427.log:327` 是 `EXIT(npm run check @ b4f7f81)=0`，而候选 `90c816a`（reflog：“docs(app): 登记 2.27 交付与两项裁定”）改动了 `openspec/changes/**`，须在候选 SHA 上重跑并重登。
2. `npm run verify`（或最小化 `cargo test --locked --workspace --all-features` / `cargo test --locked -p app --all-features`）@ `90c816a`，用于把 [PV1]/[PV4] 证据落到候选 SHA：`reports/wp427.log:439` 是 `EXIT(cargo test -p app --all-features @ b4f7f81)=0`（50+1+11+10 passed / 0 ignored），候选与 `b4f7f81` 之间只有文档提交，代码等价，但需按 check plan 重新留证。
3. 若采纳 RV2-WP5-F1/F2（纯文本 ≤2 处）：重跑 `npm run check`；无 `crates/**` 改动则 Rust 侧结论可沿用。若同时改 `crates/app/src/daemon.rs:660` 的注释，属注释级改动，仍需 `cargo fmt --check`/`clippy` 复跑。
4. CI-only（本地无等价物，务必别写成已通过）：`deps`（`cargo-deny`：`rpassword 7.5.4`/`rtoolbox 0.0.6` 的许可与来源）、`advisories`、`secrets`（gitleaks），以及 Linux `checks` 里 `#[cfg(unix)]` 路径。
5. 归档前：`tasks.md` 3.10/2.27 复选框与 `verification.md` 的 RV2 行由主 Agent 维护；本报告不代替 `workflow check`。
6. 我无法执行 `git status --porcelain @90c816a`：任务书「worktree 干净」的前提未由我复核（`find` 工具受 `.gitignore` 影响看不到 `.log`，故我先用 `ls` 确认了证据文件在盘上）。

### 未覆盖 / 无法验证（明确声明）
1. 无 shell/git/cargo/npm：所有结论基于「RV2 diff 工件 + metadata 工件 + target 工作树文件 + 既有报告」，聚合测试计数未逐行重算（只核 `wp427.log` 的 `EXIT(...)` 与 `test result: ok` 行）。
2. **metadata 工件是否需要重采**：工件生成于 `f39dda9`；我在 target 直接读了根 `Cargo.toml`（members 恰为工件的 12 个、`exclude = ["vendor/windows-local-ipc"]`）、`Cargo.lock`（`rpassword 7.5.4`、`rtoolbox 0.0.6`，`app` 依赖含 `rpassword`）与 `crates/app/Cargo.toml`（该文件在 diff 中仅注释变化），因此工件的「12 成员 / core 闭包 / app-server 直接依赖」三类事实在 `90c816a` 仍成立，**不必重采**。唯一无法覆盖的是 diff 工件范围外（根 `Cargo.toml`/`Cargo.lock`）若有并列改动——我已用 target 文件内容替代核对。
3. `rpassword`/`rtoolbox` 的 registry 级事实（许可证单/双、`rust-version = 1.85`、索引 pubtime）我**无法离线独立复核**，仅能引用 `reports/wp4b-handoff.md` §10 的实测结论；`deny.toml:88` 确含 `Apache-2.0`，`MODULE_ARCHITECTURE.md:147` 的版本号与 `Cargo.lock` 一致。
4. §5 注记里「`storage-sqlite` 的列随 `crates/app` 进入 members 的同一改动补上」属历史声明，需 `git log -S` 才能判定（不影响 F1/F2 结论）。
5. `plan.md` 的 RV1 行把 `rv1-du1.md` 写在 `openspec/changes/.../reports/` 前缀下，而该文件实际在仓库根 `reports/`（该行未在本 diff 内、非本轮修复目标；仅登记，交主 Agent 决定是否顺手改）。
6. `openspec/changes/daemon-cli-and-local-admin/specs/daemon-lifecycle/spec.md:75` 仍写「按配置间隔的**存储批量刷盘**」，与刚收敛的「broker 合并窗口」口径不一致——**写范围外**（不在本变更 docs/README/scripts 修复面），仅登记给主 Agent 判断（同 2.27 处置 WP4-F2/F4 的同一族措辞）。
7. 同类低价值观察（不作发现）：`MODULE_ARCHITECTURE.md:375` 的 `[现状]` 行仍写「周期任务（清理/刷盘）装配」——该行在 `f39dda9` 只改了「落地中→已落地」，措辞为既有；合并窗口的 `commit` 确实落盘，故不构成事实错误，仅与 `:383` 的新措辞略不同调。

### handoff_index

```yaml
handoff_index:
  task_id: "3.10 复核轮 / 6.4 候选检视"
  role: reviewer
  phase: work-package recheck
  evidence_type: REVIEW
  evidence_id: RV2
  report_path: openspec/changes/daemon-cli-and-local-admin/reports/rv2-wp5.md
  result: PASS
  evidence_status: NEW
  target_revision: 90c816a6f88fb7925c84fbca269b1c946745e9e3
  applicability_basis: >
    结构只读 reviewer（fresh 上下文，仅 read/grep/find/ls/watchdog_diff）在候选 90c816a 的固定工作树上复核 RV1-WP5
    F1–F6：逐项读 target 版本文件并与 §12.1（SECURITY_DESIGN.md:353-358）、§7.1/§7.5（CORE_PORTS_AND_STORAGE.md:763/:1220）、
    §6 第 10 条（:725/:1205）、实现（crates/app/src/daemon.rs 关闭序列与合并窗口、config.rs 端点失败关闭）与
    cargo metadata 工件逐条对照 → F1–F6 全部「已解决」；plan.md 证据路径修正彻底（plan.md 内 wp4-app-cli.log 零命中，
    PV4 四条证据文件均在盘）。新发现 2 条 P2（§7.1 第 4 条应为 §7.5 第 4 条的指针笔误；§5 注记「没有任何依赖方」术语与
    server 行 ✓ 相反），均不阻断。TARGET 收敛所需：在候选 SHA 重跑 npm run check（+ 如需重登 PV1/PV4 证据）；
    采纳两条 P2 后仅需重跑 npm run check。
  source_evidence: NOT_APPLICABLE
```

说明：本任务为只读检视，我没有写入能力；`report_path` 指向的 `rv2-wp5.md` 由运行时落盘本报告全文（我未创建/修改任何仓库文件，也未写 `progress.md`——review-only 优先）。