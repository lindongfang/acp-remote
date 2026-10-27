# 变更执行工作流

> 本文件是变更从需求走到代码的流程权威。产品、协议、安全与模块行为仍以 AGENTS.md §2 的文档为准。

## 1. 五段流程一览

| 段 | 载体 | 产物 |
|---|---|---|
| 需求规划 | OpenSpec 项目覆写 schema | proposal、specs、design |
| 执行规划 | plan.md | 工作包、写范围、依赖、合入顺序、验证策略 |
| 任务分解 | tasks.md | 有完成判据的任务 |
| 实现与复核 | 当前可用编码工具 | 实际差异、验证结果和适用的复核结论 |
| 合入与验收 | Git、GitHub CLI、适用测试工具 | PR、检查结果、真实行为证据 |

流程不依赖特定编码宿主、模型、调度器、子代理 API 或私有会话地址。可直接使用 OpenSpec CLI 或 `.agents/skills/` 的受管技能，技能由 OpenSpec 引擎维护。

## 2. 需求规划层：schema spec-driven（项目覆写版）

`openspec/config.yaml` 选择仓库内 `openspec/schemas/spec-driven/`。依赖顺序为 proposal → specs / design → plan → tasks → apply；apply 要求 plan 和 tasks 完成。纯工具、文档或内部重构可声明 `skip_specs`，不为满足流程虚构产品要求。

字段规则由 config.yaml 的 rules 下发，操作指引由 operations 下发。列表条目必须是字符串；含冒号加空格或以冒号结束的条目要加引号，避免被 YAML 当成映射而静默丢弃。

## 3. 执行规划层：plan.md

计划明确目标、执行者、写范围、依赖、合入顺序、验证位置和完成判据。简单变更可由一个执行者串行完成；不强制多 Agent、特定角色名或独立进程。

并行写入只在互不干扰的隔离工作区执行。范围重叠时串行处理，并验证合并结果。检查可以在具备依赖的工作区运行；最终检查必须覆盖待提交的集成结果，不假设隔离区一定有或没有缓存和依赖。

Code Review 按风险写明复核方式；Main E2E 选择 required 或 not-applicable，后者写明原因、依据和替代检查。模板见 `openspec/schemas/spec-driven/templates/plan.md`。

## 4. 实现层：工具无关的执行

### 4.1 执行与隔离

修改前检查 git status，记录基线，不覆盖用户未提交修改。默认由当前执行者串行完成；只有实际需要并行或用户要求时才派发工作。并行写入使用独立 Git worktree、checkout 或等效隔离，不允许多个写入者同时修改同一工作区。

### 4.2 交付物

交付必须可复核：完整差异、变更说明、实际命令及结果、未执行检查和已知限制。隔离实现可交付 patch 或提交；不要求宿主专有报告格式、会话 URI 或固定目录。

### 4.3 已知边界

核对隔离区的真实基线、依赖和构建目录。并行 Rust 构建分别设置 CARGO_TARGET_DIR，避免争用。确认产物完整交付后才清理本次拥有的临时工作区，不清理用户资源。

## 5. 检视层

取消指定宿主内置 reviewer、固定模型和每包独立启动检视进程的强制要求。普通低风险改动由执行者检查真实差异并运行适当验证，不因缺少额外检视工具、余额或特定服务阻断。

协议、安全、持久化、并发和跨模块改动按风险安排独立复核，可由另一位人员或当前可用的另一 Agent 完成，不限制工具品牌。需要独立复核时，作者自评不能冒充独立意见。记录范围、依据、发现和结论；复核不替代测试。

### 5.1 检视发现问题时怎么办

有阻断问题先修复并验证，证据不足先补证据。可在同一上下文继续修复，不强制重新启动作者或检视者。复核针对实际修改和未闭环风险；多轮不能收口时重新拆分目标。不得把失败、未运行或启动通知写成通过。

## 6. 合入层

### 6.1 本地合入（主检出）

直接实现时检查实际 diff 和暂存范围；收到 patch 时先 git apply --check 再 git apply；收到提交时按计划合并或 cherry-pick。范围重叠的工作串行合入，冲突不得靠覆盖用户修改解决。

最终运行 npm run verify；前端改动还需在 clients/app 运行 npm run check。提交前 git diff --check，确认没有混入临时文件、生成目录或用户工作。提交信息按 commitlint.config.mjs 校验。

### 6.2 远端落地：PR 是默认路径

main 受保护，日常经分支 → 推送 → PR → 必需检查全绿 → squash 合并更新。主执行者串行完成，不直推 main 或使用管理员绕过检查。

命令示例：git switch -c <branch>；git push -u origin <branch>；gh pr create；gh pr checks --watch；gh pr merge --squash。多行 PR 描述写文件并用 --body-file 传入。

六个必需检查是合同门禁 + Rust 检查、提交信息规范、依赖许可证与来源、密钥扫描、依赖安全公告、前端检查（clients/app）。机器清单及现状以 README 和 ruleset 为准。PR 落后 main 时先更新并重新检查，不得拿旧提交结果代替新提交检查。PR 标题符合 commitlint.config.mjs，squash 会采用该标题。

### 6.3 工具选择

默认用 Git 与 gh，也可用等价工具；要求差异、提交和检查可核对。自动生成提交信息同样须满足仓库词表，不得让便利工具隐式改写索引、跳过检查或扩展提交范围。

## 7. E2E 层

E2E 是从真实入口到可观察结果的端到端验收。按被测面选择可用的浏览器自动化或桌面测试工具，不依赖特定宿主的 browser / computer 接口。Web/PWA 用真实浏览器；涉及 Daemon、进程树或真实 Agent 的用例实际启动对应组件，不能用假主机替代并声称全链通过。

只启动成功、退出码为零或截图不足以证明行为，必须断言对应结果。记录命令、环境、组件、通过/失败/跳过和限制；未执行不写通过。操作用户真实输入或共享资源时遵守当前授权范围。

## 8. 机器门禁

npm run check 是合同检查唯一入口，npm run verify 追加 Rust 格式、Clippy 与测试。clients/app 是独立 npm 工程，不能用根检查代替。仅 CI 执行的供应链和密钥检查以 PR 为准。

check:openspec 检查严格校验、本地 schema 结构及引用、自写 plan 未丢失、照搬模板与上游一致、rules / guidance 未退化成 YAML 映射；它不判断需求正确性或实现完整性，也不要求安装编码宿主或启动 reviewer。

OpenSpec 调用显式关闭遥测和更新检查：OPENSPEC_TELEMETRY=0、OPENSPEC_NO_UPDATE_CHECK=1、DO_NOT_TRACK=1。

## 9. 与其它文档的关系

AGENTS.md §9 定义变更类型到权威文档的映射；MODULE_ARCHITECTURE.md 定义模块和测试边界；README 定义检查与分支保护；commitlint.config.mjs 定义提交词表。config.yaml 的 context 记录项目事实，不记录单次变更临时状态。

## 10. 隔离与复核边界

隔离依赖真实工作区和所有权，不依赖宿主钩子。执行者核对基线、写范围、合入结果和资源清理。仓库不再维护专有宿主配置、角色、命令副本或隔离钩子；本机可自选工具，其配置不能成为交付前提。

已归档计划与历史证据按原样保留，工具名称仅表示当时执行方式，不再构成后续修改的强制条件。历史遗留任务按当前工作流和真实判据收口，不为旧宿主补跑专用步骤。

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
