<!-- 已有能力 local-agent-host 的增量规范：删除 Purpose（以主规范为准），只写增量 Requirements。 -->

## ADDED Requirements

### Requirement: 超限结束的失败关闭顺序

系统 SHALL 在因单条 stdout 消息超过上限而结束 Agent 时，先把该 Agent 标记为退出（`is_running()` 为假、该运行时不得再被复用），再把超限错误投递给等待中的请求；在错误对调用方可见的任何时刻，不得仍把该 Agent 当作「仍在运行」。

#### Scenario: 错误可见时 Agent 已被标记退出

- **WHEN** 一条超限 stdout 消息触发结束，且存在等待中的未完成请求
- **THEN** 调用方观察到超限错误时，该 Agent 的 `is_running()` 已为假，且同一运行时不会被后续请求复用

#### Scenario: 自然退出路径不受本顺序约束

- **WHEN** Agent 进程自然退出（而非因超限被结束）
- **THEN** 退出状态由既有的退出观察路径按原有时序标记，本要求的「先标记后投递」只约束超限结束路径
