# Tasks

## 1. Dependabot 分组修正（WP1）

- [x] 1.1 在 `.github/dependabot.yml` 的 `/clients/app` 条目里、`groups:` **之前**新增 `ignore` 块，含单条规则：`dependency-name: "react-native"` + `update-types: ["version-update:semver-minor"]`，并附注释说明「RN 的 minor 对应 Expo SDK 的 major，升级必须与 SDK 同批」（判据：`node -e` 解析该文件后，`updates` 中 `directory === '/clients/app'` 的条目存在 `ignore` 且含该规则；PV3 从 exit 1 变为 exit 0）
- [x] 1.2 确认 `update-types` 用的是 `version-update:semver-minor` 而不是 `minor`（判据：PV3 的字符串比对通过——`ignore.update-types` 与 `groups.update-types` 取值域不同，写成 `minor` 会静默失效）
- [x] 1.3 确认其余四个 `updates` 条目（根 npm、cargo、github-actions 及 `/clients/app` 的其它键）逐字未变（判据：`git diff` 中 `.github/dependabot.yml` 只有新增块，无删除或修改行）
- [x] 1.4 由**独立 spawn** 的 reviewer 检视 WP1 的 patch，聚焦 `ignore` 作用域与 `update-types` 取值（判据：verdict 为 pass，或 findings 中无 CRITICAL/MAJOR 未闭环项；检视者不是作者那次 spawn）

## 2. CI 注释归因修正（WP2）

- [x] 2.1 重写 `.github/workflows/ci.yml` 中 `frontend` job 的说明注释：把两个症状写成「(a) 锁文件缺嵌套依赖（`react-native-worklets@0.10.4`）是 **#68 与 #69 共有**的 Dependabot 锁生成器缺陷；(b) `ERR_PACKAGE_PATH_NOT_EXPORTED` 才是 #68（RN 0.87.1）独有」，并写明 `npm ci` 先失败遮蔽了 (b)。**不得**写成「(a) 只出现在 #68 上」，也**不得**写成「#69 本来就带着该嵌套条目」（#69 的原始 head `7c8a632` 同样缺它）（判据：`git diff` 显示改动只在注释行内，不触碰 `steps`/`jobs` 键）
- [x] 2.2 确认 job 集合与 `frontend` 的关键 steps 未变（判据：PV4 exit 0，即 job 名集合仍为 `advisories,checks,commits,deps,frontend,secrets`，且 `frontend` 的 steps 仍含 `clients/app` 与 `npm ci`）
- [x] 2.3 确认归因与实测一致：锁文件缺陷**#68 与 #69 都命中**——#68 头部 `1ba6d08` 与 #69 原始头部 `7c8a632` 的锁文件里 `node_modules/expo/node_modules/react-native-worklets` 均为 0 条，而 #69 补齐后的 `02466b9a` 为 1 条；`gh pr diff 68` 中 `worklets` 出现 2 次（均为删除行）、`gh pr diff 69` 中为 0 次（两侧相同故不进 diff）（判据：用 `gh pr diff 68`、`gh pr diff 69` 与 `gh api .../contents/clients/app/package-lock.json?ref=<上述三个 sha>` 复核，结论写入 WP2 报告）
- [x] 2.4 由**独立 spawn** 的 reviewer 检视 WP2 的 patch，聚焦「只改注释、未动语义」（判据：verdict 为 pass，或 findings 中无 CRITICAL/MAJOR 未闭环项）

## 3. clients/app 依赖升级收窄（WP3）

- [x] 3.1 把 `clients/app/package.json` 的 `vite` 改为 `^8.3.2`、`vitest` 改为 `^5.0.3`；`react`/`react-dom`/`react-native`/`@types/react` 保持 `main` 原值（`19.2.3`/`19.2.3`/`0.86.3`/`~19.2.4`）（判据：`git diff clients/app/package.json` 只有这两行变化）
- [x] 3.2 以 `main` 版的 `clients/app/package-lock.json` 为基线重生成锁文件（`cd clients/app && npm install --package-lock-only`），**不是**在 PR #68 的锁文件上打补丁（判据：锁文件中 `node_modules/expo/node_modules/react-native-worklets` 条目存在，版本 `0.10.4`）
- [x] 3.3 记录重生成后的实际解析版本，确认 `vite` 落在 `^8.3.2` 内、`vitest` 落在 `^5.0.3` 内，并确认 `react-native` 仍是 `0.86.3`（判据：把四个包的解析版本写入 WP3 报告；`vite` 解析到 `8.3.4` 属正常漂移，不是缺陷）
- [x] 3.4 确认锁文件未夹带与本次目标无关的包变更（判据：把 `main` 与新版锁文件的 `packages` 键集合做差集，新增/删除的条目能逐条解释为 `vite`/`vitest` 依赖树的连带变化）
- [x] 3.5 在主检出跑 `cd clients/app && npm ci && npm run check`（判据：PV2 全绿——`npm ci` exit 0、typecheck 通过、458 项测试全过、`expo export --platform web` 产出 `dist`）
- [x] 3.6 由**独立 spawn** 的 reviewer 检视 WP3 的 patch，聚焦 `package.json` 的两行改动、RN 三件套是否与 `main` 逐字一致、worklets 嵌套条目是否存在、是否夹带无关漂移（判据：verdict 为 pass，或 findings 中无 CRITICAL/MAJOR 未闭环项）

## 4. 合入与门禁

- [x] 4.1 主 Agent 按 plan 的 Merge Order 串行 `git apply --check` → `git apply` 三个工作包（WP1 → WP2 → WP3）（判据：三次 `--check` 均通过，主检出 `git status` 只显示预期的三个文件改动）
- [x] 4.2 在主检出跑全量 Project Verify：PV1（`npm run check`）、PV2（`clients/app` 的 `npm ci` + `npm run check`）、PV3、PV4（判据：四项全部 exit 0）
- [ ] 4.3 按 `commitlint.config.mjs` 的词表手写提交信息（scope 用 `deps` 或 `ci`；**不要**跑 `omp commit`，它按目录名推 scope 会被钩子拒绝且会改动 git 索引）（判据：本地 `.husky/commit-msg` 钩子通过，提交落地）
- [ ] 4.4 为这批改动开分支、推送、`gh pr create --fill`，等待六个必需检查全绿后 `gh pr merge --squash`（判据：`gh pr checks --watch` 显示六个检查全部 pass，PR 标题符合 Conventional Commits）
- [ ] 4.5 合入后关闭 PR #68，并观察 Dependabot 下一次运行：若它新开的 PR 只含 `vite`/`vitest` 则直接关闭（与本变更重复），若仍提 `react-native` 则说明 `ignore` 未生效，需复核配置（判据：在 PR 描述或 issue 中记录观察结果；这是 design「Open Questions」的收口）
