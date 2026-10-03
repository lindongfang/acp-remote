# U1 合入报告（对应 plan.md 的 Local Merge Conditions / Report Path）

- 交付单元：U1（integrated）；目标：`refs/heads/main`，合入前 `6c093f145aa3d69dcd573a6d94e31b692acb5d4b`
- 合入后：`a6f6210bcb8c1aacd81a09bf5169cb5eb5e287c7`（**fast-forward**，候选即其祖先，`git merge-base --is-ancestor` 已核实）
- 合入方式：主 Agent 在本地主分支执行 `git merge --ff-only a6f6210…`；合入前 `git status --porcelain` 仅有未跟踪的 `openspec/changes/sync-workspaces-and-create/`（无被跟踪文件改动），无竞态
- 合入前门禁：`workflow check --stage premerge` = **PASS**（receipt 见 `verification.md` 的 `agentic-premerge` 块）；候选 `npm run verify` 退出码 0（`reports/candidate-verify-round2.log`，sha256:c328abf0…，副本 `reports/PV1.log`）
- 独立候选检视：`reports/candidate-review.md`（sha256:43bb3eef…）判 **PASS**，无 BLOCKER / MAJOR
- 合入后主分支回归：`npm run check` 全绿（`reports/main-alt1-check.log`）；`cargo test --locked --workspace --all-features` 全绿（`reports/main-workspace-test.log`）
- **未 push、未 tag、未归档**：远端交付按 `AGENTS.md` §8 走 PR 路径，属另一步，需单独授权
