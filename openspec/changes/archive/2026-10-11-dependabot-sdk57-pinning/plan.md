<!-- 执行安排：把 proposal 的改动范围与 design 的技术方案转成可派发的并行工作包。
     plan 只写“谁在哪个隔离工作区做什么、按什么顺序合入、怎么验”，不重复 proposal/design 的内容。

     本仓库的执行原语是 Oh My Pi（omp），不是外部调度器：
     - 工作包由主 Agent 用 `task` 工具批量派发（batch 形状：共享 `context` + `tasks[]`）；
     - 每个工作包跑在隔离工作区（`task.isolation.enabled: true`、`merge: patch`、`apply: false`）；
     - 交付物是 `<id>.patch`（改动）+ `agent://<id>`（报告）+ `history://<id>`（过程），
       不是共享目录，也不是 branch；
     - 合入由主 Agent 串行执行（`git apply --check` → `git apply` → 主检出 Project Verify → commit）。 -->

# Plan

## Execution Model

- 派发：主 Agent 用 `task` 工具的 batch 形状扇出工作包；每个工作包是**一次 spawn**。
- 隔离：`task.isolation`（`patch` 模式、`apply: false`）。工作包之间不共享目录。
- 交付：`<id>.patch` + `agent://<id>` + `history://<id>`；工作包不产生 commit、不产生分支。
- 合入：主 Agent 按 `## Merge Order` 串行 `git apply`，每次合入后跑主检出 Project Verify。
- 验证：隔离工作区内**没有** `node_modules/`，因此依赖 `yaml` 包的断言与仓库级检查只在主检出执行。
- 检视：每个工作包由一个**独立** spawn（不与作者同实例）读 patch 与 `agent://<id>` 后给出结构化结论。

## Work Packages

| ID | Goal / Scenarios | Dependencies | Agent | Write Scope | Deliverable | Verify |
| --- | --- | --- | --- | --- | --- | --- |
| WP1 | proposal.md「What Changes」一~三；design.md D1–D5：给 `/clients/app` 条目加 6 条 `ignore` 规则并重写该类注释 | none | wp-coder | `.github/dependabot.yml` | patch 改一个文件：`ignore` 从 1 条增至 7 条，注释段按 D4 重写；报告给出新增块原文与替换后的注释原文 | 主检出 PV1、PV2 |

工作包只有一个：本变更的全部写操作都落在同一文件上，拆成多个包只会制造写范围重叠。

## Merge Order

| Order | Work Package | Reason | Re-verify After Merge |
| --- | --- | --- | --- |
| 1 | WP1 | 无重叠（唯一工作包） | PV1、PV2 |

## Project Verify

| Check ID | Command / Working Directory | Scope / Pass Criteria |
| --- | --- | --- |
| PV1 | 仓库根：`node -e "const y=require('yaml'),fs=require('fs');const d=y.parse(fs.readFileSync('.github/dependabot.yml','utf8'));const keys=d.updates.map(u=>u['package-ecosystem']+' '+u.directory).sort().join('|');if(keys!=='cargo /|github-actions /|npm /|npm /clients/app')throw new Error('updates 条目集合变了: '+keys);const e=d.updates.find(u=>u.directory==='/clients/app');const want={'react-native':['version-update:semver-minor'],'expo':['version-update:semver-major'],'expo-router':['version-update:semver-major'],'expo-constants':['version-update:semver-major'],'expo-status-bar':['version-update:semver-major'],'react':['version-update:semver-minor'],'react-dom':['version-update:semver-minor']};const names=e.ignore.map(i=>i['dependency-name']);if(names.length!==Object.keys(want).length)throw new Error('ignore 条目数异常: '+names.join(','));for(const [n,t] of Object.entries(want)){const r=e.ignore.find(i=>i['dependency-name']===n);if(!r)throw new Error('缺少 ignore 规则: '+n);if(JSON.stringify(r['update-types'])!==JSON.stringify(t))throw new Error(n+' 取值错误: '+JSON.stringify(r['update-types']))}console.log('PV1 OK')"` | 退出码 0 且打印 `PV1 OK`：`updates` 条目集合未变；`/clients/app` 的 `ignore` 恰为 7 条，四个 expo 家族包为 `version-update:semver-major`、`react`/`react-dom` 与既有的 `react-native` 为 `version-update:semver-minor`。逐条比对取值，而非只检查键存在（该文件注释已警告取值写错会静默失效） |
| PV2 | 仓库根：`git diff --name-only` | 输出恰好一行 `.github/dependabot.yml`（proposal 的硬约束「只改一个文件」）。`.github/dependabot.yml` 中既有 `react-native` 规则与其余两个 `updates` 条目的正文应从 diff 中消失的**只有被替换的注释段** |
| PV3 | 仓库根：`npm run check` | 十道脚本全绿。**注意判据范围**：该管线不读 `.github/**`，所以它证明的是「本变更的 artifacts 通过 `check:openspec`、文档引用未被破坏」，**不是**本次配置改动的正确性证据（那是 PV1） |

## Code Review

| Work Package | Reviewer Agent | Review Focus | Pass Condition |
| --- | --- | --- | --- |
| WP1 | reviewer | 取值正确性（`dependency-name` 拼写、`update-types` 取值域）／是否误伤既有 `react-native` 规则与其余条目／注释是否只包含可核对事实且未删除 D4 要求的三点／是否引入了 proposal「本次不包含」里的东西 | 无未闭环的 CRITICAL/MAJOR |

检视输入必须是 patch 的**绝对路径** + `agent://<id>` + `history://<id>`；内置 reviewer 默认读 `git diff` / `gh pr diff`，而 patch 在会话产物目录里，不在仓库中。

## Main E2E

```yaml
mode: not-applicable
reason: "本变更不产生任何可运行的用户可见面：它只改变 Dependabot 对 /clients/app 生成哪些依赖升级提议，没有任何可启动的入口或界面。"
basis: "全部写操作落在 .github/dependabot.yml；该文件不被任何构建、运行时或现有门禁读取（npm run check 的十道脚本均不读 .github/**，上一变更的 Context 已记录该事实）。"
alternative_checks: ["PV1：YAML 解析断言（规则存在、取值逐字正确、既有规则与其余条目未变）", "PV2：改动面断言（只含一个文件）", "PV3：npm run check 全绿（证明本变更 artifacts 与文档引用未破坏）", "合入后下一次 Dependabot 运行未再生成 expo 家族 major 提议与 react/react-dom minor 提议（proposal 成功判据 1，属仓库外观测）"]
```

## Risks

- [隔离工作区无法执行 PV1] → 断言依赖根 `node_modules` 里的 `yaml` 包，而隔离工作区没有 `node_modules/`。因此 PV1 只写在 Project Verify（主检出），不写进 WP 的隔离内 Verify。
- [`yaml` 是传递依赖，不在根 `package.json` 的 devDependencies 中] → 它当前可被 `require` 解析（上一变更的 PV3 已按同一方式通过）。若将来它不再可解析，断言降级为对 `.github/dependabot.yml` 的文本级比对（逐行匹配 `dependency-name` 与 `update-types`），判定内容不变。
- [子 Agent 顺手重排 YAML 键序或删注释] → 检视焦点包含「除被替换的注释段外，其余正文不变」；PV2 要求 diff 中不出现既有规则的行。
- [规则已写对但 Dependabot 仍提议] → 属上游运行时行为，本地无法验证；由合入后的观测项覆盖（Main E2E 的第 4 条替代检查）。
- [Dependabot 锁文件生成器缺陷导致的同类红灯仍可能发生] → 与本策略无关（design.md 的 Risks 已记录），本变更不处理，也不应因此回到 D2 的通配写法。
