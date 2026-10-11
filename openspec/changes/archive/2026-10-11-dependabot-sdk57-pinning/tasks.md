# Tasks

## 1. `/clients/app` 条目的 ignore 规则与注释（WP1）

- [x] 1.1 在 `.github/dependabot.yml` 的 `/clients/app` 条目 `ignore` 列表内新增四条 expo 家族规则，`dependency-name` 分别为 `expo`、`expo-router`、`expo-constants`、`expo-status-bar`，四条的 `update-types` 均为 `["version-update:semver-major"]`（判据：主检出 PV1 通过并打印 `PV1 OK`，其中 expo 部分为逐条取值比对，不是只检查键存在）
- [x] 1.2 同一列表再新增两条规则：`react` 与 `react-dom`，`update-types` 均为 `["version-update:semver-minor"]`（判据：PV1 通过，且其对 `/clients/app` 条目的 `ignore` 条目数断言为 7）
- [x] 1.3 重写该条目里关于 expo 家族的注释段：删除以「`react` / `expo` / `expo-router` 刻意不挡」开头、含「实测它们的升级不影响判定面」的整段，改为 design.md D4 要求的三点——(a) expo 家族 major 号即 SDK 号、跨 SDK 必须整族同批升（并给出 `expo-router@58` 需要整套 SDK 58 对等依赖这一事实）；(b) 范围号挡不住 Dependabot，附 `b34ad17` 的实测先例；(c) 新增 expo 家族**直接依赖**时必须同步本清单（判据：`git diff .github/dependabot.yml` 中旧段落整体消失且新段落包含上述三点；该注释是本次唯一的文档面，与规则在同一任务组内落地）
- [x] 1.4 确认既有正文逐字未变：`react-native` 那条规则仍是 `update-types: ["version-update:semver-minor"]`，根 `cargo` / 根 `npm` / `github-actions` 三个 `updates` 条目与 `/clients/app` 条目的 `groups`、`schedule`、`cooldown`、`open-pull-requests-limit`、`commit-message` 均无改动（判据：PV1 的条目集合断言通过，且 `git status --porcelain` 只列出 `.github/dependabot.yml` 一个文件）

## 2. 合入与整体检查

- [x] 2.1 派发独立 spawn 检视 WP1 的 patch（检视者不得是作者那次 spawn），输入为 patch 绝对路径 + `agent://<id>` + `history://<id>`，检视焦点按 plan.md 的 `## Code Review`（判据：verdict 为 `pass`，且无未闭环的 CRITICAL/MAJOR；`changes-requested` 时按 plan.md 的修复流程重派，最多 3 轮）
- [x] 2.2 在主检出按 `## Merge Order` 串行合入（`git apply --check` → `git apply`），随后执行 PV1 与 PV2（判据：PV1 打印 `PV1 OK` 且退出码 0；PV2 的 `git diff --name-only` 恰好输出一行 `.github/dependabot.yml`）
- [x] 2.3 在主检出执行 PV3 `npm run check`（判据：十道脚本全绿。该管线不读 `.github/**`，因此它证明的是本变更 artifacts 通过 `check:openspec`、文档引用未被破坏，不作为配置正确性的证据）

## Workflow follow-up

- 归档本变更，确认归档后的主规范集合不变（本变更 `.openspec.yaml` 带 `skip_specs: true`）。
- 关闭 PR #76（其内容「单独把 `expo-router` 57→58」与「统一维持在 SDK 57」相反，理由可照抄 #58 的关闭评论）。
- 关闭 PR #74（按 design.md D3 的选择，`react` 19.3.0 不采纳，范围维持在 SDK 57 期望的 19.2.3）。
- 合入后的下一次 Dependabot 运行，核对未再生成 expo 家族 major 提议与 `react` / `react-dom` minor 提议（proposal 成功判据 1，属仓库外观测）。
