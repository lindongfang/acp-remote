# reviewer 报告 · RV1-WP1（WP1 契约与依赖口径冻结，交付前 branch review）

- task_id: `3.2`（工作包 WP1 的交付前独立 review；检视对象任务 2.1/2.2/2.3）
- role: reviewer
- phase: work-package review
- stage: work-package
- agent_context: 独立只读 reviewer 子 Agent（kimi-coding/k3，fork_turns="none" 等效隔离），不继承实现对话；输入为任务单给出的固定 base/target、design.md、plan.md、AGENTS.md、`scripts/check-crate-boundaries.mjs` 与实现者证据；未参与实现或修复
- target_revision: `b56b829ae2119db77a1907271a5ce799962ac10a`（WP1 代码/文档交付；同 WP1 另有依赖登记提交 `bb3df7e98813d96289661458c883bd23cdd2cc28`）
- base_revision: `889d1feead751a68338753b3e79bdfbed77494ce`
- scope（检视范围）：`git diff 889d1fe..b56b829`——`Cargo.toml`、`Cargo.lock`、`deny.toml`、`docs/MODULE_ARCHITECTURE.md`、`docs/LOCAL_ADMIN_PROTOCOL.md`；其后的报告提交 `7c6aea2`（`reports/wp1-handoff.md`）作为检视对象而非证据
- result: **PASS**（完成约定范围检视，无未解决 CRITICAL/MAJOR；3 项 MINOR 与 1 项 SUGGESTION 见 Findings，均不阻断 WP1 交付）
- issues: RV1-WP1-F1（MINOR，报告计数不准）、RV1-WP1-F2（MINOR，门禁脚本注释过期）、RV1-WP1-F3（MINOR，`windows-local-ipc` 行不受门禁强制）、RV1-WP1-F4（SUGGESTION，协议文档头部状态行略超字面写范围）
- evidence_paths: 本报告；核对所用原始材料为仓库目标版本文件与 `reports/wp1-handoff.md`、`wp1-boundaries.log`、`du1-pv1.log`、`wp1-verify.log`、`wp1-dependency-probe.log`、`wp1-lc1-linux-check.log`、`baseline-cargo-test.log`
- resource_cleanup: 只读检视，未创建资源，无需释放

## Review Context

### 版本与隔离说明（诚实声明）

- 本 reviewer 无 shell/git 权限，**不能执行** `git diff 889d1fe..b56b829`，也不能重跑任何检查命令。检视方法是：读取目标版本内容 + 用仓库内可独立核对的机制（合同门禁逻辑、锁文件内容、manifest 与矩阵逐格对照、日志内部一致性）复核。
- 版本稳定性：检视期间工作区相对 reviewer-launch HEAD 的 delta 只有 `openspec/changes/daemon-cli-and-local-admin/tasks.md` 与 `verification.md`（主 Agent 自有文件，非检视范围）。五个范围文件的工作区内容 = reviewer-launch HEAD 内容，且与 `wp1-handoff.md` 对 `bb3df7e`/`b56b829` 两提交内容的逐项描述完全吻合；`tasks.md` 显示仅 2.1–2.3 完成、`crates/` 下无 `server`/`app`、`vendor/` 不存在——与「工作区仍处 WP1 阶段」一致。据此认定所读内容对应 Target Revision。
- 实现者自报的检查日志（`wp1-*.log`、`du1-pv1.log`）按仓库 `.gitignore` 约定不入库，属工作区过程证据；本报告只把它们当作**待核对对象**，不把其中的 PASS 直接当作结论。exit code 未被本 reviewer 重跑复核（无执行权限），最终轮次按计划由 3.1/6.3/7.1 在固定版本上重跑。

### 契约依据

`design.md` 决策 1/3/5/9；`plan.md` WP1 行与 Contract Changes 节；`tasks.md` 2.1/2.2/2.3 完成条件；`AGENTS.md` §7/§8/§10/§12；`scripts/check-crate-boundaries.mjs`（门禁实际行为）。

## Findings

### RV1-WP1-F1（MINOR）：handoff 的测试计数与日志不符

- 位置：`reports/wp1-handoff.md` 任务 2.3 表与 checks 表，称「测试 **515 passed / 0 failed**」。
- 实际：`reports/wp1-verify.log` 的 69 条 `test result:` 行逐项求和为 **513 passed / 0 failed / 2 ignored**，13 个 0 用例目标；`baseline-cargo-test.log` 同样为 513，且两份日志逐目标计数完全一致。
- 影响：实质结论（无回归、与基线逐项等价、VERIFY_EXIT=0）不受影响；仅是报告里的总数数字错 2。
- 建议：主 Agent 在登记 `verification.md` 时按日志实数（513 passed / 0 failed / 2 ignored / 13 个 0 用例目标）记录。不阻断交付。

### RV1-WP1-F2（MINOR）：`check-crate-boundaries.mjs` 内嵌注释随矩阵改动过期

- 位置：`scripts/check-crate-boundaries.mjs` `readDependencyMatrix` 的行内注释，仍写「§5 矩阵当前有 **9 个「列」**……`node-link-client` / `storage-sqlite` / `server` / `app` 仍只作为「行」存在」。
- 实际：WP1 已把 `server`/`app`/`windows-local-ipc` 加为列（§5 表头现为 12 列），`server`/`app` 不再「只作为行」。
- 影响：门禁**行为**不受影响（脚本按表头动态解析列，逐格映射按索引）；仅是注释描述与文档现状漂移。
- 建议：在 WP5 收口或小改动中同步该注释。不阻断交付。

### RV1-WP1-F3（MINOR）：§5 的 `windows-local-ipc` 行不受门禁强制

- 位置：`docs/MODULE_ARCHITECTURE.md` §5 矩阵新增 `windows-local-ipc` 行（自身 `—`、其余空白）。
- 机制：门禁的依赖来源是 `cargo metadata --no-deps`，其 `packages` 只含 workspace 成员；`vendor/windows-local-ipc` 被 `workspace.exclude` 排除后永不出现在 `packages` 里，因此它**作为发起方**的依赖边不会被该门禁检查。反向边（`server → windows-local-ipc`）能被检查——这正是它必须成「列」的原因，文档表述正确。
- 影响：若 wrapper 将来新增指向 workspace crate 的 path 依赖，无任何门禁拦截。文档未声称该行被强制，故不是口径矛盾；但缺口未在任何文档登记。
- 建议：在 WP2 交付时由主 Agent 决定是否需要在 §5 注记或 wrapper 审查清单里补一句「该 crate 的依赖面靠人工 review 约束」。不阻断交付。

### RV1-WP1-F4（SUGGESTION）：协议文档头部状态/版本行略超字面写范围

- 位置：`docs/LOCAL_ADMIN_PROTOCOL.md` 第 3–4 行（状态行改为「切片 4 实现中」、新增版本行 1.2）。
- 计划 WP1 写范围字面为「仅 §3.1 注记」；实现者在 handoff 的 scope 行已如实披露。改动纯状态性、与 §3.1 注记同源（design D5），不改语义。
- 建议：无需返工；主 Agent 确认该解释（头部状态行视为注记的载体）。不阻断。

## Assessment（逐项核对摘要）

1. **§5 矩阵三列与门禁行为一致** — 通过：门禁只取 `dependency.path` 非空的成员间边，列由表头动态解析；三列追加在行尾不影响既有索引；逐格抽查与成员 manifest 一致；§5 注记与门禁判定逐字吻合；`wp1-boundaries.log` exit 0。
2. **§3.1 依赖口径与 `Cargo.toml` 逐项一致** — 通过：tokio/nix feature、clap 4.6/toml 1.1/fs4 1.1 登记与文档一致；`Cargo.lock` 只增 `memoffset 0.9.1`；`members`/`rust-version` 未动。
3. **`LOCAL_ADMIN_PROTOCOL.md` §3.1 只加状态** — 通过：§3 framing 语义原文完好；新增「实现状态」块与 design D5 逐点对应；`check:doc-links` 绿。
4. **`deny.toml` 只加注释/登记** — 通过：`[sources]` 新增为注释块，无任何键放宽。
5. **§20 依赖核验实证** — 通过（残余限制：发布日期/yanked 原始 curl 输出未入库，本地不可复核，属已登记限制；`cargo-deny` 判定只在 CI）。
6. **写范围** — 通过（限制：无 git 权限；以工作区一致性 + 合同门禁全绿缓解，未发现越范围迹象）。
7. **`workspace.exclude` 注释与 cargo 行为** — 通过（探针实证，注释与结论逐点一致）。

### 计划检查 ID 对应

| Check ID | 计划命令 | 实际日志 | 核对结论 |
| --- | --- | --- | --- |
| PV1（W0 轮，任务 2.3） | `npm run check` | `du1-pv1.log` | 与计划一致；未发现弱化规则或吞失败 |
| PV2（W0 轮，任务 2.3） | `node scripts/check-crate-boundaries.mjs` | `wp1-boundaries.log`（exit 0） | 与计划一致 |
| PV1 Rust 半（附加） | `npm run verify` | `wp1-verify.log`（513 passed/0 failed/2 ignored，`VERIFY_EXIT=0`） | 实质通过；总数数字见 F1 |
| LC1/LC2（角色附加） | 见 handoff | `wp1-lc1-linux-check.log`、`wp1-dependency-probe.log` | 日志自洽；LC1 产物已清理的事实已如实披露 |
| RV1 | 本报告 | 本文件 | 由不继承实现对话的 reviewer 完成 |

## handoff_index

```yaml
handoff_index:
  - task_id: "3.2"
    role: reviewer
    phase: work-package review
    stage: work-package
    target_revision: "b56b829ae2119db77a1907271a5ce799962ac10a"
    evidence_type: REVIEW
    evidence_id: RV1
    report_path: openspec/changes/daemon-cli-and-local-admin/reports/rv1-wp1.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "WP1 交付前 branch review：检视 889d1fe..b56b829 的五个范围文件；无 git/shell 权限，以目标版本内容 + 门禁源码 + 日志内部一致性核对代替 diff 重放；工作区范围文件在检视期间稳定；发现 F1/F2/F3（MINOR）与 F4（SUGGESTION）均不阻断"
    source_evidence: NOT_APPLICABLE
```
