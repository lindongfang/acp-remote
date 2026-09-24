# Final E2E 替代验证汇总（`core-turn-view-fields`，not-applicable 路径）

- 固定版本：**`c58c0a0`**（`HEAD == refs/heads/main`，apply 结束时的实际主分支提交；执行时工作区仅含本报告的未跟踪文件）
- 执行者：主 Agent（`plan.md` 的 `not-applicable` 替代验证）
- 执行时间：见 `reports/alt-final-verification.log` 头部 `date -Is`
- 降级依据：`plan.md` 的 `## Main E2E`（`decision: not-applicable` + `reason`/`basis` + 四项 `alternative_checks` + `downgrade_approval`，用户批准原话「同意本变更 Main E2E 记 not-applicable」）；`openspec-agentic e2e --json` 显示 `enabled=true, command=""`（未配置产品级 E2E 命令）。

## 1. 四项替代检查（逐项命令、退出码、断言）

| # | 检查 | 命令（`--locked`） | 退出码 | 关键断言（原始输出） | 日志 |
| --- | --- | --- | --- | --- | --- |
| ① | `[PV2]` core 行为面 | `cargo test -p core --all-features` | 0 | `acp_core` **94 passed / 0 failed / 0 ignored**（含注入、冲突失败关闭、版本一致与漂移、重放不重复注入、imported 保真、晚到 delta 无归属、非字符串 `turnId` 失败关闭、`expected_version` 无状态变更分支） | `reports/alt-final-verification.log` §① |
| ② | `[PV5]` 真实存储面 | `cargo test -p storage-sqlite --all-features` | 0 | 12 个测试二进制全部 ok（含 `session_version_rule` **3 passed**、`commit` 32 passed、`admin_store` 15 passed），**2 ignored 为既有 `#[ignore]`**（`crash_child`、`regenerate_v2_fixtures`） | `reports/alt-final-verification.log` §② |
| ③ | `[PV3]` 跨 crate 契约面 | `cargo test -p core -p agent-host --all-features` | 0 | `agent_host` 合计 **53 passed**（`catalog` 21 + `session` 13 + `supervision` 15 + `lib` 3 + **`view_contract` 1**）+ `acp_core` 94 passed；`view_contract` 证明适配器产出的 view **不含** `turnId`/`version`、`EndpointEvent.turn` 为 `None`、§10.3 其余最低字段齐备、core-only 类型不由适配器产出 | `reports/alt-final-verification.log` §③ |
| ④ | `[PV4]` 统一入口 | `npm run verify` | 0 | 十道合同门禁全 OK + `cargo fmt --check` + `clippy --workspace --all-targets --all-features -D warnings` + 全工作区测试 **398 passed / 0 failed / 2 ignored（54 个测试目标）** | `reports/alt-final-verification.log` §④ |
| ⑤ | `[PV1]` 合同门禁单跑 | `npm run check` | 0 | `schema fixtures`/`command catalog`/`error registry`/`feature registry`/`contract assets`/`acp matrix`/`doc links`/`crate boundaries`/`contract drift`/`agentic` 十道 OK | `reports/alt-final-verification.log` §⑤ |

**断言覆盖核对（对照 `plan.md` 的四项 `alternative_checks`）**：

- `[PV4]` ⇒ 覆盖「依赖集/端口签名/DDL/矩阵未漂移 + 无回归」：`check:boundaries`（8 crate 与 §5 矩阵一致）、`check:contract-drift`（§7 的 36 条 DDL 与 `migrate.rs` 一致；§5 的 15 trait / 87 方法签名与 `ports.rs` 一致）、全工作区 398 passed。
- `[PV2]` ⇒ 覆盖注入行为、冲突失败关闭、版本一致与漂移失败、重放不重复注入、ACP 保真与未知字段往返。
- `[PV5]` ⇒ 覆盖真实 SQLite 上的版本规则与注入一致性（`session_version_rule` 3 条 + `commit`/`admin_store` 的原子提交路径）。
- `[PV3]` ⇒ 覆盖「适配器半」的字段归属；「core 半」（注入由 core 决定）由 `[PV2]` 的 `the_turn_from_the_endpoint_is_never_trusted` 等用例共同闭合，两侧合起来覆盖 §10.3 的 `turnId`/`version` 契约。

## 2. 资源与副作用核对

| 项 | 事实 | 结论 |
| --- | --- | --- |
| 临时 SQLite 文件 | 仓库根目录无 `.db`/`.sqlite*`/`acpr-*` 残留（用例均在 `tempfile` 目录内创建并自行清理；`storage-sqlite` 的 `migration`/`commit` 用例保留既有 `#[ignore]` 的破坏性用例不执行） | 无泄漏 |
| 子进程 | 运行后 `ps` 中 `acpr-fake-acp-agent`/`cargo`/`rustc` 匹配数为 **0**（`agent-host` 的 fake 子进程由 `Drop` + Job Object 回收，`supervision` 用例覆盖超时/取消/崩溃路径） | 无遗留 |
| `CARGO_TARGET_DIR` | 未设置（使用默认 `target/`，25G 增量缓存）；本次未新增构建目录，未做 `cargo clean` | 无额外资源 |
| 资产哈希不变 | 执行前后 `git status --porcelain` 均为空 ⇒ `fixtures/**`、`compatibility/acp/v1/matrix.json`、`schemas/**`、`Cargo.lock` 与 HEAD（`c58c0a0`）逐字节一致（`git diff --stat HEAD -- fixtures compatibility schemas Cargo.lock` 为空）；`check:acp` 按 sha256 pin 校验上游 schema 与矩阵也 exit 0 | 通过 |
| 未执行项 | `cargo-deny`、`gitleaks`（仅 CI，本机无等价物）；产品级 E2E（未配置命令） | 如实声明 |

## 3. 结论

- **替代验证：PASS**（5/5 命令退出码 0；四项 `alternative_checks` 全部覆盖并留证；无资源泄漏；无资产漂移）。
- 结论适用于 `c58c0a0`；其后若产生新的主分支提交，仅当改动限于记录/注释时按变更记录的适用性口径复用本证据，并在 `verification.md` 的 Final Assessment 中如实注明。
