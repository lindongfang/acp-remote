<!-- 英文标题、中文正文，保留 - [ ] X.Y；每项写 WP/TP 或单元、负责人、显式依赖、动作、完成条件及 ID/计划引用。
     按 plan.md 复制分组、重编号并更新引用；生成/勾选规则见本次 tasks instruction 与 apply instruction。
     propose 收尾须先登记独立依赖声明审查，再通过 workflow check --stage plan，两者对当前 planningDigest 均 PASS。
     下列分组为 apply 执行任务；首次规划门禁不延后到资源准备，apply 只核对其仍有效。 -->

## 1. Dependency and Resource Setup

- [ ] 1.1 <!-- 固定任务，scout，不得删除 --> 用新的任务级最小上下文核实仓库、目标引用、工具版本、约定命令和资源，返回结构化事实、原始命令/输出及证据
- [ ] 1.2 <!-- WP ID --> 确认实现所需契约、契约冻结版本、文件归属与合并负责人，记录编码起点；只要契约已冻结即可开工，不等待上游实现完成
- [ ] 1.3 <!-- 固定任务，provisioner，不得删除；逐包准备，非全表屏障 --> 仅为依赖就绪且有池容量的包按 plan 分配/隔离资源或排独占队列，准备 worktree/依赖，检查就绪并记录释放方法；按 (WP, Attempt) 返回 worktree、基线、认领执行者、开工前接收时间，main 校验后登记 verification 的 `## Worktree Handoff` 即可开工；不预建后续包，无资源写 NOT_APPLICABLE: <依据>
- [ ] 1.4 <!-- WP ID；依赖上游交付检查 --> 集成验证前接入全部已验收上游，核对实际基线/提交包含关系并引用 verification 交接证据；无代码依赖写 NOT_APPLICABLE: <依据>
- [ ] 1.5 <!-- 全变更或交付单元；存在 contract 依赖时必填 --> 冻结契约（或确认上游契约已冻结）并固定路径与版本，登记 plan.md 的 Shared File Ownership

<!-- 1.4 仅有 upstream 交付依赖时生成，1.5 仅有 contract 依赖时生成。 -->

## 2. Implementation

<!-- 每包一行，保留同行唯一 [wp:WPn]；派发、接收、作者释放及同窗口独立检视按 tasks/apply instruction。
     开工登记 `openspec-agentic dispatch --change <变更> --wp <WP> --executor <ID> --role coder --require-ack`，作者/接管 reviewer 各自 --ack。
     受阻用 `--state blocked --reason <原因>` 释放窗口；计划拆包/删包/改 ID 时用
     `--state superseded --reason <原因>` 退役旧工作包。 -->

- [ ] 2.1 [wp:WP1] 派发 WP1 coder，按 apply 协议登记开工/接收；交付后销毁作者、同窗口新建独立 reviewer 并确认接收，review PASS 后 main 勾选
      - WP1：<!-- 实现范围、依赖、负责人、局部验证方式 -->
- [ ] 2.2 [wp:WP2] 派发 WP2 coder，按 apply 协议登记开工/接收；依赖已合入且资源就绪即开工，不等其他包
      - WP2：<!-- 实现范围、依赖、负责人、局部验证方式 -->

## 3. Branch Validation

<!-- 每个工作包至少一条独立 review 任务（带 [CR…] 并提及该 WP；reviewer 不得是该 WP 的 coder）；独立验证见 ## 7。 -->
<!-- 各 WP/TP 交付即独立启动自己的 reviewer，不等待同层所有编码或测试完成；任务分组不是批次屏障。
     每个包单独处理 PASS/FAIL/BLOCKED；返工及即时报告登记见 procedures/scheduling.md。 -->

- [ ] 3.1 <!-- WP ID --> 完成计划 Check ID 的交付前 Project Verify，逐项留完整命令、版本/配置、资源、退出码、日志或有效复用依据，确认资源释放
- [ ] 3.2 <!-- WP ID --> 作者销毁后，同窗口新建不继承实现对话的只读 reviewer 检视固定版本；修复/复核各用新实例，记录 Agent ID、版本、隔离和报告；可与 3.1 并行

## 4. Test Design and Authoring

<!-- required 按 TP ID 复制；not-applicable 删除。E2E ID 在测试设计后补入，
     仅依赖实际代码/资源的步骤等待对应交接。not-applicable 时把计划 alternative_checks 的每个 ID 作为
     一条主 Agent 任务（带 [替代检查 ID]）执行，结果写入 verification 的 ## Checks。 -->
- [ ] 4.1 <!-- TP ID、独立 tester --> 设计需求映射、稳定 E2E ID、场景/步骤/断言，以 phase: design / evidence_type: DESIGN 交付；main 汇总计划/协调歧义，不算编写完成
- [ ] 4.2 <!-- TP ID --> 交付可运行用例/数据/启动执行脚本或既定人工可复现步骤，完成基础检查；以 phase: design-author 登记 DELIVERY/CHECK，附 test_delivery（artifacts / basic_checks）、命令、零退出码和日志；Executor/分片在执行前填
- [ ] 4.3 <!-- TP ID --> 新建非作者 reviewer 审查已列用例的需求映射、真实入口和断言，修正后独立复核留证；缺失用例/盲区及最终覆盖交 main/validator

<!-- 基础检查指语法、类型、导入、配置/数据有效性、用例发现及辅助函数单元测试（见 roles/tester.md）。 -->

## 5. Integration Readiness

<!-- 5.1 是一次性任务，在首次批次集成前执行，按单元复制本组时不重复生成。
     其余任务按交付单元复制并标明单元 ID，仅依赖该单元所需实现、测试和上游交接，不全局等待无关工作包。
     依赖交接需要提前集成时，将该任务前移并重新编号。 -->
- [ ] 5.1（仅一次，不随单元复制）main 创建独立 merger，交接 roles/merger.md 全文、计划/契约、固定源提交/证据、单元复用 worktree、本地主分支/合入条件，记录实际 ID/上下文；main 不兼任，缺能力则 BLOCKED
- [ ] 5.2 <!-- 单元 ID、main --> 复核模式、WP/TP 组成及有效交付检查/review；integrated 引用组合 verify/review，independent 核对独立交付依据；变化先同步计划/依赖

## 6. Merge Unit

<!-- 每个交付单元复制一组，保留模式和全部 WP/TP；候选阶段不做 E2E。6.3 与 6.4 可并行，通过后才执行 6.5 覆盖核对。 -->
- [ ] 6.1 <!-- 单元 ID、merger；依赖单元就绪 --> 候选构建前机械核实目标仓库、主分支准确引用/当前提交并留证（区别于 1.1 scout 调查）；6.6 合入瞬间再核实，目标不明则 BLOCKED
- [ ] 6.2 <!-- 单元 ID、合入负责人；依赖 6.1 --> 基于已核实基线构造本单元候选，固定基线和候选版本，记录组成与构建结果
- [ ] 6.3 <!-- 单元 ID、检查执行者；依赖 6.2 --> 按计划 Check ID 完成候选 Project Verify，将版本、范围、结果及有效复用依据关联到 verification
- [ ] 6.4 <!-- 单元 ID、独立 reviewer；依赖 6.2，可与 6.3 并行 --> 只读审查固定候选新增交互/冲突解决，修复后独立复核，留隔离/版本/报告；交付前核对补齐的检查证据
- [ ] 6.5 <!-- 单元 ID、main；依赖 6.3、6.4 --> 核对 Coverage Index 本单元需求/场景的实现、候选 Project Verify/review 可追溯；跨单元核对本单元贡献/后续闭环，不索取未就绪单元或最终 E2E 证据；缺口重开任务
- [ ] 6.6 <!-- 单元 ID、merger；依赖 6.5 --> 以 `agentic-premerge` 块运行 `workflow check --stage premerge`，PASS 后持久化版本化 receipt，并登记 verification 的 `## Premerge History`（单元、全部 WP/TP、目标/候选提交、PASS、receipt 路径）；确认检查/review/覆盖通过，复核仓库规则/本地主分支基线，防竞态条件更新或串行合并，直接本地合入并记实际提交，无须再次询问；他人核实版本须列前置，基线变化重开候选任务
- [ ] 6.7 <!-- 单元 ID、检查执行者；依赖 6.6 --> 核对主分支与候选一致性、完成计划 Project Verify 回归；复用逐项记原证据/适用性，通过前不处理下一单元
- [ ] 6.8 <!-- 单元 ID、独立 reviewer；依赖 6.6 --> 审查合并新增差异；无差异由 main 记依据/原 review ID，不强制重复同范围审查；可与 6.7 并行，通过前不处理下一单元

## 7. Independent Validation

<!-- 每个变更保留唯一 [validation] 行；任务输入、隔离和完成条件见 schema.yaml 与 roles/validator.md。 -->

- [ ] 7.1 [validation] <!-- 全变更、validator；依赖最后一个 Merge Unit 组 --> 按 plan 的 ## Independent Validation 表验证最终主分支固定版本，交付覆盖充分性、假设判定、隔离方式及独立报告路径

## 8. Final E2E

<!-- required 保留执行、汇总、门禁三步，仅 [e2e-owned] 行以 e2e check PASS 完成；
     不得把多个执行任务都设为“以 e2e check PASS 完成”；not-applicable 时将 8.1/8.2 改为替代验证/证据核对，
     8.3 保留唯一 [e2e-owned]。生成时删除未选路径、重编号并更新 Coverage Index；执行协议见 roles/tester.md。 -->
- [ ] 8.1 <!-- 全变更、tester；依赖主分支检查 --> 固定共同版本/就绪资源，每轮一次聚合入口 openspec-agentic e2e run --change <变更> --stage final，不以 e2e check PASS 为完成条件，返回逐 ID/分片原始结果；测试 worktree 加 --planning-root <权威规划根>
- [ ] 8.2 <!-- 全变更、main；依赖 8.1 --> 主 Agent 汇总全部必要 ID 断言、版本/隔离证据，处理问题、确认资源清理，全部必要检查通过才完成
- [ ] 8.3 [e2e-owned] <!-- 全变更、扩展；依赖 8.2 --> 运行 openspec-agentic e2e check --change <变更>，仅 PASS 自动勾选；此行只检查门禁，不执行测试或汇总

## 9. Final Verification

- [ ] 9.1 [final-verification] 用 agentic-verify（亦为 /opsx:verify 入口）核对意图、需求/设计/计划/任务、全变更 Coverage Index（跨单元闭环/最终运行证据）、最终主分支证据及 User Deliverables 标准；记录当前 agentic-assessment，workflow check --stage final 全通过才完成，逐项交接产物/实际状态

<!-- 保留唯一 [final-verification]；验收期间仅该行可待办；失效任务重开并保留复验历史，不适用项记录理由和依据。 -->
