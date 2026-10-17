<!-- 执行安排：把 specs 的行为要求与 design 的技术方案转成可派发的并行工作包。
     plan 只写“谁在哪个隔离工作区做什么、按什么顺序合入、怎么验”，不重复 specs/design 的内容。 -->

## Execution Model

- 派发：主 Agent 用 `task` 工具的 batch 形状一次性扇出工作包；每个工作包是**一次 spawn**。
- 隔离：`task.isolation`（`patch` 模式、`apply: false`）。工作包之间不共享目录。
- 交付：`<id>.patch` + `agent://<id>` + `history://<id>`；工作包不产生 commit、不产生分支。
- 合入：主 Agent 按 `## Merge Order` 串行 `git apply`，每次合入后跑主检出 Project Verify。
- 验证：隔离工作区内**没有** `target/` 与 `node_modules/`，因此仓库级检查只在主检出执行。
- 检视：每个工作包由一个**独立** spawn（不与作者同实例）读 patch 与 `agent://<id>` 后给出结构化结论。

## Work Packages

| ID | Goal / Scenarios | Dependencies | Agent | Write Scope | Deliverable | Verify |
| --- | --- | --- | --- | --- | --- | --- |
| WP1 | 把两份主规格的旧口径改写到位：`scope-expansion` 删掉「设备授权包 `pack.create-session`」整条（含两个场景）、改写 Purpose 与另两条要求里的设备授权词汇；`sync-session-create` 改写「引用失败与解析失败的分类」（未登记别名 = 参数类错误、Agent 未配置 = 受理之后的 `failed`）。改写文本逐字采用 `design.md` D1–D4 与 `tasks.md` 给出的正文 | none | wp-coder | `openspec/specs/scope-expansion/spec.md`、`openspec/specs/sync-session-create/spec.md` | patch：两份主规格的 diff；报告须列出「删了哪条 / 改了哪几句」并与逐字给定文本一一对照 | `npm run check`（十项，含 `check:openspec` 的 `--strict`）与 PV2 的 grep 判据（主检出） |

## Merge Order

| Order | Work Package | Reason | Re-verify After Merge |
| --- | --- | --- | --- |
| 1 | WP1 | 唯一工作包；写范围只有两份主规格，与其它变更无重叠 | `npm run check`、PV2 |

## Project Verify

| Check ID | Command / Working Directory | Scope / Pass Criteria |
| --- | --- | --- |
| PV1 | `npm run check`（主检出根） | 十项合同门禁全绿；其中 `check:openspec` 对全部 spec 跑 `openspec validate --all --strict`，`check:docs` 校验链接与章节引用 |
| PV2 | `grep -rn "pack\.create-session\|pack\.\*\|preset\.\|authorization\.scope_denied" openspec/specs/`（主检出根） | 无匹配（合法表述里已不存在这些字符串；`docs/` 与 `compatibility/` 里的「已删除」叙述不在本检查面内） |
| PV3 | `openspec validate --specs --strict`（主检出根） | `scope-expansion` 与 `sync-session-create` 两份 spec 通过；要求条数与场景数与 `tasks.md` 的对照清单一致 |

## Code Review

| Work Package | Reviewer Agent | Review Focus | Pass Condition |
| --- | --- | --- | --- |
| WP1 | reviewer | 逐条核对改写后的每一句是否与已实现行为一致（归口证据：未登记别名在受理提交前解析失败、Agent 未配置经 `UnknownProfile` → `SessionNotEstablished` → `failed`）；是否只删/改词汇而没有夹带语义变化；是否有该删未删（尤其 `pack.`/`preset.`/`authorization.scope_denied`）；要求与场景的增删是否与 `tasks.md` 的对照清单一致 | 无 CRITICAL/MAJOR 未闭环项 |

## Main E2E

```yaml
mode: not-applicable
reason: 本变更只改两份主规格的文本，没有任何运行时行为面可驱动（不改代码、不改 wire、不改配置）。
basis: docs/WORKFLOW.md §7 按被测面选 prelude：本变更既没有 Web/PWA 面也没有桌面/原生面，prelude 无对象可驱动。
alternative_checks:
  - PV1 的 `check:openspec`（`openspec validate --all --strict`）证明规格结构与被引用章节仍然合法；
  - PV2 的 grep 判据直接证明目标词汇已从 `openspec/specs/**` 消失；
  - WP1 检视者逐条比对改写文本与已实现行为（读 `crates/core/src/use_cases.rs`、`crates/agent-host/src/error.rs`、`crates/core/src/broker.rs` 的归口点）。
```

## Risks

- **改写偏离实现**：措辞一旦与实现不一致，就把「欠账」换成了新的欠账。缓解：改写文本逐字给出（`tasks.md`），检视者按实现归口逐条核对。
- **误删仍有效的要求**：`scope-expansion` 的「本地管理能力永不远程授予」只是去掉「设备 scope」这一面，要求本身必须保留。缓解：PV3 与 tasks 的对照清单逐条点名「删什么、留什么」。
- **门禁盲区**：`npm run check` 不校验「陈述与实现一致」，因此本次的判据主要是 grep + 人工核对，而不是机器断言。已登记为 design 的 Open Question（是否给合同资产检查加一条 `openspec/specs/**` 的已删除词汇扫描）。
