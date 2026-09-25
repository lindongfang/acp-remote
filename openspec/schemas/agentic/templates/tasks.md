<!-- 英文分组标题、中文任务正文；保留 - [ ] X.Y，每项写 WP/TP 或单元、负责人、
     显式依赖、动作、完成条件及适用 ID/计划引用。按 plan.md 复制相关分组并重编号；
     生成与勾选规则见 schema.yaml 的 tasks/apply instruction。 -->

## 1. Dependency and Resource Setup

- [ ] 1.1 <!-- 全变更或交付单元；可选 environment/recon --> 使用新的任务级最小上下文核实仓库、目标引用、工具版本、约定命令及资源状态，返回结构化事实和证据；无需独立采集时删除并记录已有可信来源
- [ ] 1.2 <!-- WP ID --> 确认实现所需契约及文件所有权，记录编码起点；契约明确后允许并行编码，不等待上游实现完成
- [ ] 1.3 <!-- WP ID；可选 environment/runtime --> 分配检查所需隔离资源或确定共享资源独占队列，验证配置并记录释放方法；无共享资源时记录依据
- [ ] 1.4 <!-- WP ID；依赖所需上游交付检查 --> 在相关集成验证前接入全部已验收上游提交，核对下游实际基线与包含关系，关联 verification 的交接证据；无代码依赖时记录不适用依据

<!-- 1.1 按需生成；1.3/1.4 仅阻塞依赖对应资源/上游代码的任务。 -->

## 2. Implementation

<!-- 启动时完整读取并传入 roles/coder.md 及工作包输入；记录执行者、
     上下文方式、基线、写入范围和检查要求，单 Agent 串行实现也使用相同指令。 -->

- [ ] 2.1 <!-- WP ID、依赖、负责人、实现任务与局部验证方式 -->
- [ ] 2.2 <!-- WP ID、依赖、负责人、实现任务与局部验证方式 -->

<!-- Implementation 与 Test Design and Authoring 可并行；分组顺序不代表依赖。 -->

## 3. Branch Validation

<!-- 计划启用 Additional Independent Validation 时增加独立任务：按 roles/validator.md
     创建隔离产品编码对话的验证 Agent，交接固定输入，执行计划验证并记录报告及隔离证据。
     缺少隔离或必要证据时该任务 BLOCKED；未启用时不生成该任务，普通 Project Verify 无需该角色。 -->

- [ ] 3.1 <!-- WP ID --> 按计划 Check ID 完成交付前 project verify，逐项记录完整命令、版本/配置、实际资源、退出码及日志或有效复用依据，完成后核实释放检查资源
- [ ] 3.2 <!-- WP ID --> 新建不继承实现对话的只读 reviewer 子 Agent 检视同一版本，修复后由新子 Agent 复核；记录 Agent ID、版本、隔离方式和报告，可与 3.1 并行

## 4. Test Design and Authoring

<!-- required 按 TP ID 复制；not-applicable 删除。E2E ID 在测试设计后补入，
     仅依赖实际代码/资源的步骤等待对应交接。 -->
- [ ] 4.1 <!-- TP ID、独立测试 Agent --> 从需求设计场景、步骤和断言，提交需求映射及稳定 E2E ID，由主 Agent 汇总计划并协调歧义
- [ ] 4.2 <!-- TP ID --> 编写或复用用例、数据和启动/执行脚本，完成测试用例与脚本基础检查，记录提交、命令、范围及结果
- [ ] 4.3 <!-- TP ID --> 由非用例作者的新独立 reviewer 审查覆盖、真实入口和断言；修正后独立复核并记录证据

<!-- 基础检查指语法、类型、导入、配置/数据有效性、用例发现及辅助函数单元测试。
     收集或辅助检查会触发真实产品端到端操作时，属于 E2E，按候选/主分支执行协议调度。 -->

## 5. Integration Readiness

<!-- 5.1 是一次性任务，在首次批次集成前执行，按单元复制本组时不重复生成。
     其余任务按交付单元复制并标明单元 ID，仅依赖该单元所需实现、测试和上游交接，不全局等待无关工作包。
     依赖交接需要提前集成时，将该任务前移并重新编号。 -->
- [ ] 5.1（仅一次，不随单元复制）主 Agent 单独创建独立集成 Agent，显式交接 roles/integrator.md 全文、计划/契约、源提交及证据、独立集成 worktree、本地主分支及合入条件，记录实际 ID 及上下文方式；主 Agent 不兼任，缺少独立执行能力时相关任务 BLOCKED
- [ ] 5.2 <!-- 单元 ID、主 Agent --> 复核该单元预定模式及 WP/TP 组成，核对对应交付检查与独立 review 的有效证据；integrated 引用组合 verify/review 结果，independent 核对独立交付依据；变化先同步计划和依赖

## 6. Merge Unit

<!-- 每个交付单元复制一组，保留模式和全部 WP/TP；按计划逐组执行。
     计划要求中间 Main E2E 时在对应组增加任务。 -->
- [ ] 6.1 <!-- 单元 ID、版本确认负责人；依赖该单元就绪；机械核实可交 environment/recon --> 按计划核实目标仓库及主分支当前提交，记录准确引用及核实证据；无法确认目标时保持 BLOCKED
- [ ] 6.2 <!-- 单元 ID、集成负责人；依赖 6.1 --> 基于已核实基线构造本单元候选，固定基线和候选版本，记录组成与构建结果
- [ ] 6.3 <!-- 单元 ID、检查执行者；依赖 6.2 --> 按计划 Check ID 完成候选 Project Verify，将版本、范围、结果及有效复用依据关联到 verification
- [ ] 6.4 <!-- 单元 ID、独立 reviewer；依赖 6.2，可与 6.3 并行 --> 只读检视固定候选新增交互和冲突解决，修复后独立复核，记录隔离设置、版本及报告；待返回的检查证据交付前补齐核对
- [ ] 6.5 <!-- 单元 ID；依赖固定候选及所需资源 --> <!-- required：按执行者/分片拆分关键 E2E 任务及汇总任务，每条写明用 openspec-agentic e2e run --change <变更> --stage candidate 执行并写记录（人工路径用 --manual --by --evidence），完成条件是按 ID 汇总断言、版本与隔离证据并经主 Agent/独立 review 核对，不以 e2e check 为完成条件；not-applicable：核对理由、依据并完成适用替代检查。候选与最终的失败尝试共同计入 x-agentic.e2e.maxAttempts（默认 3）的连续失败上限；达到后停止自动重跑并交用户决策。生成时只保留所选路径 -->
- [ ] 6.6 <!-- 单元 ID、合并负责人；依赖候选各项通过 --> 确认候选检查、独立 review、required 关键 E2E 或适用替代验证通过，并核实仓库规则与本地主分支基线；以条件更新或串行合并机制防止竞态，直接合入计划中的本地主分支，无须再次询问用户，记录实际提交；版本核实由其他人负责时单列前置任务，基线变化时重开受影响候选任务
- [ ] 6.7 <!-- 单元 ID、检查执行者；依赖 6.6 --> 核对实际主分支结果与候选一致性，完成计划内必要 project verify 回归；有效复用逐项记录原证据及适用性，通过前不处理下一单元
- [ ] 6.8 <!-- 单元 ID、独立 reviewer；依赖 6.6 --> 独立检视合并新增差异；无新增差异由主 Agent 记录依据及原 review ID，不强制同范围重复审查，可与 6.7 并行；通过前不处理下一单元

## 7. Final E2E

<!-- required 保留执行、汇总、门禁三步；执行任务不以 e2e check PASS 为完成条件，
     tester 交付原始结果，只有 [e2e-owned] 行以 e2e check PASS 完成。
     不得把多个执行任务都设为“以 e2e check PASS 完成”。not-applicable 时将 7.1/7.2 改为实际替代验证/
     证据核对，7.3 保留唯一 [e2e-owned]。生成时删除未选路径、重编号并更新 Coverage Index。
     执行协议见 roles/tester.md，门禁与失败判据见 schema.yaml 的 apply instruction。 -->
- [ ] 7.1 <!-- 全变更、tester；依赖主分支检查 --> 固定共同版本与就绪资源，执行单入口 openspec-agentic e2e run --change <变更> --stage final，返回逐 ID/分片原始结果；测试 worktree 加 --planning-root <权威规划根>
- [ ] 7.2 <!-- 全变更、main；依赖 7.1 --> 主 Agent 汇总全部必要 ID 的断言、版本与隔离证据，处理问题并核实资源清理，全部必要检查通过后完成
- [ ] 7.3 [e2e-owned] <!-- 全变更、扩展；依赖 7.2 --> 运行 openspec-agentic e2e check --change <变更>，仅 PASS 自动勾选；此行只检查门禁，不执行测试或汇总

## 8. Final Verification

- [ ] 8.1 [final-verification] 使用 agentic-verify 执行最终验收（/opsx:verify 同样读取该入口），核对用户意图、需求、设计、计划、任务与最终主分支证据；记录当前 agentic-assessment 后运行 workflow check --stage final，全部通过才完成

<!-- 保留唯一 [final-verification]；验收期间仅该行可待办。失效任务重开并保留复验历史；
     不适用项记录理由和依据；/opsx:archive 不生成复选框。 -->
