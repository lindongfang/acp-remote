# WP1 fix1 Handoff — RV1-WP1 三项非阻断发现的修复（`node-link-owner` / DU1）

## Shared Report

- **task_id**: RV1-WP1-F1 / RV1-WP1-F3 / RV1-WP1-F4（WP1 的 fix 阶段；对应 review 报告 `reports/rv1-wp1.md` 的 Findings）
- **role**: coder
- **phase**: fix；**stage**: work-package
- **agent_context**: 任务级 coder 子 Agent（fix 轮次），**不继承** WP1 实现会话，只按主 Agent 下发的 fix 清单与 review 报告原文作业。工作目录 `D:\Project\acp-remote-wt\node-link-owner`（worktree，分支 `agentic/node-link-owner`）；权威规划根 `D:\Project\acp-remote`（报告与日志写在该处 `openspec/changes/node-link-owner/reports/`，其 reports 目录按 `.gitignore:27` 不入版本控制）。
- **target_revision**: `af9064ed6dcd3e88ab76b825253fb2dc95b74967`（fix 提交；base = 被检视的 `028a4d2a71ae01364012ffec4da9a85c7f8cf905`）
- **scope**: 只处理 RV1-WP1 的 F1 / F3 / F4 三项非阻断发现，写入范围严格限于三处：(a) `Cargo.toml` 本次新增的 rcgen 注释块中的 `base64` 口径；(b) `docs/MODULE_ARCHITECTURE.md` 文件头 `修订记录` 加一行；(c) `openspec/changes/node-link-owner/reports/wp1-deps.log` 的 §7.1 指针与 §11.4 勘误。**未做**：不改依赖版本/feature、不动 `Cargo.lock`、不动 `crates/server/Cargo.toml`、不改文档 §3.1/§4.9/§5 正文、不动 `版本：0.3`、不处理 F2（`crates/server/Cargo.toml` 写入范围归属仍待主 Agent 裁决）、不触碰 `plan.md`/`tasks.md`/`verification.md`。
- **changes**（1 个提交，2 个受版本控制的文件；4 insertions / 1 deletion）：
  - `Cargo.toml`（注释勘误，`[workspace.dependencies]` 的 `rcgen` 块第三行；唯一改动点）：把「除 `base64 0.22` …外不引入新 crate」改为可核对表述——「实际启用的依赖链只有 `pem`/`ring`/`rustls-pki-types`/`time`/`yasna`（`zeroize`/`x509-parser`/`aws-lc-rs` 为可选、未启用）；启用链新增的解析节点是 `pem`/`ring`/`yasna`/`time`（及 `time-core`/`time-macros`/`deranged`/`powerfmt`/`num-conv`）与 `rustls-pki-types`；`base64 0.22.1` 不是本批新增，而是 base 锁既有版本，这里由 `pem` 与 axum 的 `ws` 复用、与既有 0.23 并存（`[bans] multiple-versions = "warn"` 只提示）」。`axum`/`rustls`/`tokio-rustls`/`rustls-pki-types` 的登记行与所有 `version`/`features` 均未动。
  - `docs/MODULE_ARCHITECTURE.md`（文件头 `修订记录` 新增一行）：`> 修订记录（2026-09-26，node-link-owner 变更 WP1）：§3.1 新增依赖口径 `[决定]`（Node Link 的 HTTP/WS/TLS 栈与 dev 用自签证书生成、落选候选与解析证据）；§4.9 加注 `node_link` 已进入实现、**尚未落地**；§5 矩阵的 `server` 行把 `acpr-wire` 格改为 ✓，并在表下注记限定该依赖只用于 ACPR-CJ1 digest 前像。` 位置在该文件既有的最新一条修订记录（2026-09-25）之前，格式与既有各行同构；`版本：0.3` 未动。
  - `openspec/changes/node-link-owner/reports/wp1-deps.log`（不入库的规划根报告）：§7.1 标题后加「本段引用于首次提交；rcgen 注释第三行在 §12.1 记录的 amend 后已改为提交现状（引文里的『依赖链只有 …』不是最终提交内容）」；§11.4 末尾加一行勘误「勘误（RV1-WP1-F1）：本节『本次解析新增 `base64 0.22.1`』有误——base64 0.22.1 为 base 锁既有版本（`git show 94a64e1:Cargo.lock` 第 170-172 行已有，且被 `sqlx-core` 等消费），本批新增清单以 §7.2 的 54 个节点为准（其中 `base64` 不在内）。`Cargo.toml` 的 rcgen 注释已同步改正。」
- **checks**: `cargo fmt --all -- --check` exit=0；`node scripts/check-crate-boundaries.mjs` exit=0（12 个 crate 与 §5 矩阵一致）；`npm run check` exit=0（16 passed / 0 failed，含 `doc links OK: 378 relative links, 4066 section refs`、`crate boundaries`、`contract drift`、`check:agentic`）。提交时 `.husky/pre-commit` 另跑 fmt + `npm run check` + `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` 三步并通过（钩子按设计不跑 `cargo test`，本地亦未装 gitleaks）。
- **issues**: 三项发现按最小修复落地，无阻断。**F2 未处理**（不在本轮 fix 清单内）：`crates/server/Cargo.toml` 是否补进 `plan.md` WP1 写范围/单一写入者表，仍待主 Agent 裁决。修复是否闭环需 RV2 独立复验；本报告的 REVIEW 行按「被复用的 review 证据」如实标注（`evidence_status: REUSED`，review 本身在 base `028a4d2a…` 上做出）。
- **result**: **PASS**（本轮 fix 范围内全部适用条件满足、门禁全绿；不等于 RV2 复验、F2 裁决、E2E 或合并已完成）
- **evidence_paths**:
  - 本报告：`openspec/changes/node-link-owner/reports/wp1-fix1-handoff.md`
  - 被处理的 review：`openspec/changes/node-link-owner/reports/rv1-wp1.md`（Findings F1/F3/F4）
  - 勘误后的原日志：`openspec/changes/node-link-owner/reports/wp1-deps.log`（§7.1、§11.4；被勘误的原始数据仍在 §7.2 的 54 节点清单与 §12.1 的 amend 说明里）
- **resource_cleanup**: 未新增端口/数据库/容器/证书/外部账号；未创建临时文件（无临时脚本）；仅复用 worktree 既有 `target/` 与 `node_modules` 缓存。git 暂存区在提交后为空，工作区干净。

## RV1-WP1-F1（P2）：`base64` 版本口径勘误

- **发现原文要点**：`Cargo.toml:123` 新增注释断言「除 `base64 0.22` 外不引入新 crate」，暗示它是本批新增的重复版本；而 `base64 0.22.1` 在 base 锁已存在（`git show 94a64e1:Cargo.lock` 第 170-172 行，被 `sqlx-core` 段消费），本批真正新增的是 `pem`/`yasna`/`time` 族与 `ring`/`rustls-pki-types` 等；日志 §11.4 同源说法与自身 §7.2 互相矛盾。
- **修复**：见上「changes」；`Cargo.toml` 注释与日志 §11.4 两处同源表述一并改为可核对版本。
- **本轮新做的独立核对**（不止照抄 review）：
  - `git show 94a64e1:Cargo.lock | grep -n -A2 '^name = "base64"'` → `170:base64 / 171:0.22.1` 与 `176:0.23.1` 并存于 base 锁，证实 0.22.1 非本批引入。
  - `cargo tree --locked -p server -i base64@0.22.1` → 反向依赖正是 `axum v0.8.9`（`ws` feature）与 `pem v3.0.6`（`rcgen v0.14.7` 的 dev 链），与注释新措辞逐字对应；`base64@0.23.1` 的反向依赖是 `acpr-transcript`/`acpr-wire`/`server`。
  - `git show 94a64e1:Cargo.lock` 逐项查 `ring`/`rustls`/`rustls-pki-types`/`pem`/`yasna`/`time`/`time-core`/`time-macros`/`deranged`/`powerfmt`/`num-conv`/`axum` 均为 0 命中 → 清单中每个名字都是本批新增节点（与日志 §7.2 的 54 节点清单一致，`base64` 不在其中）。
  - `~/.cargo/registry/.../rcgen-0.14.7/Cargo.toml` 复核 `zeroize`/`x509-parser`/`aws-lc-rs` 确为 `optional = true` 依赖；`target/debug/.fingerprint` 无 x509-parser 编译单元、`cargo tree -p server -i x509-parser` 无可打印内容 → 「可选未启用」成立（评审核对结论不变）。

## RV1-WP1-F3（SUGGESTION）：文档头 `修订记录` 补行

- **发现原文要点**：`docs/MODULE_ARCHITECTURE.md:1-11` 未按既有惯例为本变更加修订记录行，§3.1 的 `[决定]`、§4.9 注记、§5 矩阵格与注记在头部没有索引。
- **修复**：在最新一条修订记录之前插入 2026-09-26 一行，一句话点明 §3.1（依赖口径 `[决定]`）、§4.9（`node_link` 已进入实现、尚未落地）、§5（`server` 行 `acpr-wire` 格与表下注记）；`版本：0.3` 不动（按派单要求）。
- **核对**：`check:doc-links`（378 links / 4066 section refs，较改前 4063 多 3 条即新增行里的 §3.1/§4.9/§5 引用）与 `check:drift` 均通过；`git diff HEAD~1 HEAD --stat` 显示仅 +1 行。

## RV1-WP1-F4（SUGGESTION）：日志 §7.1 引用与提交现状对齐

- **发现原文要点**：日志 §7.1 引用的 `Cargo.toml` diff 是 amend 前的版本（rcgen 注释第三行不同），§7.1 本身没有指针，读者会当成提交内容。
- **修复**：§7.1 标题后加引用块指针，指明该段引用于首次提交、rcgen 注释第三行已在 §12.1 记录的 amend 后改为提交现状，并把引文里那句失效原文点名标出。未重录 diff（保留首次提交的原始记录，指针成本更低）。
- **核对**：`docs` 侧无门禁覆盖该日志（`openspec/changes/**/reports/**/*.log` 被 `.gitignore` 忽略，且位于规划根而非版本控制的 worktree），因此只做人工比对；比对对象为 §12.1 的 amend 说明与本提交的 `Cargo.toml` 最终内容。

## 验证命令与原始结论

```console
$ cargo fmt --all -- --check
  exit=0
$ node scripts/check-crate-boundaries.mjs
  crate boundaries OK: 12 个 crate 的依赖方向与 §5 矩阵一致（12 个已在矩阵登记）
  exit=0
$ npm run check
  … doc links OK: 378 relative links, 4066 section refs across 257 markdown files
  … crate boundaries OK: 12 个 crate 的依赖方向与 §5 矩阵一致（12 个已在矩阵登记）
  … contract drift OK: §7 的 36 条 DDL …；§5 的 15 个 trait / 87 个方法签名 …
  … Installation: PASS
  Totals: 16 passed, 0 failed (16 items)
  exit=0
$ git commit -m "docs(deps): 订正 Node Link 依赖注释的 base64 口径并补文档修订记录"
  pre-commit: 通过（3 步；本地密钥扫描已跳过：本机未安装 gitleaks，CI 的 secrets job 仍会判定）。
  [agentic/node-link-owner af9064e] 2 files changed, 4 insertions(+), 1 deletion(-)
$ git status --porcelain
  （空）
$ git rev-parse HEAD
af9064ed6dcd3e88ab76b825253fb2dc95b74967
```

## handoff_index

```yaml
handoff_index:
  - task_id: "RV1-WP1-F1"
    role: coder
    phase: fix
    stage: work-package
    target_revision: "af9064ed6dcd3e88ab76b825253fb2dc95b74967"
    evidence_type: REVIEW
    evidence_id: RV1-WP1
    report_path: "openspec/changes/node-link-owner/reports/wp1-fix1-handoff.md"
    result: PASS
    evidence_status: REUSED
    applicability_basis: "复用的 review 证据 RV1-WP1 产生于 base 028a4d2a71ae01364012ffec4da9a85c7f8cf905；本行针对其 F1 发现的最小修复，落地于 af9064ed。修复只改 Cargo.toml 的注释文本与 wp1-deps.log §11.4，不改任何版本/feature/锁文件，故 review 的其余结论（依赖方向、ring provider、MSRV、许可证对账）不受影响，F1 的事实依据也未变。F1 是否闭环仍需 RV2 独立复验。"
    source_evidence:
      id: "RV1-WP1"
      report_path: "openspec/changes/node-link-owner/reports/rv1-wp1.md"
      target_revision: "028a4d2a71ae01364012ffec4da9a85c7f8cf905"
  - task_id: "RV1-WP1-F3"
    role: coder
    phase: fix
    stage: work-package
    target_revision: "af9064ed6dcd3e88ab76b825253fb2dc95b74967"
    evidence_type: REVIEW
    evidence_id: RV1-WP1
    report_path: "openspec/changes/node-link-owner/reports/wp1-fix1-handoff.md"
    result: PASS
    evidence_status: REUSED
    applicability_basis: "同 F1 的来源与目标版本。修复在 docs/MODULE_ARCHITECTURE.md 文件头加一行 2026-09-26 修订记录（§3.1/§4.9/§5 索引），版本号按要求不动；改动后 npm run check 的 doc-links（4066 section refs）与 crate boundaries 仍通过。F3 是否闭环仍需 RV2 独立复验。"
    source_evidence:
      id: "RV1-WP1"
      report_path: "openspec/changes/node-link-owner/reports/rv1-wp1.md"
      target_revision: "028a4d2a71ae01364012ffec4da9a85c7f8cf905"
  - task_id: "RV1-WP1-F4"
    role: coder
    phase: fix
    stage: work-package
    target_revision: "af9064ed6dcd3e88ab76b825253fb2dc95b74967"
    evidence_type: REVIEW
    evidence_id: RV1-WP1
    report_path: "openspec/changes/node-link-owner/reports/wp1-fix1-handoff.md"
    result: PASS
    evidence_status: REUSED
    applicability_basis: "同 F1 的来源与目标版本。修复只在 wp1-deps.log §7.1 标题后加一条指针（该日志不入版本控制、不被任何门禁覆盖，故无命令级证据，仅人工比对 §12.1 的 amend 说明与提交后 Cargo.toml）。F4 是否闭环仍需 RV2 独立复验。"
    source_evidence:
      id: "RV1-WP1"
      report_path: "openspec/changes/node-link-owner/reports/rv1-wp1.md"
      target_revision: "028a4d2a71ae01364012ffec4da9a85c7f8cf905"
```

## 遗留与下一步

- **F2 未处理**（不在本轮 fix 清单）：`crates/server/Cargo.toml` 是否在 `plan.md` WP1 的写范围/「文件单一写入者」表补登记（含 `Cargo.lock`），需主 Agent 裁决；本轮未改该文件、未改任何权威规划工件。
- 建议下一步：RV2 对 `af9064ed6dcd3e88ab76b825253fb2dc95b74967` 做一次只读复验（核对三处文本与 F1 的事实口径），并携带 F2 的裁决结论。
- 未执行（如实）：`npm run verify` 的 clippy + 全 workspace `cargo test`（本 fix 未改 Rust 代码；PV1 全量已由任务 3.1 的 `reports/du1-pv1.log` 覆盖）、CI 的 `deps`/`advisories`/`secrets`（本地无等价物）、任何 E2E。
