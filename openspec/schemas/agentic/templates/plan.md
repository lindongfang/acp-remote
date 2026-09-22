<!-- 将 specs 的行为要求和 design 的技术方案转为协作计划，主 Agent 统一维护。
     规则正文见 schema.yaml 的 plan instruction 与 apply instruction，角色执行细节见 roles/。
     本模板只规定填什么、ID 从哪来、何时填，不重复规则；tasks 保存执行步骤和进度，
     verification 保存实际提交、运行结果及失败/复验历史；计划更新时同步 tasks，不复制执行日志。 -->

<!-- 分阶段填写，后期信息不作为开始编码的前置条件：
     生成 tasks 前：契约、WP/TP、依赖、职责、写入范围、交付单元、目标分支、验证策略、
       E2E 适用性与候选/最终覆盖范围、资源需求。
     对应交付前：纳入测试 Agent 产出的用例、需求映射及独立审查安排，并完成约定基础检查与审查。
     正式验证前：补齐本轮固定版本、就绪资源、执行分片及报告路径；实际使用情况和结果写入 verification。
     工作包职责以 Work Packages 为准，检查定义以 Check ID、用例以 E2E ID、资源以 Runtime Resources
     中的唯一名称为准；其他表引用这些 ID，不重复复制定义。 -->

## Scope and Contracts

<!-- specs/design 可并行编写；主 Agent 在本阶段组织共同收敛，记录文档版本和核对结论：
     specs 的行为/验收条件与 design 的接口、数据、环境约定无影响拆包或验收的冲突。
     skip_specs 时记录理由及所依据的既有契约。 -->

- Specs Revision: <!-- 本次增量规范的提交或内容摘要；skip_specs 时改写所依据既有契约的路径与版本 -->
- Design Revision: <!-- design.md 的提交或内容摘要 -->
- Convergence Check: <!-- 核对结论：一致，或冲突内容与解决结果、待澄清项及受影响下游 -->
- Skip Specs: <!-- 不适用写 no；跳过时写理由，并引用既有行为契约的路径与版本 -->

## Contract Changes

<!-- 澄清改变需求或接口时，更新当前契约引用、受影响安排，并同步 specs/design/tasks/用例。
     原/新值、原因、影响分析及证据失效或复用历史写入 verification 的 Check Plan Changes，本节只引用。 -->

## Coverage Index

<!-- 唯一的机器可读覆盖索引；版本 1。路径相对权威 changeDir，也可使用绝对路径。
     source.heading 精确引用含 # 的标题；每个增量 Requirement 与 Scenario 都有一行，
     RENAMED 引用其 ## RENAMED Requirements 标题。skip_specs 引用既有契约的实际路径/标题。
     同名 Scenario 在不同需求下重复时，source.requirement 填其所属的完整 ### Requirement: 标题；
     不需要重命名既有场景，也不以行号代替稳定引用。
     tasks 为字符串编号；checks 在这些任务的描述中以 [Check ID] 声明。
     evidence 是计划的原始报告路径，规划阶段可以尚不存在，最终检查必须可读。
     实际结果只写 verification，本索引不复制结果。基础设施/审查任务可被多个覆盖行引用。
     target_ref 与 Target Repository and Main Branch 一致；远端引用仍需按计划刷新并独立核实。
     tasks 生成后运行 workflow check --stage plan，修正缺口后才开始 apply。
     进入最终 E2E 前固定索引；改变任何契约内容会使旧 E2E 契约摘要失效，勾选复选框不会。 -->
```agentic-coverage
version: 1
target_ref: refs/heads/main
rows:
  - id: R1
    source:
      path: specs/<capability>/spec.md
      heading: "### Requirement: <名称>"
    tasks: ["2.1", "3.1"]
    checks: [PV1]
    evidence: [reports/PV1.log]
```

## Work Packages

<!-- 实现工作包（WP）Owner 按 roles/coder.md 执行；阶段（implement / fix）与交付单元在派发时一并确定，
     review 或测试发现触发的修复按原 WP ID 重新派发，不预先列入本表。
     测试工作包（TP）Owner 按 roles/tester.md 执行。
     required 时按功能域/用户路径增加 TP1…TPn，Owner 为独立测试 Agent，Reviewer 为非用例作者。
     此处填写测试范围即可，不要求完整 E2E ID/场景清单。 -->

| ID | Goal / Scenarios | Dependencies | Owner | Reviewer | Branch / Worktree | Write Scope | Inputs / Outputs | Verification |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| WP1 | <!-- 目标与场景引用 --> | <!-- 前置工作包或无 --> | <!-- 实现者 --> | <!-- 非对应代码作者 --> | <!-- 分支与目录 --> | <!-- 文件归属 --> | <!-- 契约和交付物 --> | <!-- 完成检查 --> |

## Execution Waves

<!-- 每批工作包、依赖满足条件和并发上限；共享接口先确定，共享文件指定单一负责人。
     区分契约、代码和执行资源依赖：接口明确即可并行实现，相关集成验证等待上游交接；
     TP 的场景设计不依赖产品实现完成或运行环境就绪。不支持并发时记录串行策略。 -->

## Dependency Handoffs

| Downstream | Upstream | Accepted Revision / Evidence | Start Revision | Transfer / Inclusion Check | Invalidation |
| --- | --- | --- | --- | --- | --- |
| <!-- 下游 WP --> | <!-- 全部上游 WP --> | <!-- 规划时写验收条件；接入前引用 verification 中已验收提交 --> | <!-- 基线选择约定；接入前引用已确认起点 --> | <!-- 引入方式及包含关系检查 --> | <!-- 上游变化时受影响的传递下游及重验范围 --> |

<!-- 无代码依赖时写明不适用。实际提交、包含关系检查结果和旧证据的失效/复用判断保存在
     verification 的 Dependency Handoffs；本表只引用当前有效交接。 -->

## Runtime Resources

| Checks / Work Packages | Resource | Isolation / Configuration | Exclusive Scheduling | Owner / Cleanup |
| --- | --- | --- | --- | --- |
| <!-- 使用者 --> | <!-- 数据库/schema、端口、容器、账号、可写缓存/目录、外部服务 --> | <!-- 独立命名空间及配置注入方法 --> | <!-- 无法隔离时的排队与独占方式 --> | <!-- 分配、异常释放、仅清理自身资源 --> |

<!-- 本节定义唯一资源名称、隔离/独占方案和负责人；不涉及共享运行资源时记录依据。
     实际占用、释放、污染及重跑记录写入 verification 的 Runtime Resources。 -->

## Target Repository and Main Branch

- Code Repository: <!-- 代码仓库绝对路径 -->
- Target Kind / Ref: <!-- local 或 remote；本地填写 refs/heads/*，远端填写 remote、远端仓库及准确分支引用 -->
- Version Confirmation Owner: <!-- 核实目标当前提交的负责人；机械核实可交 environment/recon，目标选择仍由 main 确认 -->
- Confirmation Method / Evidence: <!-- 核实命令及目录、远端刷新或查询方式、实际提交与核实结果的记录位置 -->

## Merge Strategy

<!-- 选择 independent 或 integrated，写明主分支、集成分支/worktree、独立集成 Agent、合并顺序与
     交付单元组成：
     - independent：工作包相互独立，可单独运行和交付。
     - integrated：单元内存在接口、数据或交付依赖，或不确定能否独立交付。
     各模式的集成与合入流程见 apply instruction；同一变更可有多个不同模式的单元，各自生成一组
     Merge Unit 任务；就绪阶段只复核，变化先更新计划。 -->

| Delivery Unit / Mode | WP / TP | Owners: Implementation / Test / Review / Merge | Start / Readiness | Candidate Checks / Critical E2E | Order |
| --- | --- | --- | --- | --- | --- |
| <!-- 单元及 independent/integrated --> | <!-- 交付组成 --> | <!-- 各职责 --> | <!-- 契约、实现、资源各自条件 --> | <!-- 命令/检查 ID，关键需求路径 --> | <!-- 合入顺序 --> |

- Integration Branch / Worktree: <!-- 每个单元使用的独立集成分支或 worktree 绝对路径 -->
- Source Revision / Handoff: <!-- 固定源提交，以及已验收上游提交及其检查和 review 证据 -->
- Authorization / Report Path: <!-- 当前合并授权边界，以及集成报告的输出路径 -->

<!-- 每个交付单元依次完成候选验证、合入、主分支检查，再开始下一个；不沿用旧基线结论。
     主分支检查失败/受阻时停止后续功能合入，允许按相同验证路径交付修复。 -->

## Verification Strategy

### Local Checks

<!-- 开发中的快速检查及覆盖范围。 -->

### Project Verify

| Check ID | Stages / Work Packages | Command / Working Directory | Configuration / Environment | Scope / Pass Criteria | Evidence |
| --- | --- | --- | --- | --- | --- |
| <!-- 如 PV1，稳定 ID --> | <!-- 分支、集成、候选、主分支 --> | <!-- 完整命令或统一入口及子检查 --> | <!-- 工具/脚本/配置、运行资源 --> | <!-- 检查范围及通过条件 --> | <!-- 完整日志及子检查结果位置 --> |

### Additional Independent Validation

<!-- 仅需探索性测试或独立判断测试充分性时保留本节，否则删除；启用时按 roles/validator.md 的输入填写。 -->

- Assignment / Target: <!-- 验证任务或检查 ID、目标、范围、需求映射、固定版本与通过条件 -->
- Contracts / Evidence: <!-- 适用 specs、design、接口契约路径；中立证据的路径与版本 -->
- Execution / Output: <!-- 验证方法、隔离环境与资源、独占安排；独立报告及临时脚本目录 -->

### Code Review

<!-- 填写本次独立 reviewer 职责、各层范围、关注点、阻断标准及复核安排；引用 WP/TP 和 Check ID。
     reviewer 使用 roles/reviewer.md。 -->

### Main E2E

```yaml
mode: required # required | not-applicable；生成计划时必须明确选择
reason: "" # not-applicable 时必填：本项目和变更为何不适用
basis: "" # not-applicable 时必填：判断证据、项目约定或已有确认依据
alternative_checks: [] # not-applicable 时必填：覆盖风险的替代检查及验证方式
downgrade_approval: "" # x-agentic.e2e.enabled 为 true 或缺省且写 not-applicable 时必填：用户批准降级的原话、时间与来源
```

<!-- required：写明必须覆盖的需求/路径、候选关键范围和最终完整范围（最终完整范围为单入口聚合命令）。
     not-applicable：补齐四个字段，明确替代检查的范围、命令和证据，执行结果记为 NOT_APPLICABLE。
     项目开关开启（含缺省）时 mode 必须为 required；只有用户显式批准降级才可写 not-applicable，
     并在 downgrade_approval 留下可追溯的批准记录。适用性判断、环境缺失与降级判据见
     schema.yaml 的 plan instruction 与 procedures/acceptance.md，本计划不重复。 -->

#### E2E Ownership and Cases

<!-- required 时填写 TP 范围、角色、资源和设计任务；下表在 apply 中由主 Agent 汇总测试 Agent 的设计后补齐，
     不作为开始编码的前置条件。not-applicable 删除本节。用例与断言规则见 roles/tester.md。 -->

| Stage / Delivery Unit | Required Requirements / Paths | E2E IDs / Commands |
| --- | --- | --- |
| candidate / <!-- 单元 --> | <!-- 合入前关键覆盖；计划阶段确定 --> | <!-- apply 中由测试设计补齐 --> |
| final-main / all | <!-- 规定的完整覆盖，不以候选子集替代 --> | <!-- 完整 ID/执行命令 --> |

| E2E ID | Requirement / Scenario | Author / Work Package | Reviewer | Executor | Entry / Preconditions | Steps / Assertions | Case / Evidence Paths |
| --- | --- | --- | --- | --- | --- | --- | --- |
| <!-- 稳定 ID --> | <!-- 需求和正常/异常场景 --> | <!-- 用例编写者 --> | <!-- 独立检视者 --> | <!-- Agent 或人工执行人 --> | <!-- 真实入口、初始状态/数据 --> | <!-- 实际操作及可观察预期结果 --> | <!-- 用例/脚本、报告/截图/日志位置 --> |

#### E2E Execution Contract

- Target / Version: <!-- 阶段、交付单元、候选/主分支提交及基线、用例提交、构建产物标识/哈希、配置版本、环境/依赖版本、部署实例及版本识别方法 -->
- Startup / Readiness: <!-- 构建、启动命令及目录，依赖服务、就绪检查和超时 -->
- Driver / Execution: <!-- 已配置 x-agentic.e2e.command 时引用该入口（执行用 openspec-agentic e2e run --change <变更>）；否则写明已核实支持目标平台的浏览器/桌面操作工具和命令，或可复现人工步骤及执行人 -->
- Dependencies / Data: <!-- 真实服务、允许的外部测试替身、隔离账号/数据 -->
- Results / Cleanup: <!-- 逐用例断言/观察证据、报告与日志，重试记录及本次资源清理负责人/方法 -->

#### E2E Execution Waves

<!-- apply 中根据用例设计补齐，正式执行前确定完整分配；编写与执行分组可以不同。
     记录本轮固定版本、允许的分片资源命名空间差异及各分片独立报告路径。
     required 的最终完整 E2E 是单入口：分片在 x-agentic.e2e.command 内部并行，只写一条 stage=final 记录；
     候选阶段可按交付单元/分片分别留证，由主 Agent 汇总。 -->

| Stage / Unit / Wave / Shard | E2E IDs | Executor | Prerequisites | Fixed Versions | Isolated Resources / Exclusive Queue | Report Path |
| --- | --- | --- | --- | --- | --- | --- |
| <!-- 批次/分片 --> | <!-- 完整分配，无遗漏 --> | <!-- 独立测试 Agent 或指定人工 --> | <!-- 前置用例/资源 --> | <!-- 代码/用例/运行产物 --> | <!-- 实例、数据、会话等 --> | <!-- 分片独立路径 --> |

## Failure and Recovery

<!-- 失败后的修复负责人、传递下游失效、复测范围、主分支回滚条件与方法。
     写明共享运行资源异常释放及受污染检查的重跑方式。
     失败回路、暂停与恢复规则见 schema.yaml 的 apply instruction。
     E2E 连续失败达到 x-agentic.e2e.maxAttempts（默认 3）时必须停止自动重跑并交用户决策，
     记录本轮已尝试方案与证据；不得通过删除记录或提高上限自行续圈。 -->

## Completion Criteria

<!-- 明确 verification.md 的证据要求、最终主分支版本、阻断问题清零条件和 agentic-verify 入口。
     verification.md 从执行开始持续积累；存在计划不代表已获得合并、回滚或发布授权。 -->
