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
| — | — | — | — | 尚未进行独立 review | — | — |

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

无（尚无执行失败）。

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
