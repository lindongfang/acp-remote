<!-- 执行安排：把 proposal 的 What Changes 与 design 的 D1–D5 转成可派发的并行工作包。
     plan 只写“谁在哪个隔离工作区做什么、按什么顺序合入、怎么验”，不重复 design 的内容。

     本仓库的执行原语是 Oh My Pi（omp），不是外部调度器：
     - 工作包由主 Agent 用 `task` 工具批量派发（batch 形状：共享 `context` + `tasks[]`）；
     - 每个工作包跑在隔离工作区（`task.isolation.enabled: true`、`merge: patch`、`apply: false`）；
     - 交付物是 `<id>.patch`（改动）+ `agent://<id>`（报告）+ `history://<id>`（过程），
       不是共享目录，也不是 branch；
     - 合入由主 Agent 串行执行（`git apply --check` → `git apply` → 主检出 Project Verify → commit）。

     本变更 `skip_specs: true`：没有 specs 增量，工作包的判据来自 proposal 的 What Changes 与 design 的 D1–D5。 -->

## Execution Model

- 派发：主 Agent 用 `task` 工具的 batch 形状一次性扇出多个工作包；每个工作包是**一次 spawn**。
- 隔离：`task.isolation`（`patch` 模式、`apply: false`）。工作包之间不共享目录。
- 交付：`<id>.patch` + `agent://<id>` + `history://<id>`；工作包不产生 commit、不产生分支。
- 合入：主 Agent 按 `## Merge Order` 串行 `git apply`，每次合入后跑主检出 Project Verify。
- 验证：隔离工作区内**没有** `target/` 与 `node_modules/`，因此仓库级检查只在主检出执行。
- 检视：每个工作包由一个**独立** spawn（不与作者同实例）读 patch 与 `agent://<id>` 后给出结构化结论。

## Work Packages

| ID | Goal / Scenarios | Dependencies | Agent | Write Scope | Deliverable | Verify |
| --- | --- | --- | --- | --- | --- | --- |
| WP1 | design D2：`.github/dependabot.yml` 的 `/clients/app` 条目新增 `ignore`，只对 `react-native` 关闭 minor 版本更新 | none | wp-coder | `.github/dependabot.yml` | patch 改一个文件；报告给出新增块原文与插入位置 | 主检出 PV3（断言该条目存在且 YAML 可解析） |
| WP2 | design D4：按实测重写 `.github/workflows/ci.yml` 的失败归因（锁文件缺陷 → #68 与 #69 **共有**；`ERR_PACKAGE_PATH_NOT_EXPORTED` → #68 **独有**） | none | wp-coder | `.github/workflows/ci.yml` | patch 只改注释文本；报告给出 job 名集合的前后对比 | 主检出 PV4（断言 YAML 可解析且 job 集合与关键 steps 未变） |
| WP3 | proposal 一 / design D1+D3：`clients/app` 只升 `vite`(`^8.3.2`) 与 `vitest`(`^5.0.3`)；RN 三件套与 `@types/react` 保持 `main` 原值；锁文件以 `main` 版为基线重生成 | none | wp-coder | `clients/app/package.json`, `clients/app/package-lock.json` | patch 改两个文件；报告列出 `vite`/`vitest` 解析版本与 worklets 嵌套条目是否仍在 | 主检出 PV2（`npm ci` + `npm run check`） |

## Merge Order

| Order | Work Package | Reason | Re-verify After Merge |
| --- | --- | --- | --- |
| 1 | WP1 | 写范围无重叠（独立文件） | 无 |
| 2 | WP2 | 写范围无重叠（独立文件） | 无 |
| 3 | WP3 | 写范围无重叠（`clients/app` 前缀） | 无 |

三个工作包的写范围两两不相交，不存在「后合入者上下文失配」；合入后统一跑一次全量 Project Verify
（PV1–PV4）即可，无需逐包重跑。

## Project Verify

<!-- 只在主检出执行。全部用现有工具，不新增脚本文件：
     仓库根已有 `yaml` 包（`@fission-ai/openspec` 的传递依赖），可直接解析两份 YAML 做断言。 -->

| Check ID | Command / Working Directory | Scope / Pass Criteria |
| --- | --- | --- |
| PV1 | `npm run check`（仓库根） | 十道合同门禁全绿；含 `check:openspec` 的 `--strict` 校验 |
| PV2 | `npm ci && npm run check`（`clients/app`） | `npm ci` exit 0（锁文件与 `package.json` 同步）；typecheck + vitest 全过；`expo export --platform web` 成功产出 `dist` |
| PV3 | 仓库根：`node -e "const y=require('yaml'),f=require('fs');const d=y.parse(f.readFileSync('.github/dependabot.yml','utf8'));const e=d.updates.find(u=>u.directory==='/clients/app');const ig=e.ignore.find(i=>i['dependency-name']==='react-native');if(!ig)throw new Error('react-native ignore 缺失');if(JSON.stringify(ig['update-types'])!=='[\"version-update:semver-minor\"]')throw new Error('update-types 取值错误: '+JSON.stringify(ig['update-types']));console.log('PV3 OK')"` | 退出码 0 且打印 `PV3 OK`：`/clients/app` 条目的 `ignore` 含 `react-native` + `update-types: ["version-update:semver-minor"]` |
| PV4 | 仓库根：`node -e "const y=require('yaml'),f=require('fs');const w=y.parse(f.readFileSync('.github/workflows/ci.yml','utf8'));const names=Object.keys(w.jobs).sort().join(',');const want='advisories,checks,commits,deps,frontend,secrets';if(names!==want)throw new Error('job 集合变了: '+names);const st=JSON.stringify(w.jobs.frontend.steps);if(!st.includes('clients/app'))throw new Error('frontend steps 异常');console.log('PV4 OK')"` | 退出码 0 且打印 `PV4 OK`：job 名集合仍为六个；`frontend` 的 steps 仍指向 `clients/app` |

PV3/PV4 之所以用内联命令而不是接入 `npm run check`：`npm run check` 的十道脚本都不读 `.github/**`
（design D5），这两个文件的语义由 GitHub 侧执行，本地只做「结构未被误改」的核对。本变更**不新增**
校验脚本文件，也不改变 `package.json` 的 `check` 链。

## Code Review

| Work Package | Reviewer Agent | Review Focus | Pass Condition |
| --- | --- | --- | --- |
| WP1 | reviewer | `ignore` 是否只作用于 `react-native`；`update-types` 取值是否为 `version-update:semver-minor`（**不是** `minor`——`groups.update-types` 用 `minor`，`ignore.update-types` 用 `version-update:semver-minor`，写错会静默失效）；其余四个条目是否未被误改 | 无 CRITICAL/MAJOR 未闭环项 |
| WP2 | reviewer | 是否只改注释文本；job 名、`steps`、`working-directory`、缓存键是否均未变动；新注释的归因是否与实测一致（#68 头部 `1ba6d08` 与 #69 原始头部 `7c8a632` 的锁文件**都缺**该嵌套条目；`gh pr diff 68` 中 `worklets` 出现 2 次且均为删除行，`gh pr diff 69` 中为 0 次） | 无 CRITICAL/MAJOR 未闭环项 |
| WP3 | reviewer | `package.json` 是否只动 `vite`/`vitest` 两行；RN 三件套与 `@types/react` 是否与 `main` 原值逐字一致；锁文件中 `node_modules/expo/node_modules/react-native-worklets` 是否存在；是否夹带无关漂移 | 无 CRITICAL/MAJOR 未闭环项 |

## Main E2E

```yaml
mode: not-applicable
reason: "本变更不改动任何客户端可观察行为：只调整依赖版本（vite/vitest，均为构建与测试工具）与两份 CI 配置文件的文本，不触及页面、状态机、协议载荷或平台适配。"
basis: "proposal 的 Capabilities 声明 skip_specs；design 的 Non-Goals 明确不改变判定面与 job 集合。PV2 已覆盖唯一可能受影响的运行期产物——web bundle 的生成。"
alternative_checks:
  - "PV2 的 expo export：证明 web bundle 可生成（本变更唯一可能影响到的运行期产物）"
  - "PV1 的 check:openspec：证明变更资产本身合法"
  - "PV3/PV4：证明两份 CI 配置的语义未被误改"
```

## Risks

| 风险 | 缓解 |
| --- | --- |
| 隔离工作区可能无法访问 npm registry，WP3 生成不出锁文件 | 兜底：由主 Agent 在主检出执行 `cd clients/app && npm install --package-lock-only`，把产物纳入合入范围；该产物必须经 PV2 断言 |
| 锁文件重生成可能带来 `^`/`~` 范围内的额外漂移 | PV2 的 `npm ci` + `npm run check` 是硬判据；PR 描述里列出解析到的实际版本供复核 |
| WP3 的 patch 可能很大（锁文件全文重排） | 检视聚焦 `package.json` 的两行改动与锁文件中 worklets 嵌套条目的存在性，不逐行审锁文件 |
| `ignore` 的实际生效无法本地验证 | 语义有官方文档依据（`update-types` 只影响版本更新）；合入后观察下一次 Dependabot 运行，必要时用 `@dependabot unignore` 回退 |
