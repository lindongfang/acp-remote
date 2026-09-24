# verification — identity-auth-and-keystore

## Target

- 变更：`identity-auth-and-keystore`（schema `agentic`，changeDir `D:\Project\acp-remote\openspec\changes\identity-auth-and-keystore`）
- 仓库：`D:\Project\acp-remote`；约定目标 `refs/heads/main`
- 目标核实：主 Agent，`git rev-parse refs/heads/main` = `30f0d78b803b94346ebd8972078febceec8af8d3`（2026-09-24，工作区仅 `openspec/changes/identity-auth-and-keystore/` 未跟踪）
- 实现分支：`feat/identity-auth-and-keystore`（起点同 `30f0d78`；主 worktree，无额外 worktree）
- 版本确认负责人：主 Agent；机械核实方式为 `git -C D:\Project\acp-remote rev-parse refs/heads/main`、`git status --porcelain`、`git worktree list`（各次执行在 6.1 重新固定）

## Checks

| Check ID / Stage / Work Package | Revision / Base | Scope | Executor | Command / Steps | Environment | Result / Exit Code | Evidence |
| --- | --- | --- | --- | --- | --- | --- | --- |
| W0 baseline / 1.1 / 全变更 | `30f0d78`（main，未改动） | 合同门禁与既有 workspace 测试的基线是否可绿 | 主 Agent | `npm run check`；`cargo test --locked --workspace --all-features` @ 仓库根 | Rust 1.98.1（`rust-toolchain.toml`）、cargo 1.98.1、Node v24.19.0、npm 12.0.2、`node_modules` 已就位 | PASS（check exit 0；tests exit 0，无 failed、1 ignored） | `reports/w0-baseline-check.log`、`reports/w0-baseline-tests.log` |
| WP1 2.3 / WP1 / 全变更 | `30f0d78` + docs/依赖改动 | 文档引用归属、命令目录、合同漂移与既有 8 crate 的依赖方向（成员尚未加入） | 主 Agent（coder） | `npm run check`；`node scripts/check-crate-boundaries.mjs` @ 仓库根 | 同上 | PASS（check exit 0：doc links 367、drift OK、9 items passed；boundaries exit 0：8 个 crate 与 §5 矩阵一致） | `reports/du1-pv1.log`、`reports/wp1-boundaries.log` |
| PV3 / WP2 / 任务 2.4 | 实现分支（`crates/identity-auth` 加入 members） | 12 个固定向量逐字节重算 + 摘要 + HMAC + 验签 + SAS + 畸形输入负例 | 主 Agent（coder） | `cargo test --locked -p identity-auth --all-features --test transcripts` | Rust 1.98.1 / cargo 1.98.1 | PASS（exit 0，5 passed / 0 failed） | `reports/wp2-identity-auth-transcripts.log` |
| PV3 / WP2 / 任务 2.5、2.6、2.7、2.8 | 同上 | 配对状态机（R1–R29）、握手（R30–R52）、授权展开（R53–R68）、端口与秘密类型（R69–R71） | 主 Agent（coder） | `cargo test --locked -p identity-auth --all-features --test pairing`；同法运行 `--test handshake`、`--test authorization`、`--test ports` | 同上 | PASS（20 + 17 + 15 + 9 passed / 0 failed；含 2 个 `compile_fail` 文档测试） | `reports/wp2-identity-auth-pairing.log`、`reports/wp2-identity-auth-handshake.log`、`reports/wp2-identity-auth-expansion.log`、`reports/wp2-identity-auth-ports.log` |
| PV3（全量）/ WP2 / 任务 2.9 | 同上 | crate 全部目标与文档测试、无零用例/全跳过 | 主 Agent（coder） | `cargo test --locked -p identity-auth --all-features`；`cargo clippy --locked -p identity-auth --all-targets --all-features -- -D warnings`；`cargo fmt --all -- --check` | 同上 | PASS（75 个用例通过、0 failed；clippy/fmt 无告警） | `reports/wp2-identity-auth-full.log` |
| PV2 / WP2 / 任务 2.9 | 同上 | 成员加入后 `cargo metadata` 的实际依赖方向（`identity-auth` 只依赖 core 与三个协议/叶子 crate） | 主 Agent（coder） | `node scripts/check-crate-boundaries.mjs` | 同上 | PASS（exit 0：9 个 crate 与 §5 矩阵一致，含新成员） | `reports/wp2-boundaries.log` |
| PV1（回归）/ WP2 / 任务 2.9 | 同上 | 合同门禁在成员加入后仍绿（文档引用、命令目录、合同漂移、封闭词表、agentic 门禁） | 主 Agent（coder） | `npm run check` | 同上 | PASS（exit 0：Installation PASS、9 items passed、0 failed） | `reports/wp2-npm-check.log` |
| 自检 / WP2 / 任务 2.9 | 同上 | 平台无关与「持锁不跨 `await`」 | 主 Agent（coder） | `rg -n "cfg(windows)|cfg(unix)|cfg(target_os" crates/identity-auth/src`（零命中）；`rg -n ".await" crates/identity-auth/src` 对照持锁位置 | 同上 | PASS（`cfg` 零命中；9 处 `.await` 全部在锁外调用 keystore/公钥读取） | 见本行结论（命令输出随提交记录在 `reports/`） |
| 回归 / 全变更 | 同上 | 既有 crate 未因依赖改动（`hmac` 0.12→0.13、新增 `getrandom`/`windows-dpapi` 登记）而回退 | 主 Agent（coder） | `cargo test --locked --workspace --all-features`；`cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | 同上 | PASS（workspace 测试 exit 0；clippy 无告警） | `reports/wp2-workspace-tests.log` |
| 1.4 / WP3 | 实现分支 `5c9d2c4`（WP2 冻结形状） | WP3 只使用 `identity-auth` 的公开项且不依赖 `core` | 主 Agent（coder） | `cargo build -p identity-keystore`；`node scripts/check-crate-boundaries.mjs` | Rust 1.98.1 / cargo 1.98.1 | PASS（构建成功；boundaries exit 0：10 个 crate 与 §5 矩阵一致） | `reports/wp3-boundaries.log` |
| PV4 / WP3 / 任务 2.10、2.12、2.13、2.14 | 实现分支 | 条目往返、删除后不可用、孤儿回收、引用恢复、篡改不覆盖、失败关闭零写入、秘密不落明文 | 主 Agent（coder） | `cargo test --locked -p identity-keystore --all-features` | 同上（Windows x64，当前用户） | PASS（29 个用例通过、0 failed：单元 1 + entry 6 + fail_closed 5 + dpapi 5 + keystore 12） | `reports/wp3-identity-keystore.log` |
| PV5 / WP3 / 任务 2.12 | 同上 | 真实 DPAPI：包裹/解开往返、附加熵绑定、篡改与截断检测、条目级剪接检测、进程内签名 | 主 Agent（coder） | `cargo test --locked -p identity-keystore --all-features dpapi -- --nocapture` | Windows x64，当前用户 scope | PASS（exit 0，5 passed / 0 failed；Linux CI 不覆盖该路径） | `reports/pv5-windows-dpapi.log` |
| 2.11 / WP3 | 同上 | wrapper 传递依赖、许可证集合、MSRV/edition 与语义一致性实证 | 主 Agent（coder） | `cargo tree -p identity-keystore --target x86_64-pc-windows-msvc`；`cargo metadata --filter-platform x86_64-pc-windows-msvc` | 同上 | 合格（许可证全部落在 `deny.toml` allow 列表；MSRV ≤ 1.85；已知代价见「Dependency Handoffs」） | `reports/wp3-dpapi-verification.log` |
| PV2/PV1（回归）/ WP3 / 任务 2.15 | 同上 | 新成员加入后合同门禁与依赖方向仍绿 | 主 Agent（coder） | `npm run check`；`node scripts/check-crate-boundaries.mjs` | 同上 | PASS（check exit 0，9 items passed；boundaries exit 0） | `reports/wp3-npm-check.log`、`reports/wp3-boundaries.log` |
| 2.15 / WP3 | 同上 | `.gitleaks.toml` 增加自研条目明文形态规则，并如实记录「包裹后字节不可识别」的限制 | 主 Agent（coder） | 人工检视规则与注释；`gitleaks` 本地无等价物 | 同上 | 规则已加；**真实执行留 CI**（不声称本地通过） | `.gitleaks.toml` 注释与「Known limitations」 |
| PV1/PV2 / WP4 / 任务 2.16、2.17 | 实现分支（成员已加入） | 状态表/版本口径与 `cargo metadata`、`cargo tree` 实际一致；全量统一入口在固定版本上绿 | 主 Agent（coder） | `npm run verify`（= `npm run check` + `cargo fmt/clippy/test` 三条）；`node scripts/check-crate-boundaries.mjs` | Rust 1.98.1 / cargo 1.98.1 / Node v24.19.0 | PASS（verify exit 0：67 个测试二进制全 ok，无 failed；boundaries exit 0：10 个 crate） | `reports/du1-pv1.log`、`reports/wp4-boundaries.log` |
| PV1/PV2（WP1 交付前）/ 3.1 | WP1 提交 `ce5b3fc` | 合同门禁 + 既有 8 crate 依赖方向 | 主 Agent（coder） | `npm run check`；`node scripts/check-crate-boundaries.mjs` | Rust 1.98.1 / Node v24.19.0 | PASS（check exit 0；boundaries exit 0：8 个 crate，成员尚未加入） | `reports/du1-pv1.log`（WP1 轮次；无 `bv-pv1.log`——分支验证轮次只重跑了 PV2–PV5，PV1 以候选轮的 `reports/du1-pv1.log` 为准） |
| PV3/PV2/PV1（WP2 交付前）/ 3.3 | WP2 提交 `5c9d2c4` | crate 全量用例、依赖方向、合同门禁 | 主 Agent（coder） | `cargo test --locked -p identity-auth --all-features`；`node scripts/check-crate-boundaries.mjs`；`npm run verify` | 同上 | PASS（75 用例通过、0 failed；boundaries 9 crate；check 9 items passed） | `reports/wp2-identity-auth-full.log`、`reports/wp2-boundaries.log`、`reports/wp2-npm-check.log` |
| PV4/PV5/PV1（WP3 交付前）/ 3.5 | WP3 提交 `627bb8e` | keystore 全量用例、真实 DPAPI 用例、合同门禁 | 主 Agent（coder） | `cargo test --locked -p identity-keystore --all-features`；`cargo test … dpapi -- --nocapture`；`npm run check` | Windows x64 / Rust 1.98.1 | PASS（29 用例通过；dpapi 5 用例通过；check exit 0） | `reports/wp3-identity-keystore.log`、`reports/pv5-windows-dpapi.log`、`reports/wp3-npm-check.log` |
| PV1/PV2（WP4 交付前）/ 3.7 | WP4 提交 `6861046`（= 候选 HEAD） | 全量统一入口 + 10 crate 依赖方向 | 主 Agent（coder） | `npm run verify`；`node scripts/check-crate-boundaries.mjs` | Rust 1.98.1 / cargo 1.98.1 / Node v24.19.0 | PASS（verify exit 0：67 个测试二进制全 ok；boundaries exit 0：10 个 crate） | `reports/du1-pv1.log`、`reports/wp4-boundaries.log` |
| 候选轮次 / 分支验证（可独立复用）/ 3.1–3.7 | 候选 HEAD `6861046` | PV1–PV5 在候选版本上的独立一轮（供 6.3 复用，避免把 WP 轮次当作候选轮次） | 主 Agent（coder） | `cargo test --locked -p identity-auth --all-features`；`cargo test --locked -p identity-keystore --all-features`；`cargo test … dpapi -- --nocapture`；`node scripts/check-crate-boundaries.mjs`；`npm run verify` | 同上 | PASS（PV3 7 个测试二进制、PV4 6 个、PV5 5 个 DPAPI 用例；boundaries 10 crate；verify exit 0） | `reports/bv-pv3.log`、`reports/bv-pv4.log`、`reports/bv-pv5.log`、`reports/bv-pv2.log`、`reports/du1-pv1.log` |
| PV1–PV5（RV1 修复轮）/ 3.1–3.8 | RV1 结束后的工作树（提交见 Merge History） | 修复 WP3 的 P0/P1 与 WP1/WP2/WP4 的 P2 后重跑全部证据项 | 主 Agent（coder） | `npm run verify`；`node scripts/check-crate-boundaries.mjs`；`cargo test --locked -p identity-auth --all-features`；`cargo test --locked -p identity-keystore --all-features`；`cargo test … dpapi -- --nocapture`；`cargo clippy --locked -p identity-auth -p identity-keystore --target x86_64-unknown-linux-gnu --all-targets --all-features -- -D warnings` | Rust 1.98.1 / cargo 1.98.1 / Node v24.19.0；额外安装了 `x86_64-unknown-linux-gnu` 目标用于交叉编译与 lint | PASS（verify exit 0：68 个测试二进制全 ok（identity-auth 73 用例、identity-keystore 32 用例）；boundaries exit 0：10 个 crate；Linux 目标 clippy 无告警） | `reports/rv2-pv1.log`、`reports/rv2-pv2.log`、`reports/rv2-pv3.log`、`reports/rv2-pv4.log`、`reports/rv2-pv5.log`、`reports/rv2-linux-clippy.log` |
| 自检（RV1 后补）/ WP2 / 3.3 | 同上 | 计划自检项「`unwrap()`/`expect()`/`from_der` 在非测试代码零命中」 | 主 Agent（coder） | `grep -rn "unwrap()\|expect(\|panic!\|unreachable!\|from_der" crates/identity-auth/src crates/identity-keystore/src` | 同上 | **不符预期，如实记录**（identity-auth 1 处：`types.rs:256` 的 `Digest` 构造不变式；identity-keystore 3 处：`entropy.rs:37-38` 为**测试函数**内的固定断言、`ephemeral.rs:50` 为新建实例不可能中毒）——三处都不是可失败的外部输入路径；原计划措辞「零命中」过强，本轮修正为「可失败路径零 `unwrap`/`expect`」 | `reports/rv2-selfcheck-rg.log` |

## Check Plan Changes

无（尚未发生需求/接口澄清或检查清单调整）。

## Dependency Handoffs

- 上游已验收提交：无（本变更不依赖其它 in-flight 变更）
- WP3 ← WP2（任务 1.4）：交接版本 = 本分支 `5c9d2c4`（WP2 冻结的公开形状）；WP3 只使用 `identity-auth` 的公开项
  （`IdentityKeystore`/`EntropySource` 端口、`KeyHandle`/`SecretBytes`/`P1363Signature`/`KeystoreError`/`KeyPurpose`/`SecretPurpose`）。
  为此在 WP2 的 `identity-auth` 上补齐了 `core::model` 值对象的如实转出（§5 矩阵不允许 `identity-keystore -> core`），
  证据：`reports/wp3-boundaries.log`（10 个 crate 与 §5 矩阵一致）。
- 2.11 DPAPI wrapper 实证结论（供 2.16 采用）：**候选合格** —— `windows-dpapi 0.2.0`。
  理由：MIT OR Apache-2.0（在 `deny.toml` allow 列表内）、edition 2021、只经安全 API（本仓库 workspace 固定
  `unsafe_code = "forbid"`）、语义为「当前用户 scope 包裹字节 + 进程内解开」，与 `SECURITY_DESIGN.md` §20 的 DPAPI 档位一致；
  传递依赖 `anyhow 1.0.104`（MSRV 1.68）、`log 0.4.34`（MSRV 1.71）、`winapi 0.3.9`。
  已知代价（如实登记，不当作通过声明）：(a) 传递依赖 `winapi 0.3` 上游已停止维护，且 advisory 判定只在 CI 的 `advisories` job，本地无等价物；
  (b) wrapper 未暴露 `CRYPTPROTECT_UI_FORBIDDEN`，本实现因此**总是**传入由条目头派生的附加熵，把「无熵 + 缺主密钥时可能弹交互提示」收敛为有熵的静默路径；
  (c) wrapper 未声明 `rust-version`（edition 2021，实测可在 1.98.1 上编译；MSRV 1.85 未逐条验证）。
  证据：`reports/wp3-dpapi-verification.log`、`reports/pv5-windows-dpapi.log`。

## Runtime Resources

- 实际使用：`target/`（仓库根，串行使用，无并发执行者）；`openspec/changes/identity-auth-and-keystore/reports/`（日志，`.gitignore` 已忽略 `**/reports/**/*.log`）
- 未涉及数据库、容器、端口、外部服务、共享账号或网络资源（依据：本变更只新增两个库 crate，不接读写存储，不监听端口）
- DPAPI 用例（[PV5]）使用本机 Windows x64 当前用户 scope 与系统临时目录（`std::env::temp_dir()`）下的唯一子目录
  （`acpr-keystore-<tag>-<pid>-<n>`），按计划串行轮次执行；每个用例通过 `TempRoot` 自建并在结束时删除，
  证据：`reports/pv5-windows-dpapi.log`（5 个用例全绿；执行后无残留目录）

## Review Findings

| ID | Revision | Reviewer | Location | Severity / Impact | Resolution | Recheck Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| RV1-WP1 | `6861046` | 独立 reviewer 子 Agent（`0c414ea8`，未继承实现对话、只读） | `docs/IDENTITY_AND_AUTH_CONTRACT.md`、`docs/MODULE_ARCHITECTURE.md`、`openspec/changes/…/{design,verification}.md` | **PASS**；P2 × 6（均非阻断）：合同状态行仍写「未落地」、§2 类型清单用了未落地名、§4.1/§5.1 入口签名仍写「返回写集」且 `PeerTrust` 缺 `node_kind`/`host_binding`、§3.1 `hmac 0.12` 与 `Cargo.toml` 不一致、`verification.md` 引用了不存在的 `bv-pv1.log`、`design.md` D3/D4 仍描述返回写集 | 6 条全部修复：合同升版 0.3（§2/§4.1/§5.1 对齐实现并解释「返回领域值、调用方组装写集」的取舍）、`MODULE_ARCHITECTURE.md` §3.1 改 `hmac 0.13` 并补版本变更记录与证据、`design.md` D3/D4 对齐 + 偏离说明、`verification.md` 删除 `bv-pv1.log` 引用 | 本表 RV1 行 + `reports/rv2-pv1.log`（doc links 含 contract 的 369 链接 / 3454 章节引用全绿） |
| RV1-WP2 | `6861046` | 独立 reviewer 子 Agent（`1a30da02`） | `crates/identity-auth/**` | **PASS**（无 CRITICAL/MAJOR）；P2 × 5：R54「认证成功后才消费配对」正向往无用例、契约签名与实现漂移、D8 措辞无落点、计划自检项未留证、`PeerTrust.peer` 从未被读且 `is_usable()` 无调用者（另 P2-7 挑战缓存无上限） | 已修：新增 `first_authentication_consumes_the_approved_pairing_once`（R54 正向）、`approved_pairing_still_accepts_status_proofs_before_first_auth`、`approved_secret_is_cleared_at_expiry_even_if_never_authenticated`；**并因此发现真实缺陷**——`settle` 原先在批准/拒绝时就清除 secret，会让批准后的 `pairing-status` HMAC 证明与 `consume_pairing` 都不可用，现改为「首次认证成功或到达 `expires_at` 时清除」；挑战缓存加上 `MAX_CHALLENGES = 1024` + 过期清扫 + 最早过期淘汰（`challenge_cache_is_bounded`）；`verify_proof` 增加快照主体一致性检查（`trust.peer != submission.peer` → `UntrustedPeer`，`trust_snapshot_must_belong_to_the_same_peer`）；删除无调用的 `PeerTrust::is_usable`；固定向量用例改成精确断言（域分派错位返回 `None` 会失败） | `reports/rv2-pv3.log`（73 用例全绿）、本表 RV2-WP2 行 |
| RV1-WP3 | `6861046` | 独立 reviewer 子 Agent（`6aa306be`） | `crates/identity-keystore/**`、`.gitleaks.toml` | **FAIL**：P0 —— Linux 上 `delete_secret` 返回 `SecretMissing` 而非 `Unavailable`，`tests/fail_closed.rs` 在 Linux CI 会失败；P1 —— 读取类入口（`get_secret`）不可用平台返回 `Ok(None)` 而非显式不可用，失败关闭不完整；P2 × 9（R74 缺双向用例、`write_atomic` 清理不完整、Unix 权限位无用例、注释与 `cfg(unix)` 矛盾、label 形状未校验、`ephemeral` 锁中毒 `expect`、R86 死分支、明文堆副本未清零、`.gitleaks.toml` 注释过度声明） | 已修：引入 `Availability` 显式闸门（`Platform`/`Unavailable`），**全部 7 个入口**第一步即 `require()` → `Unavailable`（`delete_secret`/`get_secret` 不再伪装成「条目缺失」）；`with_availability` 作为测试/审计白盒缝（Linux CI 因此能真跑这些判定）；`read_secret` 返回 `SecretBytes`；`write_atomic` 用 `TempFileGuard` + 序号后缀保证失败清理；新增 `is_valid_label` 并在 `decode` 中校验；`ephemeral` 锁中毒返回 `Unavailable`；新增 `tests/permissions.rs`（`cfg(unix)`，0700/0600）；R74 双向断言（不同条目不同公钥 + 交叉验签失败）；R86 死分支改为真实两阶段替换语义；`.gitleaks.toml` 注释改为只声明可模式识别的明文形态 | `reports/rv2-pv4.log`（32 用例全绿）、`reports/rv2-linux-clippy.log`（Linux 目标编译+lint）、本表 RV2-WP3 行 |
| RV1-WP4 | `6861046` | 独立 reviewer 子 Agent（`3d9e3123`） | `docs/MODULE_ARCHITECTURE.md`、`README.md`、`docs/DEVELOPMENT_PLAN.md`、`AGENTS.md` | **FAIL（MAJOR）**：状态表/合同口径声称的 `hmac` 版本与 `Cargo.toml`/`Cargo.lock` 实际不一致（文档写 `0.12`，实际已是 `0.13`） | 已修：`MODULE_ARCHITECTURE.md` §3.1 改为 `hmac 0.13`，并按该节既有要求补一条 2026-09-24 版本变更记录（理由、MSRV、许可证、消费方、证据）；`design.md`/`plan.md` 同步 | `reports/rv2-pv1.log`；本表 RV2-WP4 行 |

## Merge History

尚未开始集成（等待第 3 组交付前验证完成）。

## Test Design and Authoring

不适用（plan 的 Main E2E mode = `not-applicable`，无 TP 与 E2E 用例设计）。

## Candidate E2E

不适用（mode = `not-applicable`）。

## Main E2E

- 项目开关（`openspec-agentic e2e --json`，2026-09-24）：`enabled = true`、`command = ""`、`maxAttempts = 3`
- `plan.md` 的 mode：`not-applicable`；降级批准来源：本会话用户原话「1和2都同意」（对应提问第 1 问：Main E2E 记 `not-applicable` 及其替代验证清单）
- reason / basis / alternative_checks 见 `plan.md` 的 Main E2E 块；E2E 结论记 `NOT_APPLICABLE`，替代检查在 `## Checks` 中逐项留证（尚未执行）

## Failures and Retests

- **RV1-WP3 判定 FAIL（2 条阻断）与修复重测**：在 `6861046` 上，独立 reviewer 发现 (a) P0：非 Windows 平台的 `delete_secret` 返回 `SecretMissing`（Linux CI 的 `fail_closed` 用例必然失败）；(b) P1：`get_secret` 在不可用平台返回 `Ok(None)`，失败关闭不完整。
  - 复现依据：本地无 Linux 运行时（无 WSL/Docker），因此不能直接执行 Linux 用例；改用**平台注入缝**（`FileKeystore::with_availability`）在 Windows 上重现同一分支。
  - 修复：7 个入口统一以 `Availability::require()` 开头（`Unavailable` 是显式错误，不再与「条目缺失」混淆）；`tests/fail_closed.rs::unavailable_backend_fails_closed_on_every_entry_point` 对 7 个入口逐一断言并检查零写入；`tests/fail_closed.rs::unavailable_platform_writes_nothing_at_all` 保留真实平台路径（Linux CI 上真实执行）。
  - 重测：`reports/rv2-pv4.log`（identity-keystore 32 用例全绿，含 fail_closed 8 条）、`reports/rv2-linux-clippy.log`（`--target x86_64-unknown-linux-gnu --all-targets` 编译+lint 通过，证明 Linux 分支可编译且无告警）、`reports/rv2-pv1.log`（`npm run verify` exit 0）。
  - **残余限制（不声称已消除）**：Linux 用例的**运行时**行为仍未在本机执行，实际判定依赖 CI 的 Linux runner；本机只提供编译、lint 与注入缝证据。
- **RV1-WP4 判定 FAIL（1 条 MAJOR）与修复重测**：文档/状态表把 `hmac` 写成 `0.12`，与 `Cargo.toml` 实际 `0.13` 不一致（WP1 已升级但文档漏改）。
  - 修复：`docs/MODULE_ARCHITECTURE.md` §3.1 改为 `hmac 0.13`；按该节机器化口径补版本变更记录（含 MSRV 1.85、`MIT OR Apache-2.0`、消费方与证据路径）。
  - 重测：`reports/rv2-pv1.log`（合同门禁全绿，含 doc-links 与 contract-drift）。
- **RV1-WP2 的 P2-1（缺 R54 正向用例）触发一处真实行为缺陷**：`settle` 原实现在批准/拒绝时立即清除内存 secret，导致「已批准配对仍可用 secret 做 `pairing-status` HMAC 证明」与「首次认证成功时给出 `consume_pairing`」两条合同要求都不可能满足。
  - 修复：`settle` 不再清除；`due_pairings` 对**所有**到达 `expires_at` 的记录清除 secret（secret 生命周期的硬上界），返回值仍只含未终结记录；新增 3 条用例覆盖「首次认证消费一次」「批准后仍可验状态证明」「未认证也按过期清除」。
  - 重测：`reports/rv2-pv3.log`（identity-auth 73 用例全绿，pairing 23 条）。
- **`npm run verify` 在修复轮出现过一次失败（已修复）**：给合同 §5.1 补签名块时引入了一个多余的代码围栏，导致 `check:docs` 把 §6–§8 标题当成围栏内文本（10 条章节引用报错）。
  - 修复：删除多余围栏；`reports/rv2-pv1.log` 中 `doc links OK: 369 relative links, 3454 section refs`。

## Final Assessment

尚未进入最终验收。

```agentic-assessment
assessment_id: "pending"
target_commit: "pending"
contract_digest: "pending"
result: BLOCKED
evidence: []
```

- Assessment ID / Time: 待最终验收轮次填写
- Target / Task: 待最终验收轮次填写
- CLI State: 待最终验收轮次填写
- Audit / Evidence: 待最终验收轮次填写
- Result / Open Issues: 待最终验收轮次填写
- Required Follow-up: 待最终验收轮次填写
