<!-- 按 proposal 的能力清单写增量行为与场景；技术方案见 design.md。
-->

## ADDED Requirements

### Requirement: 关闭时的连接排空

`daemon.stop` 被接受、且接入层不再接受新连接之后，Daemon SHALL 在一个由 `graceMs` 界定的排空窗口内等待**在途客户端连接**结束，随后才继续关闭序列的后续步骤。**发出该 `daemon.stop` 的那条连接 MUST NOT 被计入排空等待**：它的语义是「请求关闭」，不是「正在进行业务」；把它计入会使排空时长取决于该客户端进程何时被调度，而不是取决于服务端有多少在途工作。排空窗口结束时仍未结束的连接 SHALL 被关闭，并留下 `daemon.drain_timeout` 的可观察记录。

#### Scenario: 关闭请求自身的连接不进入排空

- **WHEN** 唯一的活动客户端连接就是发出 `daemon.stop` 的那条，且该客户端在收到 `accepted` 之后迟迟不关闭连接（例如进程被延迟调度）
- **THEN** 排空不等待它、也不因此等到宽限耗尽：**排空阶段**在远小于 `graceMs` 的时间内结束，且不出现 `daemon.drain_timeout`（判据取日志里 `daemon.drain` 的 `drain_ms`；整个关闭序列还含与排空无关的阶段，不属本契约）

#### Scenario: 其它在途连接仍被等待

- **WHEN** 除关闭请求者之外还有另一条在途客户端连接，且它在宽限内结束
- **THEN** 排空等到它结束才继续推进，且不出现 `daemon.drain_timeout`

#### Scenario: 宽限耗尽仍关闭剩余连接

- **WHEN** 某条在途客户端连接在宽限内始终不结束
- **THEN** 该连接在宽限结束时被关闭，并留下 `daemon.drain_timeout` 的可观察记录；关闭序列继续推进而不是永久等待
