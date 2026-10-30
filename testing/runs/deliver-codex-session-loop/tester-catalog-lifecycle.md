# WP2 catalog/lifecycle 独立验收

日期：2026-10-30。执行者：原独立 tester。结论：本冻结版的领域/真实 SQLite 专项与独立边界探测通过，未发现新增存储层阻断；不是所有 targets、实际 owner/gate、Harness 或 CL 整体通过。NET-I001 保持既有独立问题，不因单次重跑变绿关闭。

## 固定输入与环境

- 候选：`C:/Users/jack/.codex/worktrees/codex-session-catalog/acp-remote`，HEAD 基线 `65b46c01d6b95fc1f1a8223b1c92a729a885c185`。
- manifest：`.tmp/wp2-catalog-logs/catalog-v1-manifest.json`，SHA256 `ABE3FEFC1A4170414B8F2B4FCE577AAA91ED822EDF4D36FA33E5172353537A57`。11 owned files、全部44份 app/src 源码/说明逐文件首尾匹配；没有复制候选 lib/Cargo 到主目录。manifest 中 unexpectedBaselineChanges 为空；本次亦验证其8份日志摘要一致。
- 主 D 合同 §10 SHA256 `828A97CB8C2F2765CD5D116C2777D53A456CA7C5775E6B2DB5C57FC28DC19738`；tests.md `5FD9D9975EB1CB0C29694339489705C1ECA474AE6E3D5FC9DAE8C9759B3275F7`。未修改产品、规划或验收预期，保护现有 Git 工作区修改。
- Windows，rustc 1.98.1、cargo 1.98.1。实际执行 `testing/ci/windows-owner.ps1`，报告当前进程与 CI 新对象 owner 一致。独立构建，不复用 coder target；`CARGO_TARGET_DIR=D:/Project/acp-remote/.tmp/catalog-independent/target`，TEMP/TMP 均为本次专用目录。无依赖安装；专项使用 --locked，外部工程 --offline 只解析已有缓存（外部工程独立 Cargo.lock）。
- 首尾44源列表：`D:/Project/acp-remote/.tmp/catalog-independent/before.sha256`、`after.sha256`，两者 SHA256 均为 `353C8EE1C316A63054E050C72DA7E33B92BFC8651C84EA3828FCE29A73CD8F05`，包括11 owned files完整摘要。

## 实际执行

工作目录为候选仓库；命令前应用上述 owner 前置和隔离环境。所有长命令有界轮询，原始输出及独立 exit 文件保存在本次目录。

| 命令/入口 | 实际结果 | 范围 |
|---|---|---|
| `cargo test --manifest-path app/Cargo.toml --locked --lib session_storage:: -- --nocapture` | exit 0，52 passed / 0 failed，166 filtered，2.11s；独立编译29.34s | catalog、输入、历史、审批的真实 SQLite 测试；包含 catalog 16项 |
| `cargo test --manifest-path app/Cargo.toml --locked --lib application::history::tests -- --nocapture` | exit 0，22 passed / 0 failed，196 filtered，8.06s | 历史协调异步层；其输出 owner 是测试替身 |
| `cargo test --manifest-path D:/Project/acp-remote/.tmp/catalog-independent/Cargo.toml --offline --lib -- --nocapture` | exit 0，5 passed / 0 failed，1.39s；独立外部编译14.89s | 4个实际场景测试 + 1个直接调用时无操作的子进程辅助入口；不能报成5个业务场景 |
| 外部 `cargo check ... --offline --example forge_permit` | 预期 exit 101，E0451 | LifecyclePermit 的7个字段均为 private，外部不能直接伪造跨 session/claim/state/version permit |
| 外部 `cargo check ... --offline --example forge_resolved` | 预期 exit 101，E0451 | ResolvedWorkspace 两个字段 private，不能绕解析器直接构造 |

外部探测源码：`D:/Project/acp-remote/.tmp/catalog-independent/src/lib.rs`，SHA256 `6E860339BFBFD06C2DD19F64826CB6406CCAFC44C374C2AE9D4689CEAD79C483`。两个刻意不编译的 example 为负向封装探测，正常回归入口明确使用 `--lib`。

## 断言核验与独立补充

1. **CAT-T001，竞争/作用范围：PASS（SQLite）。** 现有测试实际采用两个 Storage/独立连接池和 Barrier，验证同动作重试只有一个定义/一次 claim。本外部工程另以两个连接池 Barrier 竞争同 actor/request 的 workspace.register 与 harness.register：恰一成功，另一个 RequestConflict；一条命令、一条审计、零假 session。另一个已受信 actor 仍不能认领原 actor 的 create；随后两个池对原 actor 同时 claim，恰一拿到 permit。现有测试覆盖原认证代数与新认证代数均不能认领旧命令；typed ledger 在 input/approval/catalog/四生命周期动作之间冲突，状态变化后原请求仍返回同结果；100000容量不淘汰旧结果。
2. **CAT-T002，原子事实与真实 COMMIT 故障：PASS（SQLite）。** 现有断言验证登记定义、Command、审计、node revision 同事务，audit 中断及 deferred FK 导致真正 COMMIT 失败均零残留；create 原子写 session/state/operation/command。外部另在 native_bindings INSERT 后触发 deferred FK 缺失，真实 report COMMIT 返回错误；绑定不存在、session仍creating、Command仍dispatching、审计计数不变、guard零行，不再次 claim。显式 recovery 后 Command为delivery_unknown。不是单纯模拟返回错误。
3. **CAT-T003，四动作重启：PASS（SQLite + 真实自有子进程异常退出；原生结果是替身）。** 现有生命周期恢复用例12组合中第三窗口是在 report 已提交后，不能冒称结果持久化前崩溃。本外部工程补 create/cancel/release/resume × accepted后/claim后/已构造原生结果但未调用report 三窗口，共12个独立子进程，均通过 `process::exit(93)` 非正常退出，不依赖析构或优雅close。父进程等待每个子进程exit93后重开实际库，连续两次恢复：原Command变delivery_unknown、session interrupted、不可重新claim；原request重试同Command/未知结果，无新命令；原binding完整保留。phase2构造的是固定原生结果fixture，没有运行真实Harness，不证明它实际执行过。旧permit迟到回报被拒由已独立运行的现有测试覆盖；外部子进程不能保留内存permit，未冒称跨进程递送旧permit。
4. **CAT-T004，release/resume/CAS：PASS（存储端口事实）。** 外部独立逐一仅缺 history_flushed、resources_closed、references_reclaimed 三条件，均保持releasing、绑定version1不变；新明确release请求三条件全真后才可released。resume向不同scope/替代ID回报失败，保留resuming和原ID/version。现有测试覆盖新bindingVersion使旧结果InvalidState、create未绑定前不loaded、取消保持foreground直到终态；未知cancel由owner确认旧资源结束后可继续管理，但 input/cancel Command仍delivery_unknown。原生强刷/关闭的实际真假由本层调用者提供，本测试没有证明真实资源已结束。
5. **CAT-T005，审批失效与恢复：PASS（已执行现有 SQLite 断言）。** cancel/release在审批决定前后及delivery claim前后使旧source/claim失效，`claim_current`不再有效；decision原请求仍可重放既有Command。恢复事务先执行catalog recovery、再审批source recovery，并保留未知派发事实。实际writer、client_gate与owner锁序未接线，不将数据库claim_current断言称为实际写入已被阻断。
6. **CAT-T006，可信解析/报告封装：部分覆盖。** 已读Resolver验证节点路径、非秘密config key、固定程序/配置摘要、未报告时unverified；现有运行测试验证错误摘要不接受能力报告。两项外部编译失败证明构造封装。`ActorContext`本身不是资源owner；公共内部存储端口仍由上层提供可信调用来源，`claim_lifecycle`/`report_lifecycle`、`record_harness_evidence`及`reconcile_unknown_run`尚不构成实际资源owner认证。客户端不能访问这些端口的真实接线必须由后续阶段验证，本轮不签该边界通过。

## 证据与限制

本次原始日志 SHA256（目录均 `D:/Project/acp-remote/.tmp/catalog-independent/`）：

| 文件 | SHA256 |
|---|---|
| storage.log | `98E606B40505DCBCA55C9F588699D05385D6AE9A6D4CCD0F6EA2E0BE39776162` |
| external.log | `DDA0C2554A468DC4875EB8B0AB386D30872EC7ACA0C6FE15D6B0BCD485F3C1AE` |
| history.log | `139B0E3783B2BED862E83C57EDDDC961B41B5E815287EDE65DA213A5E059FEFD` |
| forge-permit.log | `C01AE3283B3CEF0AA29DDB6BA7060FCBFE2E5EF0003BD1A177253EBA6DD8D403` |
| forge-resolved.log | `15D69149932E5D427DF0D277747A6BA29ECAA57F9A9764E434628055FF1B7CC0` |

- **NET-I001：OPEN，既有独立网络故障。** manifest明确之前allTargets为216 passed / 1 failed，Windows10054网络关闭问题。manifest中的单次network-recheck日志随后通过，不覆盖该失败；根另已独立复现5/80并交专人修复。本tester未重跑网络碰绿、未签all-targets PASS。当前专项过滤数量反映218个lib入口，不将早先全量217入口的历史日志说成当前完整覆盖。
- 未重复全量、JWS、fmt/clippy；manifest中相应日志摘要已核，属于开发同版证据，不冒充独立执行。schema迁移/拒绝不识别schema2布局的真实测试在本次52项范围内通过。
- 未执行实际Harness、共享ACP实例、实际原生create/resume/cancel、真实强刷及资源关闭、真实client_gate/issuer/writer、安全/供给链/Web、读取租约或最终CL004/014–022/031/036/042端到端验收。未签CL整体或最终交付。
- 12个子进程均由本测试创建并同步等待exit93；无按名称全局kill、无用户配置/库/凭据操作。自有fixture数据库与日志留在专用.tmp作证据，产品源码未改；独立checker首尾44源完全一致。报告之外只新增自有.tmp资源。

可交原reviewer复核同版并由主执行按owned files逐hash集成；不能覆盖主D已集成的平台/ACP或整体lib/Cargo。

## catalog-v2 / CAT-R001 独立复测追加

2026-10-30，新冻结manifest `.tmp/wp2-catalog-logs/catalog-v2/manifest.json` SHA256 `1879B7FF362CBD66D2ED0DB6DA1E6A6282DFCE68B967B3F10BB2BE4649BC71E9`。旧ABE manifest仍匹配原摘要，旧结论/NET失败保留。新44源及11owned首尾逐文件一致（其中5owned相对v1变化）；`D:/Project/acp-remote/.tmp/catalog-independent/v2/before.sha256`、`after.sha256` SHA256均 `E21073282D2C4EDE726099B2E958FE3FE4E8881F0E42D6771FBF0133CE1F992B`。新应用入口明确要求 raw 请求进入 CatalogRegistrationService，先复核actor/原始参数摘要/namespace，再对Fresh执行Resolver，最终短事务二次防重。

真实owner前置、原独立target、新v2日志/TEMP，实际独立执行：

- 原storage专项同命令：56 passed / 0 failed、166 filtered，2.03s，exit0；覆盖新增4项重放测试。
- 外部新增 `cargo test --manifest-path .tmp/catalog-independent/Cargo.toml --offline --test replay -- --nocapture`：3 passed / 0 failed，0.10s，exit0。真实自有exe改字节后原请求仍同Command；删除exe并撤销配置/让Resolver必失败后原请求仍同结果且计数不增加；新request确实调用Resolver并失败。相同request改参数/跨登记动作冲突、旧认证代数拒绝，均不调用Resolver。真实删除专用workspace后原请求仍同Command，Fresh调用解析并失败。两个独立池先都probe Fresh，再Barrier并发提交，仍仅一条workspace、一个原Command、一新一重放。
- 未改旧外部lib源码，原4场景+1辅助入口全部再次通过（5 passed，1.45s，exit0），包括12自有子进程异常退出、实际COMMIT失败、跨动作竞争及release/resume边界。

CAT-R001：本tester独立回归PASS，交原reviewer据同版关闭其问题。本次未复跑未变的history22/JWS/fmt/clippy或全量；引用新manifest开发检查不作为独立执行。公共低层resolved端口保留，真实网关必须接raw服务；实际actor gate/owner/issuer/Harness/Web仍未签。NET-I001继续独立处理，本catalog-v2不包含网络修复。

证据目录 `D:/Project/acp-remote/.tmp/catalog-independent/v2/`：storage.log SHA `BB7EE9AF7F42CF02168B22FCC01578C1598E005C9598D3F21DB69FE1D9A3CBA8`；replay.log `90DB80B0E380D67C8F7A789CFC72558A955AF0B26FA9B9F2C280947D19D53204`；external.log `410B7F9BDAA1355F82613129A618C8BDD9EE173D3E9E1F687DB3FDD7FE2C65CC`。新外部源码 `tests/replay.rs` SHA `4862747AAFC6ABFE25381C4EA271B3B24C2F8E820D044E4921ECB4F0AB519382`。未改产品或预期、未清理用户资源。
