# WP4 2.27 交接：处置 RV1-WP4 与 RV1-WP5 的全部发现

```yaml
task_id: 2.27（处置 RV1-WP4/WP1-WP5 发现，RV2 前修正）
role: 实现 Agent（coder，受限 worker；只写 README.md、docs/**、AGENTS.md（未涉及）、
      scripts/check-crate-boundaries.mjs（注释）、crates/app/**、
      openspec/changes/daemon-cli-and-local-admin/plan.md（仅证据路径与 PV4 证据列）与 reports/wp427*）
phase: apply（RV1 → RV2 之间的修正轮）
agent_context: >
  RV1 有三轮独立检视落在 f39dda9：RV1-WP4（PASS，4 条 SUGGESTION/MINOR）与 RV1-WP5（FAIL，2 条 BLOCK +
  4 条 P2）。本轮按两份报告逐条给出的**最小修复**改动，不作无关重排/重写；唯一超出「文本修复」的
  是 K 项断言在执行时暴露的实现缺陷（见 §4），它必须修好才能让 K 项判据成立且不弱化断言。
target_revision:
  branch: feat/daemon-cli-and-local-admin
  base: 7e37f9b（本轮起点 HEAD；本轮期间主 Agent 先落了一个提交 7e37f9b「登记 RV1-WP4/WP1-WP5 结论并新增任务 2.27」）
  implementation_commit: e7c01ec、69874d9、35f8f2b、fd0ad86（docs 顺序/注记/依赖登记 → plan.md 证据路径 → 合并窗口快照断言 → TOML 诊断）
  head_at_handoff: fd0ad86（本报告为紧随其后的第五个提交）
scope:
  - README.md（`app` 行的关闭顺序）
  - docs/MODULE_ARCHITECTURE.md（§4.10 关闭顺序与合并窗口措辞、§5 表下注记、§3.1 依赖登记）
  - docs/CONFIG_REFERENCE.md（`daemon.local_admin.endpoint` 实现状态、`storage.flush_interval_ms` 说明）
  - docs/DEVELOPMENT_PLAN.md（重复「本切片」与两处残留旧句）
  - docs/LOCAL_ADMIN_PROTOCOL.md（第 3 行状态行；方法表/词表/错误码未动）
  - scripts/check-crate-boundaries.mjs（`readDependencyMatrix` 的行内注释，行为未动）
  - crates/app/Cargo.toml（`[[bin]]` 上方陈旧注释）
  - crates/app/src/daemon.rs（两处合并窗口 spy 断言改为冻结快照一致读）
  - crates/app/src/config.rs（新增 TOML 诊断单测 + `toml_error_detail` 的最小修正，见 §4）
  - openspec/changes/daemon-cli-and-local-admin/plan.md（17 处证据路径 + PV4 证据列）
  - openspec/changes/daemon-cli-and-local-admin/reports/{wp427.log, wp427-handoff.md}
未改动: openspec/changes/** 的 proposal/design/specs/tasks/verification、schemas/**、fixtures/**、
        compatibility/**、crates/{core,server,storage-sqlite,identity-*}/**、关闭序列的实现顺序与任何 wire 语义。
```

## 1. 逐项修复位置与改法（F1–F6、G、H、I、J、K）

| 项 | 结论 / 位置 | 改法（一句话） |
|---|---|---|
| F1（WP5 BLOCK） | `README.md:22`、`docs/MODULE_ARCHITECTURE.md:383` | 关闭顺序统一为「停接入层 → 取消周期任务 → 停 Agent → `wal_checkpoint(TRUNCATE)` → 释放锁」；README 补「详见 `SECURITY_DESIGN.md` §12.1」，§4.10 补「排空在途连接至宽限上限」「取消周期任务**与信号监听**」「清理 endpoint/释放锁」与两处权威指向（`CORE_PORTS_AND_STORAGE.md` §7.1 第 4 条、`SECURITY_DESIGN.md` §12.1）。`grep -rn "停周期任务" docs README.md AGENTS.md` 零命中（改前 2 处，改后 0 处，无第三处） |
| F2（WP5 BLOCK） | `docs/MODULE_ARCHITECTURE.md:481`（§5 表下注记） | 把「`windows-local-ipc` 只作为「列」出现、没有行」改为「是矩阵的**列**（因为 `server` 依赖它），在矩阵里**也有一行**（该行除自身格外全空白——没有任何依赖方）；…永远不是 workspace 成员（§3.1），其依赖面不进门禁判定，因此那一行只是占位」 |
| F3（WP5 P2） | `scripts/check-crate-boundaries.mjs:49-52` | 行内注释改为与文件头一致的口径：**13 个列**（12 个 workspace 成员 + `vendor/windows-local-ipc`），`node-link-client` 仍只作为（列外的）行；并补「行缺席只上报、不硬失败（下面的 `if (!row) continue`），因此行与列的增减必须人工维护」。**只改注释**，判定逻辑一字未动（`npm run check:boundaries` EXIT=0） |
| F4（WP5 P2） | `docs/CONFIG_REFERENCE.md:51`、`:110` | `daemon.local_admin.endpoint` 行末补与 `instance_lock` 同风格的实现状态注记（本切片只接受 `auto`，显式值在配置解析阶段即以 `ConfigError` 失败关闭，待 `server::transport::local` 提供显式路径入口后再放开）；`storage.flush_interval_ms` 说明改为「同一会话短窗口内 delta 合并为一次 `commit` 的窗口（broker 的合并窗口，由组合根定时器…驱动 `Broker::pump`；`CORE_PORTS_AND_STORAGE.md` §6 第 10 条；**不是**存储刷盘间隔）」 |
| F5（WP5 P2） | `docs/DEVELOPMENT_PLAN.md:24`、`:77` | §3 切片 1 段去掉重复的「本切片」并与同文 §2 对齐（「切片 4 已完成 Daemon/CLI 接线与本地管理通道…，本切片剩余的是**端到端验收**（`server::sync`/`server::node_link`/`server::acp_facade` 与 `node-link-client` 属后续切片）」）；§4 末「建议的第一项实施变更」的「剩余工作是 Daemon/CLI 接线与端到端验收」改为「Daemon/CLI 接线也已随切片 4 落地，剩余工作是端到端验收」 |
| F6（WP5 P2） | `docs/LOCAL_ADMIN_PROTOCOL.md:3` | 状态行「切片 4…实现中」改为「切片 4 的**本地通道部分已落地**（`server::transport::local` + `server::local_admin` + `app`），`server::acp_facade` 待切片 6」。方法表/词表/错误码与 §3.1 注记均未动 |
| G（WP4-F2 MINOR） | `docs/MODULE_ARCHITECTURE.md:382`（+`:383` 同 F1） | §4.10 后台任务清单的「存储批量刷盘」改为「broker 的 delta 合并窗口（`storage.flush_interval_ms`；由组合根定时器枚举非终态会话并逐个驱动 `Broker::pump`，`CORE_PORTS_AND_STORAGE.md` §6 第 10 条）」；关闭顺序与 F1 同一次改动完成，未重复改两遍 |
| H（WP4-F3） | `docs/MODULE_ARCHITECTURE.md` §3.1（`clap`/`toml` 条目之后新增一条）、`crates/app/Cargo.toml:10` | 新增 `rpassword` 依赖条目：`7.5.4`、**Apache-2.0 单许可**（实测非 MIT/Apache 双许可，仍在 `deny.toml` allow 内）、`rust-version = 1.85`（= 仓库 MSRV）、传递依赖 `rtoolbox 0.0.6`（自述不保证向后兼容，属**已登记的残余风险**，许可/advisory 由 CI `deps`/`advisories` 判定）、只被 `app` 使用且**不进入 `core` 闭包**（`CORE_PORTS_AND_STORAGE.md` §9 判据 13 的 allow-list 不受影响）、用途为 `provider configure` 的凭据无回显读取（`specs/cli-commands` 的场景「凭据无回显录入」）。`crates/app/Cargo.toml` 顶部「本切片只落地 `daemon start`；其余 CLI 子命令与 `acp-stdio` 属 WP4b」的陈旧注释改为当前事实（唯一可执行文件、其余子命令是本地通道薄客户端、`acp-stdio` 是字节泵）。根 `Cargo.toml` 与 `crates/app/Cargo.toml` 里指向 `MODULE_ARCHITECTURE.md` §3.1 的措辞经核对**仍然成立**（该小节确实存在且现在真的登记了 `rpassword`），未改 |
| I（WP4-F4） | `openspec/changes/daemon-cli-and-local-admin/plan.md`（17 处，行 105/113/482/490/498/505/513/520/528/536/543/551/559/566/574/582/676） | 17 处不存在的 `reports/wp4-app-cli.log` 全部改为实际使用的 `reports/wp4b-cli.log`（各行的既有前辍写法原样保留：R 条目保持 `reports/…`，PV4 行保持 `openspec/changes/daemon-cli-and-local-admin/reports/…`）；PV4 检查定义行的证据列补 `…/reports/wp425.log`、`…/reports/wp426.log`（2.25/2.26 的断言证据），写法与该行既有两条一致 |
| J（WP4-F1 SUGGESTION） | `crates/app/src/daemon.rs`（`the_merge_window_pumps_every_active_session_each_interval_until_cancelled`、`a_failing_session_does_not_stop_the_rest_of_the_round_or_the_task`） | 取报告的**改法①**：两处相等断言移到 `tasks.cancel_all(...)` 之后，对**冻结后的同一快照** `frozen = (spans, pumps, failures)`（三元组）断言，取消后仍用同一快照复查「不得再枚举或 pump」。覆盖强度不变（`scans ≥ 2`、`pumps == scans*2`、`failures == 0`/`failures == scans`、`scans <= elapsed/20+2`、取消后冻结、失败不中止任务全都保留） |
| K（WP4-#12 P2） | `crates/app/src/config.rs`（新增 `toml_parse_error_detail_is_single_line_without_the_source_snippet`）+ 同文件 `toml_error_detail` 的最小修正 | 新单测断言 `detail`：不含 `\n`、不含源码片段（`endpoing = ` 与输入里的值 `wp427-marker` 都不得出现）、点明出错的键、长度 ≤ `ERROR_DETAIL_MAX_CHARS`(240)。**执行时该断言在现有实现下 RED**，暴露了一个 RV1 未发现的缺陷，见 §4 |

## 2. K 项的反例能力确认方式（RED → GREEN）

1. 临时把 `toml_error_detail` 改回「原样多行诊断（把 `text.trim()` 整段返回，含定位行与源码片段）」→
   跑 `cargo test --locked -p app --all-features --lib toml_parse_error_detail_is_single_line_without_the_source_snippet`
   → **EXIT 101 / FAILED**，panic 在「诊断必须是单行」，输出里可见 `2 | endpoing = "wp427-marker"`。
   原始探针实现只是把函数体换成 `return text.trim().to_owned();`（旧函数体暂时改名保留，避免
   `unreachable_code`），跑完立即从备份还原。
2. 还原实现后同一条用例 **EXIT 0 / PASSED**。
3. 两次输出都在 `reports/wp427.log`（含 `EXIT(...)` 行与 panic 文本）；还原后 `grep -c "WP427 RED probe"` = 0。

## 3. 检查与留证

| 检查 | 命令 | 结果 |
|---|---|---|
| 合同门禁 | `npm run check` | **EXIT 0**（`check:docs`：379 相对链接 / 4721 `§` 引用 / 237 个 md；`check:boundaries`：12 个 crate 与 §5 矩阵一致；`check:drift`：36 条 DDL + 15 trait/87 方法签名一致；`check:agentic`：13 项 PASS + 17 个宿主入口） |
| 格式 | `cargo fmt --all -- --check` | **EXIT 0** |
| 静态检查 | `cargo clippy --locked -p app --all-targets --all-features -- -D warnings` | **EXIT 0** |
| 测试 | `cargo test --locked -p app --all-features` | **EXIT 0**：72 passed / 0 failed / **0 ignored**（lib 50 [含新增 1]、`main` 目标 0 用例、1、`cli_commands` 11、`daemon_lifecycle` 10、其余 0 用例目标） |
| K 反例 | 见 §2 | RED **EXIT 101** → GREEN **EXIT 0** |
| 自检 1 | `git grep -n "wp4-app-cli.log" -- docs README.md AGENTS.md scripts crates openspec/changes/daemon-cli-and-local-admin/plan.md` | **EXIT 1（零命中）**；`grep -c "reports/wp4b-cli.log" plan.md` = 17 |
| 自检 2 | `grep -rn "停周期任务" docs README.md AGENTS.md` | **EXIT 1（零命中）** |

日志：`openspec/changes/daemon-cli-and-local-admin/reports/wp427.log`（所有 `EXIT(...)` 行为显式写出）。

**未执行**（明确声明）：`cargo-deny`（`deps`/`advisories`）与 `gitleaks`（`secrets`）只在 CI 运行，本地无等价物——
本轮新增登记里 `rpassword` 的 Apache-2.0 单许可可由 `deny.toml` 的 allow 列表覆盖，但 `rtoolbox 0.0.6` 的
许可/advisory 仍只能由 CI 判定；`PV5` 的 Windows 专项用例与 `#[cfg(unix)]` 用例分别只在对应平台编译，
本轮未在本机重跑（`crates/app` 的 unix 分支同前几轮一样不编译）。

## 4. 意外发现：K 项断言暴露的实现缺陷（本轮唯一超出「文本修复」的改动）

- **现象**：`toml_error_detail` 想取「错误正文」，但它的谓词（非空、不缩进、不以 `TOML parse error` 开头）
  在 toml 1.1 的输出格式下**命中的是源码片段行**。实测 detail =
  `2 | endpoing = "wp427-marker"`，即把用户写的配置文本原样带进日志与 CLI stdout。
- **与既有契约冲突**：同文件 doc comment 写「源码片段会回显用户配置文本，不进日志」；
  `ConfigError::Invalid` 的文档写「不含配置值的简短说明」。两处都被违反（RV1-WP4-#12 的前提
  「实现已经单行且不含源码片段」不成立，因此它只看到了「断言强度不足」这一半）。
- **改法（最小）**：新增私有 `is_toml_locator_line`，判定 `  |`、`N | <源码>`、`  | ^^^^` 三类行
  （`head.trim()` 为空或全为 ASCII 数字即以 `|` 开头即为定位/片段行），在谓词里排除它们；doc comment
  同步一句。**没有**改 `ERROR_DETAIL_MAX_CHARS`(240)、没有改错误码映射、没有改任何 wire/合同。
- **影响范围**：只影响 TOML schema 解析失败的 `detail` 文本（由「源码片段行」变为真正的正文，
  例如 ``unknown field `endpoing`, expected `endpoint` ``）。既有单测
  `managed_sections_and_unknown_keys_are_rejected`（`message.contains("data_diry")` 等）与所有进程级
  CLI 断言在改前/改后都通过——**没有**任何用例固定过错误文本，因此这不是靠弱化断言消除问题。
- **已通知主 Agent**（`progress_update`）：若主 Agent 认为应只登记发现、不在本轮修实现，则 K 项判据
  无法成立（`cargo test -p app` 会红），需要主 Agent 明确取舍。

## 5. 残留与待澄清

1. **`git grep -n "wp4-app-cli.log"` 全仓不是零命中**：仍命中 4 个（组）位置，全部在 2.27 写范围之外：
   `reports/rv1-wp4.md:81/85`（原样落盘的 review 报告）、`tasks.md:24/25/26`（2.15–2.17 的**原任务定义**，
   其完成条件里写着「日志写入 `reports/wp4-app-cli.log`」）、`tasks.md:46`（2.27 任务书正文本身）、
   `verification.md:154`（RV1-WP4-F4 的发现登记）。任务书要求「全仓零命中」，但它同时禁止改
   `openspec/changes/**` 的 tasks/verification 与报告；两者不可同时满足。**写范围内已零命中**。
   若要做到字面零命中，需主 Agent 自行决定是否改 tasks.md/verification.md 的历史陈述
   （我的判断：不该改——原任务定义与发现登记是历史记录；`tasks.md:24-26` 的完成条件描述的是当时写入的
   日志名，事实是 WP4b 轮实际写了 `wp4b-cli.log`，这属于另一处需要主 Agent 决定的陈旧陈述）。
2. `reports/wp427.log` 里保留了 RED 探针的两条 `test result: FAILED` 行——那是**有意留证**，
   不是本轮验证失败；每个命令的 `EXIT(...)` 行都紧随其后。
3. `plan.md` 的 PV4 证据列里，新补的两条沿用该行既有的全路径写法（`openspec/changes/…/reports/wp425.log`），
   而不是任务书正文里简写的 `reports/wp425.log`——两者指向同一文件，取「与本行既有两条一致」。
4. 本轮未复核 `tasks.md` 2.19/2.20/2.27 的复选框与 `verification.md` 的 RV1 行（属主 Agent 维护的判据工件）。
5. **提交粒度**：任务书建议三个提交，实际拆成四个实现提交（`e7c01ec` docs、`69874d9` plan.md、
   `35f8f2b` test(app)、`fd0ad86` fix(app)）：`crates/app` 的那批包含一个真行为修正（§4），
   与纯测试改动（J 项）分属不同 Conventional Commit 类型，混在一个提交里无法如实标注类型。
6. **`reports/wp427.log` 不在版本控制内**：`.gitignore` 的 `openspec/changes/**/reports/**/*.log`
   把变更内报告日志挡在版本控制外（`wp425.log`/`wp426.log` 同理），因此本报告与 §3 表引用的日志
   是工作区证据工件、未提交；它本身可在本机按同一命令重跑。

## 6. Handoff 索引

```yaml
handoff_index:
  task_id: "2.27"
  role: coder
  phase: apply
  stage: RV1 → RV2 之间的发现处置轮
  base_revision: 7e37f9bb191f1b326b6888af1dabb91f1b546f73
  evidence_type: HANDOFF
  evidence_id: WP427
  report_path: openspec/changes/daemon-cli-and-local-admin/reports/wp427-handoff.md
  result: PASS
  evidence_status: NEW
  applicability_basis: >
    RV1-WP5（FAIL：F1/F2 BLOCK + F3–F6 P2）与 RV1-WP4（PASS：F1–F4 全为 SUGGESTION/MINOR）
    共 10 项发现逐条按报告给出的最小修复处理；K 项断言在执行时暴露 `toml_error_detail` 的实现缺陷
    （诊断实际命中源码片段行），已按「不弱化断言」的前提出做最小实现修正。检查证据：
    npm run check EXIT=0、cargo fmt --check EXIT=0、clippy -p app -D warnings EXIT=0、
    cargo test -p app --all-features EXIT=0（72 passed / 0 ignored）、K 项 RED(101)/GREEN(0)、
    写范围内 wp4-app-cli.log 零命中、停周期任务零命中。日志 reports/wp427.log。
  source_evidence: NOT_APPLICABLE
```

## 7. 资源清理

- 临时探针：`crates/app/src/config.rs` 的 RED 探针已从备份还原（`grep -c "WP427 RED probe"` = 0），
  `cargo fmt --all -- --check` 复跑通过。
- 临时文件只落在 `/tmp/wp427/` 与 `/tmp/config.rs.wp427.bak`（仓库外，不影响工作区）。
- 无新增进程、无残留 daemon 子进程（本轮未启动真实 Daemon）；未改动工作区的暂存区快照策略——
  三次提交都用显式路径。
