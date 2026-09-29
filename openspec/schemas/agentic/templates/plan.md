<!-- 主 Agent 维护协作计划；规则见 schema.yaml，角色细节见 roles/。
     本文件写安排，tasks 写步骤，verification 写执行历史。 -->

<!-- 分阶段填写，后期信息不作为开始编码的前置条件：生成 tasks 前定契约、WP/TP、依赖、职责、写入范围、
     交付单元、目标分支、验证策略、E2E 适用性与资源需求；对应交付前补测试用例、需求映射及独立审查安排；
     正式验证前补固定版本、就绪资源、执行分片与报告路径。
     工作包、检查、用例和资源分别引用 Work Packages、Check ID、E2E ID、Runtime Resources 的唯一标识。 -->

## Scope and Contracts

<!-- 记录 specs/design 的版本及共同收敛结论；skip_specs 时记录既有契约依据。 -->

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
     target_ref 与 Target Repository and Main Branch 一致，填写已核实的本地 refs/heads/* 引用。
     tasks 生成后补齐编号；进入最终 E2E 前固定索引。 -->
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

<!-- WP Owner 按 roles/coder.md、TP Owner 按 roles/tester.md 执行；TP 按功能域/用户路径划分，与 WP 无关。
     阶段（implement / fix）与交付单元在派发时确定，review 或测试发现触发的修复按原 WP ID 重新派发（新的实现实例），不预先列入本表。
     Role 填 coder 或 tester（并发探测与池容量都按它分组）；Owner 只填执行者 ID（如 coder-A），两者不要混写。
     要用测试流水线（openspec/agentic.yaml 的 dispatch.pool.testing）必须把 Role 写成 tester（或让 Owner 含 test）；
     否则缺列且 Owner 不含 test 时会归入 coding 车道，testing 上限不会生效。
     required 时按功能域/用户路径增加 TP1…TPn；此处填写测试范围即可，不要求完整 E2E ID/场景清单。
     Dependencies 只写类型与目标（code / contract / resource），不写“整批依赖整批”；Reviewer 判据见 schema.yaml 的 plan instruction。 -->

| ID | Goal / Scenarios | Dependencies | Contract Freeze | Owner | Role | Reviewer | Branch / Worktree | Write Scope | Inputs / Outputs | Verification |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| WP1 | <!-- 场景引用 --> | <!-- none / code:WPn / contract:WPn / resource:Rn，逗号分隔 --> | <!-- 契约路径@版本；无 contract 依赖写 NOT_APPLICABLE --> | <!-- 执行者 ID --> | <!-- coder / tester --> | <!-- 独立审查者 --> | <!-- 路径 --> | <!-- 文件/目录前缀，逗号分隔 --> | <!-- 契约/产物 --> | <!-- Check ID --> |

- Dependency Declaration Review: <!-- 独立 reviewer ID；以示 phase: plan / stage: plan 核实 code / contract / resource 声明属实（code 是否真需要对方代码、契约是否真已冻结、资源是否真不可隔离）。开工前门禁：结果与报告见 verification 的 ## Dependency Declaration Review（Reviewer 非 Owner、Review ID + Round、Plan Revision = contractDigest、报告可读）；contractDigest 变化后重新派发。 -->

<!-- Dependencies 带类型：code:WPn 指未拿到上游已验收代码就无法产出或运行；contract:WPn 需 Contract Freeze 可核对；
     resource:Rn 指同一份不可隔离资源（Rn 须在 Runtime Resources 声明）。
     就绪判据按交付单元区分：同单元 code 依赖要求上游已进入“已验收集成基线”（固定提交 + 包含上游交付提交 +
     必要 Project Verify 与独立 review 已通过 + main 已接收；仅 merger 建了个集成提交不算，且不得为启动下游
     提前把上游标为 merged）；跨单元 code 依赖要求上游已合入主分支。
     Contract Freeze 写“路径@版本”且路径可读才视为已冻结；版本号只是人读标识，不做机械校验；
     缺省答案是“未冻结”，且契约由本变更产出时冻结动作本身要作为更早的前置（单独批次或上游硬交付）。 -->

## Execution Waves

<!-- 静态层级：无 code/未冻结契约依赖为第 1 层；否则为上游最早层级最大值 + 1。
     晚于最早层级须填 Serialization Reason，格式为“<码>: <证据路径>”，final/archive 时证据须可读。
     code-dependency / contract-unfrozen 仅在相应上游排晚时使用；resource-exclusive 须有 resource:Rn 依赖；
     capacity 须满足池上限 < 2，或同最早层级同角色工作包数 > 池上限。
     `openspec/agentic.yaml` 的 dispatch.pool 不复制到计划；同层工作包可多于池容量。
     运行期状态与并发窗口由 dispatch-queue.jsonl 记录，不填本表。 -->

| Wave | Work Package | Enter Condition | Serialization Reason |
| --- | --- | --- | --- |
| W1 | <!-- WP1 --> | <!-- 契约冻结于 <路径@版本> / 资源就绪 --> | <!-- NOT_APPLICABLE 或 <码>: <证据路径> --> |

## Shared File Ownership

<!-- 写入范围有重叠的工作包必须在此登记合并安排（运行期按就绪开工，不限于同一层级）；同一文件的不同区域允许并行，
     写入重叠本身不构成串行理由。路径重叠由检查识别，“是否同一处”由独立 reviewer 判断。 -->

| File | Writers (WP) | Merge Owner | Merge Order | Re-verify After Merge | Region Note |
| --- | --- | --- | --- | --- | --- |
| <!-- src/x.ts --> | <!-- WP2, WP3 --> | <!-- 合并负责人 --> | <!-- WP2 → WP3 --> | <!-- WP3: PV1 --> | <!-- 各改独立区域/函数 --> |

## Dependency Handoffs

| Downstream | Upstream | Accepted Revision / Evidence | Start Revision | Transfer / Inclusion Check | Invalidation |
| --- | --- | --- | --- | --- | --- |
| <!-- 下游 WP --> | <!-- 全部上游 WP --> | <!-- 规划时写验收条件；接入前引用 verification 中已验收提交 --> | <!-- 基线选择约定；接入前引用已确认起点 --> | <!-- 引入方式及包含关系检查 --> | <!-- 上游变化时受影响的传递下游及重验范围 --> |

<!-- 无代码依赖时写明不适用。实际提交、包含关系检查结果和旧证据的失效/复用判断保存在
     verification 的 Dependency Handoffs；本表只引用当前有效交接。
     同单元下游写集成基线提交；跨单元下游须等上游已合入主分支后再写主分支提交。 -->

## Runtime Resources

| Checks / Work Packages | Resource | Isolation / Configuration | Exclusive Scheduling | Allocate / Isolate / Start / Ready / Cleanup | Use / Cleanup Boundary |
| --- | --- | --- | --- | --- | --- |
| <!-- 使用者 --> | <!-- 数据库/schema、端口、容器、账号、可写缓存/目录、外部服务 --> | <!-- 独立命名空间及配置注入方法 --> | <!-- 无法隔离时的排队与独占方式 --> | <!-- 共享资源写 provisioner；未登记为共享且已分配给单一执行者的实例写该执行者 --> | <!-- 谁在已分配命名空间内使用与清理自身实例 --> |

<!-- 本节定义唯一资源名称、隔离/独占方案和逐资源负责人（分配/隔离/启动/就绪/清理）；不涉及共享运行资源时记录依据。
     共享资源的分配/隔离/启动/就绪/清理由 provisioner 执行（见 roles/provisioner.md），执行者只在已分配的
     隔离资源内操作自身实例，不得连接他人实例或重置共享数据。
     Exclusive Scheduling 非空且非 NOT_APPLICABLE 表示不可隔离，使用者必须分属不同批次。
     实际占用、释放、污染及重跑记录写入 verification 的 Runtime Resources。 -->

## Target Repository and Main Branch

- Code Repository: <!-- 代码仓库绝对路径 -->
- Target Ref: <!-- 已核实的本地主分支 refs/heads/*；不得以当前 HEAD 猜测 -->
- Version Confirmation Owner: <!-- 规划时的只读目标调查由 scout 执行；候选构建及合入瞬间的基线复核由 merger 执行，目标选择仍由 main 确认 -->
- Confirmation Method / Evidence: <!-- 核实命令及目录、目标分支的实际提交与核实结果的记录位置 -->

## Merge Strategy

<!-- 每个交付单元选择 independent（可独立交付）或 integrated（存在交付依赖或独立性未确定）；
     同一变更可混用模式。按单元填写组成、顺序、复用的 worktree 与合入职责。
     integrated 单元可提前组合已验收上游形成固定集成基线；同单元下游 code 依赖从该基线开工，
     跨单元依赖仍须等上游合入主分支。“已验收”= 实现检查与独立 review 通过 + 基线包含其交付提交 +
     main 已接收；仅 merger 建了个集成提交不算。 -->

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

<!-- 本节必须至少一行且字段全部非空；报告由 validator 产出，结果回写 verification.md 的同名节，
     结论只允许 PASS / FAIL / BLOCKED，且验证须早于 [e2e-owned] 门禁行完成。 -->

| Task | Target Revision | Assignment | Pass Condition | Report Path |
| --- | --- | --- | --- | --- |
| <!-- 任务 ID（tasks.md 的 [validation] 行） --> | <!-- 最终主分支固定版本与产物/环境版本 --> | <!-- 验证任务：覆盖充分性、待验证假设与风险盲区 --> | <!-- 通过条件 --> | <!-- 独立报告路径 --> |

### Code Review

<!-- 填写本次独立 reviewer 职责、各层范围、关注点与阻断标准，引用 WP/TP 和 Check ID；reviewer 使用 roles/reviewer.md。
     合入前候选只审 rebase 新产生的冲突解决与新增交互；rebase 干净且改动内容指纹未变时可复用原检视结论，
     记录复用依据与原报告编号。 -->

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

<!-- apply 阶段由测试设计产出、主 Agent 汇总，最终执行前冻结；plan 阶段可留空。仅记录最终主分支 E2E。 -->

#### E2E Ownership and Cases

<!-- apply 阶段由主 Agent 汇总测试 Agent 的设计后补齐；not-applicable 删除本节；用例与断言规则见 roles/tester.md。 -->

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

<!-- apply 阶段生成、执行前冻结：按用例设计补齐，正式执行前确定完整 ID 分配、固定版本、资源命名空间和各分片报告路径。
     人工路径在 Command 写 manual。最终分片由单一项目命令内部聚合，只留一条 stage=final 记录。 -->

| Unit / Wave / Shard | E2E IDs | Command | Executor | Prerequisites | Fixed Versions | Isolated Resources / Exclusive Queue | Report Path |
| --- | --- | --- | --- | --- | --- | --- | --- |
| <!-- 批次/分片 --> | <!-- 完整分配，无遗漏 --> | <!-- 本分片实际命令或 manual --> | <!-- 独立测试 Agent 或指定人工 --> | <!-- 前置用例/资源 --> | <!-- 代码/用例/运行产物 --> | <!-- 实例、数据、会话等 --> | <!-- 分片独立路径 --> |

## Failure and Recovery

<!-- 填修复负责人、下游失效与复测范围、资源异常释放、回滚条件及方法；
     失败暂停和 E2E 重试上限见 schema.yaml 的 apply instruction。 -->

## Completion Criteria

<!-- 填最终主分支版本、verification.md 证据要求、阻断清零条件和 agentic-verify 入口。 -->
