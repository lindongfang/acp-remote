# Tests

<!-- 独立tester编码前规划，reviewer检视。只写用例；结果另存testing/runs。
固定字段名用于结构校验，值用中文。ID跨版本稳定；Entry是命令或明确操作，不会被门禁自动执行。
每级required/not-applicable必须选一项并写理由；required至少一例。实际测试代码留所属工程。
从以下示例复制用例块并补齐，不能提交占位符。完整规则与工具策略见config.yaml。 -->

## Levels

| Level | Applicability | Reason |
| --- | --- | --- |
| L1 | required | 替换为本次类型/编译检查理由或not-applicable及原因 |
| L2 | required | 替换为静态/合同检查范围 |
| L3 | required | 替换为局部逻辑风险 |
| L4 | required | 替换为组件协作风险 |
| L5 | required | 替换为需求行为验收范围 |
| L6 | required | 替换为完整链路或不适用及替代检查 |
| LX | not-applicable | 替换为人工检查范围或无需人工的原因 |

## Cases

### CAP-001 用例标题

- Level: L5
- Requirement: 替换为spec文件及Requirement/Scenario名称；活动裸specs指增量，显式openspec/specs指长期，suite裸路径指长期；门禁只校验文件，reviewer核对场景与覆盖；skip_specs引用proposal.md的Success Criteria条目
- Executor: automation
- Tool: 替换为具体测试程序、浏览器工具或人工
- Preconditions: 替换为输入、账户/身份、状态和测试数据前提
- Environment: 替换为OS、依赖、真实/模拟边界和独立环境要求
- Steps: 替换为操作步骤，复杂步骤可引用下方四级标题或实际测试代码
- Expected: 替换为可观察结果及失败/边界断言
- Cleanup: 替换为本例资源归属、成功/失败/取消清理方式
- Schedule: acceptance,pr
- Entry: 替换为完整命令及工作目录，或Agent/人工可执行步骤

<!-- Executor允许automation/agent/human；LX须human，工具不可用记未执行。
Schedule允许acceptance/pr/daily/weekly/release/on-demand，逗号分隔。
用例新增/修改/废弃必须说明依据；长期版本注明来源变更，按能力合并，不复制执行结果。
运行报告记录目标版本、环境、逐ID结果和证据；必需失败/未执行不能交付。 -->
