<!-- main 维护安排，tasks 写步骤，verification 写历史；协作判据见本次 plan instruction，角色细则见 roles/。 -->

<!-- 分阶段填写：生成 tasks 前定契约、WP/TP、依赖、职责、写范围、交付单元、目标、验证策略、E2E mode 和资源；
     对应交付前补用例、需求映射和独立审查；正式验证前补固定版本、就绪资源、分片和报告路径。
     后期字段不阻塞编码；工作包/检查/用例/资源分别引用 WP/Check ID/E2E ID/Runtime Resources 唯一标识。 -->

## Scope and Contracts

- Specs Revision: <!-- 本次增量规范的提交或内容摘要；skip_specs 时改写所依据既有契约的路径与版本 -->
- Design Revision: <!-- design.md 的提交或内容摘要 -->
- Convergence Check: <!-- 核对结论：一致，或冲突内容与解决结果、待澄清项及受影响下游 -->
- Skip Specs: <!-- 不适用写 no；跳过时写理由，并引用既有行为契约的路径与版本 -->

## Contract Changes

<!-- 澄清改变需求或接口时，更新当前契约引用、受影响安排，并同步 specs/design/tasks/用例。
     原/新值、原因、影响分析及证据失效或复用历史写入 verification 的 Check Plan Changes，本节只引用。 -->

## Coverage Index

<!-- 唯一索引 version: 1，覆盖及单元闭环判据见本次 plan instruction。
     路径相对权威 changeDir 或为绝对路径；source.heading 精确含 #，每个增量 Requirement/Scenario 一行，
     RENAMED 引用 ## RENAMED Requirements；skip_specs 引用既有契约实际路径/标题。
     同名 Scenario 用 source.requirement 填完整所属 ### Requirement: 标题，不重命名、不用行号。
     tasks 为字符串编号，生成任务后补齐；checks 在任务中用 [Check ID] 声明；基础设施/审查任务可多行共用。
     evidence 填原始报告路径，规划时可不存在，final 必须可读；结果只写 verification。
     target_ref 与下文已核实的本地 refs/heads/* 一致；最终 E2E 前固定索引。 -->
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

| Coverage ID | Responsible Units | Candidate Checks / Contribution | Closure Unit / Stage | Final Checks |
| --- | --- | --- | --- | --- |
| <!-- R1 --> | <!-- MU1，跨单元列全部 --> | <!-- 本单元实现与候选检查 --> | <!-- MU1/premerge 或 MU2/final --> | <!-- 最终 Verify/E2E ID --> |

## Work Packages

- Test Authoring Protocol: staged-v2

<!-- WP/TP 分别按 roles/coder.md、roles/tester.md 执行；required 按功能域/用户路径划分 TP1…TPn，
     不按 WP 划分，此处只填测试范围，不要求完整 E2E ID/场景。
     implement/fix 与交付单元在派发时确定；修复用原 WP ID、新实例，不预列修复包。
     Role 填 coder/tester，Owner 只填执行者 ID；池容量与并发按 Role 分组。
     测试池为 openspec/agentic.yaml 的 dispatch.pool.testing；旧缺 Role 且 Owner 不含 test 时归 coding。
     Dependencies 填类型和目标；Reviewer 与禁止整批依赖的判据见本次 plan instruction。 -->

| ID | Goal / Scenarios | Dependencies | Contract Freeze | Owner | Role | Reviewer | Branch / Worktree | Write Scope | Inputs / Outputs | Verification |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| WP1 | <!-- 场景引用 --> | <!-- none / code:WPn / contract:WPn / resource:Rn，逗号分隔 --> | <!-- 契约路径@版本；无 contract 依赖写 NOT_APPLICABLE --> | <!-- 执行者 ID --> | <!-- coder / tester --> | <!-- 独立审查者 --> | <!-- 路径 --> | <!-- 文件/目录前缀，逗号分隔 --> | <!-- 契约/产物 --> | <!-- Check ID --> |

- Dependency Declaration Review: <!-- 稳定 Review ID；按本次 plan instruction 在 propose 收尾独立核实 code/contract/resource 声明属实，结果与报告登记 verification 的同名节。 -->

<!-- code:WPn 表示缺上游已验收代码便无法产出/运行；contract:WPn 需可核对冻结；
     resource:Rn 指 Runtime Resources 中同一不可隔离资源。单元内/跨单元就绪判据见本次 plan instruction。
     Contract Freeze 填可读“路径@版本”；版本号供人核对，不做机械校验，缺省未冻结。
     本变更产出的契约须安排更早冻结前置（单独批次或上游硬交付）。 -->

## Execution Waves

<!-- 无 code/未冻结契约依赖为第 1 层，否则为上游最早层级最大值 + 1。
     晚于最早层级填 Serialization Reason（<码>: <证据路径>），final/archive 证据须可读；
     理由前提见本次 plan instruction，code-dependency/contract-unfrozen 仅用于相应上游排晚，resource-exclusive 须有 resource:Rn。
     不复制 dispatch.pool，同层可超池容量；状态/窗口只写 dispatch-queue.jsonl。
     逐包就绪、检视与资源准备按本次 plan instruction；返工按 procedures/scheduling.md。 -->

| Wave | Work Package | Enter Condition | Serialization Reason |
| --- | --- | --- | --- |
| W1 | <!-- WP1 --> | <!-- 契约冻结于 <路径@版本> / 资源就绪 --> | <!-- NOT_APPLICABLE 或 <码>: <证据路径> --> |

## Shared File Ownership

<!-- 登记所有写范围重叠的包（不限同层）的合并安排；不同区域允许并行，重叠不构成串行理由。
     工具识别路径重叠，独立 reviewer 判断是否同一处。 -->

| File | Writers (WP) | Merge Owner | Merge Order | Re-verify After Merge | Region Note |
| --- | --- | --- | --- | --- | --- |
| <!-- src/x.ts --> | <!-- WP2, WP3 --> | <!-- 合并负责人 --> | <!-- WP2 → WP3 --> | <!-- WP3: PV1 --> | <!-- 各改独立区域/函数 --> |

## Dependency Handoffs

| Downstream | Upstream | Accepted Revision / Evidence | Start Revision | Transfer / Inclusion Check | Invalidation |
| --- | --- | --- | --- | --- | --- |
| <!-- 下游 WP --> | <!-- 全部上游 WP --> | <!-- 规划时写验收条件；接入前引用 verification 中已验收提交 --> | <!-- 基线选择约定；接入前引用已确认起点 --> | <!-- 引入方式及包含关系检查 --> | <!-- 上游变化时受影响的传递下游及重验范围 --> |

<!-- 无代码依赖写不适用；本表引用 verification 的当前有效交接（实际提交、包含关系、证据失效/复用）。
     同单元下游填集成基线提交，跨单元须等上游合入后填主分支提交。 -->

## Runtime Resources

| Checks / Work Packages | Resource | Isolation / Configuration | Exclusive Scheduling | Allocate / Isolate / Start / Ready / Cleanup | Use / Cleanup Boundary |
| --- | --- | --- | --- | --- | --- |
| <!-- 使用者 --> | <!-- 数据库/schema、端口、容器、账号、可写缓存/目录、外部服务 --> | <!-- 独立命名空间及配置注入方法 --> | <!-- 无法隔离时的排队与独占方式 --> | <!-- 共享资源写 provisioner；未登记为共享且已分配给单一执行者的实例写该执行者 --> | <!-- 谁在已分配命名空间内使用与清理自身实例 --> |

<!-- 定唯一资源名、隔离/独占及逐资源负责人；无共享资源写依据。
     共享资源的分配/隔离/启动/就绪/清理由 provisioner 按 roles/provisioner.md 执行；执行者只操作已分配的自身隔离实例，禁连接他人实例/重置共享数据。
     Exclusive Scheduling 非空且非 NOT_APPLICABLE 即不可隔离，使用者须分批。
     占用、释放、污染、重跑写 verification 的 Runtime Resources。 -->

## Target Repository and Main Branch

- Code Repository: <!-- 代码仓库绝对路径 -->
- Target Ref: <!-- 已核实的本地主分支 refs/heads/*；不得以当前 HEAD 猜测 -->
- Version Confirmation Owner: <!-- 规划时的只读目标调查由 scout 执行；候选构建及合入瞬间的基线复核由 merger 执行，目标选择仍由 main 确认 -->
- Confirmation Method / Evidence: <!-- 核实命令及目录、目标分支的实际提交与核实结果的记录位置 -->

## Merge Strategy

<!-- 按单元填组成、顺序、复用 worktree 与合入职责；可混用 independent/integrated，
     模式选择、已验收集成基线及单元内/跨单元就绪判据见本次 plan instruction。 -->

| Delivery Unit / Mode | WP / TP | Owners: Implementation / Test / Review / Merge | Start / Readiness | Candidate Checks / Critical E2E | Order |
| --- | --- | --- | --- | --- | --- |
| <!-- 单元及 independent/integrated --> | <!-- 交付组成 --> | <!-- 各职责 --> | <!-- 契约、实现、资源各自条件 --> | <!-- 命令/检查 ID，关键需求路径 --> | <!-- 合入顺序 --> |

- Merge Worktree: <!-- 本单元复用的执行 worktree 绝对路径；由 provisioner 创建/回收，交接记录由 main 校验后写入 verification 的 ## Worktree Handoff，merger 不新建 -->
- Source Revision / Handoff: <!-- 固定源提交，以及已验收上游提交（或已验收集成基线提交）及其检查和 review 证据 -->
- Local Merge Conditions / Report Path: <!-- 候选门禁、仓库规则、基线复核和集成报告路径；满足后直接本地合入，无须重复询问 -->

<!-- 记录每个单元候选验证、合入和主分支检查的顺序；失败恢复规则见 apply instruction。 -->

## Verification Strategy

### Local Checks

<!-- 开发中的快速检查及覆盖范围。 -->

### Project Verify

| Check ID | Stages / Work Packages | Command / Working Directory | Configuration / Environment | Scope / Pass Criteria | Evidence |
| --- | --- | --- | --- | --- | --- |
| <!-- 如 PV1，稳定 ID --> | <!-- 分支、集成、候选、主分支 --> | <!-- 完整命令或统一入口及子检查 --> | <!-- 工具/脚本/配置、运行资源 --> | <!-- 检查范围及通过条件 --> | <!-- 完整日志及子检查结果位置 --> |

### Independent Validation

<!-- 至少一行、字段非空；validator 报告回写 verification 同名节，结果仅 PASS/FAIL/BLOCKED。
     最终固定版本及 [e2e-owned] 前置要求见本次 plan instruction。 -->

| Task | Target Revision | Assignment | Pass Condition | Report Path |
| --- | --- | --- | --- | --- |
| <!-- 任务 ID（tasks.md 的 [validation] 行） --> | <!-- 最终主分支固定版本与产物/环境版本 --> | <!-- 验证任务：覆盖充分性、待验证假设与风险盲区 --> | <!-- 通过条件 --> | <!-- 独立报告路径 --> |

### Code Review

<!-- 按 roles/reviewer.md 填职责、分层范围、关注点/阻断标准，引用 WP/TP、Check ID。
     候选只审 rebase 新冲突解决/交互；干净 rebase 且内容指纹不变可复用，须记依据与原报告 ID。 -->

### Main E2E

```yaml
mode: required # required | not-applicable；生成计划时必须明确选择
reason: "" # not-applicable 时必填：本项目和变更为何不适用
basis: "" # not-applicable 时必填：判断证据、项目约定或已有确认依据
alternative_checks: [] # not-applicable 时必填：覆盖风险的替代检查及验证方式
downgrade_approval: "" # e2e.enabled 为 true 或缺省且写 not-applicable 时必填：用户批准降级的原话、时间与来源
```

<!-- required 填最终完整覆盖；not-applicable 填适用字段和替代检查范围、命令、证据。
     模式与降级判据见 schema.yaml 的 plan instruction。 -->

## E2E Execution Plan（apply 阶段生成，执行前冻结）

<!-- plan 可留空；apply 测试设计产出、main 汇总，仅记录最终主分支 E2E。 -->

#### E2E Ownership and Cases

<!-- not-applicable 删除；按 roles/tester.md 分阶段补齐：设计填需求映射、入口、前置状态、步骤/断言；
     编写补可运行产物、非作者 Reviewer；执行前补 Executor、分片、固定运行版本、报告，不阻塞编写开工/交付。 -->

| Stage / Delivery Unit | Required Requirements / Paths | E2E IDs |
| --- | --- | --- |
| final-main / all | <!-- 规定的完整覆盖 --> | <!-- 完整 ID --> |

| E2E ID | Requirement / Scenario | Author / Work Package | Reviewer | Executor | Entry / Preconditions | Steps / Assertions | Case / Evidence Paths |
| --- | --- | --- | --- | --- | --- | --- | --- |
| <!-- 稳定 ID --> | <!-- 需求和正常/异常场景 --> | <!-- 用例编写者 --> | <!-- 独立检视者 --> | <!-- Agent 或人工执行人 --> | <!-- 真实入口、初始状态/数据 --> | <!-- 实际操作及可观察预期结果 --> | <!-- 用例/脚本、报告/截图/日志位置 --> |

#### E2E Execution Contract

- Target / Version: <!-- 阶段（final-main）、主分支提交及基线、用例提交、构建产物标识/哈希、配置版本、环境/依赖版本、部署实例及版本识别方法 -->
- Startup / Readiness: <!-- 构建、启动命令及目录，依赖服务、就绪检查和超时 -->
- Driver / Execution: <!-- 已配置 e2e.command 时引用该入口（执行用 openspec-agentic e2e run --change <变更>）；否则写明已核实支持目标平台的浏览器/桌面操作工具和命令，或可复现人工步骤及执行人 -->
- Dependencies / Data: <!-- 真实服务、允许的外部测试替身、隔离账号/数据 -->
- Results / Cleanup: <!-- 逐用例断言/观察证据、报告与日志，重试记录及本次资源清理负责人/方法 -->

#### E2E Execution Waves

<!-- 执行前冻结完整 ID 分配、固定版本、资源命名空间、各分片报告路径；人工 Command 填 manual。
     最终分片由单一项目命令聚合，仅一条 stage=final 记录。 -->

| Unit / Wave / Shard | E2E IDs | Command | Executor | Prerequisites | Fixed Versions | Isolated Resources / Exclusive Queue | Report Path |
| --- | --- | --- | --- | --- | --- | --- | --- |
| <!-- 批次/分片 --> | <!-- 完整分配，无遗漏 --> | <!-- 本分片实际命令或 manual --> | <!-- 独立测试 Agent 或指定人工 --> | <!-- 前置用例/资源 --> | <!-- 代码/用例/运行产物 --> | <!-- 实例、数据、会话等 --> | <!-- 分片独立路径 --> |

## Failure and Recovery

<!-- 填修复负责人、下游失效与复测范围、资源异常释放、回滚条件及方法；
     失败暂停和 E2E 重试上限见 schema.yaml 的 apply instruction。 -->

## Completion Criteria

<!-- 填最终主分支版本、verification 证据要求、阻断清零条件、agentic-verify 入口及下表交付标准。
     最终回复逐项交接位置、用法、接收方、完成/受阻依据，分别说明本地合入、远端交付、部署/发布、归档实际状态。
     可归档不等于已归档/发布；交付不扩大授权，未授权操作记未执行并提供本地产物/操作说明。 -->

## User Deliverables

<!-- 规划时按用户目标填写；工具至少明确安装包/构建产物或源码入口、运行命令、配置与使用说明。
     不需要的产物写理由；需远端交付或部署时记录已有授权来源及前置条件，缺授权不视为已执行。
     新增交付或使用说明任务同步 tasks；内部测试 PASS 不能代替用户完成标准。 -->
| Deliverable | Location / Version | Usage / Configuration | Recipient | Completion Standard | Authorization / Prerequisites |
| --- | --- | --- | --- | --- | --- |
| <!-- 用户可使用的产物 --> | <!-- 文件路径或交付位置及版本 --> | <!-- 安装、启动、配置、使用说明 --> | <!-- 用户/接收方 --> | <!-- 可核对的完成条件 --> | <!-- 当前范围/授权来源；未授权操作不得执行 --> |
