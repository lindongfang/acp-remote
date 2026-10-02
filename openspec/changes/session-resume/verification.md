<!-- 主 Agent 持续维护执行证据与变更历史；判据见 schema.yaml 的 apply instruction 和 procedures/acceptance.md。 -->

## Target

- Change: `session-resume`（Planning Home: `D:\Project\acp-remote`）
- Code Repository: `D:\Project\acp-remote`
- Target Ref / Revision: `refs/heads/main` = `81e350ff340014265eb7c9251237c799d4357fee`（scout tasks 1.1 只读核实，见 reports/scout-target.md）
- Version Confirmation Owner: 规划期只读调查 = scout（tasks 1.1）；候选/合入瞬间 = merger（tasks 6.1）；目标选择 = main
- Confirmation Method / Evidence: `git rev-parse refs/heads/main`、`git rev-parse HEAD`、`git status --porcelain`（reports/scout-target.md）
- 补充（main 于 2026-09-30 记录）：本变更目录下的规划文件在主工作区未跟踪（`?? openspec/changes/session-resume/`），因此规划期目标引用与代码提交一致；工作区无产品代码改动。
- **行为契约基线（tasks 1.5，DR1-F16；已按 CR7 两次 + DR1 Round 10 一次措辞订正刷新）**：change 目录未跟踪、无提交可用，因此用内容摘要固定；下列 sha256 为 **CR7-F1/CR7-F2 修正后**的契约基线，任何一项变化都说明 proposal/specs 被改（须同步 specs/design/plan/tasks 与用例并重评证据）：

```text
7072345bc1f6749f29e82bed806d108754f2275052f398a7749631f78488b8a2  proposal.md
88ec88fe2c36389f224f14df8d740e2da0deb882984b75da7bab80f79c3457ce  specs/acp-wire-protocol/spec.md
99c6eb8e7a7c5cbcd13abdfa45a5b5c1986ccb293a3430424541dcc4fbd5092f  specs/local-agent-host/spec.md
2b1a726aecc20aca039722765cac005eaa8c006a476bad16af5fc29964b7789c  specs/node-link-owner-server/spec.md
e94efd20002023cc7296ed15ec32c9d0a75fd327dae1bddace154f674df68987  specs/storage-schema-v2-migration/spec.md
45b5b693036464025ac87d7306c151425887862d18e3dec859ab281b87b51891  specs/workspace-resolution/spec.md
```

  相对 **DR1 Round 3** 记下的基线，已变化的是：`proposal.md`（`4ca02015…` → `7072345b…`，DR1-F31）、`specs/acp-wire-protocol/spec.md`（`fc7d4093…` → `88ec88fe…`，DR1-F34 更名）、`specs/local-agent-host/spec.md`（`6f7d725d…` → `1c0b1875…`（CR7-F1）→ `99c6eb8e…`（**DR1 Round 10 的 R5 正文措辞订正**））、`specs/workspace-resolution/spec.md`（`07264034…` → `45b5b693…`，CR7-F2）；`specs/node-link-owner-server/spec.md` 与 `specs/storage-schema-v2-migration/spec.md` 未变。（DR1-F33：此前括注误把 acp 的旧值写成 local-agent-host 的旧值，已按 `sha256sum` 实算值订正。）

## Handoff Index

| Task ID | Work Package | Role / Phase / Stage | Round | Executor / Agent | Target Revision | Evidence Type / ID | Report Path | Result / Evidence Status | Applicability / Source Evidence |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 1.1 | NOT_APPLICABLE | scout / recon / recon | NOT_APPLICABLE | scout 子 Agent, run 6e6acaac-dc46-40a2-a305-e054060e6304，新上下文 fork_turns=none） | 81e350ff340014265eb7c9251237c799d4357fee | RESOURCE / NOT_APPLICABLE | openspec/changes/session-resume/reports/scout-target.md | PASS / NEW | 只读侦察本轮实跑：仓库路径、`refs/heads/main`、HEAD、工具版本（cargo 1.98.1 / node v24.19.0 / npm 12.0.2）、`package.json` scripts、`openspec/agentic.yaml`、change 目录清单 |
| 1.3 | NOT_APPLICABLE | provisioner / runtime / runtime | NOT_APPLICABLE | provisioner-P0, provisioner 子 Agent run 4c0fe451-d049-4788-87c3-efb832ee3882，新上下文） | 81e350ff340014265eb7c9251237c799d4357fee | RESOURCE / NOT_APPLICABLE | reports/provisioner-worktrees.md | PASS / NEW | 本轮实际创建并核实 8 个 worktree 与独立 `CARGO_TARGET_DIR`（空目录、未预构建）；未运行任何构建或产品检查。**该执行者即 `provisioner-P0`**，下同 |
| 1.3 | NOT_APPLICABLE | provisioner / runtime / runtime | NOT_APPLICABLE | provisioner-P1, provisioner 子 Agent run cb9baadc-9dbc-473b-9ceb-0c5e35e7b8e0 | 81e350ff340014265eb7c9251237c799d4357fee | RESOURCE / NOT_APPLICABLE | reports/provisioner-repoint-wp3.md | PASS / NEW | **provisioner-P1**：将 `wt/wp3` 重定向到集成基线 b0a387b… |
| 1.3 | NOT_APPLICABLE | provisioner / runtime / runtime | NOT_APPLICABLE | provisioner-P2, provisioner 子 Agent run 5f34e588-083f-4017-b164-a2e4ca55644f | 81e350ff340014265eb7c9251237c799d4357fee | RESOURCE / NOT_APPLICABLE | reports/provisioner-repoint-wp4-wp5.md | PASS / NEW | **provisioner-P2**：将 `wt/wp4`、`wt/wp5` 重定向到 8a08db8… |
| 1.3 | NOT_APPLICABLE | provisioner / runtime / runtime | NOT_APPLICABLE | provisioner-P3, provisioner 子 Agent session 01a0f6ac-bafd-76e3-8fe0-502f5aee166b | 81e350ff340014265eb7c9251237c799d4357fee | RESOURCE / NOT_APPLICABLE | reports/provisioner-repoint-wp6-and-tp2.md | PASS / NEW | **provisioner-P3**：将 `wt/wp6` 重定向到 5ab7e9d… 并补发 TP2 登记 |
| 1.3 | NOT_APPLICABLE | provisioner / runtime / runtime | NOT_APPLICABLE | provisioner-P4, provisioner 子 Agent session 01a0f6ac-bafd-76e3-8fe0-502f5aee166b（第 4 次重定向） | 81e350ff340014265eb7c9251237c799d4357fee | RESOURCE / NOT_APPLICABLE | reports/provisioner-repoint-tp2.md | PASS / NEW | **provisioner-P4**：TP2 开工前将 `wt/tp1` 重定向到 95051f9… |
| 2.1–2.8 | WP1–WP6, TP1, TP2 | provisioner / runtime / runtime | NOT_APPLICABLE | provisioner 子 Agent, run 4c0fe451-d049-4788-87c3-efb832ee3882） | 81e350ff340014265eb7c9251237c799d4357fee | RESOURCE / NOT_APPLICABLE | openspec/changes/session-resume/reports/provisioner-worktrees.md | PASS / NEW | 逐 (WP, Attempt=1) 交接记录见 `## Worktree Handoff`；`RESOURCE` 行绑定基线 81e350f…，`refs/heads/main` 一旦移动即失效 |
| 2.6（WP6） | WP6 | provisioner / runtime / runtime | 3（重定向） | provisioner 子 Agent（session `01a0f64c-06e0-72e3-b321-b38908673e7f`，fresh context） | 5ab7e9d640034a410ddf5a9cce07b07b10dcf3a3 | RESOURCE / NOT_APPLICABLE | openspec/changes/session-resume/reports/provisioner-repoint-wp6-and-tp2.md | PASS / NEW | WP6 worktree 指针由 `81e350f…` 重定向到 `5ab7e9d…`。**main 已独立核实**：wp6 HEAD=`5ab7e9d`、工作区干净、reflog 仅增一条 reset（纯指针移动）、tp1 未被触碰（仍 `81e350f`）、du1=`5ab7e9d`、`main`=`81e350f`；未跑任何构建/测试，未执行 `npm install`，未清理任何 target 目录 |
| 2.8（TP2） | TP2 | provisioner / runtime / runtime | 1（仅登记） | provisioner 子 Agent（session `01a0f64c-06e0-72e3-b321-b38908673e7f`） | 81e350f…（**非最终**，明标） | RESOURCE / NOT_APPLICABLE | 同上 | PASS（登记完成，**未达可开工条件**） | 补上`## Worktree Handoff` 里一直空缺的 TP2 行。**刻意不**把起点定成 `5ab7e9d`：TP2 声明 `code:WP1…WP6`，WP6 尚未并入，此刻开工 PV1 必红。开工前置条件见 `## Worktree Handoff` 的 TP2 说明行 |
| NOT_APPLICABLE（规划门禁） | NOT_APPLICABLE | reviewer / plan / plan | 2 | reviewer 子 Agent, run 96c922ec-d133-4e45-a7fb-c2ab67c03cf8，新上下文，非任何 WP Owner） | sha256:17460283ef4a8b1dcf2f3c5699affc511d66e1c0607f7171ec603f30dd619d5c | REVIEW / DR1 | openspec/changes/session-resume/reports/dr1-dependency-review-round2.md | FAIL / NEW | Round 2 recheck：F1–F8 复核为已解决；新发现 DR1-F9（identity-auth 写目标缺失）、DR1-F10（`layers.broker: native` 非法）。报告由 main 从 output-0.log 原样提取持久化 |
| NOT_APPLICABLE（tasks 1.3 续办） | NOT_APPLICABLE | provisioner / runtime / runtime | NOT_APPLICABLE | provisioner 子 Agent, run cd8a1f6d-a068-4daf-9d5b-f42f790d6d99） | 81e350ff340014265eb7c9251237c799d4357fee | RESOURCE / NOT_APPLICABLE | openspec/changes/session-resume/reports/provisioner-node-modules.md | PASS / NEW | 为 8 个 worktree 建立 node_modules 目录联接（真实输出：mklink /J exit=0；8/8 的 realpathSync.native 指向主仓库；ajv 可解析；git status 干净）；未 npm ci、未改被跟踪文件 |
| 2.1 | WP1 | coder / implement / work-package | NOT_APPLICABLE | coder-A, run 997fd1b1-7a38-4c4d-8826-37c505f60c8d，worktree wt/wp1） | 248d9b9fa69a1e6375a3b9708fdaa2a127dc7ccb | CHECK / PV1 | openspec/changes/session-resume/reports/wp1-coder.md | PASS / NEW | PV1 阶段 1（-p acp-protocol）：fmt exit 0、clippy exit 0、test exit 0（5 新增 + 35 既有） |
| 2.1 | WP1 | coder / implement / work-package | NOT_APPLICABLE | coder-A, run 997fd1b1） | 248d9b9fa69a1e6375a3b9708fdaa2a127dc7ccb | CHECK / PV2 | openspec/changes/session-resume/reports/wp1-coder.md | PASS / NEW | 分支阶段 npm run check exit 0（含 check:acp 的 ajv 强校验矩阵）；另跑 workspace clippy exit 0，证明本 WP 无跨 crate 涟漪 |
| 2.2 | WP2 | coder / implement / work-package | NOT_APPLICABLE | coder-B, run 8aed0024-4b9c-421c-9b33-31e0f4d852ce，worktree wt/wp2） | 32f71f5d5f08145cfd3e3bddd7d6f6a46a0a5992 | CHECK / PV1 | openspec/changes/session-resume/reports/wp2-coder.md | PASS / NEW | PV1 阶段 1（-p node-link-protocol -p core -p identity-auth）：fmt/clippy/test 全绿；日志 reports/wp2-coder-PV1.log 与红窗口日志 reports/wp2-coder-red-window.log |
| 2.2 | WP2 | coder / implement / work-package | NOT_APPLICABLE | coder-B, run 8aed0024） | 32f71f5d5f08145cfd3e3bddd7d6f6a46a0a5992 | CHECK / PV2 | openspec/changes/session-resume/reports/wp2-coder.md | PASS / NEW | npm run check 十道门禁全绿（command catalog OK: 13 commands；schema fixtures OK: 120 valid, 26 invalid）；日志 reports/wp2-coder-PV2.log |
| 2.7 | TP1 | tester / design-author / work-package | NOT_APPLICABLE | tester-A, run 4548c444-65b8-4b96-9d51-cfa812cdfeb0，worktree wt/tp1）；修复轮认领者 tester-A2 （台账 attempt 2，--reopen 打回 CR7 的 2×MAJOR） | sha256:449bd7b35015eaf6bcf8be6b7ca35d214aa1b002cb4535310d2d685a675ffbfd（设计报告内容摘要；设计阶段无代码提交） | DELIVERY / NOT_APPLICABLE | openspec/changes/session-resume/reports/tp1-test-design.md | PASS / NEW（但 CR7 判 FAIL，本行待修复轮通过后失效） | 58 个稳定用例 ID 映射 R1–R37；未写任何 crates/** 代码（设计阶段）；自检：37/37 heading 与 tasks 引用实存；已明确声明未编译、未执行 |
| 3.2 | WP1 | reviewer / branch / work-package | 1 | reviewer-A, run 51afad9d-0579-412b-a7e1-35ba628c1ea7，新上下文，非该 WP 作者） | 248d9b9fa69a1e6375a3b9708fdaa2a127dc7ccb | REVIEW / CR1 | openspec/changes/session-resume/reports/cr1-review.md | PASS / NEW | 分支检视：R1–R4 有代码与测试证据；0 CRITICAL/MAJOR；MINOR=F1（证据路径）、SUGGESTION=F2/F3 |
| 3.3 | WP2 | reviewer / branch / work-package | 1 | reviewer-B, run 45bd6260-1376-4458-83f1-de11190fa9f5，新上下文，非该 WP 作者） | 32f71f5d5f08145cfd3e3bddd7d6f6a46a0a5992 | REVIEW / CR2 | openspec/changes/session-resume/reports/cr2-review.md | PASS / NEW | 分支检视：封闭词表六处同批一致、D6 判据未放宽过度、未触 crates/server；0 CRITICAL/MAJOR；5 项 MINOR/SUGGESTION |
| 3.8 | TP1 | reviewer / test-case / work-package | 1 | reviewer-T1, run 594976c8-c893-48e6-b261-203c28631e66，新上下文，非用例作者） | sha256:449bd7b35015eaf6bcf8be6b7ca35d214aa1b002cb4535310d2d685a675ffbfd | REVIEW / CR7 | openspec/changes/session-resume/reports/cr7-review.md | FAIL / NEW | 用例设计检视：2×MAJOR（CR7-F1/F2）= 规格文字与已冻结设计相矛盾（规格侧已修正）；另 4×MINOR + 2×SUGGESTION，交 TP1 修复轮；TP1 的 DELIVERY 行保持 PASS（它确实交付了设计），FAIL 属检视线程 |
| NOT_APPLICABLE（规划门禁） | NOT_APPLICABLE | reviewer / plan / plan | 7 | reviewer 子 Agent, run 069ead94-1d2c-4a6b-be49-5086921d56d5，新上下文，非任何 WP Owner） | sha256:076de2d47e444afd45e7662c6f89ccfffedc5a48aa190c909f899855f0b2aae5 | REVIEW / DR1 | openspec/changes/session-resume/reports/dr1-dependency-review-round7.md | FAIL / NEW | 1×MAJOR（DR1-F31：proposal.md 漏改同一句不可满足要求）+ 2×MINOR（F32/F33）+ 2×SUGGESTION（F34/F35）；CR7-F1/F2 的修正本身自洽，计划回写四项落地；specs 内未发现第二处该类缺陷 |
| 2.3 | WP3 | coder / implement / work-package | NOT_APPLICABLE | coder-C, run 2df707ae-68a3-43ec-9698-d2c010674207，worktree `D:Projectacp-remote-wtsession-resume-wp3`） | 4f7a23554f76915cf5f29cc23e292ce270e014c9（base `b0a387b…`） | CHECK / PV1,PV2 | openspec/changes/session-resume/reports/wp3-coder.md | PASS / NEW | PV1 阶段 1（`-p core`）exit 0（136 passed，新增 15）；PV2 `npm run check` exit 0（`contract drift`：§5 的 96 个方法签名与 `ports.rs` 逐条一致）；11 文件 +1245/−31 全在写范围内；日志 `wp3-coder-PV1.log`/`-PV2.log`/`-red-window.log` |
| 3.4 | WP3 | reviewer / branch / work-package | 1 | reviewer-C, run 194a161d-8aa7-41f4-bd88-44a5d5eb1e99，新上下文，非该 WP 作者） | 4f7a23554f76915cf5f29cc23e292ce270e014c9 | REVIEW / CR3 | openspec/changes/session-resume/reports/cr3-review.md | PASS / NEW | 分支检视：D3 顺序 / 三类失败分类 / 两列提交点 / Path A / `settle_session_resume` 同形 / `workspace_cwd` 形状 / 窄读取 / `UnavailableKind` 三处显式臂 / 文档与代码逐字 / 无越界 / 无新依赖 / 正常路径无 `unwrap` —— 逐条通过；1×MINOR（CR3-F1）+ 3×SUGGESTION |
| 2.1 | WP1 | coder / deliver / work-package | 1 | coder-A | 248d9b9 | DELIVERY / 交付提交 | openspec/changes/session-resume/reports/wp1-coder.md | PASS / NEW | 交付提交 248d9b9（`acp-protocol` 的 `session/resume` DTO + 矩阵 conditional_mvp） |
| 2.2 | WP2 | coder / deliver / work-package | 1 | coder-B | 32f71f5 | DELIVERY / 交付提交 | openspec/changes/session-resume/reports/wp2-coder.md | PASS / NEW | 交付提交 32f71f5（`session.resume` 命令与封闭词表原子点） |
| 2.3 | WP3 | coder / deliver / work-package | 2 | coder-C | 4f7a2355 | DELIVERY / 交付提交（第 2 轮） | openspec/changes/session-resume/reports/wp3-coder.md | PASS / NEW | 第 2 轮交付提交 4f7a2355（`core` 的恢复用例、端口与两列落盘提交点） |
| 2.4 | WP4 | coder / deliver / work-package | 1 | coder-D | 4a3882d | DELIVERY / 交付提交 | openspec/changes/session-resume/reports/wp4-coder.md | PASS / NEW | 交付提交 4a3882d（`storage-sqlite` v5 恢复列与 `load_recovery` 窄读取） |
| 2.5 | WP5 | coder / deliver / work-package | 2 | coder-E2 | 1376e1b | DELIVERY / 修复轮交付（第 2 轮） | openspec/changes/session-resume/reports/wp5-coder-fix-cr5f1.md | PASS / NEW | 首轮 4e53fcf + 修复轮 1376e1b（`--dump-request-params`）；修复轮即本 attempt 的认领交付 |
| 2.6 | WP6 | coder / deliver / work-package | 3 | coder-F3 | a7bc596 | DELIVERY / 修复轮交付（第 3 轮） | openspec/changes/session-resume/reports/wp6-coder-f3.md | PASS / NEW | 主体 76ab011（coder-F2）+ 修复轮 a7bc596（coder-F3）；coder-F 首次实例超时零交付 |
| 2.8 | TP2 | tester / deliver / work-package | 3 | tester-A3 | 3484541 | DELIVERY / 修复轮交付（第 3 轮） | openspec/changes/session-resume/reports/tp2-tester.md | PASS / NEW | 三个提交 f8133f2/a02e2fd → f44301e → 3484541；修复轮即本 attempt 的认领交付 |
| 3.5 | WP4 | reviewer / branch / work-package | 1 | reviewer-D, run b420caa3（新上下文，非该 WP 作者） | 4a3882d | REVIEW / CR4 | openspec/changes/session-resume/reports/cr4-review.md | PASS / NEW | 检视「只追加、未触发 12-step 重建、升级保留既有行且不推导取值」；0×CRITICAL/0×MAJOR，1×MINOR（CR4-F1）+ 3×SUGGESTION |
| 2.8 | TP2 | tester / review / work-package | 3 | reviewer-T3, run 73ae307c-9c64-4779-a5c9-7fa38a2a72cb（fresh context，非用例作者） | 3484541 | REVIEW / CR8 | openspec/changes/session-resume/reports/cr8-review-round2.md | PASS / NEW | 复核轮（Round 1 见 `cr8-review.md`）：闭环 CR8-F1/F2/F3；确认 CR8-F6 的两条 `#[cfg(unix)]` 用例**仍为 PENDING**（本机未执行、无编译证据，待 Linux CI） |
| 2.5 | WP5 | coder / implement / work-package | 2（修复轮） | coder-E2, run ec4bc7f3-7066-41c3-be0e-d5f74f0dc973，fresh context，worktree `D:/Project/acp-remote-wt/session-resume-wp5`） | 1376e1b（base 4e53fcf） | CHECK / PV1 | openspec/changes/session-resume/reports/wp5-coder-fix-cr5f1.md | PASS / NEW（**实现者自评，独立 review 未做**） | PV1 阶段 1（`-p agent-host`）fmt/clippy/test 全exit 0（test 67 passed，bin 目标 0→5）；日志 `wp5-coder-fix-cr5f1-PV1.log`。**main 已独立核对 diff**：只动 `crates/agent-host/src/bin/acpr-fake-acp-agent.rs` 与 `src/host.rs`（+222/−5），`--dump-requests` 分支语义未变，`8a08db8` 仍是祖先。实现者自报「计划/任务尚未登记新选项」**与事实不符**：`plan.md` WP5 行「额外义务（CR5-F1）」与 `tasks.md` 2.5 早已登记 |
| 3.6 | WP5 | reviewer / branch / work-package | 2（CR5 复核轮） | reviewer-E2, run a72214f4-09ed-49e0-97e7-a3c95839cd6e，fresh context，非该实现作者） | 1376e1b（base 4e53fcf） | REVIEW / CR5 | openspec/changes/session-resume/reports/cr5-review-round2.md | **PASS / NEW**（0×CRITICAL、0×MAJOR） | 复核判定：CR5-F1 **已解决**（新选项产出可断言 `params`；`--dump-requests` 逐字未变；契约 `tasks.md:27`+`plan.md:122` 措辞与实现逐字一致）；CR5-F3 确为纯注释（行号位移法：resume 前零位移、后恰好 +2）；CR5-F2/F4 不翻案；**新增 CR5-F5**（`main` 里`message.get("params")` 接线无测试覆盖，转 TP2 端到端闭合）、**CR5-F6**（`TempPath` pid 残留可假失败，可选加固）。报告由 main 原样落盘（reviewer 为只读工具集）。**main 补做 reviewer 自认无法做的机械核对**：`git diff --stat 4e53fcf..1376e1b` = 恰好 2 文件（+222/−5），留档 `reports/wp5-fix-diff-4e53fcf-to-1376e1b.txt` |
| 5.1 | U1（集成基线·并入 WP3） | merger / integrate / candidate | NOT_APPLICABLE | merger 子 Agent, run 6adadfb3-d825-4647-94da-cca1d86b9f37，worktree `D:Projectacp-remote-wtsession-resume-du1`） | **8a08db8e77ac67efee317363ce241261af05c008**（`--no-ff` 单合并提交；parents `b0a387b1…` + `4f7a2355…`；`git diff 4f7a2355 HEAD` 为空） | DELIVERY / NOT_APPLICABLE | openspec/changes/session-resume/reports/merge-u1-integrate-wp3.md | PASS / NEW | 四个交付提交（WP1/WP2/WP3 + `81e350f`）均经 `merge-base --is-ancestor` 实测为祖先；**PV2 `npm run check` exit 0**（`check:drift` 报 §5 的 15 trait / 96 方法签名与 `ports.rs` 一致，无合并漂移）；红窗口**恰好** 11 条唯一诊断（E0046 ×10 + E0004 ×1），与 16 处 trait 实现点逐一对应，错误码封闭、无第二成因；`core` 136 / `acp-protocol` 40 / `node-link-protocol` 40 / `identity-auth` 83 全绿；**证据缺口（merger 如实声明）**：`app` 从未被编译（失败来自其依赖），其 3 个替身的错误被遮蔽，本轮不能断言「恰好已登记原因」 |
| 5.1 | U1（集成基线·并入 WP4） | merger / integrate / candidate | 2 | merger 子 Agent, run 1ea3fb18-5617-4a33-aa3f-77e226931b09，fresh context，worktree `D:/Project/acp-remote-wt/session-resume-du1`） | **b486c5324238e4e7a60ab24806c920e3706b43da**（`--no-ff`，parents `8a08db8e…` + `4a3882d…`；零冲突，`git diff 4a3882d HEAD` 为空） | DELIVERY / NOT_APPLICABLE | openspec/changes/session-resume/reports/merge-u1-integrate-wp4.md | PASS / NEW | main **已独立核实**：`81e350f`/`248d9b9`/`32f71f5`/`4f7a2355`/`4a3882d` 五条 `--is-ancestor` 全真、worktree 干净、`refs/heads/main` 仍为 `81e350f`（未动）。PV1 阶段 1（`-p storage-sqlite`）fmt/clippy/test exit 0（131 passed / 2 ignored）；PV2 `npm run check` exit 0；绿集合 556 passed；**红窗口恰好 10 条唯一诊断（`E0046`×9 + `E0004`×1）**，main 核对归属与已登记义务逐条对应：`server` 8 条（`command.rs:2191` 的 E0004 + 7 个替身→WP6）、`agent-host` 2 条（`session.rs:746`、`host.rs:449`→WP5）、`storage-sqlite` **零诊断**（WP4 义务完成，较上轮收窄 1 条）。日志 `merge-u1-integrate-wp4-PV1-stage1.log`/`-PV2.log`/`-green-set-test.log`/`-workspace-clippy-keepegoing.log` |
| 5.1 | U1（集成基线·并入 WP5） | merger / integrate / candidate | 3 | merger 子 Agent, run 05a8fe0e-5cf5-4d28-8ef6-681f33163683，fresh context，worktree `D:/Project/acp-remote-wt/session-resume-du1`） | **5ab7e9d640034a410ddf5a9cce07b07b10dcf3a3**（`--no-ff`，parents `b486c53` + `1376e1b`；4 文件 +895/−15，**全在 `crates/agent-host/`**） | DELIVERY / NOT_APPLICABLE | openspec/changes/session-resume/reports/merge-u1-integrate-wp5.md | PASS / NEW | main **已独立核实**：六条`--is-ancestor`（`81e350f`/`248d9b9`/`32f71f5`/`4f7a2355`/`4a3882d`/`1376e1b`）全真、worktree 干净、`refs/heads/main` 仍为 `81e350f`。PV1 阶段 1（`-p agent-host`）fmt/clippy/test exit 0（67 passed，与实现者报告逐字一致）；PV2 `npm run check` exit 0；绿集合 623 passed（556+67）；**红窗口恰好 8 条（`E0046`×7 + `E0004`×1），main 按文件归属逐条核对：全在 `crates/server/`（`local_admin/test_support.rs`×3、`node_link/command/tests.rs`×3、`node_link/resource/tests.rs`×1、`node_link/command.rs`×1），`agent-host` 已转绿**（较上轮收窄 2 条，与派发预期一致） |
| 5.1 | U1（集成基线·并入 WP6，**红窗口收官**） | merger / integrate / candidate | 4 | merger 子 Agent, run 69e783a0-9922-4b0b-9cf2-c567ee77840a，fresh context，worktree `D:/Project/acp-remote-wt/session-resume-du1`） | **95051f93db15ff867c3207386e402094790434bb**（`--no-ff`，parents `5ab7e9d` + `a7bc596`；`ort` 策略**零冲突**，merger 未手工编辑任何文件） | DELIVERY / NOT_APPLICABLE | openspec/changes/session-resume/reports/merge-u1-integrate-wp6.md | **PASS / NEW** | main **已独立核实**：七条 `--is-ancestor`（`81e350f`/`248d9b9`/`32f71f5`/`4f7a2355`/`4a3882d`/`1376e1b`/`a7bc596`）全真、worktree 干净、`refs/heads/main` 仍为 `81e350f`。**PV1 阶段 2（workspace 全量）首次全绿**：clippy **0 诊断**、test **1041 passed / 0 failed / 0 ignored**（main 按 `test result` 行逐条求和复核 = 1041）；merger 另跑 `-v` 取证「12/12 workspace 成员实际重查、无 Fresh 单元」（main 核实：该日志含 12 条 `Checking/Compiling` 行且自述 Fresh=0）。**两项 reviewer 无法自行完成的 diff 复核已完成**：①写范围 0 越界（`compatibility/`、`schemas/`、`fixtures/` 完全未触碰）；②`docs/NODE_LINK_PROTOCOL.md` **§10（369–417 行）字节级未改**，实际 2 个 hunk（`@@ -642`/`@@ -658`）全在 §12.7（605–667）内；③词表零改动，8 处 `nodelink.*` 全为既有码复用。**如实登记**：WP6 自报文档「1 个 hunk」与真实 diff 的 2 个不符（均在 §12.7 内，不越界、不需修） |
| 2.6 / 3.7 | WP6 | coder + reviewer | 2 / 2 | coder-F2, run ce6ed569）、coder-F3, run dca5ed18）、reviewer-F, run 208b74bb）、reviewer-F2, run 9d7a0b41） | `76ab011`（主体）、`a7bc596`（修复） | CHECK / REVIEW | reports/wp6-coder.md、reports/wp6-coder-f3.md、reports/cr6-review.md、reports/cr6-review-round2.md | PASS / NEW | CR6 Round 1 **PASS**（0×CRITICAL/0×MAJOR）+3 条非阻断 → 定点修复 `a7bc596` → CR6 Round 2 **PASS**（闭环 F1/F2/F3，并纠正「通用臂幂等盲区」的不成立说法=CR6-F4）。**已勾 2.6 与 3.7**（任务 14/40 → **16/40**） |
| 5.1 | U1（集成基线） | merger / integrate / candidate | NOT_APPLICABLE | merger 子 Agent, run 413997c3-b9bf-418a-bc47-fb8d59237684，worktree `D:\Project\acp-remote-wt\session-resume-du1`，分支 `integration/session-resume-du1`） | b0a387b17f87a9d15d5b07d20f1e16539566c789（base `81e350f…`；合并提交 `e8feaf9…` 含 WP1 `248d9b9`、`b0a387b1…` 含 WP2 `32f71f5`） | DELIVERY / NOT_APPLICABLE | openspec/changes/session-resume/reports/merge-u1-integrate.md | PASS / NEW | 两次 `--no-ff` 零冲突；`merge-base --is-ancestor` 实测 81e350f/248d9b9/32f71f5 均为基线祖先；**PV2 `npm run check` exit 0**（封闭词表六处同基线一致）；10 个不依赖 server 的 crate clippy+test 全绿；workspace 全量仅剩**已登记红窗口**的 `crates/server/src/node_link/command.rs:2191` `E0004`（`core_payload()` 穷尽匹配未覆盖 `CommandPayload::SessionResume`），与 WP2 分支日志逐字节一致且无第二成因，归属 WP6(W4) |

## Dependency Declaration Review

<!-- plan.md 的 Dependency Declaration Review 字段列出 Review ID；本表逐条记录独立 reviewer 的 PASS、
     Round、Plan Revision（= workflow check --stage plan 输出的 contractDigest）与原始报告路径；
     计划或契约变化后该行失效。同一 Review ID 多条记录按最大 Round 取当前结论。 -->

| Review ID | Round | Reviewer | Plan Revision | Result | Report Path |
| --- | --- | --- | --- | --- | --- |
| DR1 | 1 | reviewer-DR（实际实例：reviewer 子 Agent run d620caef-b691-4291-b5b3-62dd4dca39fa，非任何 WP Owner） | sha256:77c651a4dbc32f14944b53f890ed055e8a58e804eb4a4ce50d00634fc78eaf96 | FAIL | openspec/changes/session-resume/reports/dr1-dependency-review.md |
| DR1 | 2 | reviewer-DR（实际实例：reviewer 子 Agent run 96c922ec-d133-4e45-a7fb-c2ab67c03cf8，非任何 WP Owner，与 Round 1 不同实例） | sha256:17460283ef4a8b1dcf2f3c5699affc511d66e1c0607f7171ec603f30dd619d5c | FAIL（F1–F8 已解决；新报 F9/F10 两箽 MAJOR + F11/F12 两箽 MINOR） | openspec/changes/session-resume/reports/dr1-dependency-review-round2.md |
| DR1 | 3 | reviewer-DR（实际实例：reviewer 子 Agent run d6d426a3-df39-44e6-ac22-e8bb6d09a7c6，非任何 WP Owner，与 Round 1/2 均不同实例） | sha256:5a82f8bb8e689589b5952d2799797ceef027c289874c23e2fd613ee2e80ba4f4 | FAIL（F10/F11/F12 已解决；F9 登记项闭环；新报 F13/F14 两箽 MAJOR + F15/F16 + F17） | openspec/changes/session-resume/reports/dr1-dependency-review-round3.md |
| DR1 | 4 | reviewer-DR（实际实例：reviewer 子 Agent run f95b1555-05ad-4f30-a3ff-263d9b086038，非任何 WP Owner，与前三轮均不同） | sha256:0b6f6b5246545831da07583f9352151f7d5cc4c50db474eb8b056e627d691f11 | **PASS**（0×CRITICAL、0×MAJOR；7 项非阻断 F18–F24；F13/F14/F15/F17 复核为已解决，F16 部分解决） | openspec/changes/session-resume/reports/dr1-dependency-review-round4.md |
| DR1 | 5 | reviewer-DR（实际实例：reviewer 子 Agent run 0cc00606-4853-41aa-98af-6996727d62f0，非任何 WP Owner，与前四轮均不同） | sha256:3b208486402023d4d52139b494590756b352b6c908c164f217b1bcd94ab1391d | PASS | openspec/changes/session-resume/reports/dr1-dependency-review-round5.md |
| DR1 | 6 | reviewer-DR（实际实例：reviewer 子 Agent run 71f649db-9d25-4712-81e6-2e37b2f04176，非任何 WP Owner，与前五轮均不同） | sha256:39a3919b0d11bd78e596bcccf2c1f6af80729d2d0f216551de7196a7cf9a511d | PASS | openspec/changes/session-resume/reports/dr1-dependency-review-round6.md |
| DR1 | 7 | reviewer-DR（实际实例：reviewer 子 Agent run 069ead94-1d2c-4a6b-be49-5086921d56d5，非任何 WP Owner，与前六轮均不同） | sha256:076de2d47e444afd45e7662c6f89ccfffedc5a48aa190c909f899855f0b2aae5 | FAIL | openspec/changes/session-resume/reports/dr1-dependency-review-round7.md |
| DR1 | 8 | reviewer-DR（第八个独立实例，非任何 WP Owner） | sha256:57b47d8794f92e845f779bc40a091d74020e8b71eb0c2eafe79ee6913ccc5ee5 | PASS | openspec/changes/session-resume/reports/dr1-dependency-review-round8.md |
| DR1 | 9 | reviewer-DR（第九个独立实例 run d07ac5c6-3211-4ece-b1ad-30a24592a77a，非任何 WP Owner） | sha256:f4468e70fdc0de96c4eb4942fbd923493e90d82650d40daa47c5922d21dc682d | FAIL | openspec/changes/session-resume/reports/dr1-dependency-review-round9.md |
| DR1 | 10 | reviewer-DR（第十个独立实例 run 90677e97-0eb0-4492-a025-ffe4a0378efb，非任何 WP Owner） | sha256:de1883c88adfc3bb6e650c1997bf3e39f331598edb8024723da4686dfb64ef17 | PASS | openspec/changes/session-resume/reports/dr1-dependency-review-round10.md |
| DR1 | 11 | reviewer-DR（第十一个独立实例 run 8003b50e-c807-4a3e-ab87-6b89ebc7d8f6，非任何 WP Owner） | sha256:e75f8147c672c4dac372cf9094d489bcba449cdf8602225f90c3186d48600ea8 | PASS | openspec/changes/session-resume/reports/dr1-dependency-review-round11.md |
| DR1 | 12 | reviewer-DR（第十二个独立实例 run 108be2d0-5cad-4e64-953c-d404b14a7e09，非任何 WP Owner） | sha256:2c04a445da41ca3915e6c7c3b2375bbe4811ebb883b2ddfc7c093177d5d590de | PASS | openspec/changes/session-resume/reports/dr1-dependency-review-round12.md |
| DR1 | 13 | reviewer-DR（第十三个独立实例 run 2e063929-120f-4bc8-a752-c16bfa991bc9，非任何 WP Owner） | sha256:1c93193e982fb9d17088041ab3010c6ebdf40506e50121bc3a1ba0ea9aa305ab | PASS | openspec/changes/session-resume/reports/dr1-dependency-review-round13.md |
| DR1 | 14 | reviewer-DR（第十四个独立实例 run b7df8d77-f456-4c75-9faf-9a5a339c361e，fresh context，非任何 WP Owner） | sha256:b7650f006ecf71e2dcc742c02b974fff7b2cd5c6b9527654a683a6382205ddc2 | PASS | openspec/changes/session-resume/reports/dr1-dependency-review-round14.md |
| DR1 | 15 | reviewer-DR（第十五个独立实例 run 0e3dc534-69e4-4ec2-8b1e-16e5fac189b2，fresh context，非任何 WP Owner） | sha256:1154ae4b6cb0fb85778ded529db927ebf014d0a1de671af006e6dfbd3747d4f5 | PASS | openspec/changes/session-resume/reports/dr1-dependency-review-round15.md |
| DR1 | 16 | reviewer-DR（第十六个独立实例 run 1b8da3a7-fb1c-40c9-8e9a-3752b5e05d17，首次运行基础设施中断后由 57423f65 恢复完成，fresh context） | sha256:ce6bb8c7c5d6b92172e8a2b5f2428b416b305fd47488af557c9d923e1aad1ef3 | PASS | openspec/changes/session-resume/reports/dr1-dependency-review-round16.md |
| DR1 | 17 | reviewer-DR（第十七个独立实例 run 43307c2f-afa2-4fd3-9a5a-1d8f46c3775c，fresh context；contractDigest 由 main 代实测） | sha256:700caa254e736c1d5f8c5678ff823df5b695d4ec29b24ca70f0bd7592e5a019c | PASS | openspec/changes/session-resume/reports/dr1-dependency-review-round17.md |
| DR1 | 18 | reviewer-DR（第十八个独立实例 run 4b293eab-b23c-4487-8001-06566766188e，fresh context；contractDigest 由 main 代实测） | sha256:fed00eb68cb7ef3cfab595a49374a6bb434807d770151c9e8b2d19bea7bb96ba | PASS | openspec/changes/session-resume/reports/dr1-dependency-review-round18.md |
| DR1 | 19 | reviewer-DR（第十九个独立实例 run 9f9760d1-7287-453a-8b94-8620d758ee15，fresh context；contractDigest 由 main 代实测） | sha256:2adbf605f70bd49db880937dfa7bcddb8a3b44897663661f1e0e58cc16aca936 | PASS | openspec/changes/session-resume/reports/dr1-dependency-review-round19.md |
| DR1 | 20 | reviewer-DR（第 20 个独立实例 run 3859f164-dc71-4002-80dc-c8fcde4b7965，fresh context；contractDigest 由 main 代实测） | sha256:d720f7d6479058d77eca829619f68e4f45922c905d9fb2ebade10e995762091c | PASS | reports/dr1-dependency-review-round20.md |
| DR1 | 21 | reviewer-DR（第 21 个独立实例 run 4d8536ce-45e0-417a-aa93-29d9a99df0b4，fresh context；contractDigest 由 main 代实测） | sha256:6ee3e5d5e36075f4927fa822bef1c927757a3719f0f346e186857eb18da4dd29 | PASS | reports/dr1-dependency-review-round21.md |

> **DR1 Round 16/17 的详细结论**（表格 `Result` 列按门禁判据只留裸 `PASS`，详见对应报告与 `## Planning Findings`）：
> - Round 16（`sha256:ce6bb8c7…`）：0×CRITICAL/0×MAJOR；新报 F65–F71（5×MINOR + 1×SUGGESTION + 2×MINOR）；明确指出 `tasks.md` 追加完成说明会变更摘要（复选框状态被归一化、不入摘要）、`U1` 不是 Work Package、台账并发窗口满足。
> - Round 17（`sha256:700caa25…`，**当前有效行**）：0×CRITICAL/0×MAJOR；确认 F71/F63/F64/F65/F66/F67 已解决、F69②③④ 已解决、F62/F68/F70 部分解决；新报 F72–F77。**其中 F72（7 条 DELIVERY 行错位）与 F73（3 个执行者未登记）阻塞 premerge**，二者已于 2026-10-01 由 main 修复；F74 只阻塞 final/archive。

> **DR1 Round 14/15 的详细结论**（表格 `Result` 列按门禁判据只留裸 `PASS`，详见 `## Check Plan Changes` 与 `## Review Findings`）：
> - Round 14（`sha256:b7650f00…`）：0×CRITICAL/0×MAJOR；5×MINOR F57–F61，均为记录/措辞层；明确**不阻塞 WP6 派发**。
> - Round 15（`sha256:1154ae4b…`，当前有效行）：0×CRITICAL/0×MAJOR；F58–F61 **已解决**、**F57 部分解决**；新报 2×MINOR（F62 悬空引用「额外义务 ④」、F63 `plan.md:328` 残留裸序号「②」缺 `DR1-F42` 标签）+ 1×SUGGESTION（F64 新增 SFO 行反引号）；明确**不阻塞 WP6**，且建议不为此再压一轮（F62/F63/F64 挂起到下次触碰 `plan.md` 时同批修）。

## Checks

| Check ID / Stage / Work Package | Revision / Base | Scope | Executor | Command / Steps | Environment | Result / Exit Code | Evidence |
| --- | --- | --- | --- | --- | --- | --- | --- |
| PV2 / 预变更基线 / NOT_APPLICABLE | 81e350ff340014265eb7c9251237c799d4357fee（工作区无产品代码改动） | 合同门禁全量（含 `check:command-catalog`、`check:acp`、`check:drift`、`check:docs`、`check:agentic`） | main | `npm run check`（工作目录 D:/Project/acp-remote） | Node v24.19.0 / npm 12.0.2；Rust 1.98.1 | PASS / exit 0（该基线行的逐道退出码未单独留存；**不作为全绿依据引用**） | 本次会话原始输出（main 记录）；用于区分后续 PV2 失败是否由本变更引入 |
| C1 / final / 全变更 | 1693ab3712659a37372763b27b74bfdf8a27d835（记录闭环提交；**其后仅记录层改动**，证据适用性见 `merge-u1-main.md` §39.4 的「内容等价提交」约定） | workspace 全量单元与集成用例（含 fake ACP Agent 驱动的恢复路径） | merger-A9 | `cargo test --locked --workspace --all-features` | Rust 1.98.1；91 个 test target |PASS / exit 0（1074 passed / 0 failed / 2 ignored）| reports/PV1.log |
| C2 / final / 全变更 | 1693ab3712659a37372763b27b74bfdf8a27d835 | 合同门禁全量；**十道逐道单独执行**并各自记录退出码 | merger-A9 | `npm run check` + 十道 `npm run --silent check:*` 逐道 | 同上 |PASS / exit 0（schemas/commands/errors/features/assets/acp/docs/boundaries/drift/agentic 逐道 exit 0）| reports/PV2.log |
| 提交信息合规 / final | 1693ab3712659a37372763b27b74bfdf8a27d835 | `81e350f^..HEAD` 共 36 个提交的 type/scope 枚举 | merger-A9 | `npx commitlint --from "81e350f^" --to HEAD` | 同上 | **PASS / exit 0（0 problems；此前 exit 1 / 10 problems）** | reports/final-head2-commitlint.log |
| Rust 辅助 / final | 1693ab3… | `cargo fmt --all -- --check`；workspace `clippy -D warnings` | merger-A9 | 同左 | 同上 | exit 0 / exit 0（0 诊断） | reports/final-head2-cargo-fmt.log |
| **记录闭环 HEAD 复跑 / final** | 1693ab3712659a37372763b27b74bfdf8a27d835 | 上方 C1/C2/提交合规/Rust 辅助四行的 revision 已由 merger-A10 重绑定到本轮记录闭环提交后的 HEAD；四行的命令、范围、结论与退出码**一字未改**，仅第一列 revision 文本更新 | merger-A10 | 同上四行对应命令，在 `1693ab3` 上原样复跑 | 同上 | **四行结论全部不变（仍为 exit 0）** | reports/final-head2-*.log（`# revision:` 头绑定 `1693ab3712659a37372763b27b74bfdf8a27d835`）；逐道结论见 reports/final-head2-check-gates.log |
| PV1 / 分支（阶段 1）/ WP1–WP6、TP2 | 各自的集成基线（首次派发前为 81e350f…） | 本 WP 拥有的 crate 子集 | 各 WP Owner | 阶段 1：`cargo fmt --all -- --check` + `cargo clippy --locked -p <本 WP crate 列表> --all-targets --all-features -- -D warnings` + `cargo test --locked -p <本 WP crate 列表> --all-features`；crate 列表见 plan.md 的 PV1 行与 Work Packages 的 Verification 列 | | | 各 WP 的报告及其引用的日志 |
| PV1 / 集成基线（阶段 2）/ 全部 | | 全 workspace | | `npm run check:rust` | | | reports/merge-u1-candidate-PV1-stage2-workspace-test.log |
| PV1 / 候选、主分支（阶段 2）/ 全部 | | 全 workspace | | `npm run check:rust` | | | reports/merge-u1-candidate-PV1-stage2-workspace-test.log |
| PV2 / 分支（阶段 1）、集成基线（阶段 2）、候选、主分支 / WP1、WP2、WP3、WP4、WP6 | | 合同门禁（不含 TP2；与 plan.md 的 PV2 行逐字对齐） | | `npm run check` | | | reports/merge-u1-candidate-PV2.log |
| C1 / final / 全变更 | | 替代检查（E2E not-applicable） | | `cargo test --locked --workspace --all-features` | | PASS / exit 0（候选阶段实跑；已被上方 final 行取代）| reports/merge-u1-candidate-PV1-stage2-workspace-test.log |
| C2 / final / 全变更 | | 替代检查（E2E not-applicable） | | `npm run check` | | PASS / exit 0（候选阶段实跑；已被上方 final 行取代）| reports/merge-u1-candidate-PV2.log |

## Check Plan Changes

- 2026-10-01（合并后 `check:docs` 事故，**已固化为流程规则**）：`npm run check` 第7 道 `check:docs` 连续两次因**同一类原因**报红——**在报告里逐字复述一条“引用归属写错”的错误句子，会让复述文本本身再次触发同一个检查**。已发生三次：① `reports/merge-u1-main.md` 描述 merger 报告里的杜撰节号；② `reports/merge-u1-main.md` 描述 `plan.md` 的节号误归属；③ `reports/dr1-dependency-review-round19.md` 描述 `plan.md` 的同款误归属（自触发 2 处）。**成因**：`scripts/check-doc-links.mjs` 只按「`§` 前 48 字符、切子句、取最后一个可解析的 `.md` 名」做**字面归属**，不区分「引用」与「描述一个错误的引用」。**规则（后续所有报告适用）**：**不得逐字引用已知错误的引用句子**；改用「把 X 的第 N 节写成了归属 Y 的节号」这类**描述性表述**，并显式注明「为避免复述触发门禁，此处不逐字引用」。**已验证**：按此规则改写后 `node scripts/check-doc-links.mjs` = exit 0。

- 2026-10-01（WP6 并入后，main）：**`app` 证据缺口正式关闭**。该缺口自集成基线 `b0a387b`（只含 WP1+WP2）起连续登记三轮——`app` 直接依赖 `server`，`server` 一失败它就永远不被编译，因此 `crates/app/tests/support/owner.rs` 的三个替身错误一直被遮蔽，「它们的失败恰好是已登记原因」这句话**连续三轮不能断言**（原Handoff Index 逐轮追加行，现已由本条取代）。**关闭依据**：WP6 收口 `server` 后，merger-A4 实测 workspace clippy **0 诊断**、workspace test **1041 passed**，且额外跑 `-v` 取证**12/12 workspace 成员实际重查、无 Fresh 单元**（main 已复核该日志含 12 条 `Checking/Compiling` 行）⇒ `app` 被实际编译且无诊断。**结论**：三个替身已随 WP6 补齐并**已被真正编译验证**，不再是「只有静态证据」。

- 2026-10-01（DR1 Round 14 后，main）：Round 14 判 **PASS**（0×CRITICAL/0×MAJOR；5×MINOR F57–F61），明确**不阻塞WP6 派发**。**main 决定不将5 项 MINOR 挂到派发后**，而是一次性回写完再派 WP6——因为其中两项会直接坑到WP6：**F58**（`plan.md` WP6 写范围把「§12.7 的 `:662` 收窄句」误标为 §10，而 §10 是 **WP2** 的区域，实现者照单去改会绕过 SFO 的合并顺序撞车）与 **F57**（`tasks.md` 2.6 义务编号缺③、④重号，而派发提示与 CR6 复核清单按编号引用会指向两条不同义务）。
  - **F57**：`tasks.md` 2.6 与 `plan.md` WP6 行的义务标签**全部改用稳定 finding ID**（弃用 ①②③④ 序号，避免以后再增删时重号）：`（DR1 Round 9 · port_error_code 新臂）`、`（DR1-F42 · 五个 load_recovery 替身）`、`（DR1-F41 · 投影与 settle_session_resume）`、`（CR3-F1 · uncertain 终态）`、`（DR1-F51 · NODE_LINK_PROTOCOL 两处）`。
  - **F58**：`plan.md` WP6 写范围改为「§12.7 的 `:662` 收窄句与 §12.7 示例注记」，与 SFO 行、`tasks.md` 2.6 逐字一致。
  - **F59**：`design.md` D5 的 `CORE_PORTS_AND_STORAGE.md` 行补齐 §3.1/§3.3/§3.6/§4/§5.2/§7 标题/§7.2/§9 判据 1/28/§11.3/文件头版本记录，`NODE_LINK_PROTOCOL.md` 行补齐 WP6 的两处，并加一句「逐 WP 的具体区域以 plan.md 的 SFO 为准，本表只登记资产级范围」。
  - **F60**：`plan.md` Contract Changes 的 Round 14 段末句从「还需 WP5 重开」改为已完成态（`1376e1b` + CR5 Round 2 PASS + 已入 `5ab7e9d`）。
  - **F61**：SFO 补登 `crates/server/tests/ | WP6, TP2 | TP2 | WP6 → TP2 | TP2: PV1`。
  - **代价**：`plan.md`/`tasks.md`/`design.md` 被改 ⇒ `contractDigest` 由 `sha256:b7650f00…` 变为 `sha256:1154ae4b…`，Round 14 的 PASS 对当前摘要失效 ⇒ **Round 15 独立复核**（已派发）。**但 `requirementsDigest` 保持 `sha256:53943270…` 不变**（与 Round 14 相同），即行为契约（proposal + 5 份 specs）**一字未动**，本轮回写纯属记录层。
- 2026-10-01（DR1 Round 15 后，main）：Round 15 判 **PASS**（0×CRITICAL/0×MAJOR；F58–F61 已解决、**F57 只闭环一半**；新报 F62/F63 两项 MINOR + F64 一项 SUGGESTION），明确**不阻塞 WP6**，且**建议不要为此再压一轮才派 WP6**——main 采纳该建议，已于同日派发 WP6（coder-F），**并在派发提示中强制要求以 finding ID（`CR3-F1`/`DR1-F41`/`DR1-F42`/`DR1-F51`）而非序号引用义务**，以避开plan/tasks 当前存在的标签差异。
  - **两项残留挂起**：`plan.md:120` 的悬空引用「额外义务 ④」（F62）与 `plan.md:328` 残留的裸序号「②」缺 `DR1-F42` 标签（F63），连同 F64（新增 SFO 行 File 列的反引号与门禁裸字符串比较不匹配）**合并到下一次触碰 `plan.md` 时同批修**；因修`plan.md` 必然变更 `contractDigest`，届时需再开一轮独立复核——这是**已知且已登记**的代价，不是遗漏。
  - **如实订正上一条的一处不准确表述**：上文称「`tasks.md` 2.6 与 `plan.md` WP6 行的义务标签**全部**改用稳定 finding ID」，与实际文本不符（**`plan.md:328` 尚余 1 处裸序号**）。Round 15 已实测确认（DR1-F63）。本条为记录层订正，**不改 plan.md**（避免为一句话再变更一次摘要）。

- 2026-10-01（main，接手核查后）：本轮只做**记录层与任务状态层**的订正，**未改任何行为契约**（`proposal.md`/`specs/**`/`design.md` 的行为要求一字未动）。四处：①**修复 `## Review Findings` 表的记录损坏**——该表内曾被重复插入 4 次同一组三段日志（`## Check Plan Changes` 已有它们的正本），并因此把 DR1-F29、DR1-F39、DR1-F30、CR2-F2/F3/F4/F5 四行拦腰截断；已删除重复块并把四行接回，现表内 46 行均为完整 8 列。②**订正 CR3-F1 的挂起状态**：该行原记「该义务尚未写入 plan/tasks，挂起：WP6 派发前完成」，经实测**已实际回写**（`plan.md` WP6 行「额外义务 ④（CR3-F1）」+ `tasks.md` 2.6 额外义务 ④），属记录滞后于事实，已按DR1-F29 同类处理（更正记录、不动计划）。③勾选**2.4/3.2/3.3/3.4/3.5**（WP4 交付 `4a3882d` + CR4 PASS，以及此前已 PASS 却漏勾的 CR1/CR2/CR3）。④WP4 台账由 `reviewing` 转 `ready-to-merge`（CR4 PASS，无未闭环阻断项）。
  - **代价**：`tasks.md` 被修改 ⇒ `contractDigest` 变化 ⇒ DR1 Round 13 的 PASS 对当前摘要**已失效**（本就要开 Round 14，合并在同一轮处理）。`verification.md` 的编辑不进入摘要。
  - **同时如实记录一条流程事实**：本变更目录（`openspec/changes/session-resume/**`）**至今未进入 git**（`git status` 显示整体为untracked），因此上条摘要只能靠内容摘要固定，尚无提交可作基线；正式合入前需先把规划与证据资产入库（`reports/**/*.log` 按 `.gitignore` 不入库）。
  - **入库策略（用户 2026-10-01 明示）**：**不单独提前提 PR**；本变更的规划与证据资产（`openspec/changes/session-resume/**`，`.log` 除外）**随本变更代码一起合入 `main` 时提交**。**随之而来的风险如实登记**：在该合入发生之前，这数十份规划/证据文件**只存在于本机工作区**（`D:/Project/acp-remote`），既无提交、也无远端备份；此时若工作区被清理或磁盘损坏，规划基线与全部review 证据将不可恢复（已用 `## Target` 的 6 个内容摘要做最低限度的篡改/丢失检测，但那不是备份）。此风险在合入 `main` 时由 6.x 的实际合入动作消除。

- 2026-09-30（W3 的 CR4/CR5 后，main）：WP4 交付 `4a3882df`、WP5 交付 `4e53fcf9`，**CR4 与 CR5 均判 PASS**（各 0×CRITICAL/0×MAJOR）。三个包报出「已实现但未被计划登记」的义务，本轮一次性补登（CR3-F1 → WP6 结 `uncertain`；CR4-F1 → §11.3 补登记；CR5-F1 → 新增 `--dump-request-params`），并新增 `## Contract Changes` 的 **Round 14** 段。**因触及 `plan.md`/`tasks.md`，摘要变化 ⇒ Round 13 的 PASS 失效 ⇒ 须 Round 14 独立复核；** 其中 CR5-F1 需 **WP5 小范围重开**补该选项，再由新 reviewer 复核；CR5-F2 的 TP1 前提调整与 CR4-F2/F3/F4、CR5-F3/F4 均为非阻断建议，记入 `## Review Findings` 与 TP 侧派发提示。


- 2026-09-30（WP3 并入集成基线后，main）：merger 产 **`8a08db8e`**（含 WP1+WP2+WP3），**PV2 绿**、`core` 与三个协议/身份 crate 全绿，红窗口恰好 11 条已登记诊断且经「错误码封闭 + 16 实现点逐一对应」证明**无第二成因**；`app` 的错误被其依赖遮蔽（merger 已如实声明该证据缺口，收口后由 WP6 补齐断言）。**由此记录一条流程提示（CR3 与 WP1/WP2/WP3 三方均报告）**：所有 WP worktree 的 `.husky/_` 不存在，git 静默跳过 pre-commit/commit-msg；影响可控（真判据是 PV1/PV2 的实跑记录，且红窗口内 pre-commit 的 workspace clippy 本也会失败），处理方式为**在 worktree 里手动跑 `node scripts/pre-commit.mjs`**，并已写入各 WP 的派发提示。


- 2026-09-30（WP3 的 CR3 后，main）：WP3 交付 `4f7a2355`、CR3 判 **PASS**（0×CRITICAL/0×MAJOR；1×MINOR + 3×SUGGESTION），已勾选 **2.3**（任务 6/40 → **7/40**），台账转 `ready-to-merge`。**CR3-F1 的裁决已记录但尚未写入 plan/tasks**（见 `## Review Findings` 的 CR3-F1 行）：采纳「由 WP6 结 `uncertain`」，作为 **WP6(W4) 派发前必须完成的计划回写**，与 DCR Round 14 同批；本轮不因此改 `plan.md`，以免在派发 WP4/WP5（W3，不受该义务影响）之前白开一轮门禁。


- 2026-09-30（DR1 Round 4 后，main）：Round 4 判 **PASS**（0×CRITICAL/0×MAJOR），但有 7 项非阻断；本轮回写它们（**不改任何行为契约**）：tasks 1.5 补 sha256 完成条件（F18）、补齐 Round 3 报告（F19，不改 plan）、PV1/PV2 阶段与进入条件口径统一（F20）、`## Checks` 的 PV1 行拆为阶段 1/阶段 2（F21，不改 plan）、登记 `crates/storage-sqlite/tests/` 整目录（F22）、红窗口内 `--no-verify` 说明（F23）、`fixtures/acp/v1/` 入 WP1 写范围（F24）。**代价：plan.md/tasks.md 被修改 ⇒ contractDigest 变化 ⇒ Round 4 的 PASS 对当前版本失效**（该行保留为历史轮次），须以新 contractDigest 重做（Round 5）。另：本会话实测确认 `contractDigest` **不覆盖 `verification.md`**（编辑该文件前后摘要不变），因此 F19/F21 的修复不影响摘要。
- 2026-09-30（DR1 Round 12 后，main）：Round 12 判 **PASS**（0×CRITICAL/0×MAJOR；F47/F48/F49 残留复核为已解决），按 R12 报告的编号如实记录：**F54（MINOR）已闭环**（`## Target` 第 16 行原为旧值 `1c0b1875…`，批量替换未命中 → 已按 `sha256sum` 实算整块重写为 `99c6eb8e…`，括注改为三段链条）；**F55（MINOR）当时未落地**（`tasks.md` 2.4 缺 §9 判据 1/28 与文件头）→ 已于 **Round 13 后**补齐；**F56（MINOR）当时未落地**（`## Review Findings` 的 CR2-F2/F3/F4/F5 行位置标签原记「§10 措辞」，实为 `docs/NODE_LINK_PROTOCOL.md:662` 的 §12.7 收窄句）→ 已于 **Round 13 后**订正并补记 F51 的处置。以上三项均**只改 `verification.md`/`tasks.md`**；其中 tasks.md 的改动使摘要变化，故 Round 12 的 PASS 对当前摘要**已失效**，由 Round 13 接续。另如实记录：main 因自身疏漏**对同一摘要重复派发了一次独立复核**, run d1ab2ea2，同为 Round 12 目标摘要、同样 PASS、findings 为 F54/F55 的子集）；两次回执都保留在 `reports/` 下供审计，DCR 表只登记先派的那一次（Round 12）。

- 2026-09-30（DR1 Round 6 后，main）：Round 6 判 **PASS** @ `sha256:39a3919b…511d`（0×CRITICAL/0×MAJOR；两项格式修复经 reviewer 按 `workflow-check.mjs` 的解析逻辑独立验证；F25/F26/F27 复核为已解决，F28 部分解决）。两项非阻断的闭环方式（**均不改 plan.md/tasks.md，因此本次 PASS 保持有效**）：
  - **DR1-F29（MINOR）**：`plan.md:16` 的将来时措辞实际未改，而 F28 处置栏声称已改。处理：**更正记录而不动 plan.md**（为一句措辞改 plan.md 会使摘要失效需再开一轮，收益为零）。实质未受影响：specs/proposal 的基线摘要已在 `## Target` 就位（6 行 sha256），`plan.md` 的 F16 行回写也已落地。详细说明保留在 `## Dispatch Reconciliation` 的 Round 5 记录段（该段的措辞已按 F30 订正）——记录落点与模板的「证据失效历史写入 Check Plan Changes」略有偏差，已在此声明为已知偏离并已内联订正。
  - **DR1-F30（SUGGESTION）**：Round 5 记录段三处措辞与机器事实不符，已逐条订正：①判据实为「去空白后**忽略**大小写必须恰好等于 PASS」，触发原因是单元格含 `**…**（0×CRITICAL…）` 内容；②只有 DCR 字段修复触及 plan.md（⇒ 摘要变化），`Result` 单元格修复只改 verification.md；③详细结论落在 `verification.md:99` 与 `reports/dr1-dependency-review-round5.md`（非本节）。
- 2026-09-30（W1 审查后；DR1 Round 8 的 F36–F40 处置，main）：Round 8 判 **PASS**（0×CRITICAL/0×MAJOR；F31–F35 复核为已解决），另报 4×MINOR + 1×SUGGESTION。按 reviewer 的明确建议，**只做不变更 contractDigest 的部分**，以保住本次 PASS：
  - **F37/F38/F40 已修**（只落在 `verification.md` 与 `reports/**`）：第 7 轮报告已落盘（八份 DR1 报告全部可读）；CR1 finding 行恢复准确表述；TP1 报告的 R1 旧标题引述交其修复轮。
  - **F36/F39 不改契约，改口径登记**（rec 的「不改 digest」路径）：本变更固定如下口径，并写入 WP5/WP6/TP2 的派发提示与 CR5/CR6 的复核清单：
    1. **能力未宣告路径的合格判据**＝「未发送 `session/resume`」+「本次拉起的子进程已终止并回收（无残留进程/绑定）」+「会话状态不变」+「未降级为新建会话」；**不是**「完全没有 spawn」——门控必然发生在 spawn 之后、发送之前（D4）。据此，design/plan 里「无副作用」的叙述应读作「无**残留**副作用」。
    2. **终态唯一**：`uncertain` **只**属于崩溃窗口（R32）；能力未宣告、两列为 NULL、cwd 复校验失败、payload 非法、越权这五类**确定性失败**的终态一律是 `failed`（或提交前拒绝），不得用 `uncertain` 掩盖。
  - **为什么不动 `design.md`/`specs/`**：改动它们会使 `contractDigest` 变化、Round 8 的 PASS 失效并需第 9 轮；而这两项本质上是**读法口径**（F39 的析取在同一条 MUST 内、F36 只是叙述层），用派发提示 + 本节登记即可固定，代价与收益不成比例。若后续任何 reviewer 认为必须改契约文字，再走正常轮次。

## Dependency Handoffs

- 2026-09-30（W1 结案后，main）：CR1/CR2 均 PASS（0 CRITICAL/0 MAJOR），按 tasks 2.1/2.2 的完成条件「review 通过后由主 Agent 勾选」，**已勾选 2.1 与 2.2**（任务进度 4/40 → 6/40），台账状态由 `reviewing` 转 `ready-to-merge`。两者仍是各自分支上的提交，`main` 未动（仍 81e350f），**尚未合入**。

- **集成基线已建成（phase=integrate，merger 报告 `reports/merge-u1-integrate.md`）**：`integration/session-resume-du1` = **`b0a387b17f87a9d15d5b07d20f1e16539566c789`**，base `81e350f…`（`refs/heads/main` 全程未移动），两次 `--no-ff` **零冲突**，包含关系已用 `git merge-base --is-ancestor` 实测。**PV2 `npm run check` exit 0**——这是封闭词表原子点在集成后的关键验证：两包的合同改动在同一基线上仍然一致。
  - **红窗口实测形态**：workspace 全量唯一的 rustc 错误是 `crates/server/src/node_link/command.rs:2191` 的 `E0004`（`&submit.payload` 穷尽匹配未覆盖 `CommandPayload::SessionResume`），与 WP2 分支日志逐字节一致且**无第二成因**（10 个不依赖 server 的 crate 全绿）。归属 WP6(W4)，**已登记、有主、有界**。
  - **解锁**：WP3 的四个就绪条件现已全部满足（上游 WP2 已进集成基线、契约已冻结 `57b47d87…`、`wt/wp3` 与 `CARGO_TARGET_DIR` 就绪、台账未开工）→ 已派发 WP3（coder-C）。
  - 集成 worktree 与其 `CARGO_TARGET_DIR` 按计划**保留复用**，未 push、未 amend、未合入 `main`。

- **PV1 阶段 2 的口径澄清（避免 merger 误判）**：plan.md 的 PV1 行把「集成基线」列在阶段 2，但同一文件的红窗口段已登记「从 WP2 合入集成基线起、到 WP6 合入为止，workspace 级 PV1 预期为红（归属 WP6）」。二者按**「集成基线」指全部 WP 集成后的最终基线**来读才自洽：当前这个只含 WP1+WP2 的**中间**集成基线属于红窗口，merger 的职责是**准确记录** `crates/server/src/node_link/command.rs` 的 `E0004`（`core_payload()` 穷尽匹配、0 个通配臂）并确认**没有第二个成因**，而不是修它。

- **worktree 重定向机制（2026-09-30，main 记录；由 WP3 的 BLOCKED 触发）**：首次 provisioner 在 09:50 一次性按当时的基线 `81e350f…` 建好 8 个 worktree；此后 merger 建成 U1 集成基线 `b0a387b…`，而 `plan.md` 规定 integrated 单元的**下游包必须从该基线开工**，但**没有任何角色被要求把已存在的下游 worktree 指针移到新基线**。coder-C 自行实测 `git rev-parse HEAD` ≠ 任务书要求而 BLOCKED，暴露了该缺口。
  - **已处置**：选方案 A（由 provisioner 重定向，执行者不得自行 `reset`/切分支——`roles/coder.md` 与 `roles/provisioner.md` 明确 worktree 归 provisioner 独占）。main 复核：重定向前 wp3 工作区干净、分支无独有提交、wp4/wp5/wp6/tp1 同为 `dirty=0`；重定向后 HEAD == `b0a387b…` 且两条 `--is-ancestor` 均为真，`crates/core/src/broker.rs:130` 已有 WP2 落地的 `session.resume` 臂。
  - **固化为流程动作**：后续每次派发 **W3(4/5)、W4(6)、W5(TP2)** 之前，先派 provisioner 把对应 worktree 重定向到**届时的**集成基线，并把 `(WP, Attempt, 新起点, 交接时刻)` 登记到本表。wp4/wp5/wp6/tp1 本次**有意不动**（其上游尚未进基线，提前挪会造成“看似就绪”的误导）。

 - **（W1 期间，2026-09-30 的交付登记；已被后续集成基线取代）** WP1 交付提交 `248d9b9fa69a1e6375a3b9708fdaa2a127dc7ccb`（完成 R1–R4）；WP2 交付提交 `32f71f5d5f08145cfd3e3bddd7d6f6a46a0a5992`（封闭词表原子点 + node-link wire，21 个文件）。两者都基于基线 `81e350f…`（`git diff --stat 81e350f..<branch>` 已核）；**尚未合入集成基线**，因此 W2（WP3）仍为 unstarted——按规则，同交付单元的 code 依赖要求上游进入「已验收集成基线」（交付提交 + PV1 阶段 1/PV2 PASS + 独立 review PASS + main 已接收）。本次仅登记交付，不提前标 merged。

- 两个实现者自报、待独立 reviewer 裁决的开放项（已写入 CR1/CR2 的核查清单，不作为已闭环项）：
  1. WP1 把 `crates/acp-protocol/src/methods.rs` 中 `session/resume` 的 `implemented` 置为 `true`（CR1 核查点 4）——若裁定为虚报能力，需回改并由新实例重交。
  2. WP2 修改了 `crates/node-link-protocol/tests/envelope_fixtures.rs` 的两个硬编码计数常量，该文件未逐文件登记在 plan.md 的 Shared File Ownership（CR2 核查点 5）——若裁定为登记缺失，需评估是否必须回写计划并重过 plan 阶段检查（那会改变 `contractDigest`，需重做依赖声明审查轮次）。

- **node_modules 破坏性操作禁令（provisioner 转达，已核实依据）**：禁止在 worktree 内跑 `npm install` / `npm ci` / `npm update`（会穿透 junction 改写主仓库依赖树，连带影响其余 7 个 worktree）；禁止对 junction 用 `rm -rf`（MSYS 会递归穿透删除）；断联只能用 `cmd //c rmdir "<worktree>\node_modules"` 且必须在 `git worktree remove` 之前。

- 目前无下游接入（W1 尚未合入集成基线）。WP3←WP2、WP4←WP3、WP5←WP1,WP3、WP6←WP1–WP5、TP2←WP1–WP6 的接入时点、包含关系检查与失效范围见 plan.md 的 `## Dependency Handoffs`。

## Runtime Resources

- 构建缓存与测试临时目录：每 WP 独立 `CARGO_TARGET_DIR`（provisioner 已按 (WP, Attempt=1) 分配，见 `## Worktree Handoff`）与独立 tempdir；无共享运行资源（依据：`openspec/config.yaml` 的 Runtime Environment 记录「暂无共享运行资源」）。
- 本地 SQLite：各执行者自建临时库文件，不使用仓库内共享数据目录。
- 释放方法（provisioner 记录）：核实无未提交改动 → `git worktree remove` → 合入后 `git branch -d` → `rm -rf <CARGO_TARGET_DIR>` → `git worktree list` 复核。

| WP5 | 2 | D:/Project/acp-remote-wt/session-resume-wp5（agentic/session-resume-wp5） | 8a08db8（含 WP1–WP3；由 coder-E2 在 attempt 1 之上接续，起点未变） | provisioner 子 Agent, run 4c0fe451… | coder-E2 | 2026-10-01T06:54:19Z | reports/wp5-coder-fix-cr5f1.md |
| WP6 | 2 | D:/Project/acp-remote-wt/session-resume-wp6（agentic/session-resume-wp6） | 5ab7e9d（U1 集成基线，含 WP1–WP5） | provisioner 子 Agent, session 01a0f6ac… | coder-F2 | 2026-10-01T07:54:19Z | reports/wp6-coder.md |
| WP6 | 3 | D:/Project/acp-remote-wt/session-resume-wp6（agentic/session-resume-wp6） | 5ab7e9d（同 attempt 2 起点，修复轮未换基线） | provisioner 子 Agent, session 01a0f6ac… | coder-F3 | 2026-10-01T08:21:00Z | reports/wp6-coder-f3.md |
| TP2 | 2 | D:/Project/acp-remote-wt/session-resume-tp1（agentic/session-resume-tp1） | 95051f9（含 WP1–WP6；第 1 轮起点） | provisioner 子 Agent, session 01a0f6ac… | tester-A2 | 2026-10-01T08:00:00Z | reports/tp2-tester.md |
| TP2 | 3 | D:/Project/acp-remote-wt/session-resume-tp1（agentic/session-resume-tp1） | 95051f9（同 attempt 2 起点，修复轮未换基线） | provisioner 子 Agent, session 01a0f6ac… | tester-A3 | 2026-10-01T08:35:00Z | reports/tp2-tester.md |
## Worktree Handoff

| Work Package | Attempt | Worktree | Baseline Revision | Provisioner | Executor | Received At | Evidence |
| --- | --- | --- | --- | --- | --- | --- | --- |
| WP1 | 1 | D:/Project/acp-remote-wt/session-resume-wp1 | 81e350ff340014265eb7c9251237c799d4357fee | provisioner-P0 | coder-A | 2026-09-30T02:38:00Z | reports/provisioner-worktrees.md |
| WP2 | 1 | D:/Project/acp-remote-wt/session-resume-wp2 | 81e350ff340014265eb7c9251237c799d4357fee | provisioner-P0 | coder-B | 2026-09-30T02:38:00Z | reports/provisioner-worktrees.md |
| TP1 | 1 | D:/Project/acp-remote-wt/session-resume-tp1 | 81e350ff340014265eb7c9251237c799d4357fee | provisioner-P0 | tester-A | 2026-09-30T02:38:00Z | reports/provisioner-worktrees.md |
| TP1 | 2 | D:/Project/acp-remote-wt/session-resume-tp1 | 81e350ff340014265eb7c9251237c799d4357fee | provisioner-P0 | tester-A2 | 2026-09-30T05:39:00Z | reports/provisioner-worktrees.md |
| WP3 | 1 | D:/Project/acp-remote-wt/session-resume-wp3 | 81e350ff340014265eb7c9251237c799d4357fee | provisioner-P0 | coder-C | 2026-09-30T12:13:00Z | reports/provisioner-worktrees.md |
| WP3 | 2 | D:/Project/acp-remote-wt/session-resume-wp3 | b0a387b17f87a9d15d5b07d20f1e16539566c789 | provisioner-P1 | coder-C | 2026-09-30T12:44:00Z | reports/provisioner-repoint-wp3.md |
| WP4 | 1 | D:/Project/acp-remote-wt/session-resume-wp4 | 8a08db8e77ac67efee317363ce241261af05c008 | provisioner-P2 | coder-D | 2026-09-30T16:08:00Z | reports/provisioner-repoint-wp4-wp5.md |
| WP5 | 1 | D:/Project/acp-remote-wt/session-resume-wp5 | 8a08db8e77ac67efee317363ce241261af05c008 | provisioner-P2 | coder-E | 2026-09-30T16:08:00Z | reports/provisioner-repoint-wp4-wp5.md |
| WP5 | 2 | D:/Project/acp-remote-wt/session-resume-wp5 | 8a08db8e77ac67efee317363ce241261af05c008 | provisioner-P2 | coder-E2 | 2026-10-01T06:34:00Z | reports/provisioner-repoint-wp4-wp5.md |
| WP6 | 1 | D:/Project/acp-remote-wt/session-resume-wp6 | 81e350ff340014265eb7c9251237c799d4357fee | provisioner-P0 | coder-F | 2026-10-01T07:23:00Z | reports/provisioner-worktrees.md |
| WP6 | 2 | D:/Project/acp-remote-wt/session-resume-wp6 | 5ab7e9d640034a410ddf5a9cce07b07b10dcf3a3 | provisioner-P3 | coder-F2 | 2026-10-01T07:54:00Z | reports/provisioner-repoint-wp6-and-tp2.md |
| WP6 | 3 | D:/Project/acp-remote-wt/session-resume-wp6 | 5ab7e9d640034a410ddf5a9cce07b07b10dcf3a3 | provisioner-P3 | coder-F3 | 2026-10-01T08:17:00Z | reports/provisioner-repoint-wp6-and-tp2.md |
| TP2 | 1 | D:/Project/acp-remote-wt/session-resume-tp1 | 81e350ff340014265eb7c9251237c799d4357fee | provisioner-P0 | tester-A | 2026-10-01T09:03:00Z | reports/provisioner-worktrees.md |
| TP2 | 2 | D:/Project/acp-remote-wt/session-resume-tp1 | 81e350ff340014265eb7c9251237c799d4357fee | provisioner-P0 | tester-A2 | 2026-10-01T09:33:00Z | reports/provisioner-worktrees.md |
| TP2 | 3 | D:/Project/acp-remote-wt/session-resume-tp1 | 95051f93db15ff867c3207386e402094790434bb | provisioner-P4 | tester-A3 | 2026-10-01T10:09:00Z | reports/provisioner-repoint-tp2.md |
## Dispatch Reconciliation

| Work Package | Attempt | Executor | State | Evidence |
| --- | --- | --- | --- | --- |
| WP1 | 1 | coder-A | merged | openspec/changes/session-resume/dispatch-queue.jsonl |
| WP2 | 1 | coder-B | merged | openspec/changes/session-resume/dispatch-queue.jsonl |
| TP1 | 2 | tester-A2 | merged | openspec/changes/session-resume/dispatch-queue.jsonl |
| WP3 | 2 | coder-C | merged | openspec/changes/session-resume/dispatch-queue.jsonl |
| WP4 | 1 | reviewer-D | merged | openspec/changes/session-resume/dispatch-queue.jsonl |
| WP5 | 2 | reviewer-E2 | merged | openspec/changes/session-resume/dispatch-queue.jsonl |
| WP6 | 3 | reviewer-F2 | merged | openspec/changes/session-resume/dispatch-queue.jsonl |
| TP2 | 3 | reviewer-T3 | merged | openspec/changes/session-resume/dispatch-queue.jsonl |

> **台账逐字对应说明（DR1-F68）**：上表 `Attempt` / `Executor` / `State` 三列是 `dispatch-queue.jsonl` 归约后的**机器事实**，不是叙述。`Executor` 取台账**最后一次持有该工作包**的执行者（多为完成复核的 reviewer）；**实现该轮代码的认领执行者**如下，供审计对照：
> WP1 = `coder-A`（248d9b9）、WP2 = `coder-B`（32f71f5）、WP3 = `coder-C`（第 2 轮，4f7a2355）、WP4 = `coder-D`（4a3882d）、
> WP5 = `coder-E`（首轮 4e53fcf）→ `coder-E2`（修复轮 1376e1b）、WP6 = `coder-F`（超时、零交付）→ `coder-F2`（主体 76ab011）→ `coder-F3`（修复轮 a7bc596）、
> TP1 = `tester-A`（设计报告，CR7 判 FAIL）、TP2 = `tester-A` → `tester-A2` → `tester-A3`（依次 f8133f2/a02e2fd、f44301e、3484541）。
> **TP1 已结案为 `merged`**（台账 2026-10-01T16:07:15Z，attempt 2，认领执行者 `tester-A2`）：按用户 2026-10-01 决定，TP1 的修正与可执行用例已并入 TP2 完成并随候选合入；该状态同时在 `plan.md` 的 `## Merge Strategy` 的 U1 就绪名单中移除（DR1-F71），故 TP1 不作为 U1 的合入就绪条件，但台账事实为 `merged`、如实记录。
> **U1（交付单元）不在本表**：它不是 Work Package（见 `plan.md` 的 Merge Units 表），其台账记录已标记 `superseded`；执行证据见 `## Handoff Index` 的 merger DELIVERY 行与 `## Premerge` 的 `delivery_unit: U1`。


## Review Findings

| ID | Work Package | Revision | Reviewer | Location | Severity / Impact | Resolution | Recheck |
| --- | --- | --- | --- | --- | --- | --- | --- |
| CR-PM | U1 | 69f1ac1 → 0cc795f | reviewer-PM（run 9a86b747，无 shell） | 合并后差异与主分支回归证据 | **MAJOR ×2**（F1 tasks 6.7 记录不实门禁结论并勾选；F2 被检视版本门禁红 + 关键证据未入库 + Merge History 空表） | F1/F2 已闭环：6.7 一度撤回并改写为如实记录，新 HEAD 取得 exit 0 后才重新勾选；`plan.md` 修正与三份报告已入库；`## Merge History` 已回填 | **Round 2 已复核闭环**（reports/cr-pm-post-merge-review-round2.md） |
| CR-PM | U1 | 0cc795f | reviewer-PM2（run 6f9fa48c，无 shell） | 合并后差异复核第二轮 | **MAJOR ×3**（F7 Round 1 报告从未落盘却被引用、且 Round 1 判定未登记进本表；F8 工作区 tasks.md 未入库，与 F2 同类复发；F9 `## Checks` 缺最终 HEAD 的 C1/C2 行）+ MINOR ×2（F10 tasks 6.7 撤回记录用现在时；F11 暂存核对表另一处份数不自洽） | F7/F8/F9/F10/F11 均已处置：Round 1 报告补录入库、本表补 CR-PM 行；工作区 tasks.md 随本轮入库；`## Checks` 补最终 HEAD 的 C1/C2/提交合规/Rust 辅助四行；F10 改为历史时点表述；F11 份数统一 | **Round 3 已复核闭环** |
| CR-PM | U1 | 0cc795f | reviewer-PM3（run 4ce133b6，fresh context，无 shell） | 合并后差异复核第三轮（最终） | **PASS**（0×CRITICAL/0×MAJOR）：Round 2 的 F7–F11 全部真正闭环（F7 按「CR-PM 两行是否落在表头与分隔行之后」复核、F9 按「每行 revision 是否与其 Evidence 日志的 `# revision:` 逐字相等」复核，均通过）；验证 37 行证据链一致；唯一新发现 F12（措辞过时）非阻断 | 已闭环；报告 `reports/cr-pm-post-merge-review-round3.md`（由 9715911 入库）。该实例无 shell 未能亲跑的三项（`git diff --name-only`、`git ls-files *.log`、`commitlint`）由 main 代跑并全部证实：合并后仅动 `AGENTS.md` 与 `commitlint.config.mjs`、零个 `.log` 入库、commitlint exit 0 / 0 problems | **已闭环**（Round 3 PASS） |
| CR3-F1 | WP3 | 4f7a2355 | reviewer, run 194a161d…） | `broker.rs` 的 `factory.create` 成功后两列提交失败 + `server/…/command.rs` 的 `Failed` 分支 | MINOR（**reviewer 明示不判阻断**）：两列提交失败时 `Create` 已落盘、适配层归 `failed` → 「会话已存在却被报 failed」的孤儿会话；该类在基线已存在（`factory.create` 失败即走同支），提交点是 DR1 Round 9 冻结的取舍 | **main 裁决：采纳选项 ①** —— 由 **WP6** 结 `uncertain`（沿用其自身「不能用 `failed` 撒谎」的既有判据）。该义务**已随 DCR Round 14 批次回写**：`plan.md` WP6 行「额外义务 ④（CR3-F1）」与 `tasks.md` 2.6 额外义务 ④ 均已写明（本行原记「尚未写入 plan/tasks」属记录滞后，2026-10-01 经实测订正；**本行原先引用的「额外义务 ④（CR3-F1）」标签已于 2026-10-01 随 DR1-F62/F63 的回写一并废弃**，现两处均为 `额外义务（CR3-F1 · uncertain 终态）`（DR1 Round 17 复核确认），见 `## Check Plan Changes`） | **已回写完成**（2026-10-01 实测确认；WP6 派发时按此执行） |
| CR3-F2 | WP3 | 4f7a2355 | reviewer, run 194a161d…） | design D3 步骤 5 的「（会话状态抬回可交互态）」措辞 | SUGGESTION：`resume_session` 在 core 侧不写 `StateChange`；该保证由「端点重新绑定」承担 | 接受为措辞项；口径已在本行登记为「指端点绑定」；下次触及 design 时收紧措辞 | 已处裁量 |
| CR3-F3 | WP3 | 4f7a2355 | reviewer, run 194a161d…） | `use_cases.rs` 的 R31 用例用 `Actor::Device` | SUGGESTION：Device 分支不读会话行，故对「Node 分支先于本机读取」举证力弱（代码本身满足 R31 字面） | 接受；已列入 **TP2** 派发提示（补一条 `Actor::Node` 的 R31 用例：同 requestId 对「存在但不覆盖 agent」与「不存在」断言同一响应且 `load_recovery` 计数为 0） | 待 TP2 |
| CR3-F4 | WP3 | 4f7a2355 | reviewer, run 194a161d…） | design D3 括注「≤4096 字节」 | SUGGESTION：实现与 §3.6 用 `char_count`（**字符**），且 `try_new` 把 `Empty`/`TooLong` 折叠为 `InvalidValue::Field` | 接受；无行为差异（两条路径拒绝同一批输入）。单位措辞待下次触及 design 时改为「字符」 | 已处裁量 |
| CR4-F1 | WP4 | 4a3882df | reviewer, run b420caa3…） | `docs/CORE_PORTS_AND_STORAGE.md` §11.3 vs plan WP4 写范围 | MINOR：`user_version` 5→6 属**必要连带修正**（`FILE_FORMAT_VERSION=5` 后原句不成立；先例 0.14 的 v3→v4 同批改过），但 §11.3 未登记给任何写者 | CR4 裁定「必要连带修正，应保留并补登记」→ 已把 §11.3 补进 plan WP4 行 + SFO 区域注记 + tasks 2.4 | 已完成（Round 14 回写） |
| CR4-F2/F3/F4 | WP4 | 4a3882df | reviewer, run b420caa3…） | 迁移用例的 v3 用例命名/DDL 断言、红窗口日志头写的是 base 而非 target | SUGGESTION：断言可更精确；日志头只写 base（内容为交付版本） | F2/F4 接受为已知项。**F3 由 main 于 2026-10-01 裁决：不重开 WP4，转为 TP2 的强制输入**——理由：`crates/storage-sqlite/tests/` 的 Writers 是 WP4→TP2、Merge Owner 本就是 TP2，而判别式断言（能区分「ALTER 追加」与「12-step 重建」）属**可执行用例**范畴，正是 TP2 的交付物；已写入 TP2 派发提示。**代价：若 TP2 未补，spec R13/R16 的「只追加、不重建」性质在测试层就没有判别式证据**（产品行为本身已由 CR4 确认） | **待 TP2**（已登记，非阻断本轮合入） |
| CR5-F1 | WP5 | 4e53fcf9 | reviewer, run d001a864…） | fake child 的 `--dump-requests` 只写 method vs TP1 的 SR-R8-1/SR-R26-2 要读 `params.sessionId`/`params.cwd` | MINOR：**R26/R8 的线级cwd 断言缺可观察通道**；TP2 无法按字面实现 | 采纳方案 **(a)**：**新增并列选项 `--dump-request-params <path>`**（每行 `{method, params}`），**不动** `--dump-requests` 的冻结语义 →已写进 plan WP5 行 + tasks 2.5；**需 WP5 小范围重开补该选项** | **修复已交付 `1376e1b`（2026-10-01，main 已核对 diff：只动 `crates/agent-host/` 两个文件，`--dump-requests` 分支未变）；待新 reviewer（reviewer-E2 / CR5 Round 2）复核** |
| CR5-F2 | WP5 | 4e53fcf9 | reviewer, run d001a864…） | `ensure_runtime_tracked` 复用活进程 vs TP1 的 SR-R12-2 前提「新进程 P2」 | MINOR：R12 的**保证不受影响**（单绑定已由既有用例覆盖）；受影响的是 TP1 的用例前提文字 | 由 main 把 SR-R12-2 的前提改为「进程复用、单一心跳文件」，断言改为「恰好一个可派发端点 + `session/resume` 恰好一行 + 只有一个进程」——**不改代码**；已记入 TP 侧派发提示 | 待 TP1 修复轮/TP2 |
| CR5-F3 | WP5 | 4e53fcf9 | reviewer, run d001a864…） | 复用既有 runtime 时不重跑 `initialize`，门控读的是缓存的 `runtime.capabilities` | SUGGESTION：仍属「协商到的能力」，不虚报；design D4 第 3 条的措辞可能误导 | 接受；建议在 `resume` 注释补一句「复用既有进程时读到该进程既有协商结果」——记入 WP5 修复轮 | **已在 `1376e1b` 落地**（`host.rs` 注释，main 已核对为纯注释改动）；随 CR5 Round 2 一并复核 |
| CR5-F4 | WP5 | 4e53fcf9 | reviewer, run d001a864…） | `discard_spawned_runtime` 关闭该 runtime **全部**会话 | SUGGESTION：只在「同一 Agent 并发 create/resume」的窄竞态下可达，无数据损坏，与既有 reclaim 写法一致 | 接受为已知取舍并在此登记；**CR5 Round 2 复核：该路径行号零位移，未变好亦未变坏，不翻案** | 已处裁量（Round 2 复核维持） |
| CR5-F5 | WP5 | 1376e1b | reviewer, run a72214f4…） | `acpr-fake-acp-agent.rs:152-153`（`main` 接线）对照 `:889-1000`（5 个单测） | SUGGESTION：5 个单测全部直接调 `dump_inbound`，**`main` 里 `message.get("params")` 的接线无测试覆盖**；若被改成传 `None`，5 个单测仍全绿而 R8/R26 的线级通道静默退化为 `params: null` | 当前实现正确，故不改代码；已转为 **TP2 的强制口径**：SR-R8-1/SR-R26-2 必须经**真实子进程** + `--dump-request-params` 读行才能算数 | **待 TP2**（已登记） |
| CR5-F6 | WP5 | 1376e1b | reviewer, run a72214f4…） | `acpr-fake-acp-agent.rs:832-857`（`TempPath`）、`:960`、`:978-979` | SUGGESTION：`TempPath` 用 pid + 固定名定位并依赖 `Drop`；上次运行崩溃 + Windows pid 复用时 `!exists()` 断言会**假失败**（反向不成立，不会假通过） | 接受为已知项（只影响本地/CI 偶发红）；如再次出现偶发红，优先在此处加固（构造时先 `remove_file` 忽略错误） | 已处裁量 |
| CR6-F1 | WP6 | 76ab011 → a7bc596 | reviewer, run 208b74bb… 提出 / run 9d7a0b41… 复核） | `command.rs:1037-1040`（resume 幂等比对的第四项） | MINOR：`session.resume` 的 payload 恒为 `{}`，语义全在 `sessionRef`；同一 requestId 改指另一会话时指纹恒同 → 回**首次会话**的终态而非幂等冲突。**resume 是第一个 payload 恒空、该盲区必然命中的命令** | 已修（`a7bc596`）：`same` 追加 `record.session() == Some(&target.session)`，不一致回**既有**码 `nodelink.command.idempotency_conflict`；**未**扩到通用 mutation 臂（reviewer 划的边界）。**Round 2 已复核：已解决**——含「`None ⇒ 冲突` 分支在真实持久化下不可达」的无误伤论证（core `resume_session` 无条件写 `Some(session)`，且 `session.resume` 无 `CommandPayload` 变体故无第二写入路径） | **已解决**（Round 2 PASS@`a7bc596`） |
| CR6-F2 | WP6 | a7bc596 | reviewer, run 208b74bb… 提出 / run 9d7a0b41… 复核） | `command/tests.rs:139-142/162-164/363/2907-2919` | MINOR：用例名声称「不读取会话行」但只断言 `commit_calls` 不变，**无法区分**「授权先于读取」与「授权先于副作用」 | 已修：给 `CommandStore` 加 `load_recovery` 原子读计数，被拒路径断言读取计数**基线不变**（与提交计数两条独立断言）；doc 注释改写为两条独立命题，不再夸大。Round 2 确认 `load_recovery` 是读会话行的唯一入口 ⇒ 该计数是直接证据 | **已解决**（Round 2 PASS） |
| CR6-F3 | WP6 | a7bc596 | reviewer, run 208b74bb… 提出 / run 9d7a0b41… 复核） | `docs/NODE_LINK_PROTOCOL.md:663-664` | SUGGESTION：`local_terminal` 兜底（终态落盘失败仍发本地 `completed` 且 `terminalEventId: null`，持久记录仍 `accepted`）未在§12.7 登记；与 `session.create` 的 RV2 同源但 resume 副作用更重 | 已修：§12.7 `session.resume` 结果契约段补同形收窄注记（并登记跨会话 requestId 复用口径），**纯文档、零行为变更**；采用「分段登记」而非跳 create/resume 统一表述，Round 2 判为可接受（差异写在各自该在的地方，且已显式指回 `:649`） | **已解决**（Round 2 PASS） |
| CR6-F4 | WP6 | a7bc596 | reviewer, run 9d7a0b41…） | `core/src/broker.rs:2783-2810` vs `command.rs:392-470` | SUGGESTION（**纠正一个被延后的遗留判断**）：coder-F3 自报的「通用 mutation 臂存在同形幂等盲区」**不成立**——通用臂的幂等五项比对在 core 的 `Broker::idempotency` 里**已含 `session`**，存储层 `commit` 再比一次；resume 之所以是唯一命中点，只因它在调 core **之前**就短路返回首次结果 | **main 裁定：据此关闭 coder-F3 的待澄清问题 #1，不新开工作包、不改通用臂行为**（无产品影响） | **已关闭**（无后续动作） |
| CR1 | WP1 | 248d9b9 | reviewer, run 51afad9d…） | plan.md 的 PV1/PV2 Evidence 列 | MINOR：（历史）当时声明的日志路径 `reports/PV1-{WP}.log` 不存在，真实证据是各 WP 的报告与其引用的日志（DR1-F38：本行原先被批量替换改坏，已恢复准确表述） | 已改 PV1 行的证据列为「各 WP 报告 + 其引用日志」 | 已闭环（计划回写） |
| CR1-F2 | WP1 | 248d9b9 | reviewer, run 51afad9d…） | `docs/SESSION_CONTINUITY_DESIGN.md` 写「当前为 NotImplemented」 | SUGGESTION：已陈旧（WP1 已置 Implemented） | 该文档本就在 WP6 写范围（「更新状态注记」）；已归 WP6 收口 | 归 WP6 |
| CR1-F3 | WP1 | 248d9b9 | reviewer, run 51afad9d…） | `crates/acp-protocol/src/methods.rs` 的 `implemented: true` | SUGGESTION：`implemented` 是 crate 级语义、不等于端到端落地 | 已按 reviewer 建议登记口径：该标志表示「本 crate 的 wire 层已实现」，不代表端到端可用（端到端可用性以 matrix 的 `facade: not_advertised` 与能力门控为准） | 已登记（不改代码） |
| CR2-F1 | WP2 | 32f71f5 | reviewer, run 45bd6260…） | plan.md Shared File Ownership 缺 `crates/node-link-protocol/tests/` | MINOR：WP2 为新增 fixture 改了 `envelope_fixtures.rs` 的两个计数常量（38→40、9→10），未增删用例、未弱化断言 | 已补登一行（WP2, TP2；Merge Owner TP2；Order WP2→TP2；Re-verify TP2: PV1） | 已闭环（计划回写） |
| CR2-F2/F3/F4/F5 | WP2 | 32f71f5 | reviewer, run 45bd6260…） | accepted.result 的 schema/if-then、**`docs/NODE_LINK_PROTOCOL.md:662` 的 §12.7 结果投影收窄句**（原记「§10 措辞」，DR1-F56 订正；§10 的 grant 表是 DR1-F11 的对象且已由 WP2 处理）、SYNC §11.5 列约定、日志首行 revision 标注 | MINOR/SUGGESTION：均为文档/注释层面，非阻断 | 接受为已知项：F3 的对象是 `docs/NODE_LINK_PROTOCOL.md:662`（§12.7 收窄句）；**已把该文件加入 WP6 写范围**（DR1-F51，两处：`:662` 收窄句加 `session.resume`、§12.7 示例加「可见版本为 2」的非规范注记，见 `## Check Plan Changes`）；其余在最终验收前由 main 复核是否需要补，不影响依赖与可产出性 | 已处裁量 |
| CR7-F1 | TP1 | sha256:449bd7b3… | reviewer, run 594976c8…） | specs/local-agent-host R10 vs design D4 | **MAJOR：规格 MUST 物理不可满足**（能力只经 initialize 得知，而 initialize 必先 spawn） | 已改规格（不发送 + 回收子进程）并更名 Scenario；design D4 不变；Coverage Index R10 heading 同步 | 待 CR7 Round 2 复核 |
| CR7-F2 | TP1 | sha256:449bd7b3… | reviewer, run 594976c8…） | specs/workspace-resolution 的 NULL 归入服务端不可用类 vs design D3 | **MAJOR：同一输入两个互斥错误码** | 已改规格：NULL 与能力不支持同路径（`nodelink.command.unsupported`）；其余校验失败仍为服务端不可用类 | 待 CR7 Round 2 复核 |
| CR7-F3/F4/F5/F7/F8 | TP1 | sha256:449bd7b3… | reviewer, run 594976c8…） | TP1 报告的引用路径/注入强度/fake agent 义务/心跳文件口径/端口登记 | MINOR/SUGGESTION：引用不实、某个注入可能空转、fake agent 义务未写进计划、备选口径不可观察、端口未登记 | 计划侧已处理 F5（WP5/tasks 补义务）与 F8（Runtime Resources 补 listener 行）；F3/F4/F7 属 TP1 报告内容，交 TP1 修复轮（新实例 Round 2）重做 | 待 TP1 修复 |
| CR7-F6 | TP1 | sha256:449bd7b3… | reviewer, run 594976c8…） | SR-R32-1 的 in-process 恢复入口 | SUGGESTION：reviewer 裁决「in-process 足够，无需真实进程重启」，建议显式化持久化读回维度 | 接受该裁决；已交 TP1 修复轮按建议强化（重开同一 data_dir 后 `command.status`） | 待 TP1 修复 |
| CR8-F7 | TP2 | f44301e → 3484541 | reviewer-T3（独立隔离子 Agent，run 73ae307c） | a3 两份日志的 `# revision` 记 base `f44301e`，而交接行绑定 `3484541` | MINOR：测试在提交前的工作区执行，日志本身无法自证被测版本 | **已由 main 以真实 git diff 闭环**：`git diff -U0 f44301e..3484541` 的 38 行 +/- **全部以 // 或 /// 开头，非注释改动 0 行**，断言零改动（见 cr-c1-candidate-review.md 文末） | **已闭环**（Round 2 PASS） |
| CR8-F8 | TP2 | 3484541 | reviewer-T3（run 73ae307c） | tp2-tester.md 的 `result: NOT_EXECUTED` 不在 role-report 枚举内 | MINOR：交接行枚举不合规（方向保守：未把未执行写成 PASS） | 记录层瑕疵；本文件 Handoff Index 以合规值登记，TP2 报告原文保留为历史证据不改写 | **已登记为已知瑕疵** |
| CR8-F9 | TP2 | 3484541 | reviewer-T3（run 73ae307c） | session_resume_e2e.rs 的 `details` 相等断言 | SUGGESTION：实测两侧均为 `{}` 且同走硬编码分支，该断言是回归护栏而非当前判别器 | **不加固**、只补注释；R31 判别力由「目录已删除」前提 + 读取计数 + 计数器自检承担 | **已接受** |
| CR-C1-F1 | WP6 | 2ed142d | reviewer-C1（候选检视，run f494a2c4） | owner.rs 的 ScriptedEndpoint 字段 doc | MINOR：TP2 改了 `ScriptedBackends::create` 的 `agent_session_id`，WP6 当初写的 doc 仍称「创建脚本端点不产生」——注释与代码矛盾（CR8-F1 同类跨包残留） | 纯注释、不影响断言与行为；登记为已知项，本轮不改 | **已登记** |
| CR-C1-F2 | TP2 | 2ed142d | reviewer-C1（run f494a2c4） | Broker::resume_session × 生产 AgentHost::resume 的组合覆盖 | MINOR：e2e 注入 ScriptedBackends 而非生产 AgentHost，该组合未被同一测试覆盖 | **非阻断**（AGENTS.md §9 允许 fake ACP Agent；两侧各有真实覆盖，且是 WP1–WP6 既有结构） | **已登记为已知覆盖边界** |

## Planning Findings（DR1 线程）

> 本表承载**规划门禁线程 DR1** 的发现（Round 1–16）。它们**不属于任何工作包**，故与上方 `## Review Findings` 分表存放：门禁只对 `## Review Findings` 逐行校验「`Work Package` 必须是计划内的工作包 ID」，规划级发现在该表中会因 `NOT_APPLICABLE（规划）` 被判非法。逐条结论与处置见本表、对应轮次报告与 `## Dependency Declaration Review`。

| ID | Work Package | Revision | Reviewer | Location | Severity / Impact | Resolution | Recheck |
| --- | --- | --- | --- | --- | --- | --- | --- |
| DR1-F1 | NOT_APPLICABLE（规划） | sha256:77c651a4… | reviewer, run d620caef…） | plan.md WP2/WP5 Dependencies 与 Waves | MAJOR：封闭词表原子点被拆到两个并行 WP，分支级 PV2 不可产出 | 已把封闭词表原子点合并为单一 WP2（含 core `required_grant` 臂），WP3 声明 code:WP2；见 plan.md `## Contract Changes` | 待 DR1 Round 2 复核 |
| DR1-F2 | NOT_APPLICABLE（规划） | sha256:77c651a4… | reviewer, run d620caef…） | design.md D1 的 `pack: null` vs check-command-catalog.mjs | MAJOR：冻结契约在现行门禁下不可实现，修补路径未登记 | 新增 design D6：豁免判据改为「transport 不含 sync」；脚本与门禁说明四处写入 WP2 写范围 | 待 DR1 Round 2 复核 |
| DR1-F3 | NOT_APPLICABLE（规划） | sha256:77c651a4… | reviewer, run d620caef…） | plan.md WP5 写范围 / D5 资产表 | MAJOR：`docs/SYNC_PROTOCOL.md` §11.5 与 `docs/SECURITY_DESIGN.md` §10.2 无归属 | 两文档（含「12 条」计数）已写入 WP2 写范围与 Shared File Ownership | 待 DR1 Round 2 复核 |
| DR1-F4 | NOT_APPLICABLE（规划） | sha256:77c651a4… | reviewer, run d620caef…） | plan.md TP1 Dependencies/Verification | MAJOR：TP1 声明 none 却要交付依赖未实现 API 的编译用例 | 拆为 TP1（W1，仅设计、不出可编译用例）与 TP2（末波次，声明 code:WP1…WP6） | 待 DR1 Round 2 复核 |
| DR1-F5…F8 | NOT_APPLICABLE（规划） | sha256:77c651a4… | reviewer, run d620caef…） | plan.md/design.md 各处 | MINOR/SUGGESTION：Contract Freeze 漏 D1、文档区域登记不准、WP5 未声明 code:WP1、scout 证据路径写错 | 已分别补 D1、改 §3.1/§5.2 与 §7/§7.2 区域、显式声明 code:WP1、路径改为 reports/scout-target.md | **DR1 Round 2 复核：已解决**（reports/dr1-dependency-review-round2.md） |
| DR1-F9 | NOT_APPLICABLE（规划） | sha256:17460283… | reviewer, run 96c922ec…） | plan.md WP2 写范围 / D5 资产表 / proposal Impact | MAJOR：封闭词表原子点漏第七个强制写目标 `crates/identity-auth`（`GRANTS` 镜像 + 既有验收测试），WP2 分支的 PV1 必红 | `crates/identity-auth/`（`src/authorization.rs` + `tests/authorization.rs`）已写入 WP2 写范围并各登一行 Shared File Ownership；`docs/IDENTITY_AND_AUTH_CONTRACT.md` 核实为 no-op（不列 grant 会员）并记录理由；design D5 与 proposal Impact 已补 | 待 DR1 Round 3 复核 |
| DR1-F10 | NOT_APPLICABLE（规划） | sha256:17460283… | reviewer, run 96c922ec…） | design.md D5 冻结的 `layers.broker: native` vs `schemas/acp/compatibility-matrix.schema.json` | MAJOR：该取值不在 schema 枚举内（`check:acp` 用 ajv 强校验），WP1 的 PV2 必红，且 `schemas/acp/` 无写归属 | D5 改为枚举内的 `project_and_preserve`（与 `method.session_prompt` 同值），不新增 schema 枚举；WP1 的 Contract Freeze 补 D5，tasks 2.1 明写禁止写 `native` | 待 DR1 Round 3 复核 |
| DR1-F11 | NOT_APPLICABLE（规划） | sha256:5a82f8bb… | reviewer, run d6d426a3…） | tasks.md 2.2 / design D5 的 NODE_LINK_PROTOCOL 同步区域 | MINOR：§10 的 grant→命令表会陈旧（门禁不覆盖） | 同步清单与 tasks 2.2 已补「§10 grant 表」 | **DR1 Round 3 复核：已解决** |
| DR1-F13 | NOT_APPLICABLE（规划） | sha256:5a82f8bb… | reviewer, run d6d426a3…） | plan.md WP2 写范围 / `crates/server/src/node_link/command.rs` 的 `core_payload()` | MAJOR：WP2 落地第 13 个 payload 变体后 server 穷尽匹配失配，而该文件属 WP6(W4)；WP2 的 workspace PV1 不可产出（实测 `core_payload` 有 0 个 `_ =>` 通配臂） | **PV1 改为阶段制**（分支只查本 WP 的 crate 子集；集成/候选/主分支跑 workspace 全量）；WP6 写范围扩至整个 `crates/server/` 并明写接手 `core_payload` 收口；红窗口与责任人已登记在 Dependency Handoffs | 待 DR1 Round 4 复核 |
| DR1-F14 | NOT_APPLICABLE（规划） | sha256:5a82f8bb… | reviewer, run d6d426a3…） | plan.md WP3 写范围 / `crates/server/src/local_admin/test_support.rs:1021` | MAJOR：WP3 新增两个必需 trait 方法后，`NotTouched`（`#[cfg(test)]`）无任何 WP 拥有 → 候选/主分支 PV1 永不可绿 | 同 F13 的阶段制 PV1 + WP6 写范围扩至整个 `crates/server/`（含 `src/local_admin/test_support.rs`）；**不采用默认实现**（保留能力诚实边界）；四个下游实现点逐点归主（WP3/WP4/WP5/WP6） | 待 DR1 Round 4 复核 |
| DR1-F15 | NOT_APPLICABLE（规划） | sha256:5a82f8bb… | reviewer, run d6d426a3…） | plan.md 的 `crates/**/tests/` 行与 tasks 2.5/2.6 | MINOR：WP5/WP6 需扩展的既有测试目录未登记 | Shared File Ownership 补 `crates/agent-host/tests/`（WP5,TP2）与 `crates/app/tests/`（WP6,TP2）两行 | 待 DR1 Round 4 复核 |
| DR1-F16 | NOT_APPLICABLE（规划） | sha256:5a82f8bb… | reviewer, run d6d426a3…） | plan.md Contract Changes 第 5 行 vs tasks 1.5 | MINOR：声称固定 specs 基线但无对应任务 | tasks 1.5 补完成条件；`verification.md` 的 `## Target` 已写入 6 个 sha256 摘要 | 待 DR1 Round 4 复核 |
| DR1-F17 | NOT_APPLICABLE（规划） | sha256:5a82f8bb… | reviewer, run d6d426a3…） | plan.md 的 `@design-rev-2/3/4` 标签 | SUGGESTION：同一目录版本标签不一致 | 已统一为 `design-rev-4`（8 处） | **DR1 Round 4 复核：已解决** |
| DR1-F18 | NOT_APPLICABLE（规划） | sha256:0b6f6b52… | reviewer, run f95b1555…） | plan.md Contract Changes F16 行 / Completion Criteria vs tasks.md 1.5 | MINOR：声称「tasks 1.5 补了完成条件」但实际未补（声称改过实际未改） | tasks.md 1.5 已补 sha256 完成条件；plan.md 该行改为引用 `verification.md` 的 `## Target`；摘要已实写 | 待 DR1 Round 5 复核 |
| DR1-F19 | NOT_APPLICABLE（规划） | sha256:0b6f6b52… | reviewer, run f95b1555…） | `reports/dr1-dependency-review-round3.md` 在仓库中缺失 | MINOR：`## Dependency Declaration Review` 第 3 行是 `result=FAIL` 的 NEW 证据行，其 `report_path` 不可读，违反「NEW/REUSED 报告路径必须可读」 | 已从 Round 3 的 run 日志原样提取并落盘 `reports/dr1-dependency-review-round3.md`（123 行，含 4 份证据行与 handoff_index）；四份报告现已全部可读 | 待 DR1 Round 5 复核 |
| DR1-F20 | NOT_APPLICABLE（规划） | sha256:0b6f6b52… | reviewer, run f95b1555…） | plan.md PV1/PV2 阶段列、W2–W5 Enter Condition、Local Checks | MINOR：同一份计划对「集成阶段/进入条件里的 PV1 是否必须 workspace 全绿」给出两种读法 | PV1 阶段列改为「分支（阶段 1）/ **集成基线（阶段 2，全部 WP 集成后、候选前）** / 候选 / 主分支」；W2 的 Enter Condition 改为「PV1 **阶段 1** 与 PV2 PASS」；PV2 的工作包集合去掉 TP2（与 TP2 只标 [PV1] 对齐）；Local Checks 补「不代替阶段 2」 | 待 DR1 Round 5 复核 |
| DR1-F21 | NOT_APPLICABLE（规划） | sha256:0b6f6b52… | reviewer, run f95b1555…） | verification.md 的 `## Checks` PV1 行 | MINOR：检查记录行仍是 workspace 全量旧口径，会掩盖阶段 1/2 区别 | 已拆为三行：PV1 阶段 1（分支，按 crate 子集，证据 各 WP 的报告与其引用的日志（如 `reports/wp1-coder.md`、`reports/wp2-coder-PV1.log`））、阶段 2（集成基线）、阶段 2（候选/主分支） | 待 DR1 Round 5 复核 |
| DR1-F22 | NOT_APPLICABLE（规划） | sha256:0b6f6b52… | reviewer, run f95b1555…） | plan.md Shared File Ownership 仅登记 `crates/storage-sqlite/tests/migration.rs` | MINOR：其余至少 8 个既有测试文件含全字段字面量，与 WP4 的实际写入面不一致 | 该行已改为登记整个 `crates/storage-sqlite/tests/`（Writers=WP4,TP2；Merge Owner=TP2；Order=WP4→TP2；Re-verify=TP2: PV1），并在区域说明里点名那些文件 | 待 DR1 Round 5 复核 |
| DR1-F23 | NOT_APPLICABLE（规划） | sha256:0b6f6b52… | reviewer, run f95b1555…） | plan.md 红窗口段 vs `.husky/pre-commit`→`scripts/pre-commit.mjs` | MINOR：钩子在涉及 Rust 的提交上跑 workspace 全量 clippy，红窗口内 WP3–WP6 的提交会被拒 | 红窗口段已补一句：该情形按 AGENTS.md §8 用 `--no-verify`，不改变 PV1 阶段判定责任，也不得借此跳过非 Rust 检查；tasks 3.1 同步注明 | 待 DR1 Round 5 复核 |
| DR1-F24 | NOT_APPLICABLE（规划） | sha256:0b6f6b52… | reviewer, run f95b1555…） | plan.md WP1 写范围 vs `fixtures/acp/v1/manifest.json` 登记约定 | SUGGESTION：若实现者新增 ACP fixture 会写到无归属路径并使 PV2 变红 | WP1 写范围已加 `fixtures/acp/v1/`（含 `manifest.json`），并在 Inputs/Outputs 与 tasks 2.1 写明二选一：新增 fixture 必须登记 manifest，或只用内联字节而不动 fixture 目录 | **DR1 Round 5 复核：已解决** |
| DR1-F25 | NOT_APPLICABLE（规划） | sha256:3b208486… | reviewer, run 0cc00606…） | verification.md DCR 第 4 行的 report_path（`reports/dr1-dependency-review-round4.md`）不可读 | MINOR：result=PASS/NEW 行的报告路径必须可读（role-report 硬要求），且「四份报告现已全部可读」断言不实 | 已从 Round 4 的 run 日志原样提取并落盘 `reports/dr1-dependency-review-round4.md`（158 行）；Round 5 报告也已落盘；五份报告全部可读，相关断言已改为准确表述 | 记录层修复，reviewer 已声明无需重开轮次 |
| DR1-F26 | NOT_APPLICABLE（规划） | sha256:3b208486… | reviewer, run 0cc00606…） | verification.md 的 `## Checks` PV2 行仍含 TP2、仅「分支」阶段 | MINOR：与 plan.md 的 PV2 行（WP1/WP2/WP3/WP4/WP6，四段阶段）不一致 | 已改为「分支（阶段 1）、集成基线（阶段 2）、候选、主分支 / WP1、WP2、WP3、WP4、WP6」，与 plan.md 逐字对齐 | 记录层修复，无需重开轮次 |
| DR1-F27 | NOT_APPLICABLE（规划） | sha256:3b208486… | reviewer, run 0cc00606…） | plan.md 的 W3–W5 Enter Condition 只引用 W2 行定义的术语 | SUGGESTION：降低人工核对时「阶段 1 是否就是该上游的判据」的可读性；无矛盾 | 已在 Execution Waves 表后补一行术语定义（指向 W2 行），消歧而不复制 | 待 DR1 Round 6 复核 |
| DR1-F28 | NOT_APPLICABLE（规划） | sha256:3b208486… | reviewer, run 0cc00606…） | plan.md 的 F16 记录措辞（将来时、未标 Round 5 已闭环）与 Coverage Index 未列阶段 1 日志 | SUGGESTION：均为记录措辞/登记完整性层面 | 部分解决：(b) F16 行已回写「Round 5 复核：已解决」；(c) 阶段 1 证据路径已在 `## Checks` 的三行 PV1 与 tasks 3.1 登记（Coverage 行保留阶段 2 路径，已由 Completion Criteria 说明阶段 1 为开发期证据）；**(a) 实际未做**——`plan.md:16` 仍为将来时，已由 DR1-F29 以「更正记录而不动 plan.md」的方式如实闭环 | DR1 Round 6 复核：部分解决（残留转 DR1-F29） |
| DR1-F29 | NOT_APPLICABLE（规划） | sha256:39a3919b… | reviewer, run 71f649db…） | plan.md:16 仍为将来时，而 F28 处置栏声称已改 | MINOR：记录称已改而实际未改（与 F18/F25 同类） | 更正记录：F28(a) 如实标为未落地并说明理由（摘要实际已在 `## Target` 就位）；**不动 plan.md**（避免为一句措辞白开一轮）；见 `## Check Plan Changes` 的 Round 6 条 | 记录层修复，reviewer 已声明无需重开轮次 |
| DR1-F31 | NOT_APPLICABLE（规划） | sha256:076de2d4… | reviewer, run 069ead94…） | proposal.md 的 constraints 与 success_criteria | **MAJOR：同一句「MUST NOT 为此启动 Agent 进程」在 proposal 里漏改**，与修正后的 specs/design 相反；proposal 计入 requirementsDigest | 已改文不改设计：两处均改为「MUST NOT 发送 session/resume + MUST 回收本次拉起的子进程」；assumptions 的「返回不可用」改为「显式返回不支持（nodelink.command.unsupported）」；design.md 的 Risks 缓解指向已订正为 proposal.md + specs | 待 DR1 Round 8 复核 |
| DR1-F32 | NOT_APPLICABLE（规划） | sha256:076de2d4… | reviewer, run 069ead94…） | plan.md 的 Completion Criteria vs PV1 行；verification.md 的 Checks 与 F21 处置栏 | MINOR：CR1-F1 的证据路径只在 PV1 行撤铺，其余仍是旧路径 | 已全部统一为「各 WP 的报告及其引用的日志」（3 处，已核实无残留） | 待 DR1 Round 8 复核 |
| DR1-F33 | NOT_APPLICABLE（规划） | sha256:076de2d4… | reviewer, run 069ead94…） | verification.md 的 ## Target 括注 vs 现值行 | MINOR：括注把 acp 的旧哈希写成 local-agent-host 的旧值，内部矛盾 | 已用 sha256sum 实算值重写 6 行现值与括注，并列出相对 Round 3 的四个变化面 | 待 DR1 Round 8 复核 |
| DR1-F34 | NOT_APPLICABLE（规划） | sha256:076de2d4… | reviewer, run 069ead94…） | specs/acp-wire-protocol 的 R1 标题「圆形保真」 | SUGGESTION：round-trip fidelity 的误译，宜作「往返保真」 | 已同批改 spec 标题与 plan.md 的 R1 heading（必须逐字一致），并刷新摘要 | 待 DR1 Round 8 复核 |
| DR1-F35 | NOT_APPLICABLE（规划） | sha256:076de2d4… | reviewer, run 069ead94…） | verification.md 的 ## Handoff Index | SUGGESTION：缺 CR1/CR2/CR7 行；TP1 行状态陈旧 | 已补 CR1/CR2/CR7 与 DR1-R7 行，TP1 行标注「CR7 FAIL，待修复轮」 | 待 DR1 Round 8 复核 |
| DR1-F36 | NOT_APPLICABLE（规划） | sha256:57b47d87… | reviewer, run c8ef681b…） | design.md 的 Goals/Risks 与 plan.md 的 Code Review 关注点 | MINOR：三处以「无副作用」描述能力未宣告路径，与 design.md 自述「该路径会短暂 spawn 并回收」在叙述层不一致 | **不改契约/计划的处置（避免白开一轮）**：已在本节记录正确口径（不合格判据是「未发送 session/resume + 子进程已回收 + 会话状态不变」，**不是「完全没有 spawn」**），并写入 WP5/CR5 的派发提示与 tasks 3.6 的复核口径 | 已处裁量（见 Check Plan Changes） |
| DR1-F37 | NOT_APPLICABLE（规划） | sha256:57b47d87… | reviewer, run c8ef681b…） | verification.md 指向的第 7 轮报告不可读（同类问题第三次复发：F19→F25→F37） | MINOR：FAIL/NEW 行的报告路径必须可读 | 已从第 7 轮的 run 日志原样提取并落盘 `reports/dr1-dependency-review-round7.md`（162 行）；八份 DR1 报告现已全部可读；另已在派发提示中停止断言「报告均在仓库内」 | 已闭环（记录层） |
| DR1-F38 | NOT_APPLICABLE（规划） | sha256:57b47d87… | reviewer, run c8ef681b…） | verification.md 的 CR1 finding 行 Impact 单元 | MINOR：F32 的批量替换把 CR1 的原始描述改坏，出现了不实陈述 | 已恢复为「（历史）当时声明的 `reports/PV1-<WP>.log` 不存在…」并注明修复痕 | 已闭环（记录层） |
| DR1-F39 | NOT_APPLICABLE（规划） | sha256:57b47d87… | reviewer, run c8ef681b…） | specs/node-link-owner-server 的 R37 THEN 允许 `status = failed 或 uncertain` | MINOR：同一输入允许两个终态，可能用 `uncertain` 掩盖一个已确定的失败 | **不改契约的处置**：已在本节与 `## Check Plan Changes` 固定口径——`uncertain` **只**属于崩溃窗口（R32）；能力不支持路径的终态必须是 `failed` + `nodelink.command.unsupported`；写入 WP6/TP2 的派发提示 | 已处裁量（见 Check Plan Changes） |
| DR1-F40 | NOT_APPLICABLE（规划） | sha256:57b47d87… | reviewer, run c8ef681b…） | TP1 报告的 R1 heading 仍引旧标题「…圆形保真」 | SUGGESTION：契约侧已改为「往返保真」 | 已写入 TP1 修复轮的派发提示（只改该报告，不动计划/契约） | 已处裁量 |
| DR1-F30 | NOT_APPLICABLE（规划） | sha256:39a3919b… | reviewer, run 71f649db…） | `## Dispatch Reconciliation` 的 Round 5 记录段与机器事实不符（三处） | SUGGESTION：①比较方式；②触及哪些文件；③详细结论落点 | 三项已逐条订正（见 `## Check Plan Changes` 的 Round 6 条）：①去空白后忽略大小写必须恰好等于 PASS；②仅 DCR 字段修复触及 plan.md；③详细结论在 `verification.md` 的 Round 5 段与 `reports/dr1-dependency-review-round5.md`；记录落点偏离已声明 | 记录层修复，无需重开轮次 |
| DR1-F12 | NOT_APPLICABLE（规划） | sha256:17460283… | reviewer, run 96c922ec…） | plan.md `crates/**/tests/` 行与 WP 整 crate 写范围、tasks 2.4 | MINOR：两条声明不能同时为真 | 该行改为逐文件登记 `crates/storage-sqlite/tests/migration.rs`（WP4,TP2；Merge Owner=TP2；Order=WP4→TP2），并明确 WP1–WP6 含各自 `src/**` 内联单元测试 | 待 DR1 Round 3 复核 |
| DR1-F62 | NOT_APPLICABLE（规划） | sha256:1154ae4b… | reviewer, run 0e3dc534…） | `plan.md:120` Contract Changes 的 Round 14 表「落点」列 | MINOR：仍写「plan WP6 行「额外义务 ④」」，而全库已无任何「额外义务 ④」标签（本轮回写已弃用序号）⇒ **悬空引用**，按序号检索会落空 | **main 处置（2026-10-01）：按 reviewer 建议不为此再压一轮才派 WP6**；本行改用 finding ID 表述，WP6 派发提示已强制要求用 `CR3-F1`/`DR1-F41`/`DR1-F42`/`DR1-F51` 而非序号引用。**待下次触碰 `plan.md` 时与 F63 同批修**（会变更摘要，需再开一轮） | **挂起：下次触碰 `plan.md` 时** |
| DR1-F63 | NOT_APPLICABLE（规划） | sha256:1154ae4b… | reviewer, run 0e3dc534…） | `plan.md:328` WP6 行对照 `tasks.md:29` | MINOR：**F57 只闭环了一半**——`tasks.md` 五个义务标签已全部改为 finding ID，但 `plan.md:328` 仍留一个裸「②」且无 `DR1-F42` 标签；另两处义务顺序相反 | 同 F62：不阻塞派发。**如实记录 `verification.md` 的 F57 条曾写作「两处全部改用 finding ID」，与实际文本不符**（plan 侧尚余1 处序号），已在 `## Check Plan Changes` 的 Round 15 段订正。**待与 F62 同批修** | **挂起：下次触碰 `plan.md` 时** |
| DR1-F64 | NOT_APPLICABLE（规划） | sha256:1154ae4b… | reviewer, run 0e3dc534…） | `plan.md:375` 新增 SFO 行的 File 列 | SUGGESTION：该行 File 带反引号（`` `crates/server/tests/` ``），而门禁 `workflow-check.mjs:106` 的 `scopeOverlap` 是裸字符串比较、不去反引号⇒ 将来若被检出也匹配不上（当前无实际影响，该登记对门禁本就惰性） | 接受为已知项（与同表其余 22 行风格不一致，但不影响门禁判定）；**待与 F62/F63 同批去掉反引号** | 已处裁量 |

## Premerge

> 候选 `2ed142d` 的 **Candidate Project Verify**（PV1 阶段 2 + PV2）与**独立候选 review** 证据。主 Agent 于合入前写入本块，随后在候选工作区运行 `workflow check --stage premerge`；**仅在 PASS 后**才把该块持久化为 `## Premerge History` 的版本化 receipt 并合入本地主分支。
>
> ②（**DR1 Round 16 已订正为不实**）此前此处写「复用依据见 `## Merge History`」，经复核**该表述不成立**：`## Merge History` 在合入前是**空表**，且 merger-A5 的待澄清项是 `tasks.md` **6.3**（不是 6.6）。正确落点：`reports/merge-u1-candidate.md` §6/§8 与本文件 `## Handoff Index` 的候选级 PV 行；**tasks 6.3 记 REUSED**——PV1 阶段 2 与 PV2 由 merger 在**同一候选提交 `2ed142d`** 上执行（零冲突、候选 tree 与 `3484541^{tree}` 逐字节相同），复用不引入任何版本或命令差异。

```agentic-premerge
version: 1
delivery_unit: U1
target_ref: refs/heads/main
target_commit: 81e350ff340014265eb7c9251237c799d4357fee
candidate_commit: 2ed142dedec2facf8f6e174d1aa165f549cbc47e
contract_digest: sha256:2adbf605f70bd49db880937dfa7bcddb8a3b44897663661f1e0e58cc16aca936
requirements_digest: sha256:5394327093e57d05b49ea4b04db708fcb97160791ea6cca8cec5a833154408fa
verify:
  result: PASS
  candidate_commit: 2ed142dedec2facf8f6e174d1aa165f549cbc47e
  evidence: {path: reports/merge-u1-candidate-PV1-stage2-workspace-test.log, sha256: "sha256:a93f928e74d1e517b3185d22492d6f8b67ea65f0fd0dbe5931ee73ff99761f0a"}
review:
  result: PASS
  candidate_commit: 2ed142dedec2facf8f6e174d1aa165f549cbc47e
  reviewer: reviewer-C1（独立隔离子 Agent run f494a2c4-800c-462f-9ad3-92a436a80162，CR-C1 Round 1，0×CRITICAL/0×MAJOR）
  author: WP1–WP6 = coder-A/B/C/D/E/E2/E3/F、TP2 = tester-A/A2/A3（合并执行者 merger-A2/A3/A4/A5）
  evidence: {path: reports/cr-c1-candidate-review.md, sha256: "sha256:04e99e078b48b95e95193f4c93c9cf3404df72d05cce34f931e8cfc3b3b9969b"}
alternative_checks:
  - name: "C1：cargo test --locked --workspace --all-features（含 fake ACP Agent 驱动的恢复路径；1074 passed / 0 failed / 2 ignored）"
    result: PASS
    candidate_commit: 2ed142dedec2facf8f6e174d1aa165f549cbc47e
    evidence: {path: reports/merge-u1-candidate-PV1-stage2-workspace-test.log, sha256: "sha256:a93f928e74d1e517b3185d22492d6f8b67ea65f0fd0dbe5931ee73ff99761f0a"}
  - name: "C2：npm run check（合同漂移、依赖方向、封闭词表与 ACP 固定向量；10 道门禁全绿）"
    result: PASS
    candidate_commit: 2ed142dedec2facf8f6e174d1aa165f549cbc47e
    evidence: {path: reports/merge-u1-candidate-PV2.log, sha256: "sha256:ea718e0d81b9d09dca30081cdfcb5fd9732e2a05f6bd6faf900d70dad0648f4c"}
```

## Merge History

| Merge ID | Delivery Unit | Target Ref | Merger | Candidate Commit | Merged Commit | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| M1 | U1 | refs/heads/main | merger-A5（合入）/ merger-A6（主分支回归）/ merger-A7（闭环修正） | 2ed142dedec2facf8f6e174d1aa165f549cbc47e | **0d2be6d**（`--no-ff`，父提交对 `81e350ff340014265eb7c9251237c799d4357fee` + 候选 `2ed142d…`，零冲突）+ 修正提交 `69f1ac1` | reports/merge-u1-main.md、reports/merge-u1-main-diff-materials.md、reports/premerge-receipt-u1.md |
| M1-后续 | U1 | refs/heads/main | merger-A7 | 2ed142dedec2facf8f6e174d1aa165f549cbc47e | **20c1623ba3cf8ce751082b2699ef66b76f76f3cf**（追加提交，父提交 `69f1ac1bc6af81461d199257f512965f646ec55c`；`plan.md` 引用归属修正 + 三份新报告入库 + `## Merge History` 回填；用于消除 CR-PM-F2「HEAD 门禁红」）。其后另有一次仅改 `tasks.md` 6.7 与本轮报告的闭环提交，sha 见 `reports/merge-u1-main.md` 第 3 轮 | reports/cr-pm-post-merge-review.md、reports/merge-u1-main.md |
| M1-收尾 | U1 | refs/heads/main | merger-A8 | 2ed142dedec2facf8f6e174d1aa165f549cbc47e | `ac5de2a6bc91cb3320c3a9286a5169fb4183753a`（父提交 `976b34ccebbf755e61aa87eaeafac35e6f1f480e`；validator-A 独立验证结论入库：`## Independent Validation` 7.1 行回填 + `reports/validation-session-resume.md`）+ 本行 `## Merge History` 回填与用户裁定行（提交 sha 见 `reports/merge-u1-main.md` 第 4 轮）。两次提交均在**各自最终 HEAD** 上取得完整门禁 exit 0 | reports/validation-session-resume.md、reports/merge-u1-main.md、reports/final-main-gates-npm-check.log |
| M1-提交信息合规 | U1 | refs/heads/main | merger-A9 | `2ed142dedec2facf8f6e174d1aa165f549cbc47e`（未变） | **未改动历史**（无 rebase / amend / 改提交信息 / merge / push）。唯一的入库提交 `cae3dbd19e38fede8243ebc63f07012ab438c721`（父提交 `2ec1f0fbf82785ae28963558734c8ef0b1b7a7d0`，tree `4a80460`）只扩充 `commitlint.config.mjs` 的 `SCOPES` 两项——新增 `session-resume`（会话恢复跨层交付面）与 `test`（历史兼容项，如实注明由来）——并同步 `AGENTS.md` §8 一句。使 `npx commitlint --from "81e350f^" --to HEAD` 由 **exit 1 / 10 problems** 转为 **exit 0**；`npm run check` 十道门禁逐道 exit 0、三条 Rust 门禁 exit 0（workspace 1074 passed / 0 failed / 2 ignored）均在 **`cae3dbd…`** 上取得，其后仅追加一条纯文档的记录提交 `ee5179539e120e7fa2d08bc28eb4cb24e57b5edb`（本行 + `reports/merge-u1-main.md` 第 5 轮），该提交上的同组检查亦全绿，取据见 `reports/merge-u1-main.md` §36 | reports/merge-u1-main.md、reports/commitlint-scope-fix-commitlint.log、reports/commitlint-scope-fix-npm-check.log、reports/commitlint-scope-fix-cargo-test.log、reports/commitlint-scope-fix-finalhead-*.log |

## Premerge History

| Merge ID | Delivery Unit | Work Packages | Target Commit | Candidate Commit | Result | Receipt Path |
| --- | --- | --- | --- | --- | --- | --- |
| M1 | U1 | WP1, WP2, WP3, WP4, WP5, WP6, TP2 | 81e350ff340014265eb7c9251237c799d4357fee | 2ed142dedec2facf8f6e174d1aa165f549cbc47e | PASS | reports/premerge-receipt-u1.md |

## Test Design and Authoring

待 TP1（tasks 2.7 / 4.1）与 TP2（tasks 2.8 / 4.2）执行后填写。

## Independent Validation

| Task | Target Revision | Result | Report Path |
| --- | --- | --- | --- |
| 7.1 | 69f1ac1bc6af81461d199257f512965f646ec55c | PASS（0×CRITICAL/0×MAJOR；覆盖充分性 PASS、迁移 PASS、能力门控 PASS 含 1 处 MINOR 盲区、回滚限制 PASS 含 1 处记录层 MINOR；待验证假设以**只读侦察**判 PASS，**非 fake Agent 自证**） | reports/validation-session-resume.md |

> **7.1 的待验证假设为何不是 BLOCKED（依据摘要）**：本机存在真实 ACP Agent `omp.exe`（Oh My Pi）；validator 对其二进制做**静态取证**，内嵌源码逐字含 `sessionCapabilities: { list: {}, fork: {}, resume: {}, close: {} }`（宣告成立）与 `async resumeSession(e)` 的实质实现（读取 sessionId / cwd / mcpServers，入参与本仓 `acp-protocol` 的 `SessionResumeRequest` 逐字段兼容）。对照之下 `codex.exe` 对 `sessionCapabilities` 命中为 0（不宣告该能力，走已被 R10/R37 覆盖的不支持路径）。**限制（随结论传递）**：**未启动 `omp`、未发起任何真实会话**，故为**静态证据**而非运行时握手观测；若要求运行时证据，本项应改判 PENDING，且需单独授权「启动真实 Agent 进程」（超出本次只读边界）。
> **仍未闭合（不计入 PASS）**：① 2 条 `#[cfg(unix)]` 用例 **PENDING**（validator 独立复现交叉编译失败 `cargo check --target x86_64-unknown-linux-gnu` exit 101、缺 `x86_64-linux-gnu-gcc`，零编译零执行证据；证据链无一处写成已通过）；② `cargo-deny` / `gitleaks` 本机不可执行，只由 CI 判定。
> **用户裁定（2026-10-01）：不做运行时验证。** validator 提出的「是否升级为运行时握手证据」选项（需授权在本机启动真实 Agent 进程 `omp.exe`）**由用户明确否决**。因此第（2）项**保持 PASS，依据为静态取证**（内嵌源码含能力宣告与 `resumeSession` 实质实现，入参与本仓 `SessionResumeRequest` 逐字段兼容），**不升级为运行时观测**。此裁定一并记录理由：本变更的风险点不在上游 Agent 的能力宣告上（那属于上游 Agent 自身行为），且不应为流程收尾引入对真实 Agent 进程的运行期依赖。

## Main E2E

- mode: `not-applicable`（reason/basis/alternative_checks 见 plan.md 的 `### Main E2E`；`downgrade_approval` 已记录用户 2026-09-30 原话，见 plan.md）
- 替代检查 C1/C2 的记录见本文件 `## Checks`。

## Failures and Retests

| Issue ID / Task | 事实 | 处置与从中学到的规则 |
| --- | --- | --- |
| **WP6 的 coder-F 实例超时终止**（2026-10-01T07:54:19Z 台账 `fixing`，Attempt 2） | coder-F 开工后约 30 分钟被宿主空闲上限终止（与 WP3 同一失效模式）。**本次不是零损失**：工作区留下**未提交**改动 5 文件 **+935/−56**（`node_link/command.rs` +408、`node_link/command/tests.rs` +515、`local_admin/test_support.rs` +23、`node_link/resource/tests.rs` +7、`app/tests/support/owner.rs` 已改）；被杀时正在修`command/tests.rs` 的一个多余 `}`（现场日志 `reports/wp6-coder-workspace-clippy-keepegoing.log` 为中途状态：3 个 error，含 1 个 `unneeded return statement` + 编译中断）；**报告未产出**（`wp6-coder.md` 不存在） | 现场完整保留在 worktree，**未做任何 reset/checkout**。已按「未交付崩溃用 `--reopen --reason` 打回」登记 Attempt 2（coder-F2）。派发书明确要求：先评估并**保留**已有实现、先取准确的错误基线（不得把中途日志当事实）、必要时可重写但须说明理由；日志另起 `wp6-coder-f2-*.log`，不覆盖现场记录。**新增时间纪律**（本轮写入派发书）：体量大的包**不得长时间空等**，每个里程碑就往下推进；若一轮做不完，先提交已自洽的部分并在报告里列明剩余项，由 main 决定打回或接受分批交付 |
 | | **WP3 实例超时终止**（12:44:31 台账 `fixing`，Attempt 2） | coder-C 在 12:13 开工后被我要求「保持存活、等 Round 9/10/11 门禁」，**期间零落笔**；宿主 1800s（30 min）空闲上限到期将其终止 | 现场零损失：`wt/session-resume-wp3` 的 `git status` 为空、HEAD 仍 `b0a387b`、`wp3-coder.md` 未产生。已按「未交付崩溃可用 \`--reopen --reason\` 打回」登记 Attempt 2。<br>**从此固化的规则**：工作包在等门禁/等前序交付**时不得长时间空闲存活**——要么在门禁 PASS 后**立即重新派发一个新实例**（fresh context 成本远低于被超时杀掉重头再来），要么明确把它停在无需存活的等待点（等待期间不做任何事就应释放实例）。**代价**：coder-C 已做的零风险准备（6 个 \`SessionStore\` 实现点、\`port_error_code\` 位置、三处文档行号的勘查）随实例一起丢失，WP4/WP5/WP6 的新实例需重做这部分勘查。 |

| 挂起决策 | 决定 | 范围与后果 | 何时必须解 |
| --- | --- | --- | --- |
| **TP 存废**（A 保留+补登记偏离／B TP1 并入 TP2／C 删 TP1·TP2） | **2026-10-01 用户明示（方案 B：TP1 并入 TP2）**——原「暂时不考虑」同日转为明确选择 | **不再单独派发 TP1 的修复轮**：CR7 判 FAIL 的 2×MAJOR（规格 MUST 物理不可满足 / 同一输入两个互斥错误码）与 4×MINOR+2×SUGGESTION 的**修正**，连同「编写可执行用例」一并由 **TP2** 承担。**后果**：① TP2 的派发口径扩大为「先按 CR7 与 Round 8 后的规格修正设计、再据此写可执行用例」，不再消费 `reports/tp1-test-design.md` 的**未修正版**；② `tasks.md` 的 2.7/4.1（TP1 设计）**不单独勾选**，其完成判据改由 TP2 交付的**修正版设计**满足（勾选时在 tasks 行内注明「由 TP2 交付合并结案」）；③ 已交付的 `reports/tp1-test-design.md` 保留为历史证据，**不删除、不改写**。**待办**：本决定改变 `plan.md` 的 Waves/WP 组成（TP1 不再是独立开工包）⇒ 须与已挂起的 F62/F63/F64 合并到**下一次触碰 `plan.md` 的同一轮回写**，并重开一轮独立复核（当前 `sha256:1154ae4b…` 的 PASS 届时失效）。**该回写不阻塞 WP6 正在进行的工作** | 下次触碰 `plan.md` 时（与 F62/F63/F64 同批） |

| **7.1 validator 的未验证假设**（目标 Agent 是否真的宣告并实现 `sessionCapabilities.resume`） | **2026-09-30 用户明示：不阻塞当前实现，先挂着** | 只影响最终验收：若届时无可用的真实 Agent，validator 应记 **BLOCKED 而非 PASS**，需在 `## Independent Validation` 与交付说明中写明依据 | 进入 7.1 独立验证之前 |

| Issue ID / Task | Source / Check or E2E ID / Attempt / Version | Owner | Fix / Recovery | Review / Readiness Evidence | Retest Evidence | Current Status / Basis |
| --- | --- | --- | --- | --- | --- | --- |

## Final Assessment

<!-- 语义审计后写入唯一 agentic-assessment 块；apply 尚未进入实现阶段，留待 tasks 9.1。 -->


