# Delta: local-agent-host

<!-- 按 proposal 的能力清单写增量行为与场景；技术方案和执行安排分别见 design.md、plan.md。 -->

## MODIFIED Requirements

### Requirement: 失败路径的明确结果与 turn 不设超时

系统 SHALL 对启动失败、短请求超时、Agent 异常退出、非法 JSON 以及乱序或未知请求标识的响应给出明确失败结果与可区分原因；`session/prompt` 所代表的 turn MUST NOT 因超时被终止，只有显式取消或进程退出才结束它。

#### Scenario: 启动超时失败并回收

- **WHEN** `initialize` 在 10 s 内没有完成
- **THEN** 返回启动超时错误并回收该进程树，不留下可继续写入的会话

#### Scenario: 进程无法启动时返回明确不可用

- **WHEN** 会话创建或打开指向一个程序不存在或不可执行的 Agent profile，spawn 本身失败
- **THEN** 请求以明确的不可用类错误（区别于请求非法与凭据不可用）失败，不启动任何进程，也不留下可复用的运行时或会话映射；同一 Agent 的后续重试不受本次失败影响

#### Scenario: turn 不因超时被杀

- **WHEN** 一个 prompt 长时间未完成但 Agent 仍在正常产出更新
- **THEN** 系统不终止该 turn 也不杀掉进程，只有显式取消或进程退出才结束它

#### Scenario: 异常退出成为明确事件

- **WHEN** Agent 进程在 turn 进行中退出
- **THEN** 该会话收到明确的失败或退出事件，后续写入被拒绝，其进程与子进程被回收

#### Scenario: 未知响应标识不被误配

- **WHEN** 子进程返回一个与任何未完成请求都不匹配的请求标识，或返回无法解析的 JSON
- **THEN** 该响应被记为明确的协议错误，不解析为任何会话的结果，也不使既有未完成请求被错误完成
