# coder 报告 · WP1（契约与依赖口径冻结，交付单元 DU1）

- task_id: `2.1`、`2.2`、`2.3`（工作包 WP1）
- role: coder
- phase: implement
- stage: work-package
- agent_context: 独立子 Agent（worker / coder 角色），主 worktree 的分支 `feat/daemon-cli-and-local-admin`；只继承本任务的契约输入（design.md、plan.md、`docs/MODULE_ARCHITECTURE.md`、`docs/LOCAL_ADMIN_PROTOCOL.md`、`scripts/check-crate-boundaries.mjs`、`AGENTS.md`）与仓库现状，不继承规划阶段的探索对话；本变更各波次串行，无并发写入者，无 `CARGO_TARGET_DIR` 隔离需求
- target_revision: `b56b829ae2119db77a1907271a5ce799962ac10a`（WP1 的文档/依赖口径提交；同 WP1 另有一提交 `bb3df7e`，见下「提交与交付对应」；本报告由后续的报告提交收录，其父提交即 target_revision）
- base_revision: `889d1feead751a68338753b3e79bdfbed77494ce`（分支起点；基线 main = `ab62773d8768f3c6a8424f6aefa862a738482eb8`）
- scope（写入范围，逐字取自任务单）：`docs/MODULE_ARCHITECTURE.md`、`docs/LOCAL_ADMIN_PROTOCOL.md`（仅 §3.1 加实现状态注记 + 头部状态/版本行）、`Cargo.toml`（`[workspace.dependencies]` + `workspace.exclude`，**未动** `[workspace] members`）、`deny.toml`、`openspec/changes/daemon-cli-and-local-admin/reports/`。**未改**任何 crate 源码、`docs/CONFIG_REFERENCE.md`、以及 `openspec/changes/daemon-cli-and-local-admin/` 下的 plan/tasks/verification/proposal/specs/design
- dependencies（上游输入，无代码依赖）：`design.md` 决策 1/3/5/9、`plan.md` 的 WP1 行与 Contract Changes 节、`docs/MODULE_ARCHITECTURE.md` §3/§3.1/§4.9/§4.10/§5、`docs/LOCAL_ADMIN_PROTOCOL.md` §3.1、`docs/SECURITY_DESIGN.md` §20、`scripts/check-crate-boundaries.mjs`、`AGENTS.md` §7/§8/§10/§12
- result: `PASS`（本工作包的检查全部满足；**不代表**独立 review、E2E 或合并已完成）
- issues: 见「问题与需主 Agent 决策的事项」——1 处需要主 Agent 排期/扩范围的缺列后果（`storage-sqlite` 必须在 `crates/app` 加入 `members` 前升为 §5 的列），1 处 WP2 的 `.gitignore` 归属该由主 Agent 决定
- evidence_paths: `openspec/changes/daemon-cli-and-local-admin/reports/du1-pv1.log`、`wp1-boundaries.log`、`wp1-verify.log`、`wp1-lc1-linux-check.log`、`wp1-dependency-probe.log`、本报告

## 提交与交付对应

| 提交 | 类型 | 内容 | 覆盖任务 |
| --- | --- | --- | --- |
| `bb3df7e` | `build(deps)` | `Cargo.toml`（tokio/nix feature、clap/toml/fs4、`workspace.exclude`）、`Cargo.lock`、`deny.toml` | 2.2 |
| `b56b829` | `docs(server)` | `docs/MODULE_ARCHITECTURE.md`、`docs/LOCAL_ADMIN_PROTOCOL.md` | 2.1 |
| 本报告提交 | `docs(server)` | `reports/wp1-handoff.md`（本文件） | 2.1/2.2/2.3 的交接 |

`git status --porcelain` 在本报告提交前只有：上述文件 + 主 Agent 自己维护的 `openspec/changes/daemon-cli-and-local-admin/tasks.md`（**未触碰、未暂存**）。工作区无未登记散落文件；`reports/*.log` 按仓库约定被 `.gitignore` 排除，不入库。

## 任务 2.1（文档口径）

| 位置 | 改动 |
| --- | --- |
| `docs/MODULE_ARCHITECTURE.md` §3（表后） | 新增 `[现状]`（2026-09-25，切片 4 落地中）：`server` 本切片只含 `transport` 本地通道部分与 `local_admin`，`sync`/`node_link`/`acp_facade` 仍待后续切片；`app` 的 daemon/CLI/组合根本切片落地；新增 `vendor/windows-local-ipc` 目录说明（path 依赖、不在 `members`）；并指出 §5 的列比 §3 的行少是「已知且被门禁拦住」的缺口 |
| §3.1（新增 `[决定]` 条目的一组子条） | 登记本轮依赖口径：tokio 增开 `net`/`io-util`/`signal`（含与 `agent-host` 本地增量 feature 并存不产生歧义的原因）、nix 增开 `socket`/`user`（含「不需要 `net`」的依据）、新增 `clap 4.6`/`toml 1.1`、`fs4 1.1` 的四判据核验与落选 `fd-lock` 的理由、`fs4` 调用点必须全限定（避免落到 std 1.89 的同名方法而抬 MSRV）、`vendor/windows-local-ipc` 的 path 依赖登记 |
| §4.9 | 新增 `[现状]` 注记：只有 `transport::local` 与 `local_admin` 两条路径；facade 缺席期 `0x02` 行为指向 `docs/LOCAL_ADMIN_PROTOCOL.md` §3.1；职责划分不变 |
| §4.10 | 新增 `[现状]` 注记：本切片落地范围（启动/关闭序列、单实例锁与 `instanceId`、配置加载与首次种子导入、周期任务装配、全部 CLI 子命令与 `doctor`/`acp-stdio`）；二维码渲染记「终端不支持」；职责描述不变 |
| §5 依赖矩阵 | 表头新增三列 `server`、`app`、`windows-local-ipc`；**追加在行尾**，因此既有列的单元格索引不变；`server` 行对 `windows-local-ipc` 标 ✓、自身格改 `—`；新增 `windows-local-ipc` 行（自身 `—`、其余空白）；`app` 行对 `server` 标 ✓、自身格改 `—`；其余行只补 3 个空单元格（逐格保持现行值）；表下注记改写为「三列已成列 / `storage-sqlite` 与 `node-link-client` 仍只作为行 / `storage-sqlite` 缺列的后果」 |
| `docs/LOCAL_ADMIN_PROTOCOL.md` 头部 | 状态行改为「编码前契约；切片 4 实现中——实现期差异只允许出现在 §3.1 末尾的实现状态注记里」；新增版本行 `1.2`（保留 `1.1` 历史行） |
| `docs/LOCAL_ADMIN_PROTOCOL.md` §3.1（`**边界**` 之后） | 新增 `**实现状态**` 块：facade 落地前 Daemon 在完成 §3 framing 校验后立即关闭 `0x02` 连接并记结构化警告、不返回错误帧、不转发字节、不分配可用 `FacadeAttachmentId`；`acp-stdio` 遇立即关闭以明确错误非零退出；明确「不改变本节任何语义」，切片 6 只替换分发目标。**未改** §1–§3 与 §4 起的任何语义文字 |

与门禁行为一致性的自检：`scripts/check-crate-boundaries.mjs` 只把 workspace **成员**之间的 path 边拿去比矩阵，列由表头决定；把新列追加在行尾不改变既有列解析，且 `server`/`app` 行新增的 ✓（`server → windows-local-ipc`、`app → server`）与实际 path 依赖一致——`PV2` 已复验（10 个成员全部在矩阵登记、依赖方向一致）。

## 任务 2.2（依赖登记）

`Cargo.toml`：

- `tokio`：`features = ["rt-multi-thread", "macros", "sync", "time", "net", "io-util", "signal"]`（新增 `net`/`io-util`/`signal`）。读 `crates/agent-host/Cargo.toml` 后确认：`agent-host` 在自己 manifest 里增量要 `process`/`io-util`/`macros`/`rt`，workspace 基线登记的是跨 crate 共用面，两处并存不冲突（feature 加法）；单一登记点仍是 `[workspace.dependencies]`。
- `nix`：`features = ["signal", "process", "socket", "user"]`（新增 `socket`/`user`）。核对 `nix 0.30.1` 的源码门控：`sys::socket` 由 `#![feature = "socket"]` 开启，`sockopt::PeerCredentials`（`SO_PEERCRED`）与 `getsockopt` 都不在 `net` 门后 → 不需要 `net`，与 `design.md` 决策 3 的结论一致。
- 新增 `clap = { version = "4.6", features = ["derive"] }`、`toml = "1.1"`、`fs4 = "1.1"`（文件锁）。
- `workspace.exclude = ["vendor/windows-local-ipc"]`；**未动** `members`、**未动** `rust-version = "1.85"`。
- `Cargo.lock` 的唯一增量：`memoffset 0.9.1`（MIT，`autocfg` 已有）。原因：`agent-host` 以 `nix.workspace = true` 消费 baseline，`socket` feature 开启 `memoffset`。`clap`/`toml`/`fs4` 本身**尚未**进入 lock——workspace 里没有成员使用它们，按计划由 WP3/WP4 加入成员时随 `Cargo.lock` 一并出现；这解释了「为什么本次 lock 只多了一个包」。

`deny.toml`：只在 `[sources]` 增加注释块，登记 path 来源依赖的判定归属（`source = null` 不经 `unknown-registry`/`unknown-git` 判定、没有可写的 `allow` 条目），并写明它改由哪三条约束承担（`workspace.exclude` 的成员身份、`[licenses] allow`、`[bans] allow-wildcard-paths` 对 private **依赖方** 的豁免）。**未新增/放宽任何键**（`allow` 列表、`exceptions`、`[bans] deny`、`wildcards` 全部保持不变）。

### 选型核验结论（`docs/SECURITY_DESIGN.md` §20 四判据）

原始证据：`reports/wp1-dependency-probe.log`（探针 crate 位于临时目录、不在仓库内，已删除；复现见该文件头部的命令）。

| 候选 | 版本 | 许可证（`cargo metadata` 的 `license` 字段） | MSRV | 维护状态 | 结论 |
| --- | --- | --- | --- | --- | --- |
| `clap` | 4.6.7 | `MIT OR Apache-2.0` | 声明 `1.85` | 4.6.7 发布 2026-09-14；`clap-rs/clap` | 采用（`derive`） |
| `toml` | 1.1.6+spec-1.1.0 | `MIT OR Apache-2.0` | 声明 `1.85` | 1.1.6 发布 2026-09-10；`toml-rs/toml` | 采用（默认 feature：`std`+`serde`+`parse`+`display`；只读配置，不引入 `toml_edit`） |
| `fs4` | 1.1.0 | `MIT OR Apache-2.0` | 声明 `1.75.0` | 1.1.0 发布 2026-04-28；Windows 侧要求 `windows-sys ^0.61`，与本仓库 lock 中已有的 `0.61.2` 同族 | **采用**（单实例锁） |
| `fd-lock` | 4.0.4 | `MIT OR Apache-2.0` | **未声明**（不可核） | 4.0.4 发布 2025-03-10（最久约 18 个月）；Windows 侧 `windows-sys >=0.52,<0.60` | 落选 |

- 语义（`fs4`，读源码核实）：Unix 走 `rustix::fs::flock(LockExclusive / NonBlockingLockExclusive)`，Windows 走 `LockFileEx(LOCKFILE_EXCLUSIVE_LOCK[, LOCKFILE_FAIL_IMMEDIATELY])` 且锁定整个文件范围（`!0, !0`）；公开 API 是 `FileExt::{lock, try_lock, unlock, …}`，`unsafe` 收敛在 crate 内部 → 与 workspace `unsafe_code = "forbid"` 兼容，且**只有独占锁**，符合 `design.md` 决策 4 的「OS advisory 文件锁互斥」。
- 落选理由（`fd-lock`）：① 未声明 `rust-version`，MSRV 无法核验；② 发版节奏明显更旧；③ API 是读写双分支（`RwLock::read` 走 `LOCK_SH`），共享锁与「单实例锁必须互斥」不匹配（调用方必须自觉只用 `write` 分支），且 Windows 侧只锁 1 字节。
- 闭包级核验（探针 `cargo metadata`，50 个包）：全部许可证落在 `deny.toml` 的 allow 列表内（`MIT`、`Apache-2.0`、`Apache-2.0 WITH LLVM-exception`、`Unicode-3.0` 的 AND 组合如 `unicode-ident` 已被既有依赖使用）；闭包内最高 `rust_version` 为 `1.85.0`（`clap` 系、`toml` 系、`hashbrown`、`indexmap`）→ **不抬高 MSRV**；`cargo generate-lockfile` 输出 `Locking 50 packages to latest Rust 1.85 compatible versions`，与 resolver 3 的 MSRV 感知一致。
- 维护状态证据命令（原始输出未入库，可重放）：`curl https://index.crates.io/<前缀路径>/<crate>`（`rust_version`/`yanked`）、`curl -H 'User-Agent: …' https://crates.io/api/v1/crates/<crate>`（`max_version`/`updated_at`/`versions[].created_at`）。`fs4 1.1.0`：yanked=false。

### 依赖包含关系（本次登记会带进什么）

- `tokio` 新 feature：`net`（Named Pipe/Unix socket safe API）、`io-util`、`signal`——都是 tokio 自身的模块，不引入新的第三方 crate。
- `nix` 新 feature：`socket` → 传递依赖 `memoffset 0.9.1`（MIT，MSRV 未声明；在固定工具链 1.98.1 上编译通过）；`user` → 仅开启 `feature` 模块。仅 Unix 构建生效。
- `clap 4.6.7`：`clap_builder`/`clap_derive`/`clap_lex`/`anstream`/`anstyle*`/`colorchoice`/`strsim`/`is_terminal_polyfill`/`once_cell_polyfill`/`utf8parse`/`syn 3`/`heck`/`windows-sys 0.61`（后者已在 lock 中）。
- `toml 1.1.6`：`toml_datetime`/`toml_parser`/`toml_writer`/`serde_spanned`/`serde_core`/`winnow`/`hashbrown`/`equivalent`/`indexmap`（后两者已在 lock 中）。
- `fs4 1.1.0`：Unix → `rustix 1.x`（+ `linux-raw-sys`/`errno`/`libc`/`bitflags`，均已在 lock 中）；Windows → `windows-sys ^0.61`（无新版本族）。
- 以上判定**只在 CI 的 `deps`/`advisories` job 成立**，本地 `npm run verify` 不是许可证/来源/advisory 的证据（无 `cargo-deny`，`AGENTS.md` §8/§10）。

## 任务 2.3（W0 局部验证）与角色局部检查

| Check ID | 命令（cwd = `D:\Project\acp-remote`） | 退出码 | 日志 | 结论 |
| --- | --- | --- | --- | --- |
| PV1（W0 轮） | `npm run check` | 0 | `reports/du1-pv1.log` | PASS：10 道合同门禁全绿（含 `crate boundaries OK: 10 个 crate`、`doc links OK: 374 relative links, …`、agentic 13/13） |
| PV2（W0 轮） | `node scripts/check-crate-boundaries.mjs` | 0 | `reports/wp1-boundaries.log` | PASS：`10 个 crate 的依赖方向与 §5 矩阵一致（10 个已在矩阵登记）` |
| PV1 的 Rust 半（W0 轮附加证据，非计划内的独立 Check ID） | `npm run verify`（= check + `cargo fmt --all -- --check` + `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` + `cargo test --locked --workspace --all-features`） | 0 | `reports/wp1-verify.log` | PASS：fmt 无差异、clippy 零警告、测试 **515 passed / 0 failed**（13 个 0 用例的既有 test 目标与基线逐项相同，2 个 1 ignored 也是既有项），与 `reports/baseline-cargo-test.log` 逐项等价 → 无回归 |
| LC1（角色局部检查，本 WP 附加） | `cargo check --locked -p agent-host --all-features --target x86_64-unknown-linux-gnu` | 0 | `reports/wp1-lc1-linux-check.log` | PASS：nix 的新 feature 只在 Unix 生效，而 Windows 本机不编译 `nix`，因此额外做 Linux 目标编译校验；`agent-host` Linux 目标 check 通过，`cargo tree --target … -i memoffset` 显示 `memoffset ← nix ← agent-host`，目标目录中存在 `libnix-*.rmeta`/`libmemoffset-*.rmeta` |
| LC2（角色局部检查，本 WP 附加） | 独立探针：`cargo metadata`/`cargo tree`/`cargo build`（见下） | 0 | `reports/wp1-dependency-probe.log` | PASS：`workspace.exclude` 的 path 依赖语义实证 |

LC2 的实证结论（决定 WP2 的 manifest 形状）：把 `vendor/<crate>` 写进 `workspace.exclude` 后，该 crate **不是** workspace 成员（`cargo metadata --no-deps` 的 packages 只有成员），但仍被成员以 path 依赖正常消费（依赖条目的 `path` 有值、`req = "*"`），`cargo build` 成功，且**不需要**在 vendor crate 里再写 `[workspace]`；被排除的 crate 也可以在自己的目录里独立构建（副作用：会在 `vendor/<crate>/` 下生成 `Cargo.lock` 与 `target/`）。

证据适用性说明：`PV1`/`PV2`/`wp1-verify.log` 的日志在 **三份提交的最终内容 + 本报告**（即待收录的同一棵工作树）上重新采集，`target/` 当时已清空，因此是全量重编译；测试结果与基线逐项等价（515 passed / 0 failed，13 个 0 用例目标相同）。`LC1`/`LC2` 只取决于依赖登记提交 `bb3df7e`（文档改动不影响它们），该提交在其后未再改动，故两条日志对该修订及其后的文档改动持续适用。检查后执行过 `cargo clean --target x86_64-unknown-linux-gnu` 做资源释放，副作用是连带清空了本仓库共享的 `target/`（`target/` 被 `.gitignore` 排除，无追踪文件受影响）——**LC1 的交叉目标产物因此已不在磁盘上**，其证据是日志内的命令与输出；复验只需重跑该命令（会重新编译，约数百 MB）。

## 问题与需主 Agent 决策的事项

1. **`storage-sqlite` 缺列会在 WP4 让门禁硬失败（需排期或扩范围，本 WP 不擅自扩范围）**：任务单把 §5 表头新增限定为三列（`server`/`app`/`windows-local-ipc`），同时要求注明 `storage-sqlite` 仍只作为行。但 `scripts/check-crate-boundaries.mjs` 对成员之间的 path 边做 `columns.includes(dependency)` 判定：`crates/app` 一旦加入 `members`（WP4，写范围只含 `crates/app/**` + `members` 一行 + `Cargo.lock`，**不含** `docs/MODULE_ARCHITECTURE.md`），`app → storage-sqlite` 这条 path 边就会因「不在 §5 的矩阵列里」直接报错 → 该波次的 PV1 变红。已按任务单口径把这一后果写进 §5 表下注记（措辞与门禁行为一致），并在注记里点明「该列必须与 `crates/app` 加入 `members` 在同一改动里补齐」。**建议主 Agent 二选一**：①把 WP5 的 §5 收口提前到 WP4 同波次（或把 `docs/MODULE_ARCHITECTURE.md` 的 §5 一行追加进 WP4 的写范围）；②现在就授权本 WP 追加 `storage-sqlite` 列（本角色不擅自做，因为它超出任务单冻结的三列）。本 WP 交付不因此失败。
2. **WP2 的 `.gitignore` 归属**：LC2 显示被排除的 vendor crate 若在本仓库目录里独立 `cargo build`，会在 `vendor/windows-local-ipc/` 下留下 `Cargo.lock` 与 `target/`（根 `.gitignore` 的 `/target/` 只匹配仓库根）。WP1 的写范围不含 `.gitignore`，故未改；建议主 Agent 在 WP2 的写范围里明确「要么不独立构建，要么同一改动补忽略规则」。
3. **`fs4` 的两条调用点约束（交给 WP4 落地）**：① 固定工具链 1.98.1 的 `std::fs::File` 自带 `lock`/`try_lock`（1.89 稳定），与 `fs4::FileExt` 同名且方法解析优先级更高——调用点必须写全限定（`fs4::FileExt::try_lock(&file)`）或显式 `use fs4::FileExt;`，否则会静默用 std 的版本并把 MSRV 抬到 1.89；② `TryLockError` 的形态跨平台不对称：Windows 侧 `fs4` 把 `ERROR_LOCK_VIOLATION` 映射为 `TryLockError::WouldBlock`，Unix 侧返回 `TryLockError::Error(io::Error)`（其 `kind()` 为 `WouldBlock`），CLI 判定「Daemon 是否持有锁」需要同时接受这两种形态。
4. **未执行项（平台/环境限制，不算通过）**：`cargo-deny`（`deps`/`advisories`）与 `gitleaks`（`secrets`）本地没有等价物，本次新增依赖与 `vendor/windows-local-ipc` 的许可证/来源/advisory 判定只在 CI 完成；`[PV3]`/`[PV4]`/`[PV5]` 不适用于本 WP（对应 WP3/WP4）；`[PV2]` 的 W0 轮只核对既有成员（`server`/`app` 尚未加入 `members`），成员加入后需按计划重跑。
5. **待补的独立证据**：`RV1`（WP1 的独立 review，必须由不继承实现对话的执行者完成）尚未返回；本报告不把它写成 PASS。

## checks（逐 Check ID）

| Check ID | 命令 / 目录 | 配置与环境 | 结果 | 日志 |
| --- | --- | --- | --- | --- |
| PV1（W0 轮） | `npm run check` @ 仓库根 | Node v24.19.0 / npm 12.0.2；不联网；`scripts/check-crate-boundaries.mjs` 需本地 `cargo`（1.98.1） | 退出码 0；10 道门禁逐项通过 | `reports/du1-pv1.log` |
| PV2（W0 轮） | `node scripts/check-crate-boundaries.mjs` @ 仓库根 | 同上 | 退出码 0；10 个成员依赖方向与 §5 一致 | `reports/wp1-boundaries.log` |
| PV1 的 Rust 半（附加） | `npm run verify` @ 仓库根 | `rust-toolchain.toml` 固定 1.98.1；`--locked`；Windows x64 | 退出码 0；测试 515 passed / 0 failed，与基线逐项等价；clippy 零警告 | `reports/wp1-verify.log` |
| LC1（附加） | `cargo check --locked -p agent-host --all-features --target x86_64-unknown-linux-gnu` @ 仓库根 | 已安装 `x86_64-unknown-linux-gnu` std；仅对 `agent-host`（nix 的唯一消费方）做目标编译 | 退出码 0；`memoffset ← nix ← agent-host` | `reports/wp1-lc1-linux-check.log` |
| LC2（附加） | 临时探针 crate：`cargo generate-lockfile` / `cargo metadata` / `cargo build`（探针根 + `vendor/<crate>`），见日志头部 | 探针在系统临时目录，已删除；不影响仓库 | 退出码 0；`exclude` 语义与 MSRV/许可证闭包数据如上 | `reports/wp1-dependency-probe.log` |

## 资源释放

| 资源 | 归属 | 状态 |
| --- | --- | --- |
| 依赖探针 crate（`%TEMP%` 下两个临时 workspace） | 本 WP | **已删除**（`node fs.rmSync`，删后逐路径确认不存在） |
| 交叉编译产物 `target/x86_64-unknown-linux-gnu/`（763 MB） | 本 WP | 已通过 `cargo clean --target x86_64-unknown-linux-gnu` 释放；副作用：共享的 `target/` 被一并清空（`.gitignore` 排除，无追踪文件受影响） |
| 仓库 `target/`、临时数据目录、pipe/socket | 各 WP | 本 WP 未创建需要独占的运行时资源；后续检查在共享 `target/` 上串行执行 |

```yaml
handoff_index:
  - task_id: "2.1"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "b56b829ae2119db77a1907271a5ce799962ac10a"
    evidence_type: CHECK
    evidence_id: PV1
    report_path: openspec/changes/daemon-cli-and-local-admin/reports/wp1-handoff.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "W0 轮次：crate 成员未加入（crates/server、crates/app 均不在 members）时执行；命令与任务单 2.3 逐字一致（npm run check）；Node v24.19.0 / cargo 1.98.1 / Windows x64；日志 reports/du1-pv1.log（该文件与 PV2 全量覆盖写，属于变更目录内的工作区证据）"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.2"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "bb3df7e98813d96289661458c883bd23cdd2cc28"
    evidence_type: CHECK
    evidence_id: PV2
    report_path: openspec/changes/daemon-cli-and-local-admin/reports/wp1-handoff.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "W0 轮次只核对既有成员：10 个 workspace 成员的 path 依赖边与 §5 矩阵（含新增三列）逐条一致；命令 node scripts/check-crate-boundaries.mjs @ 仓库根；本地 cargo 1.98.1；日志 reports/wp1-boundaries.log"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.3"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "b56b829ae2119db77a1907271a5ce799962ac10a"
    evidence_type: VALIDATION
    evidence_id: LC1
    report_path: openspec/changes/daemon-cli-and-local-admin/reports/wp1-handoff.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "本 WP 附加的 Linux 目标编译校验：证明只对 Unix 生效的 nix socket/user feature 在目标平台可编译（Windows 本机不编译 nix，故该等价性无法在默认 target 上得到）；日志 reports/wp1-lc1-linux-check.log；注意交叉目标产物随后已清理，复验需重跑命令"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.2"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "bb3df7e98813d96289661458c883bd23cdd2cc28"
    evidence_type: VALIDATION
    evidence_id: LC2
    report_path: openspec/changes/daemon-cli-and-local-admin/reports/wp1-handoff.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "本 WP 附加的依赖选型探针：clap/toml/fs4/fd-lock 的许可证与 MSRV（cargo metadata 的 license/rust_version）、workspace.exclude 的 path 依赖语义（cargo metadata/build）；探针 crate 位于系统临时目录、已删除，原始输出在 reports/wp1-dependency-probe.log"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.1"
    role: reviewer
    phase: review
    stage: work-package
    target_revision: "b56b829ae2119db77a1907271a5ce799962ac10a"
    evidence_type: REVIEW
    evidence_id: RV1
    report_path: openspec/changes/daemon-cli-and-local-admin/reports/rv1-wp1.md
    result: BLOCKED
    evidence_status: PENDING
    applicability_basis: "WP1 的交付前独立 review 尚未返回；必须由不继承实现对话的执行者按 roles/reviewer.md 对 b56b829（+ bb3df7e）检视。本报告不把自检当作 RV1 结论"
    source_evidence: NOT_APPLICABLE
```
