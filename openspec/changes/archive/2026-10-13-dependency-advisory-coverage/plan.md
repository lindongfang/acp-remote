<!-- 执行安排：把 proposal 的 What Changes 与 design 的决策 1–4 转成可派发的并行工作包。
     plan 只写“谁在哪个隔离工作区做什么、按什么顺序合入、怎么验”，不重复 design 的内容。

     本仓库的执行原语是 Oh My Pi（omp），不是外部调度器：
     - 工作包由主 Agent 用 `task` 工具批量派发（batch 形状：共享 `context` + `tasks[]`）；
     - 每个工作包跑在隔离工作区（`task.isolation.enabled: true`、`merge: patch`、`apply: false`）；
     - 交付物是 `<id>.patch`（改动）+ `agent://<id>`（报告）+ `history://<id>`（过程），
       不是共享目录，也不是 branch；
     - 合入由主 Agent 串行执行（`git apply --check` → `git apply` → 主检出 Project Verify → commit）。

     本变更 `skip_specs: true`：没有 specs 增量，工作包的判据来自 proposal 的 What Changes 与 design 的决策 1–4。 -->

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
| WP1 | design 决策 4 + 决策 3：`scripts/check-dependency-advisories.mjs` 改为对仓库根与 `clients/app` 两个 npm 工程各跑一次 `npm audit`；`ALLOWED` 条目新增「适用工程」字段并按 (advisory ID, 工程) 匹配；登记 design 决策 1/2 判定的 4 条豁免（`braces` 两工程、`node-forge`/`uuid`/`decode-uri-component` 仅 `clients/app`），逐条带理由与到期条件；失败输出标明工程路径 | none | wp-coder | `scripts/check-dependency-advisories.mjs` | patch 改一个文件；报告给出新 `ALLOWED` 全文、工程列表、以及本地实跑的输出 | 主检出 PV2 + PV3 |
| WP2 | proposal「同步权威文档」：`README.md` 的「合同检查」说明该 job 现覆盖两个 npm 工程（并保留「本地无等价物」的既有声明）；`docs/adr/0008-ci-supply-chain-tooling.md` 残余风险/记录节登记范围扩展与 4 条豁免的**性质差异**（3 条不可达 vs 1 条真实可达但延后） | none | wp-coder | `README.md`, `docs/adr/0008-ci-supply-chain-tooling.md` | patch 改两个文件；报告给出两处改动的前后原文 | 主检出 PV4 |
| WP3 | proposal「同步权威文档」的 CI 部分：`.github/workflows/ci.yml` 的 `deps` job 更新那段注释，使其与新判定范围一致（**步骤命令不变**） | none | wp-coder | `.github/workflows/ci.yml` | patch 只改注释文本；报告给出 job 名集合与 `deps` 步骤的前后对比 | 主检出 PV5 |

## Merge Order

| Order | Work Package | Reason | Re-verify After Merge |
| --- | --- | --- | --- |
| 1 | WP1 | 写范围无重叠（独立文件） | 无 |
| 2 | WP2 | 写范围无重叠（`README.md` / `docs/adr/`，与 WP1 不相交） | 无 |
| 3 | WP3 | 写范围无重叠（`.github/`） | 无 |

三个工作包的写范围两两不相交，不存在「后合入者上下文失配」；合入后统一跑一次全量 Project Verify
（PV1–PV5）即可，无需逐包重跑。

**同批合入是硬要求**（design 的 Migration Plan 第 1 条）：只合 WP1 会让 `deps` 立刻红（4 条未登记），
只合 WP2/WP3 则无任何效果。三者必须在同一批一起落地。

## Project Verify

<!-- 只在主检出执行。隔离工作区跑不了这些命令（没有 target/ 与 node_modules/）。
     实测确认：`npm audit` 只读锁文件、不需要 `node_modules`；本机可执行该脚本（本会话已实跑通过）。 -->

| Check ID | Command / Working Directory | Scope / Pass Criteria |
| --- | --- | --- |
| PV1 | `npm run check`（仓库根） | 十道合同门禁全绿；含 `check:openspec` 的 `validate --all --strict` 校验本变更资产的合法性 |
| PV2 | `node scripts/check-dependency-advisories.mjs`（仓库根） | 退出码 0；输出显示**两个**工程都被审计；逐条打印 4 条豁免的理由与到期条件；无未登记项 |
| PV3 | 仓库根：`node -e "const s=require('fs').readFileSync('scripts/check-dependency-advisories.mjs','utf8');for(const p of ['clients/app'])if(!s.includes(p))throw new Error('工程列表缺 '+p);console.log('PV3 OK')"` | 退出码 0 且打印 `PV3 OK`：脚本的工程列表含 `clients/app`（覆盖范围确实扩了，而不是只改了文档） |
| PV4 | 仓库根：`npm run check` 的 `check:doc-links`（已含在 PV1）+ `node -e "const s=require('fs').readFileSync('README.md','utf8');if(!/两个/.test(s)&&!/clients\/app/.test(s))throw new Error('README 未更新');console.log('PV4 OK')"` | 退出码 0 且打印 `PV4 OK`：README 的 `deps` 描述已覆盖两个工程；ADR 的登记存在 |
| PV5 | 仓库根：`node -e "const y=require('yaml'),f=require('fs');const w=y.parse(f.readFileSync('.github/workflows/ci.yml','utf8'));const names=Object.keys(w.jobs).sort().join(',');const want='advisories,checks,commits,deps,frontend,secrets';if(names!==want)throw new Error('job 集合变了: '+names);const st=w.jobs.deps.steps.find(s=>s.run&&s.run.includes('check-dependency-advisories'));if(!st)throw new Error('deps 步骤命令被改动');console.log('PV5 OK')"` | 退出码 0 且打印 `PV5 OK`：job 名集合仍为六个；`deps` 仍以 `node scripts/check-dependency-advisories.mjs` 为判定命令（**只改注释，不改命令**） |

PV5 用内联命令而不是接入 `npm run check`：`npm run check` 的十道脚本都不读 `.github/**`
（与 precedent `dependabot-sdk-alignment` 的 PV4 同理），该文件的语义由 GitHub 侧执行，
本地只做「job 集合与判定命令未被误改」的结构核对。本变更**不新增**校验脚本文件，
也不改变 `package.json` 的 `check` 链。

## Code Review

| Work Package | Reviewer Agent | Review Focus | Pass Condition |
| --- | --- | --- | --- |
| WP1 | reviewer | (ID, 工程) 匹配是否真的生效（构造「A 工程登记、B 工程同号未登记」的情形应失败）；`braces` 是否覆盖两个工程；4 条豁免的理由是否与 design 决策 1 的判定一致、`decode-uri-component` 是否写明「真实可达、延后」而**不是**写成「不可达」；`critical` 一律失败是否保留；失败输出是否标明工程路径；是否夹带无关重构 | 无 CRITICAL/MAJOR 未闭环项 |
| WP2 | reviewer | 是否只改文档；README 是否仍声明该项「本地无等价物」（该事实未变）；ADR 是否区分了「不可达豁免」与「真实可达但延后」两类；是否误改了其它章节 | 无 CRITICAL/MAJOR 未闭环项 |
| WP3 | reviewer | 是否只改注释文本；job 名集合、`steps` 的 `run` 命令、`working-directory`、缓存键是否均未变动 | 无 CRITICAL/MAJOR 未闭环项 |

## Main E2E

```yaml
mode: not-applicable
reason: "本变更不改动任何客户端可观察行为：它只扩大一个 CI 判定脚本的审计范围，并同步文档与 CI 注释。不触及页面、状态机、协议载荷、平台适配或任何运行期产物。"
basis: "proposal 的 Capabilities 声明 skip_specs；design 的 Non-Goals 明确不改依赖版本、不改 job 集合、不改任何 crate/协议/存储。变更面只有 `scripts/` 下一个非运行时脚本与两份文档、一处 CI 注释。"
alternative_checks:
  - "PV2：实跑改造后的脚本，证明两个工程都被审计且 4 条豁免命中（这是本变更唯一的行为面）"
  - "PV3：证明工程列表确实含 clients/app（覆盖范围真的扩了）"
  - "PV5：证明 CI 的 job 集合与判定命令未被误改"
  - "PV1：证明变更资产与既有合同门禁仍全绿"
```

## Risks

| 风险 | 缓解 |
| --- | --- |
| 隔离工作区无 `node_modules`，WP1 无法在隔离区实跑脚本 | 兜底：脚本只读**锁文件**（实测确认不需要 `node_modules`），若隔离区仍缺依赖则由主 Agent 在主检出执行 PV2 作为该包的行为判据 |
| 本机到 npm registry 的网络不稳定（`npm audit` 需联网取 advisory 数据） | PV2 的失败若为网络错误而非判定失败，按「未验证」记录并说明；CI 的 `deps` job 是最终判据（与既有的依赖/公告/密钥三项同理） |
| 扩范围后 `deps` 在 CI 上仍红（实测与 CI 的 advisory 数据面存在差异） | 按 CI 的实际输出修正登记项；本变更的登记基于本机实测的根因 advisory 集合，diff 应可解释 |
| 隔离区基线是整个工作目录的副本，若主检出有半成品改动会被带入 | 派发前确认主检出工作区干净（本会话已确认：`git status` 空） |
| `decode-uri-component` 的豁免长期挂着被遗忘 | 豁免条目带 `expires`（SDK 58 进 `latest` 时升级并删除）+ WP2 在 README 与 ADR 各留一处指回 |
