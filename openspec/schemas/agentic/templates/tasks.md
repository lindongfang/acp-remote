<!-- 英文分组标题、中文任务正文；保留 - [ ] X.Y，每项写 WP/TP 或单元、负责人、
     显式依赖、动作、完成条件及适用 ID/计划引用。按 plan.md 复制相关分组并重编号；
     生成与勾选规则见 schema.yaml 的 tasks/apply instruction。 -->

## 1. Dependency and Resource Setup

- [ ] 1.1 <!-- 任务 ID；固定任务：由 scout 执行，不得删除 --> 使用新的任务级最小上下文核实仓库、目标引用、工具版本、约定命令及资源状态，返回结构化事实（原始命令 + 输出）和证据
- [ ] 1.2 <!-- WP ID --> 确认实现所需契约、契约冻结版本、文件归属与合并负责人，记录编码起点；只要契约已冻结即可开工，不等待上游实现完成
- [ ] 1.3 <!-- 任务 ID；固定任务：由 provisioner 执行，不得删除 --> 按 plan.md 的资源判定分配/隔离资源或确定共享资源独占队列，创建并分配 worktree，验证配置并记录释放方法，并按 (WP, Attempt) 返回结构化交接记录（worktree、基线、本轮认领执行者、开工前接收时间）由 main 校验后写入 verification 的 `## Worktree Handoff`；资源判定为空时写 NOT_APPLICABLE: <依据>
- [ ] 1.4 <!-- WP ID；依赖所需上游交付检查 --> 在相关集成验证前接入全部已验收上游提交，核对下游实际基线与包含关系，关联 verification 的交接证据；无代码依赖时写 NOT_APPLICABLE: <依据>
- [ ] 1.5 <!-- 全变更或交付单元；存在 contract 依赖时必填 --> 冻结契约（或确认上游契约已冻结）并固定路径与版本，登记 plan.md 的 Shared File Ownership

<!-- 1.1 与 1.3 是固定任务，不得因“看起来不需要”而删除；1.4 仅当存在 upstream 交付依赖时生成；1.5 仅当存在 contract 依赖时生成。 -->

## 2. Implementation

<!-- 每个工作包复制一行，保留同行唯一 [wp:WPn]；派发与检视按 schema.yaml 的 tasks/apply instruction。
     受阻用 `--state blocked --reason <原因>` 释放窗口；计划拆包/删包/改 ID 时用
     `--state superseded --reason <原因>` 退役旧工作包。 -->

- [ ] 2.1 [wp:WP1] 派发 WP1 的 coder 子 Agent；开工前 `openspec-agentic dispatch --change <变更> --wp WP1 --executor <ID> --role coder`，交付后销毁并起 reviewer，review 通过后由主 Agent 勾选
      - WP1：<!-- 实现范围、依赖、负责人、局部验证方式 -->
- [ ] 2.2 [wp:WP2] 派发 WP2 的 coder 子 Agent；开工前 `openspec-agentic dispatch --change <变更> --wp WP2 --executor <ID> --role coder` 登记状态（coding）；依赖已合入且资源就绪时开工，不等其他工作包
      - WP2：<!-- 实现范围、依赖、负责人、局部验证方式 -->

## 3. Branch Validation

<!-- 每个工作包至少一条独立 review 任务（带 [CR…] 并提及该 WP；reviewer 不得是该 WP 的 coder）；独立验证见 ## 7。 -->

- [ ] 3.1 <!-- WP ID --> 按计划 Check ID 完成交付前 project verify，逐项记录完整命令、版本/配置、实际资源、退出码及日志或有效复用依据，完成后核实释放检查资源
- [ ] 3.2 <!-- WP ID --> 新建不继承实现对话的只读 reviewer 子 Agent 检视同一版本（与 coder 同一窗口，coder 已销毁）；修复由新的实现实例承担，修复后由新子 Agent 复核；记录 Agent ID、版本、隔离方式和报告，可与 3.1 并行

## 4. Test Design and Authoring

<!-- required 按 TP ID 复制；not-applicable 删除。E2E ID 在测试设计后补入，
     仅依赖实际代码/资源的步骤等待对应交接。not-applicable 时把计划 alternative_checks 的每个 ID 作为
     一条主 Agent 任务（带 [替代检查 ID]）执行，结果写入 verification 的 ## Checks。 -->
- [ ] 4.1 <!-- TP ID、独立测试 Agent --> 从需求设计场景、步骤和断言，提交需求映射及稳定 E2E ID，由主 Agent 汇总计划并协调歧义
- [ ] 4.2 <!-- TP ID --> 编写或复用用例、数据和启动/执行脚本，完成测试用例与脚本基础检查，记录提交、命令、范围及结果
- [ ] 4.3 <!-- TP ID --> 由非用例作者的新独立 reviewer 审查已列用例与需求映射、真实入口和断言；缺失用例与风险盲区由主 Agent 与 validator 负责，不由 reviewer 判最终覆盖；修正后独立复核并记录证据

<!-- 基础检查指语法、类型、导入、配置/数据有效性、用例发现及辅助函数单元测试（见 roles/tester.md）。 -->

## 5. Integration Readiness

<!-- 5.1 是一次性任务，在首次批次集成前执行，按单元复制本组时不重复生成。
     其余任务按交付单元复制并标明单元 ID，仅依赖该单元所需实现、测试和上游交接，不全局等待无关工作包。
     依赖交接需要提前集成时，将该任务前移并重新编号。 -->
- [ ] 5.1（仅一次，不随单元复制）主 Agent 单独创建独立合入 Agent，显式交接 roles/merger.md 全文、计划/契约、源提交及证据、本单元复用的执行 worktree、本地主分支及合入条件，记录实际 ID 及上下文方式；主 Agent 不兼任，缺少独立执行能力时相关任务 BLOCKED
- [ ] 5.2 <!-- 单元 ID、主 Agent --> 复核该单元预定模式及 WP/TP 组成，核对对应交付检查与独立 review 的有效证据；integrated 引用组合 verify/review 结果，independent 核对独立交付依据；变化先同步计划和依赖

## 6. Merge Unit

<!-- 每个交付单元复制一组，保留模式和全部 WP/TP；候选阶段不做 E2E。6.3 与 6.4 可并行，通过后才执行 6.5 覆盖核对。 -->
- [ ] 6.1 <!-- 单元 ID、合入负责人（merger）；依赖该单元就绪 --> 由 merger 在构建候选前机械核实目标仓库及主分支当前提交，记录准确引用及核实证据；规划时的只读目标调查属 1.1 的 scout，合入瞬间还要在 6.6 复核基线；无法确认目标时保持 BLOCKED
- [ ] 6.2 <!-- 单元 ID、合入负责人；依赖 6.1 --> 基于已核实基线构造本单元候选，固定基线和候选版本，记录组成与构建结果
- [ ] 6.3 <!-- 单元 ID、检查执行者；依赖 6.2 --> 按计划 Check ID 完成候选 Project Verify，将版本、范围、结果及有效复用依据关联到 verification
- [ ] 6.4 <!-- 单元 ID、独立 reviewer；依赖 6.2，可与 6.3 并行 --> 只读检视固定候选新增交互和冲突解决，修复后独立复核，记录隔离设置、版本及报告；待返回的检查证据交付前补齐核对
- [ ] 6.5 <!-- 单元 ID、主 Agent；依赖 6.3、6.4 --> 核对 plan.md 的 Coverage Index：每个需求/场景行都有实现证据与检查证据（判据来自计划，不按 proposal 逐个“看一遍”）；未覆盖的行不得勾选，重开受影响任务
- [ ] 6.6 <!-- 单元 ID、合入负责人；依赖 6.5 --> 先以 `agentic-premerge` 块运行 `workflow check --stage premerge` 并取得 PASS，再把该块持久化为版本化 receipt、在 verification 的 `## Premerge History` 记一行（交付单元、全部 WP/TP、目标/候选提交、PASS、receipt 路径），确认候选检查、独立 review、覆盖核对均通过，并核实仓库规则与本地主分支基线；以条件更新或串行合并机制防止竞态，直接合入计划中的本地主分支，无须再次询问用户，记录实际提交；版本核实由其他人负责时单列前置任务，基线变化时重开受影响候选任务
- [ ] 6.7 <!-- 单元 ID、检查执行者；依赖 6.6 --> 核对实际主分支结果与候选一致性，完成计划内必要 project verify 回归；有效复用逐项记录原证据及适用性，通过前不处理下一单元
- [ ] 6.8 <!-- 单元 ID、独立 reviewer；依赖 6.6 --> 独立检视合并新增差异；无新增差异由主 Agent 记录依据及原 review ID，不强制同范围重复审查，可与 6.7 并行；通过前不处理下一单元

## 7. Independent Validation

<!-- 每个变更保留唯一 [validation] 行；任务输入、隔离和完成条件见 schema.yaml 与 roles/validator.md。 -->

- [ ] 7.1 [validation] <!-- 全变更、validator；依赖最后一个 Merge Unit 组 --> 按 plan.md 的 ## Independent Validation 表在最终主分支固定版本执行验证，返回覆盖充分性结论、待验证假设判定、隔离方式与独立报告路径

## 8. Final E2E

<!-- required 保留执行、汇总、门禁三步，仅 [e2e-owned] 行以 e2e check PASS 完成；
     不得把多个执行任务都设为“以 e2e check PASS 完成”；not-applicable 时将 8.1/8.2 改为替代验证/证据核对，
     8.3 保留唯一 [e2e-owned]。生成时删除未选路径、重编号并更新 Coverage Index；执行协议见 roles/tester.md。 -->
- [ ] 8.1 <!-- 全变更、tester；依赖主分支检查 --> 固定共同版本与就绪资源，执行单入口 openspec-agentic e2e run --change <变更> --stage final（每轮仅一次聚合入口，不以 e2e check PASS 为完成条件），返回逐 ID/分片原始结果；测试 worktree 加 --planning-root <权威规划根>
- [ ] 8.2 <!-- 全变更、main；依赖 8.1 --> 主 Agent 汇总全部必要 ID 的断言、版本与隔离证据，处理问题并核实资源清理，全部必要检查通过后完成
- [ ] 8.3 [e2e-owned] <!-- 全变更、扩展；依赖 8.2 --> 运行 openspec-agentic e2e check --change <变更>，仅 PASS 自动勾选；此行只检查门禁，不执行测试或汇总

## 9. Final Verification

- [ ] 9.1 [final-verification] 使用 agentic-verify 执行最终验收（/opsx:verify 同样读取该入口），核对用户意图、需求、设计、计划、任务与最终主分支证据；记录当前 agentic-assessment 后运行 workflow check --stage final，全部通过才完成

<!-- 保留唯一 [final-verification]；验收期间仅该行可待办；失效任务重开并保留复验历史，不适用项记录理由和依据。 -->
