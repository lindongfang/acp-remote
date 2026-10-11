# 变更执行工作流

> 状态：本文件是本仓库**变更如何从需求走到代码**的唯一权威来源（谁做、在哪个工作区做、产物是什么、怎么合入、怎么验）。
> 产品行为、协议语义、安全模型、模块边界仍以 `AGENTS.md` §2 列出的权威文档为准；本文件不重复它们。

## 1. 五段流程一览

| 段 | 载体 | 产物 | 权威/判据 |
|---|---|---|---|
| 1 需求规划 | OpenSpec schema `spec-driven`（项目覆写版） | `proposal.md`、`specs/**/spec.md`、`design.md` | 本文件 §2 |
| 2 执行规划 | 同上 | `plan.md`（工作包、写范围、合入顺序、验证策略） | 本文件 §3 |
| 3 任务分解 | 同上 | `tasks.md`（可勾选任务） | 本文件 §4 |
| 4 实现与检视 | Oh My Pi（omp）`task` 工具 | 每个工作包一份 `<id>.patch` + `agent://<id>` + 检视结论 | 本文件 §4–§6 |
| 5 合入与 E2E | git + omp `browser` / `computer` | 主检出串行合入 → 分支 → PR → squash 合并；E2E 观察证据 | 本文件 §6–§7 |

流程入口是斜杠命令（`.omp/commands/opsx-*.md`，与 `.agents/skills/openspec-*/` 同源）：
`/opsx-propose` → `/opsx-apply` → `/opsx-archive`，另有 `/opsx-update`、`/opsx-sync`、`/opsx-explore`。
这些是**引擎安装的受管文件**，由 `openspec update` 维护，不手工编辑。

## 2. 需求规划层：schema `spec-driven`（项目覆写版）

`openspec/config.yaml` 的 `schema: spec-driven` 解析到**仓库内覆写版**（`openspec/schemas/spec-driven/`），
它按引擎的 schema 解析优先级（项目本地 → 用户覆盖 → 包内置）遮住同名的内置版。
项目覆写版比内置版多一个 `plan` artifact，
并把 apply 门禁改为 `requires: [plan, tasks]`——**计划缺失时 `openspec status` 与 `apply` 会直接
报 blocked**，这是 CLI 级判据，不靠人记得先写计划。

```text
proposal ─┬─> specs ─┐
          └─> design ─┴─> plan ──> tasks ──> apply
```

字段级约束写在 `openspec/config.yaml` 的 `rules:`（按 artifact 注入 `openspec instructions <id> --json`
的 `rules` 字段）与 `operations.apply.guidance`（每轮 apply 的 `operationGuidance`）。两处都只经由引擎下发，
不复制进任何 artifact 文件。

三个必须知道的引擎行为（都实测过）：

1. `rules` / `operations.guidance` 的条目必须是**纯字符串标量**。裸写含 `": "` 的句子会被 YAML 解析成映射，
   引擎只打印一行 warning 并**静默丢弃整条规则**。`check:openspec` 会拦下这种写法。
2. `openspec schema fork` **不会**改 `config.yaml` 的 `schema:` 行，需要手工改。
3. 项目本地 schema 优先于用户覆盖，用户覆盖优先于引擎内置。同名 fork 会盖住内置 schema。

## 3. 执行规划层：`plan.md` 是并行派发的输入

`plan.md` 把 specs 的行为要求与 design 的技术方案转成**可派发的工作包**。它必须回答：

- **写范围**：每个工作包改哪些文件/目录前缀。范围重叠的两个包必须在 `## Merge Order` 里串行，
  并声明后合入者要重跑的检查（前一个 patch 已经改变了工作区）。
- **合入顺序**：主 Agent 应用 patch 的串行序列。
- **Project Verify**：只在主检出可执行的命令。隔离工作区没有 `target/` 与 `node_modules/`，
  `cargo test`、`npm run check`、`npm run verify` 在那里必然失败，写在那边等于没验。
- **Code Review**：每个工作包一行，检视者必须是**另一次 spawn**，不能是作者。
- **Main E2E**：`required` 还是 `not-applicable`（后者要写 reason/basis/alternative_checks）。

模板见 `openspec/schemas/spec-driven/templates/plan.md`。

## 4. 实现层：omp 原语

### 4.1 派发

主 Agent 用 `task` 工具的 **batch 形状**一次扇出多个工作包（共享背景写 `context`，逐包写 `task` 与
`solutionSpace`）。工作包之间不共享目录、不共享上下文，因此每个包的说明必须自包含。

项目级配置（`.omp/config.yml`）固定了隔离方式：

```yaml
task:
  isolation:
    enabled: true   # 才在 task item 上暴露 isolated 字段
    merge: patch    # 不产生 commit、不产生分支
    apply: false    # 不自动改写主检出，合入由主 Agent 显式完成
```

`apply: false` 是本仓库的硬约束：它保证每个工作包的改动都先过 `git apply --check`，并且不会与开发者的
在途修改相撞。**不要把它改成 true**——那会把 patch 直接打进主检出工作区，跳过校验与串行合入。

只读 spawn（检视、侦察）不传 `isolated`，因此不会产生工作区副本，也不受开关影响。

### 4.2 交付物

一个工作包的交付是三元组，全部由引擎写出，不依赖子 Agent 的自觉：

| 产物 | 位置 | 内容 |
|---|---|---|
| `<id>.patch` | 会话产物目录（`~/.omp/agent/sessions/<项目>/<会话>/`，**不在仓库内**） | 该工作包的全部文件内容差异（含新建文件） |
| `agent://<id>` | 同上 | 结构化最终报告：`status`、`summary`、`changed_paths` 等 |
| `history://<id>` | 同上 | 完整过程记录：读了什么、跑了什么命令、为什么这么改 |

### 4.3 已知边界

- **patch 只记录内容差异**：不含 commit、不含分支、不含空目录、不含 git 历史。
- **隔离工作区跑不了仓库级检查**：没有 `target/`、`node_modules/`、`clients/app/node_modules`。
  子 Agent 不得尝试，仓库级验证只在主检出做。
- **工作区跑完即销毁**：这是设计如此——改动在销毁前已被捕获成 patch。
- 单次 spawn 的工作区不跨 spawn 复用；需要多轮迭代的工作包应拆成多个工作包，或用同 `agent` 的
  新 spawn 并传入上一轮 patch。
- **隔离区的基线包含主检出工作区状态**。实测：主检出里一个未提交、未跟踪的文件
  （`docs/probe-fixbase.md`）在隔离区里存在，且隔离区的 `git status` 也显示主检出的未提交修改
  （`M .github/workflows/ci.yml` 等）。也就是说隔离是**整个工作目录的副本**，不是 `git clone` 干净基线。

  这条有两个后果：

  1. **好处**：修复 spawn 天然能看到主检出的在途状态，不会把未提交改动误判为"不存在"。
  2. **风险**：如果主检出里有开发者的半成品改动，隔离 spawn 会带着它运行，产出的 patch 可能
     隐含依赖那些改动。所以**派发工作包前主检出的工作区应当相对干净**；有在途改动时，
     要么先提交，要么明确记录"这些改动是本次基线的一部分"。

  另外，隔离工作区目录在 spawn 结束后**不一定被立即清理**（实测留下过 `~/.omp/wt/<id>/`，累计 30M）。
  定期的 `omp worktree clear` 或手工清理是需要的。

## 5. 检视层

每个工作包由一个**独立 spawn** 检视，用 omp **内置的 `reviewer` agent**（只读工具集 + `@slow` 模型），
**不自建检视 agent** —— 内置的已经够用，重复造一份只会多一个维护面。

检视的输入契约在**派发时**用 per-call `outputSchema` 覆盖：

```json
{
  "agent": "reviewer",
  "schemaMode": "strict",
  "outputSchema": {
    "type": "object",
    "required": ["verdict", "explanation"],
    "properties": {
      "verdict": { "enum": ["pass", "changes-requested", "blocked"] },
      "explanation": { "type": "string" },
      "blocked_reason": { "type": "string" },
      "findings": {
        "elements": {
          "properties": {
            "severity": { "enum": ["blocker", "major", "minor"] },
            "path": { "type": "string" },
            "line": { "type": "number" },
            "expected": { "type": "string" },
            "actual": { "type": "string" },
            "fix": { "type": "string" }
          }
        }
      }
    }
  }
}
```

**必须显式传 patch 的绝对路径**：内置 `reviewer` 的工作方式是读 `git diff` / `gh pr diff`，
而我们的 patch 落在会话产物目录（`~/.omp/agent/sessions/.../<id>.patch`），
既不在仓库里、也不是 git 命令的输出。实测把它作为绝对路径写进 task 文本即可正常检视。

两条规则：

- 作者自评不是检视证据。`agent://<id>` 里的 `summary` 用来核对"作者声称的意图"与"patch 实际内容"
  是否一致——不一致本身就是 blocker。
- 需要判断改动是否正确时，检视者必须读被改函数的**调用方**；只看改动点会漏掉集成错误。

### 5.1 检视发现问题时怎么办

`verdict` 决定下一步，三种取值对应三条不同路径：

| verdict | 含义 | 主 Agent 的动作 |
|---|---|---|
| `pass` | 无阻断项（可有 minor 建议） | 进入合入队列 |
| `changes-requested` | 有必须修的阻断项 | 派发修复 spawn（见下） |
| `blocked` | 证据不足，无法判断 | 补齐证据后**重新检视**，不是派修复 |

**`changes-requested` 的修复循环：**

```text
WP1 coder (isolated) ──► WP1.patch ──► reviewer ──► changes-requested
                                            │
                                            ▼
                          新 spawn（agent: wp-coder，isolated: true）
                          prompt 携带三样东西：
                            1. 原工作包目标与写范围
                            2. 上一版 patch（或其内容）
                            3. reviewer 的 findings 原文
                                            │
                                            ▼
                                     WP1-fix1.patch ──► 新 reviewer spawn 复核
```

三条硬性要求：

1. **修复必须是新 spawn，不是复用旧实例。** 隔离 spawn 结束后无法再被唤起或发消息
   （引擎会在结果里明说「ran isolated and cannot be resumed or messaged」），
   所以"让原 coder 改一下"在机制上不成立。
2. **修复的 prompt 必须自带上下文。** 新 spawn 没有上一轮的对话，必须显式传入工作包目标、
   写范围、上一版 patch 与 findings。缺掉任何一项，修复质量就会退化。
3. **复核用新的 reviewer spawn，而不是原 reviewer。** 复核同样要带上「原 findings」与「修复 patch」，
   逐条核对是否闭环。原 reviewer 建议返回 `pass` 才算闭环；只修了一部分则继续循环。

**循环上限。** 同一个工作包最多修 3 轮（与旧 agentic 流程的 `implementation` 重试上限一致）。
超过 3 轮说明工作包本身拆得不对（目标含糊、写范围过大、或依赖没理清），应当回到 `plan.md` 重新拆分，
而不是继续在同一处打补丁。

**修复轮次如何计数。** 产物命名带轮次（`WP1.patch`、`WP1-fix1.patch`、`WP1-fix2.patch`），
每次复核的 reviewer 报告里引用它检视的是哪一版。合入时**只应用最终通过的那一版**，
中间版本不进入主检出。

## 6. 合入层

合入分两段：**本地合入**（在主检出应用 patch）与**远端落地**（分支 → PR → squash）。两段都由主
Agent 串行执行，不交给子 Agent——子 Agent 的交付止于 `<id>.patch`。

### 6.1 本地合入（主检出）

五个步骤：

```bash
git apply --check <patchPath>       # 1. 只校验，不改工作区；失败则退回该工作包
git apply <patchPath>               # 2. 按 plan.md 的 Merge Order 顺序应用
npm run verify                      # 3. 在**主检出**跑仓库级检查
# 4. 检视者读应用后的真实 diff（不是 patch 文本）
# 5. 本地提交
```

两个坑：

- `git apply` 之间互相影响：先合入的 patch 改变了文件后，后合入者的 patch 上下文可能已不匹配，
  `--check` 会失败。这正是 `plan.md` 必须登记写范围与合入顺序的原因。
- 提交信息按 `commitlint.config.mjs` 的 type/scope 词表**手写**。`omp commit` 按文件路径目录名推 scope
  （例如 `chore(storage-sqlite):`），与本仓库的模块级词表（`storage`）不一致，会被 `.husky/commit-msg` 拒绝；
  被拒时提交不落地、改动留在暂存区，改 scope 重提即可。

### 6.2 远端落地：PR 是默认路径

`main` 由 ruleset `main-protection` 保护（设置现状与判据见 `README.md`「分支保护」），只能经由 PR
更新。因此本地合入之后必须走完这五步：

```bash
git switch -c <branch>              # 1. 为这一批 patch 开一个分支
git push -u origin <branch>         # 2. 推送分支；本地钩子已在 commit 时跑过
gh pr create --fill                 # 3. 开 PR；标题必须是 Conventional Commits
gh pr checks --watch                # 4. 等六个必需检查全绿
gh pr merge --squash                # 5. squash 合并，PR 标题即 main 上的提交信息
```

六个必须知道的点：

- **六个必需检查**是 ruleset 的 required status checks，名字必须是 check-run 的上报名：
  **合同门禁 + Rust 检查**、**提交信息规范**、**依赖许可证与来源**、**密钥扫描**、**依赖安全公告**、
  **前端检查（clients/app）**。后三个（依赖判决、advisory、密钥扫描）只在 CI 跑，本地没有等价物——
  所以它们是**合并门禁**，不是可选步骤，直推 main 会整体跳过；前端检查覆盖 `clients/app` 这个
  独立 npm 工程（根检查链不覆盖它），原因见 §8 与 `README.md` 的「合同检查」。
- `strict_required_status_checks_policy = true`：PR 落后于 main 最新提交时不能合并，先 rebase
  并让检查在新提交上重跑。
- `required_approving_review_count = 0`：单人仓库不需要第二人批准，但批准不能替代检查。
- `Repository admin / always` bypass 是**紧急出口**，不是日常路径；日常一律走 PR。
- **一批 patch 一个分支一个 PR**：不要把多个变更批攒在一个分支上，否则某个检查变红时分不清
  是哪一批的回归，rebase 与 revert 也一起变贵。
- PR 标题会被 squash 当成提交信息，因此同样要过 `commitlint.config.mjs`；分支上的本地提交也要过——
  CI 的 `commits` job 校验本次推送/合并请求引入的提交范围，`--no-verify` 绕得过本地钩子但绕不过它。

### 6.3 为什么不用 omp 原生的 git 能力

omp 自带 `omp git`、`omp commit`、`omp worktree` 与 `task.isolation.merge: branch`，但**都承担不了这个合入环节**。
下面每条都实测过，不要再重复评估：

| 原生能力 | 实测结论 |
|---|---|
| `omp git` | 纯交互 TUI（「Interactive fullscreen git UI」），自动化脚本无法驱动 |
| `omp commit` | ① 按目录名推 scope（实测生成 `feat(storage-sqlite): …`），被 `commitlint.config.mjs` 的词表拒绝；② `--dry-run` 也会**改动 git 索引**——实测两次 dry-run 把 `.husky/commit-msg` 与 `.husky/pre-commit` 的执行位从索引里抹成 `100644`（内容哈希未变，但 Linux/CI 上钩子会因此无法执行），需 `git reset` 恢复 |
| `merge: branch` | 让引擎自行 commit + cherry-pick。本机**未实测**（Windows 上 stash-pop 冲突是文档标注的已知风险），且它会**绕开** `--check` 这道门禁 |
| `omp apply` / patch 应用 | **不存在**。查遍 CLI 与工具没有 patch 应用入口；`@oh-my-pi/pi-natives/vcs` 只暴露 `branch`/`worktree`/`config`/`push` |

选择 `git apply` 的理由不是「更原生」，而是**它是唯一能在改动落地前做校验的路径**：
`git apply --check` 先判断上下文能否干净应用，失败即退回该工作包，不污染工作区。
`omp commit` 会污染索引，`omp git` 无法自动化，`merge: branch` 跳过预检——三者都在这个关键点上不合格。

**§6.2 的工具选择**：远端落地用 `gh` CLI 写（与 `README.md`「分支保护」、`.github/pull_request_template.md`
同一入口）；omp 的 `github` 工具（`pr_create` / `pr_checkout` / `pr_push` / `run_watch`）是等价能力，
可以替换，步骤序列不变。PR 替代不了本地合入：required checks 校验的是分支上的提交，而 patch 校验
必须在它进分支**之前**做。

## 7. E2E 层

按被测面选 prelude，两层都要时分别跑：

| 被测面 | 工具 | 开关 |
|---|---|---|
| Web/PWA（真实浏览器、DOM/ARIA/CDP） | `browser` prelude | 默认开（`browser.enabled: true`） |
| 桌面/原生（Zed、Daemon、Windows Job Object 进程树） | `computer` prelude | 日常开（`.omp/config.yml` 的 `computer.enabled: true`） |

`.omp/config.yml` 日常启用 computer prelude，因此桌面/原生 E2E 不需要额外开关。跑的时候传
`--config .omp/e2e.yml` 叠一层**审批收紧**：`tools.approvalMode` 从默认 `yolo` 收到 `write`，
只读检查（窗口枚举、截图、AX 读取、剪贴板读）自动通过，输入与变更类操作需要确认。桌面 E2E 会产生
真实副作用（移动真实鼠标、真实键盘输入），这一层默认要求人在场；确需无人值守全自动时用
`--approval-mode yolo` 显式覆盖，并在运行记录里写明这是一次性授权。`.omp/e2e.yml` 本身仍是
一次性覆盖层，只影响当次运行，不写入持久配置。

`computer` prelude 是 eval 主机的桥调用，**不经过 `tool_call` / `tool_result` 钩子**，
因此现有门禁（如 §10 的隔离门禁）拦不住它；上面那层审批收紧是它的配套缓解，不是替代品。

E2E 通过的标准是**断言了可观察行为**；只启动成功、退出码为零、截图都不能算通过。
Web 层可参考 `clients/app/e2e/run-e2e.mjs`（真实 Chromium + CDP 的既有入口）。

## 8. 门禁

`npm run check` 的第十项 `check:openspec`（`scripts/check-openspec.mjs`）断言五件事：

1. `openspec validate --all --strict`（变更与规范资产）；
2. `openspec/schemas/` 下每个 schema 的结构合法（artifact 依赖、`apply.requires`、模板引用）；
3. `openspec/config.yaml` 的 `schema:` 指向一个真实可解析的 schema；
4. 覆写 schema 里自写的 artifact 仍在（防 `cp -r` 抹掉 `plan`），且「照搬」的模板与上游逐字节一致；
5. `rules` 与 `operations.guidance` 的条目不会被 YAML 静默丢弃。

第 5 项的判定条件比直觉宽：**含 `": "` 或以 `":"` 结尾** 都会被 YAML 解析成映射、被引擎丢弃整条规则。
后者（裸尾冒号，如 `- 以下规则生效:`）是实测补上的——初版只查 `": "`，会漏判。
引号包裹的条目豁免（`- "含 : 的文本"` 是合法字符串）。

它显式关闭引擎的遥测与更新检查（`OPENSPEC_TELEMETRY=0`、`OPENSPEC_NO_UPDATE_CHECK=1`、`DO_NOT_TRACK=1`）——
引擎默认开启这两项，只在检测到 CI 环境变量时才关闭，而 `AGENTS.md` §3 不允许未授权的遥测与云依赖。

通过 `check:openspec` 只等于"资产结构合法"，不等于需求描述正确，也不等于 specs 与代码一致：
后者由其余九道合同门禁负责。

CI 侧另有一道**前端门禁**：`前端检查（clients/app）`（`.github/workflows/ci.yml`）跑
`npm ci` + `npm run check`。它存在的原因见 `README.md` 的「合同检查」——`clients/app` 是独立 npm 工程，
根目录的检查链不覆盖它。job 清单与必需检查状态同样以 `README.md` 为准，本节不复制。

## 9. 与其它文档的关系

- **选那个 schema / 那个命令**：本节 §2。`openspec/config.yaml` 的 `context` 是项目画像，随落地刷新。
- **crate 边界与依赖方向**：`docs/MODULE_ARCHITECTURE.md`。
- **测试要求**：`docs/MODULE_ARCHITECTURE.md` §11。
- **仓库级检查清单**：`README.md` 的「合同检查」。
- **提交信息词表**：`commitlint.config.mjs`。
- **变更类型 → 权威文档**：`AGENTS.md` §9。

## 10. 隔离门禁：为什么这条规则不能只写在文档里

`.omp/config.yml` 的 `task.isolation.enabled: true` **只让 `isolated` 字段出现在 task 参数里，不改变默认值**。
不传该字段的子 Agent 会用父会话的 cwd，直接改写主检出——实测中一次漏传就让探针文件出现在主检出，
而不是被捕获成 patch。

因此仓库装了 `.omp/hooks/pre/enforce-isolation.ts`：在 `tool_call` 阶段拦截 task 调用，
**任一会写文件的工作包没有 `isolated: true` 就整批拒绝**，并给出补法。只读角色
（`reviewer` / `security-reviewer` / `scout`）豁免，因为它们不写文件，
为它们复制工作区只是浪费（那是 15M 的完整副本）。

这条门禁的边界：

- 它按**角色**判定，不按「这次会不会写」。`sonic` 有全量工具，因此不豁免。
- 它拦不住 `bash` 里的写操作（例如 `echo x > file`）。要把「任何改动都走 patch」变成硬保证，
  需要的是文件系统级隔离，而不是钩子；本仓库选择的是可核对的流程约束 + 主检出的 `git status` 复核。
- `computer` prelude 与 `browser` prelude 是 eval 主机的桥调用，**不经过 `tool_call` 钩子**，
  因此不受本门禁覆盖；它们各自的开关与风险见 §7。

## 11. 为什么 workflow schema 必须 fork，副本怎么维护

引擎的 schema 定义里 `template` 是**必填字段**，而它的路径校验（`relativePathSchema`）拒绝绝对路径
与任何含 `..` 的路径段——**不能指向 schema 目录之外**。加上开源版引擎不支持 schema 继承
（`SchemaYamlSchema` 只有 `name`/`version`/`description`/`artifacts`/`apply`），结论是：

> 要新增一个 artifact（本仓库的 `plan`），就必须自建一个 schema 目录，并自带一整套模板。

因此 `openspec/schemas/spec-driven/` 里除了 `schema.yaml`，还有 5 个模板文件。维护策略是**把分歧面压到最小**：

| 模板 | 来源 | 升级引擎时 |
|---|---|---|
| `proposal.md` / `spec.md` / `design.md` / `tasks.md` | 逐字节照搬引擎 `spec-driven` | `cp` 覆盖，无三方合并 |
| `plan.md` | 本仓库自写（引擎里不存在这个 artifact） | 不受升级影响 |

`check:openspec` 的第 4 项判定断言前四个与上游逐字节一致；上游没有的文件（`plan.md`）自动豁免。
这样做的目的是：升级 `@fission-ai/openspec` 后，上游模板若变化，本地的副本不会安静地过期而没人发现。

**不要**把照搬的模板本地化（例如把英文占位注释翻成中文）：那会把零分歧副本变成需要人工维护的译本，
每次升级都要重新本地化。语言要求放在 `config.yaml` 的 `rules:` 里（见 §2），模板只管结构。

### 11.1 为什么要同名覆写，以及它带来的三条硬约束

本仓库的 schema 目录名为 `spec-driven`，与引擎内置版**同名**。这样做的唯一目的是命名连续性：
15 个已归档变更的 `.openspec.yaml` 都写着 `schema: spec-driven`，新变更沿用同一个名字。

先说清楚：这条选择**不是功能需要**。实测归档变更根本不在 `changes/` 下，`status`/`show` 对它们本来就
报 not-found；归档读取时若 schema 名解析不出来，引擎会回落去跑校验（`archive.js` 的注释明写
「names a schema that does not resolve → forces validation」）。所以不一致也不会有功能故障。

同名带来三条必须遵守的约束：

**一、不能再用 `schema fork` 从上游同步。** 实测：本地已有同名 schema 时，
`schema fork spec-driven spec-driven` 报 `Cannot fork schema 'spec-driven' onto itself`——
因为「源」按解析优先级解析到了本地那份，与「目的地」是同一个目录。要重新同步只能**先删本地目录再 fork**，
或者手工 `cp`。

**二、升级引擎时的同步动作必须只覆盖模板，绝不覆盖 `schema.yaml`。** 安全写法：

```bash
# 只同步「照搬」的四个模板；plan.md 是本仓库自写的，不要被覆盖
cp node_modules/@fission-ai/openspec/schemas/spec-driven/templates/{proposal,spec,design,tasks}.md \
   openspec/schemas/spec-driven/templates/
```

`cp -r` 整个目录**会连 `schema.yaml` 一起覆盖，抹掉 plan artifact**——这是同名覆写下最容易犯的错。

**三、`schema.yaml` 的 `name:` 必须与目录名一致（都是 `spec-driven`），
且它的 `description` 要写明这是项目覆写版**，否则从文件内部看不出它与内置版的区别。

`check:openspec` 的第 4 项判定在同名条件下仍然能区分「照搬」与「漂移」：它以**引擎内置目录**里的同名文件
为基准，因此本地有、上游没有的 `plan.md` 自动豁免（实测：单独改它不报错），
本地与上游都有的四个文件一旦不一致就报错（实测：改 `proposal.md` 报错并给出文件名）。
