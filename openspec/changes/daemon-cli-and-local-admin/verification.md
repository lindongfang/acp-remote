# Verification — daemon-cli-and-local-admin

> 主 Agent 持续维护执行证据与变更历史；结构以 `openspec/schemas/agentic/templates/verification.md` 为准。

## Target

- 变更：`daemon-cli-and-local-admin`
- 仓库：`D:\Project\acp-remote`
- 目标主分支：`refs/heads/main`
- 基线核实（任务 1.1，2026-09-25，主 Agent 直接执行机械核实）：
  - `git rev-parse refs/heads/main` → `ab62773d8768f3c6a8424f6aefa862a738482eb8`
  - `git status --porcelain` → 仅本变更目录与误建的临时文件（已删除）；无用户未提交改动
  - `git worktree list` → 单一 worktree `D:/Project/acp-remote`（main）
  - 工具链：cargo 1.98.1（`rust-toolchain.toml` channel 1.98.1）、Node v24.19.0、npm 12.0.2
  - 基线检查（在 ab62773 工作区）：`npm run check` 退出码 0（日志：`reports/baseline-npm-check.log`）；`cargo test --locked --workspace --all-features` 退出码 0（日志：`reports/baseline-cargo-test.log`，全部 crate 无失败）
- 变更分支：`feat/daemon-cli-and-local-admin`（自 ab62773 创建）

## Handoff Index

| Task ID | Role / Phase / Stage | Target Revision | Evidence Type / ID | Report Path | Result / Evidence Status | Applicability / Source Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| 1.1 | environment / recon / recon（主 Agent 机械核实） | ab62773d8768f3c6a8424f6aefa862a738482eb8 | RESOURCE / NOT_APPLICABLE | reports/baseline-npm-check.log, reports/baseline-cargo-test.log | PASS / NEW | 基线绿，见 Target 节 |
| 2.1–2.3 | coder / implement / work-package | b56b829ae2119db77a1907271a5ce799962ac10a（+报告 7c6aea2030a4108859dd3089e55e9371ae3817af） | DELIVERY / NOT_APPLICABLE | reports/wp1-handoff.md | PASS / NEW | WP1 交付；RV1 独立 review 待调度（3.2） |
| 2.3 | coder / implement / work-package | b56b829ae2119db77a1907271a5ce799962ac10a | CHECK / PV1（W0 轮，npm run check 部分） | reports/wp1-handoff.md（日志 reports/du1-pv1.log） | PASS / NEW | 退出码 0，10 道门禁全绿 |
| 2.3 | coder / implement / work-package | b56b829ae2119db77a1907271a5ce799962ac10a | CHECK / PV2（W0 轮） | reports/wp1-handoff.md（日志 reports/wp1-boundaries.log） | PASS / NEW | 退出码 0 |
| 2.3 | coder / implement / work-package | b56b829ae2119db77a1907271a5ce799962ac10a | CHECK / PV1 附加（npm run verify 全量） | reports/wp1-handoff.md（日志 reports/wp1-verify.log） | PASS / NEW | 513 passed / 0 failed / 2 ignored（RV1-WP1-F1 修正后的日志实数），与基线逐项等价 |
| 3.1 | coder / project-verify / work-package | b56b829ae2119db77a1907271a5ce799962ac10a | CHECK / PV1、PV2（WP1 交付前轮次） | reports/wp1-handoff.md（日志 reports/wp1-verify.log、reports/du1-pv1.log、reports/wp1-boundaries.log） | PASS / NEW | 与 2.3 同批证据；在 WP1 最终工作树上执行 |
| 3.2 | reviewer / work-package review / work-package | b56b829ae2119db77a1907271a5ce799962ac10a | REVIEW / RV1 | reports/rv1-wp1.md | PASS / NEW | 隔离子 Agent（kimi-coding/k3）；无 CRITICAL/MAJOR；F1–F4 处理见 Review Findings |
| 2.4–2.6 | coder / implement / work-package | 6361f5d2a34d9f6306f62b41c852749e7d86bc4d（+报告 d9c7c6e） | DELIVERY / NOT_APPLICABLE | reports/wp2-handoff.md | PASS / NEW | WP2 交付；RV1（3.4）待调度 |
| 2.4–2.6 | coder / implement / work-package | 6361f5d2a34d9f6306f62b41c852749e7d86bc4d | CHECK / PV5（WP2 部分） | reports/wp2-handoff.md（日志 reports/pv5-windows-ipc.log） | PASS / NEW | 11 passed / 0 failed；跨用户拒绝以 SDDL 文本静态证据替代（本机单账号，已如实记录） |
| 2.7–2.9 | coder / implement / work-package | c12957ee3c4ffdcab9db8533d23b42e4673107ba（+报告 3390e9cc5688d667a9b2c4e4a1d9230873ca3297） | DELIVERY / NOT_APPLICABLE | reports/wp3a-handoff.md | PASS / NEW | WP3a 交付；RV1（3.6）待 WP3b 后调度 |
| 2.7–2.9 | coder / implement / work-package | c12957ee3c4ffdcab9db8533d23b42e4673107ba | CHECK / PV3（WP3a 部分） | reports/wp3a-handoff.md（日志 reports/wp3-server-transport.log、reports/wp3-server-envelope.log、reports/wp3-verify.log） | PASS / NEW | 53 passed / 0 failed / 0 ignored；fmt/clippy（含 Linux 目标）零告警 |
| 2.8 | coder / implement / work-package | c12957ee3c4ffdcab9db8533d23b42e4673107ba | CHECK / PV5（WP3a 部分） | reports/wp3a-handoff.md（日志 reports/pv5-windows-ipc.log） | PASS / NEW | 真实 Named Pipe 4 用例通过（sid_hash 已记录）；跨用户拒绝本机不可构造，如实登记 |
| 2.7–2.9 | coder / implement / work-package | c12957ee3c4ffdcab9db8533d23b42e4673107ba | CHECK / PV2（WP3a 轮） | reports/wp3a-handoff.md（日志 reports/wp3-server-boundaries.log） | PASS / NEW | 11 个 crate 全登记；server 唯一工作区内依赖边 = windows-local-ipc |
| 3.4 | reviewer / work-package review / work-package | 6361f5d2a34d9f6306f62b41c852749e7d86bc4d | REVIEW / RV1 | reports/rv1-wp2.md | PASS / NEW | 隔离子 Agent（deepseek/deepseek-flash）；无 P0/P1；3 条 P2 处理见 Review Findings；重派前的失败运行见 Failures and Retests |
| 2.10, 2.12 | coder / implement / work-package | f1a3cd4（+报告 4ec6bf3；另 fee6073 含中间版本） | DELIVERY / NOT_APPLICABLE | reports/wp3b1-handoff.md | PASS / NEW | WP3b1 交付；RV1 待 2.11/2.21 后并入 3.6 |
| 2.10, 2.12 | coder / implement / work-package | f1a3cd4 | CHECK / PV3（WP3b1 部分） | reports/wp3b1-handoff.md（日志 reports/wp3-server-methods.log） | PASS / NEW | 91 passed / 0 failed / 0 ignored；fmt/clippy（Win + Linux 目标）零告警；npm run verify 697 passed |
| 2.21 | coder / implement / work-package | 1cd7a3b（主 Agent 并发提交，含 2.21 全部 12 个文件；后接 dfb1c99 vendor 修复与 8dd24e4 报告） | DELIVERY / NOT_APPLICABLE | reports/wp321-handoff.md | PASS / NEW | 正则三处一致；drift 25 项；fixtures 118 valid/24 invalid；F1/F2 已修（dfb1c99） |
| 2.21 | coder / implement / work-package | dfb1c9972aad35e3bbb880e1e9ecfbe01a696257 | CHECK / PV1、PV2（2.21 轮） | reports/wp321-handoff.md（日志 reports/wp321-contract.log、reports/wp3-server-methods.log） | PASS / NEW | `npm run check` exit 0（显式 EXIT 行）；`cargo test -p server` 93 项
| 2.11, 2.13 | coder / implement / work-package | eef75f4（+测试 9b30ab4；报告 dbebd0c） | DELIVERY / NOT_APPLICABLE | reports/wp3b2-handoff.md | PASS / NEW | WP3b2 交付；RV1（3.6）待 2.23 后调度 |
| 2.11, 2.13 | coder / implement / work-package | eef75f4 | CHECK / PV3（WP3b2 部分） | reports/wp3b2-handoff.md（日志 reports/wp3-server-pairing.log、reports/wp3-server-methods.log、reports/du1-pv1.log） | PASS / NEW | 112 passed（新增 19）；fmt/clippy（Win+Linux 目标）零告警；npm run verify 627 passed |
| 2.23 | coder / implement / work-package | 313d2a2（含 0968c7a；报告 reports/wp323-handoff.md） | DELIVERY / NOT_APPLICABLE | reports/wp323-handoff.md | PASS / NEW | 三个 revoke 入口的前置读取 + `local.not_found`；fake 保真修正使断言非退化；RED 证据已留 |
| 2.23 | coder / implement / work-package | 313d2a2 | CHECK / PV3（2.23 轮） | reports/wp323-handoff.md（日志 reports/wp3-server-methods.log、reports/du1-pv1.log） | PASS / NEW | 114 passed；fmt/clippy 零告警；`npm run check` EXIT=0 |
| 3.6 | reviewer / work-package review / work-package | 313d2a2 | REVIEW / RV1 | reports/rv1-wp3.md | PASS / NEW | 隔离子 Agent（deepseek/deepseek-flash）；C1–C10 静态核对；无 CRITICAL/MAJOR；F1/F2/F3/F6 交 2.24，F4/F5 已由主 Agent 收敛措辞 |
| 3.5 | main（代行 coder 的 Project Verify）/ project-verify / work-package | 313d2a2 | CHECK / PV1、PV2、PV3、PV5（WP3 最终轮） | reports/wp3-final-verify.log、reports/pv5-windows-ipc.log | PASS / NEW | `npm run verify` EXIT=0（75 个测试块全 ok）；合同门禁全绿；Windows IPC 用例 EXIT=0 |
| 2.22 | coder / implement / work-package | fe093b3（含 bb96a10；报告 64823a1） | DELIVERY / NOT_APPLICABLE | reports/wp422-handoff.md | PASS / NEW | `AuditStore`（append/query over `owned_audit`）；复用 `insert_audit_rows` 编码、不改 DDL；111 passed（+14） |
| 2.22 | coder / implement / work-package | bb96a10 | CHECK / PV1、PV2（2.22 轮） | reports/wp422-handoff.md（日志 reports/wp4-storage-audit.log、reports/du1-pv1.log） | PASS / NEW | fmt/clippy 零告警；`check:drift` 36 条 DDL 不变；`npm run check` 两轮 EXIT=0 |
| 2.24 | coder / implement / work-package | cb983c2（报告 reports/wp324-handoff.md） | DELIVERY / NOT_APPLICABLE | reports/wp324-handoff.md | PASS / NEW | F1/F2/F3/F6 全修复；F2 留 RED（无 guard 时 `export.create` 静默覆盖并返回成功，exit=101）；F3 用可注入 `settle_accept` 缝实测 |
| 2.24 | coder / implement / work-package | cb983c2 | CHECK / PV1、PV3（2.24 轮） | reports/wp324-handoff.md（日志 reports/wp3-server-methods.log、reports/du1-pv1.log） | PASS / NEW | fmt/clippy EXIT=0；`cargo test -p server` EXIT=0（lib 88 ok）；`npm run check` EXIT=0 |
| 2.14 | coder / implement / work-package | c4e4c3a（e9b8e1d + c628e87；报告 c4e4c3a） | DELIVERY / NOT_APPLICABLE | reports/wp4a-handoff.md | PASS with notes / NEW | 新建 `crates/app`（18 文件 +6389/−16）；37 passed（27 unit + 1 audit.export e2e + 9 生命周期）；workspace 682 passed；⚠️ `storage.flush_interval_ms` 被实现成 no-op（见裁定③→任务 2.25） |
| 2.14 | coder / implement / work-package | c4e4c3a | CHECK / PV1、PV2、PV4（WP4a 轮） | reports/wp4a-handoff.md（日志 reports/wp4-app-daemon.log、reports/du1-pv1.log） | PASS / NEW | fmt/clippy EXIT=0；`-p app` EXIT=0；`check-crate-boundaries` EXIT=0（12 crate、§5 新列成真）；`npm run check` 三次 EXIT=0 |
| 2.15–2.17 | coder / implement / work-package | 0886c18（c8e44eb；报告 0886c18） | DELIVERY / NOT_APPLICABLE | reports/wp4b-handoff.md | PASS with notes / NEW | CLI 全子命令（§5.8 逐行映射 + 反向拒绝 `daemon doctor`/`session create`/`node rotate-key`/`audit export`）、`doctor`、`acp-stdio` 字节泵；12 文件 +3614/−76 |
| 2.15–2.17 | coder / implement / work-package | c8e44eb | CHECK / PV1、PV2、PV4（WP4b 轮） | reports/wp4b-handoff.md（日志 reports/wp4b-cli.log、reports/du1-pv1.log） | PASS / NEW | fmt/clippy EXIT=0；`cargo test -p app` EXIT=0（64 passed = 43 unit + 1 + 11 cli_commands + 9 lifecycle）；`check-crate-boundaries`/`npm run check` EXIT=0 |
| 2.25-A | coder / implement / work-package | 7089426 | DELIVERY / NOT_APPLICABLE | reports/wp425-handoff.md | PASS with notes / NEW | 合并窗口定时器真正接线（`MergeWindow` 注入 + `broker` 句柄 + `merge_window` 周期任务）；证据 = spy 断言（`pumps == scans × 2`、取消后冻结）+ 真实二进制 probe（`interval_ms:100`/默认 250、每 tick 一条非终态会话 SELECT、`task_stopped{merge_window}` 先于 `storage_closed`）；⚠️ 本切片无 `session.create`，端到端无可观察效果（已说明） |
| 2.25-B | coder / implement / work-package | a4cb119 | DELIVERY / NOT_APPLICABLE | reports/wp425-handoff.md | PASS / NEW | `accept()` 非 cancel-safe 写入源码文档 + 断言用例（取消后 `pending.is_none()`、客户端 `Ok(0)`、endpoint 仍可用） |
| 2.25-C | coder / implement / work-package | 7089426 | DELIVERY / NOT_APPLICABLE | reports/wp425-handoff.md | PASS / NEW | 交互式拒绝 → `*.pair.reject`（`{pairingId, reason:null}`）+ 非零退出 `local.conflict`；reject 失败原样上抛方法码；断言「拒绝只调 reject、确认只调 confirm」 |
| 2.25 | coder / implement / work-package | 7819b1f | CHECK / PV1、PV2、PV3、PV4（2.25 轮） | reports/wp425-handoff.md（日志 reports/wp425.log、reports/du1-pv1.log） | PASS / NEW | fmt/clippy(workspace) EXIT=0；`-p server` EXIT=0（89 lib）；`-p app` EXIT=0（49+1+11+9，0 ignored）；`npm run check` EXIT=0 |
| 2.26 | coder / implement / work-package | 1ea291a（82d376d；报告 1ea291a） | DELIVERY / NOT_APPLICABLE | reports/wp426-handoff.md | PASS / NEW | 定位：`remaining:1` = CLI 自己的停止连接；根因是 **mio `NamedPipe::Drop` 不 `CloseHandle`**，句柄被挂起 overlapped 读持有，而 CLI 的 current_thread runtime 缓存在 `Context` 里、等锁期间只 `sleep` 从不被驱动 ⇒ 句柄活到进程退出。修正：`Context::release_runtime()` 在 `drop(client)` 后、等锁前释放（关闭顺序与 `crates/server/**` 一行未改）。实测 3099 ms → **102 ms**，`daemon.drain_timeout` 消失 |
| 2.26 | coder / implement / work-package | 82d376d | CHECK / PV1、PV4（2.26 轮，含 RED/GREEN） | reports/wp426.log（`EXIT(...)` 行）、reports/du1-pv1.log | PASS / NEW | 新增回归断言 `stop_does_not_wait_for_the_full_grace_without_other_clients`（`--grace-ms 3000`，断言无 `drain_timeout`、`elapsed < 2000ms`、锁可再取）：RED（临时禁用修正）= 3023 ms / EXIT 101 → GREEN = 0.29 s / EXIT 0 |
| 1.4 | main（代行 coder 的接线核对）/ implement / work-package | f39dda9 | CHECK / PV2、PV3、PV4 | reports/wp4a-handoff.md、reports/wp4b-handoff.md、reports/check-boundaries（reports/wp4wp5-final-verify.log） | PASS / NEW | WP4 经 path 依赖接入上游已验收形状：`server` 的 `DaemonControl`/`LocalAdminDeps`/`LocalAdminRouter::new`/`PairingSessions::new`/`ConnectionCloser`/`AuditHook` 全部按 WP3b1/WP3b2 冻结签名实现（`reports/wp4a-handoff.md` 装配图逐项可核）；`vendor/windows-local-ipc` 只经 path 依赖被 `crates/server` 使用（`check:boundaries` 12 crate 一致；`workspace.exclude` 生效） |
| 2.18 | main（代行 coder 的局部验证）/ implement / work-package | f39dda9（WP4 期间在 82d376d/1ea291a 上首跑） | CHECK / PV1、PV4 | reports/wp4-app-daemon.log、reports/wp425.log、reports/wp426.log、reports/wp4wp5-final-verify.log | PASS / NEW | WP4 交付前局部验证：`cargo fmt --all -- --check`、`cargo clippy -p app --all-targets --all-features -- -D warnings`、`cargo test -p app --all-features`（2.25/2.26 轮分别 EXIT=0；2.26 修正后整套耗时从「每 stop 等 10 s」降至亚秒级） |
| 2.19 | coder / implement / work-package | d09dab7（94067e1 + c429492 + f39dda9） | DELIVERY / NOT_APPLICABLE | reports/wp5-handoff.md | PASS with notes / NEW | 6 文件状态写回（MODULE_ARCHITECTURE §3/§3.1/§4.1/§4.9/§4.10/§5 说明段、README 12/12 crate 表与「尚未开始」收敛、DEVELOPMENT_PLAN §2、AGENTS.md §4 表）、`nodeId` 派生式写入 `IDENTITY_AND_AUTH_CONTRACT.md` §7 第 7 项、`CONFIG_REFERENCE.md` 仅 `daemon.instance_lock` 标注 `ipc` 未接线；主 Agent 随后清理四处陈旧陈述（`CORE_PORTS_AND_STORAGE.md` 两处、`AGENTS.md` §9、`DEVELOPMENT_PLAN.md` §3、`check-crate-boundaries.mjs` 注释） |
| 2.20 | coder / implement / work-package | 0a04637（文档提交后） | CHECK / PV1、PV2 | reports/wp5-verify.log（原始 wp5-pv1-raw.log/wp5-pv2-raw.log） | PASS / NEW | `EXIT(npm run verify)=0`、`EXIT(check-crate-boundaries)=0`；82 个测试目标、717 passed、0 failed、2 ignored |
| 3.7 | main（代行 coder 的 Project Verify）/ project-verify / work-package | **f39dda9**（冻结版） | CHECK / PV1、PV2、PV5（WP4 冻结轮） | **reports/wp4wp5-final-verify.log**（完整输出，非 tail）、reports/du1-pv1.log、reports/pv5-windows-ipc.log | PASS / NEW | `EXIT(npm run verify)=0`（日志完整：82 targets、**717 passed**、0 failed、2 ignored；含 fmt/clippy/workspace test 三子项。⚠️ 计数更正：先前记录的 730 来自粗匹配 `grep -o "[0-9]* passed"`，把 `openspec validate` 的 `13 passed` 汇总行也算进去了；按 `test result: ok.` 行精确求和为 717，已用脚本复核）、`EXIT(check-crate-boundaries)=0`（12 crate）、`EXIT(cargo test windows-local-ipc)=0`（真机 Named Pipe 用例） |
| 3.9 | main（代行 coder 的 Project Verify）/ project-verify / work-package | **f39dda9**（冻结版） | CHECK / PV1、PV2（WP5 冻结轮） | 同 [3.7] 的 reports/wp4wp5-final-verify.log、reports/wp5-verify.log | PASS / NEW | 同上一条；WP5 为纯文档变更，冻结轮与前一轮（0a04637）结论一致 |
| 3.8 | reviewer / work-package review / work-package | f39dda9 | REVIEW / RV1（RV1-WP4） | reports/rv1-wp4.md | **PASS with notes** / NEW | 结构只读 reviewer（fresh、deepseek-flash）；12 项优先核对 + 12 条断言有效性抽样；无 CRITICAL/MAJOR、无阻塞；4 条发现（F1 spy 非快照读=SUGGESTION、F2 §4.10 关闭顺序与刷盘措辞=MINOR、F3 `rpassword` 未登记 §3.1=SUGGESTION、F6? F4 plan 证据路径不存在=SUGGESTION）→ 全部转任务 2.27 |
| 3.10 | reviewer / work-package review / work-package | f39dda9 | REVIEW / RV1（RV1-WP5） | reports/rv1-wp5.md | **FAIL** / NEW | F1/F2 两条 BLOCK 级事实错误（README/§4.10 关闭顺序；§5 注记称 `windows-local-ipc` 无行而矩阵有行）+ F3–F6 四条 P2 → 全部转任务 2.27；**修后须以新 Review ID 复核（RV2-WP5）**，故 3.10 暂不勾选 |
| 2.27 | coder / implement / work-package | b4f7f81（e7c01ec + 69874d9 + 35f8f2b + fd0ad86；报告 b4f7f81） | DELIVERY / NOT_APPLICABLE | reports/wp427-handoff.md | PASS / NEW | RV1-WP4 的 F1–F4 + 抽样#12 与 RV1-WP5 的 F1–F6 全部按最小修复落地；两项自检零命中（`停周期任务`、写范围内 `wp4-app-cli.log`） |
| 2.27 | coder / implement / work-package | b4f7f81 | CHECK / PV1、PV4（2.27 轮，含 K 的 RED/GREEN） | reports/wp427.log（gitignored，可复跑） | PASS / NEW | `npm run check` EXIT=0（379 链接 / 4742 §引用 / 12 crate / 36 DDL+15 trait-87 方法 / agentic 13 PASS）；fmt EXIT=0；clippy EXIT=0；`cargo test -p app` EXIT=0（72 passed / 0 ignored，lib 50 含新增 2 条）；K 的 RED（多行诊断）FAILED/EXIT 101 → GREEN EXIT 0 |
| 3.10 | reviewer / work-package recheck / work-package | 90c816a | REVIEW / RV2（RV2-WP5） | reports/rv2-wp5.md | **PASS** / NEW | RV1-WP5 的 F1–F6 全部已解决（逐项给出 target 版本位置与判据）；新发现 2 条 P2（RV2-WP5-F1 章节号笔误 `§7.1`→应 `§7.5`、RV2-WP5-F2 括注「没有任何依赖方」与同表事实相反）→ 主 Agent 已就地修正（含 `crates/app/src/daemon.rs:660` 注释的同源笔误） |
| 3.8 | reviewer / work-package recheck / work-package | 90c816a | REVIEW / RV2（RV2-WP4） | reports/rv2-wp4.md | **PASS** / NEW | RV1-WP4 的 F1–F4 与抽样 #12 全部已解决；独立确认「未发现断言被弱化」（`config.rs` 仅新增用例、`daemon.rs` 断言搬家并加强，`failures` 纳入冻结） |
| 5.1 | 主 Agent / integration-readiness / integration | 90c816a | DELIVERY / NOT_APPLICABLE | reports/integrator-phaseA.md | PASS / NEW | 独立集成 Agent 已单独创建（**不由主 Agent 兼任**）：子 Agent 角色 `integrator`，会话 ID `01a0d88f-2a78-7138-b91a-8b19eaeeb5a8`（运行 `6d132632-057a-46ba-947c-564ac543252d`），**新起子 Agent、未继承实现/测试/reviewer 对话**（上下文方式=fresh；输入仅 `roles/integrator.md` 全文 + 计划/契约/证据清单）。交接清单：交付单元 DU1（`integrated`、WP1–WP5 单单元）、仓库与集成分支（按 plan.md 第 654 行复用主 worktree / `feat/daemon-cli-and-local-admin`）、源提交与目标 `refs/heads/main` 及其基线 `ab62773…`、[PV1]–[PV5] 清单、已验收上游证据与 RV 报告清单、授权边界（**仅本地合入；推送/回滚/发布需另行授权**） |
| 5.2 | 主 Agent / integration-readiness / integration | 90c816a | DELIVERY / NOT_APPLICABLE | 本行自身（核对汇总见 reports/integrator-phaseA.md） | PASS / NEW | DU1 模式 `integrated` 与组成（WP1–WP5）经复核无变化；[PV1]–[PV5] 有有效证据（候选轮由集成 Agent 重跑全绿）、RV1-WP4/RV1-WP5 已完成且发现全部处置并由 RV2 复核 PASS；无未解决阻断项；漂移均已登记（Check Plan Changes / Failures & Retests / Review Findings） |
| 6.1 | integrator / candidate / merge-unit | 6d132632 | CHECK / 基线核实 | reports/integrator-phaseA.md | PASS / NEW | 目标仓库 `D:Projectacp-remote`、`git rev-parse refs/heads/main` = `ab62773d8768f3c6a8424f6aefa862a738482eb8`（与 verification.md 1.1 记录一致，未移动）；`git merge-base --is-ancestor main HEAD` = 0（候选包含基线、无冲突解决差异）；`git worktree list` 与 `status --porcelain` 干净 |
| 6.2 | integrator / candidate / merge-unit | 6d132632 | CHECK / 候选构造 + PV1–PV5 | reports/candidate-verify.log（3033 行，`sha256=5215a54c4583c60800c8861573cafdd59ad3897262ee380def4567b2055d75e4`） | PASS / NEW | 候选 = 分支 `feat/daemon-cli-and-local-admin` 的 `90c816a`；`cargo build --locked --workspace` 与 vendor build EXIT=0；[PV1] EXIT=0（82 targets / 718 passed / 0 failed / 2 ignored）、[PV2] EXIT=0（12 crate）、[PV3] EXIT=0（7 targets / 117 passed）、[PV4] EXIT=0（6 targets / 72 passed）、[PV5] EXIT=0（vendor 11 passed）；候选 vs 基线 105 文件（A=86/M=19） |
| MD2（AuditStore 缺口） | main / plan-review / work-package | f1a3cd4 | DELIVERY / NOT_APPLICABLE | 《本行自身》 | **PASS / NEW** | 已由任务 2.22 关闭（`bb96a10`/`fe093b3`；`crates/storage-sqlite/src/admin/audit.rs`）。原始缺口描述： `storage-sqlite` 缺 `AuditStore` 生产实现（已核实）；新增任务 2.22 处理，完成后关行 |
| MD1（WP3a 移交的契约不一致） | main / design-review / work-package | c12957ee3c4ffdcab9db8533d23b42e4673107ba | DELIVERY / NOT_APPLICABLE | reports/wp3a-handoff.md（契约问题①②③④节） | BLOCKED / PENDING | ① `node.rotate-key.begin` 与 §4 正则/schema 词表不一致：**用户已裁决选项 a**（新增任务 2.21 原子交付契约补齐 + Rust 变体），本行待 2.21 完成后关；②nil UUID 哨兵、③message 回显方法名、④Unix 凭据仅 Linux/Android——已接受并登记（见 Check Plan Changes） |

## Checks

| Check ID / Stage / Work Package | Revision / Base | Scope | Executor | Command / Steps | Environment | Result / Exit Code | Evidence |
| --- | --- | --- | --- | --- | --- | --- | --- |
| baseline / recon / 全变更 | ab62773 | 合同门禁基线 | 主 Agent | `npm run check` @ 仓库根 | Node v24.19.0 | PASS / 0 | reports/baseline-npm-check.log |
| baseline / recon / 全变更 | ab62773 | cargo 测试基线 | 主 Agent | `cargo test --locked --workspace --all-features` @ 仓库根 | cargo 1.98.1, Windows x64 | PASS / 0 | reports/baseline-cargo-test.log |

## Check Plan Changes

- 2026-09-25（WP1 交付后）：**§5 `storage-sqlite` 列的归属调整**。原值：WP1 收敛 §5 缺列注记、WP5 收口状态。新值：WP1 已新增 `server`/`app`/`windows-local-ipc` 三列并在 §5 注记写明后果；`storage-sqlite` 列必须在 `crates/app` 加入 `members` 的同一提交补齐（否则 `check:boundaries` 对 `app → storage-sqlite` 硬失败），因此 **WP4 的写范围扩至包含 `docs/MODULE_ARCHITECTURE.md` §5 矩阵一处**（仅新增 `storage-sqlite` 列与 `app` 行对应格，不动其他内容），WP5 仍负责状态收口。理由：门禁只校验实际 path 依赖边，列与成员加入必须原子。风险覆盖：该调整不改变需求与设计语义；影响任务 2.14（写范围）。依据：WP1 handoff（`reports/wp1-handoff.md`）开放问题第 1 项与 §5 注记。
- 2026-09-25（WP1 交付后）：**WP2 附带 `.gitignore` 维护**。依据：WP1 handoff 开放问题第 2 项——vendor crate 独立构建会在 `vendor/windows-local-ipc/` 下产生 `Cargo.lock` 与 `target/`，根 `.gitignore` 不覆盖；WP2 写范围因此包含根 `.gitignore`（仅新增 vendor 条目）。
- 2026-09-25（用户裁决选项 a，WP3a 报告契约问题①）：**`node.rotate-key.begin` 进入 v1 方法词表**。原值：§4 方法名正则与 `envelope.schema.json#/$defs/methodName`（enum 24 项、pattern 不允许连字符）与 §5.7/§5.4 的连字符名字矛盾，该调用实际被判 `local.invalid_request`。新值：pattern 放宽为段内允许连字符（`^[a-z][a-z0-9]*(\.[a-z0-9]+(-[a-z0-9]+)*)*$`），enum 增至 25 项（含 `node.rotate-key.begin`），实现按 §5.7 返回 `local.unsupported`；同步 `docs/LOCAL_ADMIN_PROTOCOL.md` §4 表格、§8 版本注记、`fixtures/local-admin/v1/`（新增一条 valid 与一条 invalid）与 `scripts/check-command-catalog.mjs` 的方法标题正则（原正则同样排除连字符，不改则集合比对必然失败，已核实）。`commands.json` 的 `localCapabilities` 不变（`local.node.rotate-key` 已登记）。新增任务 **2.21**（契约与 Rust 枚举同批原子交付，避免分支上 drift 测试变红）；`npm run check` 必须全绿。**规划资产同步**：本变更 `specs/local-admin-methods/spec.md` 的 method 正则同步为新取值（错误码正则不变）；另外 §5.7 原先用 h3 标题且名称叠在标题里，门禁只认 `#### \`name\``，因此 2.21 按 §5.6 同款结构改为 `### 5.7 明确推迟（\`local.node.rotate-key\`）` + `#### \`node.rotate-key.begin\``（三条 bullet 语义逐字不变；主 Agent 已批准），门禁保持单一条正则。依据：用户在 2026-09-25 本会话对选项 a 的答复（原话「a」）。
- 2026-09-25（WP3a 交付后，主 Agent 接受）：**三处实现细节登记为已接受**，不改契约：② 信封 `id` 缺失/非法时以 nil UUID 哨兵回 `local.invalid_request`（§4 规则 2「响应必带相同 id」与规则 4「非法信封回错误」在 id 不可用时无法同时满足；哨兵是最小偏差且已写入代码注释）；③ `local.unsupported` 的 `error.message` 回显方法名（仅方法名、≤512、不含 params 与路径）；④ Unix 对端凭据校验收窄到 Linux/Android（其它 Unix 目标每次连接失败关闭，避免 macOS 编译失败；与 §2.2 写明的 `SO_PEERCRED` 口径一致）。三者在 WP5 文档收口时如需写回合同，再单独提出。
- 2026-09-25（WP3b1 交付后，主 Agent 核实）：**新增任务 2.22：`storage-sqlite` 实现 `AuditStore`**。原因：`core::UseCases` 的 `UseCaseDeps.audit: Arc<dyn AuditStore>` 是必需字段，`audit.export`（任务 2.12，本变更范围内）靠 `AuditStore::query` 读 `owned_audit`，但工作区里只有 core 测试内的 `TestAudit`，生产实现缺失（已核实：`crates/storage-sqlite/src/admin/` 只有 export/local_config/trust 三个 impl；`owned_audit` 表与 `insert_audit_rows` 已存在）。写范围：`crates/storage-sqlite/src/admin/audit.rs`、`crates/storage-sqlite/src/admin/mod.rs`；归属 WP4 波次（在 2.14 之前），检查 PV1/PV2 + 该 crate 测试。影响：任务 2.22（新增）；需求未变。
- 2026-09-25（WP3b1 待澄清项裁定，主 Agent）：② `agent.configure` 保留既有 `ProviderEnvBinding`——**接受**（合同 §5.2 的 `params` 不含绑定字段，保留既有绑定是正确行为）；③ keystore 条目命名约定（`keystore_ref = sha256(providerId)[..16]@v<version>`、标签 `<ref>.<field>`）需写回权威文档 `IDENTITY_AND_AUTH_CONTRACT.md` §7 与 `CORE_PORTS_AND_STORAGE.md` §11.6——**并入 WP5 文档收口**；④ 错误消息回显标识符类输入（参数名/方法名/类别名）——与 WP3a 同口径，接受；⑤ 三处收窄（`values` 非空、嵌套 closed、alias 用 node-link 同类正则）——接受（经核实 node-link 的 `workspaceAlias` 与文档 §5.2 正则**完全相同**，无行为差异）；⑥ §7「方法失败」无 §14.2 对应类别——本轮结构化日志 + 保留接线点，接受并登记；①AuditStore 缺口见上条新增任务。
- 2026-09-25（RV1-WP1 后）：**WP5 写范围增加两处 MINOR 修复**——`scripts/check-crate-boundaries.mjs` 的过期注释（RV1-WP1-F2）与 `docs/MODULE_ARCHITECTURE.md` §5 注记补「`windows-local-ipc` 依赖面靠人工 review 约束」（RV1-WP1-F3）。两者均非行为变化；影响任务 2.19（写范围）。
- 2026-09-25（2.21 交付后）：**WP5 写范围再增一处文档 bug**——`schemas/local-admin/v1/README.md:13` 引用了不存在的 `scripts/check-local-admin-contract.mjs`（本地管理词表实际由 `scripts/check-command-catalog.mjs` 断言）；属既有问题，2.21 未改，归 WP5 顺手修正。
- 2026-09-25（WP1 交付后）：**WP4 调用点约束传递**——固定工具链 1.98.1 下 `std::fs::File::try_lock`（1.89 稳定）与 `fs4::FileExt` 同名且优先级更高，WP4 必须全限定调用 `fs4::FileExt::try_lock`，否则等于把 MSRV 抬到 1.89（依据：WP1 handoff 开放问题第 3 项，已写入 `MODULE_ARCHITECTURE.md` §3.1）。

- 2026-09-25（WP3b2 裁决，主 Agent）：**配对 URL 的 origin 来源与 QR payload 组装方式**。D1：`public_origin: Option<String>` 由 WP4 组合根注入 `PairingSessions::new`（server 不读配置、不经 `DaemonControl` 取值）；`None` 时 `device.pair.begin`/`node.pair.begin(owner)` 在**任何副作用之前**失败关闭并回 `local.unavailable`（§6 无「配置缺失」专用码，语义最贴近；成因需在 message 与注释中说明）。D2：`crates/server` 新增 `sync-protocol` + `node-link-protocol` 生产依赖（§5 矩阵允许），用两边的 `QrPayload` 组装 `#data=`，不手写 wire 键名，并补往返测试；若协议 crate 有既有 URL helper 优先使用，否则用 base64 `URL_SAFE_NO_PAD`（server 不引入 `acpr-wire`）。影响：WP4 的 `PairingSessions` 构造签名 + server 的两条依赖边 + `Cargo.lock`。依据：WP3b2 实现者的 D1/D2 提问与本会话裁定。
- 2026-09-25（WP3b2 交付后，主 Agent 裁定）：**五条开放项处置**。(1) `*.revoke` 重试语义与 §7 冲突——**实现侧修正**（已核实 `export.rs:401`/`trust.rs:519`/`trust.rs:552` 均为 `COALESCE(revoked_at, ?)`；新增任务 2.23：三个 revoke 入口先读、不存在或已撤销 → `local.not_found`）。(2) 待澄清项作废：`PairingRecord` 本身持久化 `requested_scopes`/`requested_grants`（`core/src/model/identity.rs:440-441`），因此 status 返回的就是登记的请求集合，**不是偏离**。(3) `consumed → "approved"`、设备 `pending_confirmation → "claimed"` 的字面量映射——在 §5.3/§5.4 的封闭词表内取值，接受。(4) `preset.*` 在 `device.pair.begin` 被拒——与 §5.3「取值限于 `pack.*` 与命令名集合」的字面表述一致，接受（展开由 CLI 承担）。(5) `window::add_millis` 与 `storage-sqlite` 的日期算术重复——core 未暴露时间工具，接受并登记为已知重复（不新增跨 crate 依赖）。

- 2026-09-25（RV1-WP3 后，主 Agent）：**F4/F5 契约措辞收敛**（不改语义）——`docs/LOCAL_ADMIN_PROTOCOL.md` §7 的「审计与日志」条改为「拒绝连接 + §14.2 类别覆盖的安全动作 + 撤销记审计；方法失败只记结构化日志」（§14.2 封闭词表无「方法失败」，不得复用 `authorization.denied`）；`specs/local-admin-methods/spec.md` 场景 WHEN 列表同步。`specs/local-admin-channel/spec.md` 的 attachment MUST 句加「facade 落地后」范围限定并指向 §3.1 实现状态注记。影响：无行为变化，仅合同/规范措辞与实现口径对齐。新增任务 2.24（F1/F2/F3/F6 实现侧修复）。

- 2026-09-25（2.22 交付后，主 Agent 裁定）：三项开放项处置。①`query` 不跨 `imported_audit`——**接受**：`imported_audit` 带 `owner_node_id`/`export_id`/`session_id`/`request_id` 四列，而 `AuditRecord`（合同 §3 值对象）无法承载这些归属列，跨表投影会静默丢归属；本切片无 imported 会话，`owned_audit` 是唯一相关表。②`append` 受容量门约束、满载时独立拒绝审计——**接受**：与 §7.5⑥「宁可拒绝写也不静默丢证据」一致，拒绝路径落结构化日志。③新增 `crates/storage-sqlite/tests/admin_audit.rs`（测试文件）超出任务单列举的两个 src 路径——**接受**：仅测试资产，不影响依赖面与合同。

- 2026-09-25（WP4a 交付后，主 Agent 裁定 5 项上游决策）：
  - **① `nodeId` 派生 → 接受**：无「本节点身份」的权威存储位置（`core::ports` 只有 `NodeRecord` 描述**对端**节点），故 `nodeId = SHA-256("acp-remote/node-id/v1" ‖ SEC1 公钥) 前 16 字节（UUID v8）`成立；密钥轮换改变 `nodeId` 与 `node.rotate-key` 后必须重配对的既定语义一致（`IDENTITY_AND_AUTH_CONTRACT.md` §6.3「撤销与身份变化」：身份材料变化不得自动接受，必须重新配对）。派生式与「不得凭空发明身份」的约束相容。**WP5（2.19）须把该派生式与域分离串写进权威文档**（`IDENTITY_AND_AUTH_CONTRACT.md` §7 或 `MODULE_ARCHITECTURE.md` §4.10），不得只留在代码里。
  - **② 锁文件/实例记录拆分 → 接受并修正契约措辞**：Windows 字节范围锁会让读取同文件的进程收到 `ERROR_LOCK_VIOLATION`，单文件方案下 CLI 读不回记录。已改 `design.md` §4 与 `specs/daemon-lifecycle/spec.md` 首条：`daemon.lock` 只承载互斥，`daemon.instance.json` 承载可读记录（instanceId/pid/endpoint/publicOrigin），并写明「取锁成功后原子写入、正常关闭删除、有记录而锁可获取 = 陈旧记录 = 未运行」不变量。**非弱化**：CLI 仍不打开数据库，互斥仍由 OS 文件锁保证。
  - **③ `storage.flush_interval_ms` 实现为 no-op → 不接受，转任务 2.25**：该键是 broker 的 delta 合并窗口（`CORE_PORTS_AND_STORAGE.md` §6 第 10 条），`broker.rs:10`/`:1358` 明确「合并窗口由组合根的定时器触发 `pump`」，`UseCases::list_sessions` 使枚举可行 ⇒ 属真实契约缺口（缓冲 delta 永不落盘/广播）。同时接受 `RevocationCloser.closed_connections = 0`（连接代际属 facade 期）与 `AuditSink` 记录形状（与 §14.2 十列一致）。
  - **④ `LocalEndpoint::accept()` 非 cancel-safe → 接受 app 侧规避 + 转任务 2.25 的 B 项**：`app` 已用「专用 accept 任务 + 容量 1 channel」规避；须在 server 侧加文档注释钉死该约束，避免后续调用方误用 `select!`。
  - **⑤ `MODULE_ARCHITECTURE.md` §5 说明段仍称 `storage-sqlite`「只作为行出现」→ 转 WP5（2.19）**：本轮仅获准改矩阵列本身，说明段与其余 §5 文本保持不动。

- 2026-09-25（WP4b 交付后，主 Agent 裁定 6 项就地判断）：
  - **① Daemon 未运行时 `daemon status`/`daemon stop` 退 0 → 接受**：spec 的「Daemon 未运行时管理命令明确失败」场景**逐字排除**了 `daemon start|stop|status`/`doctor`；`daemon status` 是查询、离线回答「未运行」是成功完成查询，退 0 与 spec 一致。
  - **② 用法错误退 2、方法/传输错误退 1 → 接受**：spec 只要求「失败非零」，clap 惯例退 2 与 `--help/--version` 退 0 保留 stdout 均合规。
  - **③ SAS/指纹不匹配 → `local.invalid_params` → 接受**：错误码表无更贴切的码（§6 无 `local.mismatch`），且 spec 对此只要求「非零退出、不调 confirm、不改状态」。
  - **④ `acp-stdio` 绝不在 stdout 写字 → 接受（且为合同要求）**：stdout 即 ACP 字节流，日志与错误一律走 stderr。
  - **⑤ 非交互拒绝不调 `*.pair.reject` → 就地判断接受，但交互式拒绝改为调用（转 2.25 C）**：`device.pair.reject`/`node.pair.reject` 确在 25 项词表内（§5.3），交互式 `n` 下不调用会让会话悬置到 5 分钟到期；已在任务 2.25 增补 C 项。
  - **⑥ 额外 `--pack`/`--expires-in-ms`/`--grace-ms` → 接受**：均为 §5.2/§5.3 方法参数的直映射（kebab-case ↔ camelCase），不新增语义。
- 2026-09-25（WP4b 的 §20 核验修订主 Agent 登记）：`design.md` §7 关于 `rpassword` 的许可证说法**已改正为 Apache-2.0 单许可**（实测 7.5.4 非双许可），并登记 `rtoolbox` 的 0.0.x 残余风险；本地无法判定的 `cargo-deny`/`gitleaks` 仍只在 CI 判定。
- 2026-09-25（2.25 交付后，主 Agent 处置 4 项）：
  - **① 关闭顺序措辞冲突 → 主 Agent 已改文档**：实现顺序（停接入 → 排空 → 取消周期任务与信号监听 → 停 Agent → `wal_checkpoint(TRUNCATE)` → 清理并释放锁）与 `SECURITY_DESIGN.md` §12.1 逐字一致；`CORE_PORTS_AND_STORAGE.md` §7.5 第 4 条原措辞「先停周期任务，再停接入层与 Agent」是唯一孤例且会误导后续实现，已改写为「以 §12.1 为准」的显式序列并说明周期任务取消位置的理由（让它们在存储关闭前停止写入，不是先于接入层）。该条措辞在全仓只此一处副本（`grep -rn 停周期任务` 已确认为零命中）。
  - **② `daemon stop` 等满宽限 → 转任务 2.26 诊断**：`drain_connections` 逻辑正确、CLI 已 `drop(client)`，故「哪条连接未结束」未定；不猜测定性，交由 2.26 用受控探针定位并加回归断言（当前行为下须 RED）。
  - **③ `flush_interval_ms` 只校验下界（`>=1`）→ 接受**：`CONFIG_REFERENCE.md` §5 未给上界，合同无要求；`0` 失败关闭已实现。
  - **④ 交互式提示 stdin 读失败 → `local.internal` → 接受**：终端故障属内部错误而非「用户拒绝」，语义正确；拒绝路径仍走 reject + 非零退出。
- 2026-09-25（主 Agent，加速决策）：**W3/W4 波次合并**——任务 2.26（写 `crates/app/**`）与 WP5（写 `docs/**`、`README.md`、`AGENTS.md`、`reports/**`）**并行派发**，偏离规划「每波 1 个写者」的串行安排。理由：两者写范围**不相交**；规划真正要守的不变量是「文件单一写入者」，而非「全局单写者」。风险与缓解：并发 `git add` + 裸 `git commit` 会互相卷入暂存内容（PRO-1/PRO-2 的成因），故两个 Agent 均被指令**只用 `git commit --only <显式路径>`**、禁止 `git add` 后裸提交；WP5 被明确禁止触碰任何 `crates/**`。另注：两者共享 `target/` 目录，存在 cargo 构建锁竞争（会变慢但不会失败），WP5 被要求先做文档提交、最后才跑分钟级 [PV1]/[PV2]。
- 2026-09-25（2.26 定位结论，主 Agent 复核登记）：根因**不在**关闭序列也不在 `drain_connections`，而在 CLI 侧的 runtime 生命周期（`mio` 的 Windows Named Pipe 句柄释放依赖 I/O driver 处理完 completion）。**不变量教训（供后续切片参考）**：CLI 的同步等锁路径不得持有已 drop 掉连接的 runtime；`Context::release_runtime()` 现由 `daemon_stop` 在 `drop(client)` 之后立即调用。此项已由回归断言钉住（宽限 3000 ms 时必须明显早于宽限完成）。
- 2026-09-25（3.7/3.9 冻结轮，主 Agent）：实现侧冻结版本 = **f39dda9**（此后不再有代码写者）。冻结轮统一验证完整输出落 `reports/wp4wp5-final-verify.log`（**不再用 `tail` 截断**：第一次尝试只落了 25 行、计数不可核，已重跑并保留全文，此为 PRO-4 类操作教训，已就地纠正）。计数：82 targets、**717 passed**、0 failed、2 ignored（`crash_child`、`regenerate_v2_fixtures` 自述性忽略；计数按 `test result: ok.` 行精确求和，见 2.27 后的更正说明）。[PV5] 在当前版本重跑并追加 `reports/pv5-windows-ipc.log`（`EXIT(cargo test windows-local-ipc)=0`）。
- 2026-09-25（RV1-WP4/WP1-WP5 后，主 Agent）：两轮独立 review 的结论与处置见上表 Review Findings。**RV1-WP5 判 FAIL**（两条 BLOCK 级事实错误：文档关闭顺序、§5 注记与自身矩阵矛盾），故新增任务 2.27 承载全部最小修复（含 RV1-WP4 的 4 条非阻塞发现与 1 条断言强度缺口），修后以**新 Review ID（RV2-WP4/RV2-WP5）**复核；3.10 在复核通过前不勾选。`plan.md` 的证据路径修正属判据工件的可追溯性修复（不改变需求/任务语义），登记于此以免被视为未登记的漂移。
- 2026-09-25（2.27 交付后，主 Agent 裁定）：
  - **接受 2.27 的连带实现修正**：K 项断言在现有实现下必 RED，原因是 `toml_error_detail` 的谓词实际命中 **toml 的源码定位行**（如 `2 | endpoing = "wp427-marker"`），使 CLI 的「一行简述」可能带出配置文件内容——这与该函数 doc comment 及 `ConfigError::Invalid` 既有口径（「不含配置值的简短说明」）冲突。修法为排除 `  |` / `N | <源码>` / `  | ^^^^` 三类行，未改长度上限、错误码映射与任何 wire/合同，无用例固定过旧文本 ⇒ **不是弱化断言过关**，属正向修正；已接受并留 RED/GREEN 证据。
  - **tasks.md 的 2.15–2.17 留证路径就地修正**：三处任务定义中的 `reports/wp4-app-cli.log`（不存在）改为实际使用的 `reports/wp4b-cli.log`；`reports/rv1-wp4.md` 中的同名引用**保持原样**（已落盘的检视报告是事实记录，不得回改）；2.27 自述中的引用保留（描述缺陷本身）。
- 2026-09-25（RV2 与候选轮，主 Agent）：
  - **测试计数自我更正**：`reports/wp4wp5-final-verify.log`（f39dda9 冻结轮）的**准确计数为 82 targets / 717 passed / 0 failed / 2 ignored**，先前记录的 730 源于粗匹配 `grep -o "[0-9]* passed"` 把 `openspec validate` 的 `Totals: 13 passed` 等汇总行一并计入；已按 `test result: ok.` 行精确求和复核并更正全文。候选轮（90c816a）为 82 targets / 718 passed（集成 Agent 报告）。
  - **PV5 平台命令的语义澄清（集成 Agent F1，P2）**：plan 中 [PV5] 的「Windows 本机用例」以测试名过滤执行，在 `app` 轮命中 0 条、`server` 轮命中 3 条；该场景的**真实覆盖**在同一轮的 [PV3]（`local_endpoint_windows.rs` 4 passed）与 [PV4]（`daemon_lifecycle.rs` 10 + `cli_commands.rs` 11，真实子进程 + Named Pipe）以及 vendor 的 `cargo test --all-features`（11 passed）中。结论：平台场景已被覆盖，PV5 命令本身仅作补充留证，**不改判据**。
  - **章节指向统一**：`CORE_PORTS_AND_STORAGE.md` §7.5 第 4 条 + `SECURITY_DESIGN.md` §12.1 为关闭顺序的正式指向（RV2-WP5-F1 指出 `§7.1 第 4 条` 是笔误——§7.1 第 4 条讲的是权限位）。已修正 `docs/MODULE_ARCHITECTURE.md:384`、`crates/app/src/daemon.rs:660` 注释与本文件；已落盘的 `reports/rv1-wp4.md`/`rv1-wp5.md` 保留原文（历史记录不回改）。
  - **集成 Agent 登记的既有残余（范围外，未处置）**：`storage-sqlite` 测试自 2026-09-18 起在 `/tmp` 遗留约 16191 个 `acpr-*` 临时目录（早于本基线，属既有行为），仅登记不清理。
## Dependency Handoffs

| Downstream | Upstream | Accepted Revision / Evidence | Transfer / Inclusion Check | Invalidation |
| --- | --- | --- | --- | --- |
| WP3 | WP2 | `6361f5d2a34d9f6306f62b41c852749e7d86bc4d`：冻结 API 三函数（`create_pipe_server`/`current_user_sid`/`client_user_sid`，均 `cfg(windows)`，`io::Result`）+ `reports/wp2-handoff.md`（11 用例通过，`cargo check --target x86_64-unknown-linux-gnu` exit 0，根 `npm run check` 绿） | `server` 以 path 依赖只调用三个公开函数；wrapper 非 Windows 为空 crate，server 调用点必须 cfg-gate | wrapper API 形状变化 → WP3/WP4 复验 |

备注：WP2 移交的未核实项——同名后续实例是否继承首实例的受限 DACL（需 `GetSecurityInfo` 或跨用户用例），列入 WP3 的 [PV5] 范围。

## Runtime Resources

- 本变更不涉及数据库服务、容器、端口、外部账号或网络资源（依据：仅本机 IPC 与临时目录；见 plan.md「Runtime Resources」说明行）。
- 已登记隔离方案：集成用例临时 Daemon 数据目录（用例自建自删）；并行执行者各自 `CARGO_TARGET_DIR`（本变更默认串行，暂无并行占用）；[PV5] Windows IPC 用例串行轮次。

## Review Findings

| ID | Revision | Reviewer | Location | Severity / Impact | Resolution | Recheck Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| RV1-WP1-F1 | b56b829 | RV1-WP1（隔离子 Agent） | reports/wp1-handoff.md | MINOR：handoff 总数 515 与日志实数 513 不符 | 主 Agent 已按日志实数（513 passed / 0 failed / 2 ignored）登记本文件；不阻断 | 不适用（记录修正） |
| RV1-WP1-F2 | b56b829 | 同上 | scripts/check-crate-boundaries.mjs 内嵌注释 | MINOR：注释仍写「9 列」与现状漂移；门禁行为不受影响 | 并入 WP5 写范围同步该注释（Check Plan Changes 已登记） | WP5 交付时由 RV1-WP5 复核 |
| RV1-WP1-F3 | b56b829 | 同上 | docs/MODULE_ARCHITECTURE.md §5 | MINOR：`windows-local-ipc` 作为发起方的依赖边不受门禁强制 | 接受缺口并在 WP5 于 §5 注记补「该 crate 依赖面靠人工 review 约束」一句（Check Plan Changes 已登记） | WP5 交付时由 RV1-WP5 复核 |
| RV1-WP1-F4 | b56b829 | 同上 | docs/LOCAL_ADMIN_PROTOCOL.md 头部 | SUGGESTION：状态/版本行略超字面写范围 | 主 Agent 确认：头部状态行视为 §3.1 注记的载体，不返工 | 不适用 |
| RV1-WP2-F1 | 6361f5d | RV1-WP2（隔离子 Agent） | vendor/windows-local-ipc/Cargo.toml:32 | MINOR：注释称 `fs4`/`clap` 引入 windows-sys 0.61.2，实际由 `mio`/`tokio` 引入 | 并入任务 2.21 顺手修复（一行注释） | 2.21 交付时由 RV1-WP3 复核 |
| RV1-WP2-F2 | 6361f5d | 同上 | vendor/windows-local-ipc/Cargo.toml:16 | MINOR：`license = "MIT OR Apache-2.0"`，但仓库仅含 Apache-2.0 正文 | 并入任务 2.21：改为 `license = "Apache-2.0"` | 2.21 交付时由 RV1-WP3 复核 |
| RV1-WP2-F3 | 6361f5d | 同上 | reports/du1-pv1.log、reports/pv5-windows-ipc.log、wp2-handoff.md | MINOR：两个 [PV1] WP2 轮未记退出码；fmt 无运行记录 | 主 Agent 已补录：`du1-pv1.log` 追加 `EXIT(npm run check)=0` 轮次；`pv5-windows-ipc.log` 追加 `EXIT(cargo fmt -- --check)=0`（均真实执行） | 追加日志已核实 |
| RV1-WP3-F1 | 313d2a2 | RV1-WP3（隔离子 Agent） | crates/server/src/local_admin/router.rs:461-470 | MINOR：`import.remove` 缺「未知合法 id」常驻断言 | 已由 2.24 修复（`cb983c2`） | 待最终验收 RV2 复核 |
| RV1-WP3-F2 | 313d2a2 | 同上 | crates/server/src/local_admin/test_support.rs:493-505 | MINOR：`FakeExports::put_export` 比真实存储更严（未撤销的既有 id 在存储里是覆盖 `Ok`）⇒ `export.create` 查重断言无但例能力 | 已由 2.24 修复（fake 同形 + RED 证据 `exit=101`） | 待 RV2 复核 |
| RV1-WP3-F3 | 313d2a2 | 同上 | crates/server/src/transport/local/platform/windows.rs:67-80 | MINOR：`connect()`/补建失败时 `pending` 保持 None ⇒ endpoint 之后永久不可用（与代码自述不变量冲突） | 已由 2.24 修复（`settle_accept` 先补建再判定 + 注入式单测） | 待 RV2 复核 |
| RV1-WP3-F4 | 313d2a2 | 同上 | docs/LOCAL_ADMIN_PROTOCOL.md:594 + spec 场景 WHEN | MINOR（契约措辞）：§7 要求「方法失败」入审计，但 §14.2 封闭类别无此类别 | **主 Agent 已收敛**：§7 改为「方法失败只记结构化日志」并说明不得复用其他类别；spec 同步 | 已改由 workflow check --stage plan 复验（PASS） |
| RV1-WP3-F5 | 313d2a2 | 同上 | specs/local-admin-channel/spec.md（MUST 句） | MINOR（契约措辞）：`0x02` 立即分配 `FacadeAttachmentId` 在本切片无可行路径 | **主 Agent 已收敛**：MUST 句加「facade 落地后」限定并指向 §3.1 注记 | 同上 |
| RV1-WP3-F6 | 313d2a2 | 同上 | params.rs:317-326、envelope.rs:255-262 | SUGGESTION：`ProviderConfigure` 的 Debug 可打印明文凭据；两处不可达分支 | 已由 2.24 修复（手写 `Debug` 只打字段名 + 清理不可达分支） | 待 RV2 复核 |
| RV1-WP4-F1 | f39dda9 | RV1-WP4 | crates/app/src/daemon.rs:1325-1334、1376-1383 | SUGGESTION：合并窗口 spy 的相等断言非快照一致读（tick 边界约 1e-5 概率假失败），机制覆盖本身有效 | 已由 2.27 修复（快照一致读） | RV2 复核 |
| RV1-WP4-F2 | f39dda9 | RV1-WP4 | docs/MODULE_ARCHITECTURE.md §4.10 后台任务清单末两条 | MINOR：关闭顺序写成「停周期任务 → 停接入层」（与 §12.1/§7.1 与实现相反）；「存储批量刷盘」应为 broker 合并窗口 | 已由 2.27 修复 | RV2 复核 |
| RV1-WP4-F3 | f39dda9 | RV1-WP4 | docs/MODULE_ARCHITECTURE.md §3.1 依赖登记；crates/app/Cargo.toml:1 | SUGGESTION：新依赖 `rpassword` 未登记进 §3.1；`crates/app/Cargo.toml` 头注释仍说 CLI 子命令属 WP4b | 已由 2.27 修复（§3.1 补 `rpassword` 登记） | RV2 复核 |
| RV1-WP4-F4 | f39dda9 | RV1-WP4 | plan.md（17 处） | SUGGESTION：PV4 与 R10–R72 的证据指向不存在的 `reports/wp4-app-cli.log`（实际为 `reports/wp4b-cli.log`） | 已由 2.27 修复（17 处路径） | RV2 复核 |
| RV1-WP4-抽样#12 | f39dda9 | RV1-WP4 | crates/app/src/config.rs:460-477 与其测试 | 断言强度不足：`toml_error_detail` 无「单行/无源码片段/长度上限」任何断言，实现退化时用例照过 | 已由 2.27 修复（断言 + 连带修掉源码片段泄漏，见下条裁定） | RV2 复核 |
| RV1-WP5-F1 | f39dda9 | RV1-WP5 | README.md:22、docs/MODULE_ARCHITECTURE.md:383 | **P1 BLOCK**：关闭顺序陈述与权威合同（§12.1/§7.1 第 4 条）及实现相反，且同变更刚改正过该口径 | 已由 2.27 修复 | RV2-WP5 必须复核 |
| RV1-WP5-F2 | f39dda9 | RV1-WP5 | docs/MODULE_ARCHITECTURE.md:481 | **P1 BLOCK**：§5 表下注记声称 `windows-local-ipc`「没有行」，而矩阵最后一行就是它 | 已由 2.27 修复 | RV2-WP5 必须复核 |
| RV1-WP5-F3 | f39dda9 | RV1-WP5 | scripts/check-crate-boundaries.mjs:49-52 | P2：行内注释仍写「9 个列」与「storage-sqlite/server/app 仍只作为行」，与文件头注释及实际 13 列矛盾（RV1-WP1-F2 的范围内项未完成） | 已由 2.27 修复 | RV2-WP5 复核 |
| RV1-WP5-F4 | f39dda9 | RV1-WP5 | docs/CONFIG_REFERENCE.md:51、:110 | P2：`daemon.local_admin.endpoint` 的「显式值用于排查」在当前切片不可用（实现直接失败关闭）但未标注；`storage.flush_interval_ms` 被描述为刷盘间隔而非合并窗口 | 已由 2.27 修复 | RV2-WP5 复核 |
| RV1-WP5-F5 | f39dda9 | RV1-WP5 | docs/DEVELOPMENT_PLAN.md（切片 1 段落与「剩余工作」句） | P2：出现「本切片切片 4」重复措辞，且与 §2 的剩余项口径不一致；文中仍有旧句 | 已由 2.27 修复 | RV2-WP5 复核 |
| RV1-WP5-F6 | f39dda9 | RV1-WP5 | docs/LOCAL_ADMIN_PROTOCOL.md:3 | P2：状态行仍写「实现中」，而本地通道已落地 | 已由 2.27 修复（主 Agent 决定纳入写范围） | RV2-WP5 复核 |
| RV1-WP2-R1（残余风险） | 6361f5d | RV1-WP2 | crates/server/src/transport/local/platform/windows.rs（后续实例） | 风险（非缺陷）：后续实例 DACL 是否继承首实例未证实 | 已登记入 Dependency Handoffs；[PV5] 在 WP3b/WP4 用 `GetSecurityInfo`/跨账号用例钉死 | 待 [PV5] |

## Merge History

（合入前记录唯一 agentic-premerge 块。）

## Test Design and Authoring

不适用（Main E2E mode = not-applicable）。

## Candidate E2E

不适用（mode = not-applicable）。

## Main E2E

- 项目开关：`npx --quiet --no-install openspec-agentic e2e --json` 实测 `enabled=true`、`command=""`、`maxAttempts=3`。
- mode = `not-applicable`；reason/basis/alternative_checks/downgrade_approval 见 `plan.md` 的 Main E2E 块（2026-09-25 本会话用户原话「1. 同意降级」）。
- 替代检查在上方 Checks 表逐项留证（执行后填入）。

## Failures and Retests

| Issue ID / Task | Source / Check or E2E ID / Attempt / Version | Owner | Fix / Recovery | Review / Readiness Evidence | Retest Evidence | Current Status / Basis |
| --- | --- | --- | --- | --- | --- | --- |
| PRO-1（主 Agent 流程异常） | commit `fee6073`（登记 RV1-WP2 记录时误用 `git add -A`，把仍在运行的 WP3b1 部分工作树一并提交） | 主 Agent | 未做历史改写（子 Agent 同 worktree 并发工作，改写有风险）；已 steer 通知 WP3b1、要求其交付提交用显式路径；后续主 Agent 提交一律用显式路径 | 无（不影响代码内容与门禁：`npm run check`/fmt/clippy 均绿） | WP3b1 交付后核对其 handoff 的提交组成与最终 `git status` | 已登记；待 WP3b1 交付时核实完成组成 |
| PRO-2（主 Agent 流程异常，同一根因） | commit `1cd7a3b`（我提交 spec/证据时，并发暂存的 2.21 十二个文件被一并带入；提交信息未提及契约补齐） | 主 Agent | 不做历史改写；自本次起主 Agent 一律用 `git commit --only <paths>`；已在 WP3b2 任务单里明确要求实现者只 add 自身路径 | 无（内容完整：正则三处一致、drift 25 项、fixtures 118/24；门禁与测试全绿） | WP3b2/3.6 复核时会核对 1cd7a3b 的完整组成 | 已登记；`1cd7a3b` 为 2.21 的权威交付提交（内容层面） |
| PRO-3（宿主超时，非产品质量问题） | WP4a 运行 `21e7ff77` 在 1800000ms 后被宿主判定超时中止；中止时未提交也未写交接报告 | 主 Agent | 工作树完好（`crates/app/**` 约 6000 行 + `Cargo.toml`/`Cargo.lock`/`MODULE_ARCHITECTURE.md`），日志显示 `npm run check` 已 EXIT=0；已用 `resume` 复活为运行 `227fce16`，并要求其只做「检查收尾 → 显式路径提交 → 写 wp4a-handoff.md」的限定步骤（集成测试只跑一次、挂住即报告用例名，不弱化断言） | 中止前产物未经验证，不充当证据 | 待 `227fce16` 返回后核对 target SHA 与检查退出码 | 已登记；当前状态 PENDING |
| RV1-WP2-RUN1 | 任务 3.4，reviewer 子 Agent 运行 `ecc2d081-dfcb-41eb-b845-800aceb7512d`（目标 6361f5d）；运行在回报前中止，只输出了开场句，未产生报告 | 主 Agent | 原因判定为子 Agent 运行时中断（非产品/证据问题：目标提交与输入未变）；已用 deepseek/deepseek-flash 重新派发 RV1-WP2 | 旧运行无报告产出，未作为结论 | 待重派结果 | 未解决（等待重派；不影响 WP3a 进行） |

## Final Assessment

（最终验收时填写。）
