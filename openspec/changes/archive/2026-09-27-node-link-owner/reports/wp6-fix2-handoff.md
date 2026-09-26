# WP6 fix2 Handoff — RV2-WP6 的两条 P2（`node-link-owner` / WP6 node_link 命令管线）

## Shared Report

- **task_id**：`RV2-WP6-F1`（「幂等行落盘前失败」窗口的口径与注释过宽，P2）/ `RV2-WP6-F2`（`settle_session_create` 不校验 `record.command()`，P2）——对应 review 报告 `reports/rv2-wp6.md` 的 Findings 表。
- **role**：coder；**phase**：fix（wp6-fix2）；**stage**：work-package；**evidence_id**：`RV2-WP6`
- **agent_context**：任务级 coder 子 Agent（第二轮 fix），**不继承** WP6 实现或 review 会话，只按主 Agent 下发的 fix 清单与 `rv2-wp6.md` 原文作业。工作目录 `D:\Project\acp-remote-wt\node-link-owner`（worktree，分支 `agentic/node-link-owner`）；权威规划根 `D:\Project\acp-remote`（报告与日志写在该处 `openspec/changes/node-link-owner/reports/`，不入版本控制）。工具集含 shell 与 Git：本轮 diff、门禁与红向证据都由本 Agent 亲自执行。
- **base / target_revision**：base = `f630859fec505773995adb5f6cf800c4c2da3115`（RV2-WP6 的 Target Revision；开工时 `git rev-parse HEAD` 与之一致、`git status --porcelain` 为空）；**target = `1efd69ff886e72a932bf897e44003273217d670c`**（本轮唯一提交后的 HEAD，`git status --porcelain` 为空、无 staged 文件、无未跟踪文件）。
- **scope**（严格按派单允许写入面，未越界）：
  - `crates/core/src/broker.rs`（F2 的 guard + `settle_session_create` 文档注释 + 负向用例 `broker::tests::settle_session_create_rejects_other_commands`）；
  - `crates/server/src/node_link/command.rs`（F1 的窗口注释收窄，仅注释）；
  - `docs/CORE_PORTS_AND_STORAGE.md`（仅 §6 第 20 条两条子条目）、`docs/NODE_LINK_PROTOCOL.md`（仅 §12.7 的 `session.create` 结果契约内一条注记）。
  - **未改**：`openspec/**`（spec/plan/tasks/verification 由主 Agent 维护，含 F1 提到的 R66）、`crates/server/src/node_link/command/tests.rs`、`crates/core/src/use_cases.rs`、`storage-sqlite`、`schemas/**`、`compatibility/**`、`fixtures/**`、`crates/app/**`、两份文档的文件头版本行/修订记录行（见「残余风险」第 2 条）。
- **changes**（1 个提交，4 个文件，+53/−2）：
  1. **F2（生产改动）**：`core::Broker::settle_session_create` 在 `find_request` 命中后立刻加一行 guard——`record.command() != "session.create"` → `PortError::InvalidRequest` 且零写入（不走进终态提交、不改写那条命令的幂等行）。放在 `status().is_terminal()` 判定**之前**，使「别的命令的 requestId」这一误用无论记录是否已终结都被显式拒绝。函数文档注释同步写明这条前置条件与「wire 不可达、适配层误用」的定性。
  2. **F2（用例）**：新增负向用例 `broker::tests::settle_session_create_rejects_other_commands`——用 `Harness` 走真实 `submit_mutation` 路径落下一条 `session.prompt` 的 `accepted` 记录（脚本只发 delta，turn 保持 running，记录保持 `Accepted`），把同一个 `requestId` 交给 `settle_session_create(Completed, Some(result), None)`，断言得到 `InvalidRequest`、且该行的 `command()` 与 `status()` 逐项不变。
  3. **F1（注释收窄，`command.rs`）**：原注释把「创建在幂等行落盘前失败（授权、本机 workspace 解析、写盘失败）」并列为「确定的（同一请求重试得到同一结果）」。现收窄为：本层能确认的只有**确定类失败**——授权拒绝、本机 workspace 解析失败；**存储写失败落在同一窗口却不属这一类**：它没有留下持久首次结果，同 `requestId` 重查 `command.status` 回 `nodelink.command.not_found`，重试可以创建出另一个会话。行为零改动。
  4. **F1（文档）**：
     - `docs/CORE_PORTS_AND_STORAGE.md` §6 第 20 条新增两条子条目：① **落盘前失败不构成持久首次结果**（同 `requestId` 重查回 `nodelink.command.not_found`、重试可得到不同结果；`failed` 只对同一 `requestId` 可复现的确定类失败成立，不得把落盘失败描述成「重试结果确定」）；② **`settle_session_create` 只终结 `session.create` 的记录**（`command != "session.create"` → `InvalidRequest` 且零写入，适配层误用、wire 不可达）。
     - `docs/NODE_LINK_PROTOCOL.md` §12.7 的 `session.create` 结果契约内新增一条注记：Owner 在那一轮仍会发一帧本地 `command.terminal`，但它不是持久首次结果——同 `requestId` 的 `command.status` 重查回 `nodelink.command.not_found`，重试也可以创建出另一个会话；Access 不得把落盘失败类的 `failed` 当成可稳定重放的终止事实。
- **checks**（原始输出见 `reports/wp6-command.log` 的「wp6-fix2 轮次」分节）：
  - `cargo fmt --all -- --check` exit=0（改动后先红了一次（断言换行被 rustfmt 折行），格式化后复跑 exit=0）。
  - `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` exit=0。
  - `cargo test --locked -p core -p server --all-features` exit=0：core lib **113** passed / 0 failed（base 112，本轮 +1 即新负向用例）、server lib **281** passed / 0 failed（**未变**，回归计数只增不减的判据保持），其余 7 个 test binary 全 ok / 0 failed。
  - `npm run check` exit=0：`schema fixtures OK`（118 valid / 25 invalid）、`command catalog OK`（12 命令）、`error registry OK`（58 codes）、`feature registry OK`（11 ids）、`contract assets OK`、`ACP compatibility matrix OK`、`doc links OK: 378 relative links, 4132 section refs`（新增一条 §6 子条目即 +1 个 section ref，仍全绿）、`crate boundaries OK`（12 crate）、`contract drift OK`（§7 36 条 DDL + §5 15 trait / 93 方法签名逐条一致——本轮**未**动 §5 的 rust 块与 §7 的 DDL）、`check:agentic` 16 items 全 PASS。
  - 提交时 `.husky/pre-commit` 三步（fmt → `npm run check` → workspace clippy）全通过，commitlint 通过（钩子提示本机未装 gitleaks，CI 的 `secrets` job 仍会判定）。
  - **红向证据（F2 的 guard 不是装饰）**：从 `/tmp/broker.rs.bak` 逐字节备份后临时删掉 guard（`perl` 精确匹配那 4 行），`cargo test --locked -p core --all-features settle_session_create_rejects_other_commands` **FAILED**：
    `非 session.create 的记录不得被终结: true`（panic 在测试断言处）——即无 guard 时该调用返回 `Ok(true)`，真的把 `session.prompt` 的幂等行推进成了 `Completed`；随后从备份还原、`grep` 确认 guard 回到 `broker.rs:1360` 起、复跑同一条用例 **ok**。两次运行的原始输出都在日志里。
  - **未执行（如实记录）**：`cargo-deny` 与 `gitleaks` 只在 CI 运行、本地无等价物；本轮未跑 workspace 全量 `cargo test`（改动面只到 `core` 一个 crate + `server` 的注释，workspace 全量在 base 上已由 `[PV3]` 记录）；未跑 `[PV5]`/E2E 与候选门禁（不属本角色）。
- **issues**：两条 P2 均已落地；无阻断项。需要主 Agent / reviewer 知晓的两点见「残余风险」第 1、2 条。
- **result**：**PASS**（本轮 fix 范围内全部适用条件满足、门禁全绿；**不代表** RV3 独立复核、`[PV5]`、E2E 或合并已完成）
- **evidence_paths**：
  - 本报告：`openspec/changes/node-link-owner/reports/wp6-fix2-handoff.md`
  - 被处理的 review：`openspec/changes/node-link-owner/reports/rv2-wp6.md`
  - 本轮原始输出（F2 的红向 panic 与绿向 ok、fmt/clippy/core+server 测试/`npm run check` 的 tail）：`openspec/changes/node-link-owner/reports/wp6-command.log` 的「wp6-fix2 轮次」分节
- **resource_cleanup**：未新增端口、数据库、容器、证书或外部账号；未安装依赖、未改 `Cargo.toml`/`Cargo.lock`/任何 schema 与 compatibility 资产。临时备份与输出只写在系统临时目录（`/tmp/broker.rs.bak`、`/tmp/wp6fix2.log`、`/tmp/wp6fix2.msg`，非仓库内）；提交后 `git status --porcelain` 为空。

## handoff_index

```yaml
handoff_index:
  - { task_id: "RV2-WP6-F2", role: coder, phase: fix, stage: work-package, target_revision: "1efd69ff886e72a932bf897e44003273217d670c", evidence_type: FIX, evidence_id: "RV2-WP6", report_path: "reports/wp6-fix2-handoff.md", result: PASS, evidence_status: NEW, applicability_basis: "core::Broker::settle_session_create 在 find_request 命中后加 command != \"session.create\" → InvalidRequest 的 guard（置于 is_terminal 判定之前、零写入），函数文档注释写明前置条件；负向用例 broker::tests::settle_session_create_rejects_other_commands 走真实 submit_mutation 落一条 session.prompt 的 accepted 记录后断言 InvalidRequest 且 command()/status() 不变。红向证据：删掉 guard 后该用例 FAILED（返回 Ok(true)，真的改写了 session.prompt 的终态），还原后 ok。", source_evidence: NOT_APPLICABLE }
  - { task_id: "RV2-WP6-F1", role: coder, phase: fix, stage: work-package, target_revision: "1efd69ff886e72a932bf897e44003273217d670c", evidence_type: FIX, evidence_id: "RV2-WP6", report_path: "reports/wp6-fix2-handoff.md", result: PASS, evidence_status: NEW, applicability_basis: "command.rs 的「幂等行落盘前失败」窗口注释收窄为确定类失败（授权拒绝、本机 workspace 解析），并明确存储写失败落在同一窗口但不是持久首次结果（同 requestId 重查回 nodelink.command.not_found、重试可得到不同结果）；docs/CORE_PORTS_AND_STORAGE.md §6 第 20 条与 docs/NODE_LINK_PROTOCOL.md §12.7 注记各补同义一句。纯注释与文档改动（行为零改动，server lib 用例计数 281 未变），check:docs 378 links / 4132 section refs 仍全绿。R66 的 spec 文本由主 Agent 维护，本轮未改 openspec/**。", source_evidence: NOT_APPLICABLE }
  - { task_id: "RV2-WP6-F1/F2", role: coder, phase: fix, stage: work-package, target_revision: "1efd69ff886e72a932bf897e44003273217d670c", evidence_type: CHECK, evidence_id: "PV3", report_path: "reports/wp6-command.log", result: PASS, evidence_status: NEW, applicability_basis: "日志「wp6-fix2 轮次」段记录 HEAD=1efd69ff886e72a932bf897e44003273217d670c、改动清单、fmt exit=0、clippy exit=0、cargo test -p core -p server --all-features exit=0（core lib 113 / server lib 281 / 其余 7 个目标 0 failed）、npm run check exit=0，以及 F2 的红向 panic 与还原后的绿向 ok。reviewer 未独立复跑：候选门禁前须由 Project Verify 在同一提交重跑。", source_evidence: NOT_APPLICABLE }
```

## 提交（供 reviewer 逐条核对）

| SHA | 标题 | 文件 |
|---|---|---|
| `1efd69ff886e72a932bf897e44003273217d670c` | `fix(core): settle_session_create 拒绝非 session.create 的记录` | `crates/core/src/broker.rs`、`crates/server/src/node_link/command.rs`、`docs/CORE_PORTS_AND_STORAGE.md`、`docs/NODE_LINK_PROTOCOL.md`（4 files, +53/−2） |

## 改动逐条（按 review Findings 对照）

| review ID | 位置（review 给出） | 本轮改动 | 判据 |
|---|---|---|---|
| RV2-WP6-F1 | `command.rs:815-836` 注释 + spec R66 | 注释收窄为确定类失败并写明落盘失败窗口；§6 第 20 条 + §12.7 注记各补一句同义语义 | `git show 1efd69f -- crates/server/src/node_link/command.rs docs/`；`doc links OK` |
| RV2-WP6-F2 | `core/broker.rs:1343-1375` | `record.command() != "session.create"` → `InvalidRequest` guard + 一条负向用例 | 新用例 1 绿 1 红；core lib 112 → 113 |

## 需要主 Agent 裁决 / 知晓（残余风险）

1. **F1 的 spec 文本（R66）不在本轮写入面**：`openspec/**` 按派单留给主 Agent。本轮把同一句话落在了两份权威文档（`CORE_PORTS_AND_STORAGE.md` §6 第 20 条、`NODE_LINK_PROTOCOL.md` §12.7）里；R66 如果是可断言要求，建议主 Agent 用与文档同义的措辞补一条（「落盘前失败不算持久首次结果、重查回 `not_found`、重试可得到不同结果」），避免 spec 与文档再漂移。
2. **两份文档的文件头版本行/修订记录行未更新**：派单把写入面限定在「§6 第 20 条附近」与「§12.7 注记内」，因此本轮没有动 `CORE_PORTS_AND_STORAGE.md` 顶部的版本行（当前是 0.13）与 `NODE_LINK_PROTOCOL.md` 顶部的修订记录列表。按 §10 的文档维护惯例（WP6 上一轮就补过一行修订记录）应补一条指向本轮（RV2-WP6-F1/F2）；本 Agent 未擅自扩大写入面，请主 Agent 决定由谁补。
3. **WP6 上一轮 handoff 的第 5 条口径已被本轮收窄**：`wp6-fix1-handoff.md` 的「F1 的实现取舍」第 5 条写的是「这类失败是确定的（同一请求重试得到同一结果）」，恰好是被 RV2-WP6-F1 指出的过宽表述。本轮在代码与两处文档中收窄，**未回头改写上一轮 handoff**（报告是历史证据，不宜事后修改）；复核时以本轮措辞为准。
4. **`settle_session_create` 的其余前置仍未断言**：guard 只覆盖了命令名（review 指出的那一条）。`record.session()` 缺失仍走既有的 `Corrupt`（`create_session` 的提交总会回填会话，wire 不可达），未额外加断言以免超出本轮范围。
5. **未跑项**：`cargo-deny` / `gitleaks`（只在 CI）、workspace 全量 `cargo test`、`[PV5]`/E2E 与候选门禁（3.11 / 2.22）——均不由本角色执行，`[PV3]` 结论只覆盖日志中记录的这四条命令。
