# 最小完整变更示例：CSV 导出

本例展示六种交付件及最后交接，不是已执行的证据。命令、绝对路径、版本和执行者均须在真实项目核实。
示例项目是已有 npm run verify / npm run e2e 的 Node CLI；用户要求增加 CSV，保留默认 JSON，禁止上传数据。
示例按 agentic 流程在门禁通过后合入本地 main。不含推送/发布；不得复制示例 PASS 作为实际证据。

## proposal.md

````markdown
## Why
用户需要在电子表格中查看本地数据。
## What Changes
增加 --format csv；不传参数时保留 JSON。
## Intent and Constraints
```agentic-intent
sources: ["示例会话 2026-09-22：增加 CSV 导出，保留默认 JSON；不得上传数据"]
constraints: ["默认 JSON 和已有退出码兼容", "数据只在本地处理"]
non_goals: ["不增加云端导出或数据采集方式"]
success_criteria: ["真实 CLI 输出可解析 CSV，特殊字符不丢失，默认 JSON 不变"]
decision_bounds: ["可选内部序列化实现；改变默认行为或增加联网依赖需用户决策"]
assumptions: ["无待确认产品假设；输入格式沿用已有契约"]
```
## Capabilities
### New Capabilities
- export: 本地 CSV 导出。
### Modified Capabilities
无。
## Impact
CLI 参数解析、序列化模块和真实 CLI 测试。
````

## specs/export/spec.md

```markdown
## Purpose
为需要在电子表格和其他本地工具中分析数据的用户提供 CSV 导出能力，同时保留已有 JSON 默认行为以及数据不离开本地环境的约束。
## ADDED Requirements
### Requirement: Local CSV export
系统 SHALL 在指定 CSV 时导出标准 CSV，保留默认 JSON 行为且不发起网络请求。
#### Scenario: CSV quoting
**WHEN** 用户以真实 CLI 指定 CSV，输入包含逗号、引号和换行
**THEN** 标准 CSV 解析器还原全部原始字段值
#### Scenario: Default JSON compatibility
**WHEN** 用户不指定导出格式
**THEN** 输出和退出码保持原有 JSON 契约
```

## design.md

```markdown
## Context
现有 CLI 已支持本地输入；产品约束引用 proposal。
## Goals / Non-Goals
增加序列化分支，不改变默认参数和输入协议。
## Decisions
参数解析选择 CSV 或既有 JSON 序列化器，数据结构不变，不引入网络依赖。
测试从真实 CLI 子进程入口执行，用标准 CSV 解析器验证往返结果。
## Risks / Trade-offs
特殊字符处理错误 → 往返断言；默认行为回归 → 保留 JSON 测试。
```

## plan.md

下面的覆盖索引在测试设计后固定。早期先定范围，tasks 生成后补齐准确编号；正式 E2E 前固定契约。

````markdown
## Scope and Contracts
Specs/Design Revision：当前内容版本，正式验证前取 workflow check 摘要。
Convergence Check：CSV、默认 JSON、离线约束与接口一致；Skip Specs：no。
## Contract Changes
无；变化时在 verification 保存原/新值、依据和影响，不直接刷新摘要。
## Coverage Index
```agentic-coverage
version: 1
target_ref: refs/heads/main
rows:
  - id: R1
    source: {path: specs/export/spec.md, heading: "### Requirement: Local CSV export"}
    tasks: ["3.1", "3.2", "8.1"]
    checks: [PV1, CR1, E1]
    evidence: [reports/PV1.log, reports/CR1.md, reports/E1.json]
  - id: S1
    source: {path: specs/export/spec.md, heading: "#### Scenario: CSV quoting"}
    tasks: ["8.1"]
    checks: [E1]
    evidence: [reports/E1.json]
  - id: S2
    source: {path: specs/export/spec.md, heading: "#### Scenario: Default JSON compatibility"}
    tasks: ["3.1", "8.1"]
    checks: [PV1, E1]
    evidence: [reports/PV1.log, reports/E1.json]
```
## Work Packages
- Dependency Declaration Review：CR-DEP（独立 reviewer 核实 code / contract / resource 声明属实）
| ID | Goal / Scenarios | Dependencies | Contract Freeze | Owner | Role | Reviewer | Branch / Worktree | Write Scope | Inputs / Outputs | Verification |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| WP1 | 场景 CSV quoting、Default JSON compatibility | none | NOT_APPLICABLE | coder-A | coder | 非作者 reviewer | wt/wp1 | src/cli、src/serializer | 固定契约；固定提交与检查报告 | PV1、CR1 |
| TP1 | E1-CSV / E1-JSON | contract:WP1 | specs/export/spec.md@当前版本 | tester-A | tester | 非用例作者 reviewer | wt/tp1 | test/export | specs 与接口约定；用例与基础检查报告 | E1、CR1 |
输入为固定契约，输出为固定提交/检查报告；分别使用独立 worktree 和非作者 reviewer。
## Execution Waves

| Wave | Work Package | Enter Condition | Serialization Reason |
| --- | --- | --- | --- |
| W1 | WP1 | 契约/资源就绪 | NOT_APPLICABLE |
| W1 | TP1 | 契约已冻结于 specs/export/spec.md@当前版本 | NOT_APPLICABLE |

WP1/TP1 能同时开工，不互等；设计不等产品，执行等固定候选。
## Shared File Ownership

| File | Writers (WP) | Merge Owner | Merge Order | Re-verify After Merge | Region Note |
| --- | --- | --- | --- | --- | --- |
| <!-- src/x.ts --> | <!-- WP2, WP3 --> | <!-- 合并负责人 --> | <!-- WP2 → WP3 --> | <!-- WP3: PV1 --> | <!-- 各改独立区域/函数 --> |

本变更无同批写入重叠，本节只保留表头与说明。
## Dependency Handoffs
候选必须包含 WP1/TP1 已验收提交，merger 核对包含关系；上游变化重评下游。
## Runtime Resources
每用例独立临时输入/输出目录（未登记为共享、已分配给 tester），由 tester 在自身命名空间内启动、使用与清理；无共享运行服务。
## Target Repository and Main Branch
Code Repository：实际仓库绝对路径；local refs/heads/main。
scout 用 git rev-parse 做规划时的只读目标核实，main 记录到 verification，不以任意工作区 HEAD 代替；
候选构建及合入瞬间的基线复核由 merger 执行。
## Merge Strategy
MU1 integrated，包含 WP1/TP1；独立 merger 复用 WP1/TP1 的执行 worktree（不新建）。
按计划核对本地合入条件，报告 reports/MU1.md；不推送/发布。
## Verification Strategy
## Independent Validation

| Task | Target Revision | Assignment | Pass Condition | Report Path |
| --- | --- | --- | --- | --- |
| 7.1 | 最终主分支固定版本 | 覆盖充分性（R1/S1/S2）与待验证假设 | 无未解决覆盖缺口 | reports/VL1.md |
### Local Checks
针对改动运行序列化单测。
### Project Verify
PV1：仓库根 npm run verify，核实类型/单元/集成子检查，零发现/全跳过不通过。
分支、候选、主分支分别留固定版本证据；同内容复用需说明依据。
### Code Review
CR1：分支/用例/组合/候选/主分支分阶段独立审查；reports/CR1.md 索引各轮原报告。
重点检查无联网路径、兼容性、断言和集成交互；阻断问题必须复核。
### Main E2E
```yaml
mode: required
```
#### E2E Ownership and Cases
E1-CSV / E1-JSON 对应上述场景；真实 CLI 操作，解析输出并核对退出码；用例作者不自审。
#### E2E Execution Contract
e2e.command = npm run e2e；每轮 final 一个聚合入口，任一失败/零执行非零退出。
执行前固定代码/用例提交、Node/依赖版本、构建标识，逐用例报告 reports/E1.json。
#### E2E Execution Waves
单分片串行运行两个用例，各自隔离目录；最终一次执行覆盖两个用例。
## Failure and Recovery
产品修复交 WP1，测试修复交 TP1；独立复核后重建候选/复验，环境恢复留证。
达到项目重试上限停止并交用户决策，不删失败记录。
## Completion Criteria
全部交付/集成检查、review 和 E2E 通过；main 按 agentic-verify 审计，再执行 workflow check final。
## User Deliverables
| Deliverable | Location / Version | Usage / Configuration | Recipient | Completion Standard | Authorization / Prerequisites |
| --- | --- | --- | --- | --- | --- |
| CSV 导出工具及使用说明 | 最终本地主分支；源码入口和文档路径见交付报告 | 报告注明安装、启动、导出命令与配置方法 | 用户 | 用户可按说明导出 CSV；测试与原始证据有效 | 本地实现/合入；远端交付、部署/发布与归档按用户已有授权执行 |
最终回复逐项交接，并分别说明本地合入、远端交付、部署/发布和归档的实际状态。
````

## tasks.md

所有项初始待办；下面示例的任务描述已经包含负责人、依赖和完成条件。
这些是 apply 执行任务；propose 已完成规划、独立依赖声明审查与 workflow check --stage plan，
并在 verification.md 登记当前 planningDigest 的 PASS。apply 先确认该门禁有效，再创建执行 worktree；旧全文摘要记录仍按 contractDigest 核对。

```markdown
## 1. Setup
- [ ] 1.1 [MU1] main；依赖：无；核实契约、文件所有权、目标、授权和资源，记录起点。
- [ ] 1.2 [MU1] provisioner；依赖：1.1；创建并分配 WP1/TP1 的 worktree，按 (WP, Attempt) 返回结构化交接记录（worktree、基线、本轮认领执行者、开工前接收时间），由 main 校验后写入 verification 的 `## Worktree Handoff`。
## 2. Implementation
- [ ] 2.1 [wp:WP1] 派发 WP1（coder）子 Agent；开工前 `openspec-agentic dispatch --change demo --wp WP1 --executor coder-A --role coder` 登记状态（coding），交付后销毁 coder、同一窗口起 reviewer（reviewing），review 通过后由主 Agent 勾选
      - WP1：coder；依赖 1.1；实现 CSV，局部单测通过后返回固定提交。
- [ ] 2.2 [wp:TP1] 派发 TP1（tester）子 Agent；开工前 `openspec-agentic dispatch --change demo --wp TP1 --executor tester-A --role tester` 登记状态（coding），契约已冻结即可开工，不等待 WP1 编码完成
      - TP1：tester；依赖 1.1；设计 E1-CSV/E1-JSON、编写用例与脚本，基础检查通过。
## 3. Branch Validation
- [ ] 3.1 [WP1] [PV1] coder；依赖：2.1；完整 verify 通过，日志绑定提交。
- [ ] 3.2 [WP1] [CR1] 新 reviewer；依赖：2.1；独立审查，阻断问题复核闭环。
## 4. Test Design and Authoring
- [ ] 4.1 [TP1] tester；依赖：1.1；设计 E1-CSV/E1-JSON，返回需求/断言映射。
- [ ] 4.2 [TP1] tester；依赖：4.1；编写用例，基础检查通过并返回固定提交。
- [ ] 4.3 [TP1] [CR1] 新 reviewer；依赖：4.2；独立审查已列用例与需求映射、入口和断言，缺失场景交主 Agent 与 validator。
## 5. Integration Readiness
- [ ] 5.1 [MU1] main；依赖：1.1；创建独立 merger，交接固定输入、权限和报告路径。
- [ ] 5.2 [MU1] [PV1] merger；依赖：3.1、3.2、4.3、5.1；组合 WP1/TP1，verify 通过，核对版本包含关系。
- [ ] 5.3 [MU1] [CR1] 新 reviewer；依赖：5.2；组合接口审查无未解决阻断项。
## 6. Merge Unit
- [ ] 6.1 [MU1] merger；依赖：5.3；在构建候选前机械核实最新 main 准确引用与提交。
- [ ] 6.2 [MU1] merger；依赖：6.1；构造固定候选并记录源/base/target。
- [ ] 6.3 [MU1] [PV1] merger；依赖：6.2；候选 verify 通过或留有效复用依据。
- [ ] 6.4 [MU1] [CR1] 新 reviewer；依赖：6.2；审查候选新增交互，阻断项闭环。
- [ ] 6.6 [MU1] merger；依赖：6.3、6.4；先以 `agentic-premerge` 块运行 `workflow check --stage premerge` 并取得 PASS，再把该块持久化为版本化 receipt，并把交付单元、全部 WP/TP、目标/候选、PASS、receipt 作为结构化记录返回 main；由 main 校验后写入 verification 的 `## Premerge History`；复核基线后条件合入本地 main 并记录实际结果。
- [ ] 6.7 [MU1] [PV1] merger；依赖：6.6；主分支必要回归通过，核对候选一致性。
- [ ] 6.8 [MU1] [CR1] 新 reviewer；依赖：6.6；主分支新增差异审查，无差异时 main 留复用依据。
## 7. Independent Validation
- [ ] 7.1 [VL1] [validation] 新 validator；依赖：6.8；在全部单元合入后的最终主分支固定版本独立判断覆盖充分性与假设，返回报告、隔离方式与证据。
## 8. Final E2E
- [ ] 8.1 [全变更] [E1] tester；依赖：6.7、6.8；固定版本并运行 e2e run --change demo --stage final，返回原始结果即交付，不等门禁 PASS。
- [ ] 8.2 [全变更] main；依赖：8.1；核对完整覆盖、处理问题并确认清理，必要断言全部通过。
- [ ] 8.3 [全变更] [e2e-owned] 扩展；依赖：8.2；e2e check --change demo PASS 自动勾选，不承担执行/汇总。
## 9. Final Verification
- [ ] 9.1 [全变更] [final-verification] main；依赖：8.3；按 agentic-verify 审计意图/契约/证据，填写当前验收块，workflow check final PASS 后完成。
```

## verification.md 与最后交接

使用 verification 模板的全部适用节，从 apply 开始积累：Target、Dependency Declaration Review、Checks、Check Plan Changes、
Handoff Index（含 Executor / Agent）、Dependency Handoffs、Runtime Resources、Worktree Handoff、
Dispatch Reconciliation（Attempt / Executor / State 与台账一致）、Review Findings、Independent Validation、
Merge History、Premerge History、Test Design and Authoring、
Main E2E、Failures and Retests、Final Assessment。无变更/无问题的节明确写无，
不伪造示例执行结果。检查记录关联任务/版本/原始报告，失败保留问题、修复、独立复核和复验轮次。

审计后运行 `workflow check --change demo --stage plan --json` 取得 contractDigest 和已有证据摘要，
核对原报告后填写唯一当前块；额外 review/资源/集成报告也列入 evidence。不要手编摘要。

```agentic-assessment
assessment_id: "实际轮次 ID"
target_commit: "实际核实的完整提交"
contract_digest: "实际输出的 sha256 摘要"
result: BLOCKED
evidence: []
```

这里有意保留 BLOCKED：只有真实语义审计通过、报告路径及摘要齐备才填写 PASS 并运行 final 检查。
final 通过后勾选 9.1，归档前运行 archive 检查，再在授权内调用上游 archive。
版本/契约/证据变化先重评，不直接复制旧 PASS；原轮次正文保留，当前机器块保持唯一。
