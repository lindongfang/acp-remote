<!-- 按 proposal 的能力清单写增量行为与场景；技术方案见 design.md。
-->

## ADDED Requirements

### Requirement: 文件改动的转发现源限于类型化 Diff

后端 SHALL 把 ACP 工具调用内容中的类型化 Diff 元素作为文件改动事件的唯一来源，并在转发该工具调用时一并交付其 Diff 元素。转发 MUST 保留 Diff 元素的原始结构与逐字节原始文档，MUST NOT 为便于投影而先解析再重写该元素；MUST NOT 从工具调用的自由形状原始输入中猜测文件改动。不含 Diff 元素的工具调用 MUST NOT 被上报为文件改动。

#### Scenario: 含 Diff 的工具调用连同 Diff 元素一并交付

- **WHEN** Agent 的一次工具调用内容中包含一个文件 Diff 元素
- **THEN** 后端把该工具调用连同该 Diff 元素交付给事件sink，且该次交付的原始文档与 Agent 发出的字节一致

#### Scenario: 不含 Diff 的工具调用不构成文件改动

- **WHEN** Agent 的一次工具调用内容中只有普通内容块与终端引用
- **THEN** 后端不把它上报为文件改动，也MUST NOT 从其自由形状原始输入中推断出文件改动

#### Scenario: 不解析原始输入以推断改动

- **WHEN** 某次工具调用的自由形状原始输入中包含看似文件编辑的结构，但该调用内容中没有 Diff 元素
- **THEN** 后端不据此派生文件改动事件

### Requirement: profile 进程的连接生命周期可被观察

后端 SHALL 在某个 Agent profile 的进程首次建立并可服务会话、以及该进程退出（包括被空闲回收终止）时，分别上报一次该 profile 的连接与断开，使该生命周期可被观察。该生命周期归属于 profile 的进程而非任何单个会话：同一 profile 的进程被多个会话复用时，MUST NOT 因会话的建立或活跃而重复上报连接。被复用的既有进程 MUST NOT 被当作新连接。进程超限退出时，断开上报 MUST NOT 早于该运行时被判定为已退出。

#### Scenario: 首次建立时上报一次连接

- **WHEN** 某个 Agent profile 的进程首次建立并可服务会话
- **THEN** 后端上报一次该 profile 的连接，且此后该进程被多个会话复用时不再重复上报

#### Scenario: 复用既有进程不重复上报

- **WHEN** 另一个会话复用了一个已经建立并正在服务其它会话的既有进程
- **THEN** 后端不因这次复用上报新的连接

#### Scenario: 退出时上报一次断开

- **WHEN** 某个 Agent profile 的进程退出，或被空闲回收按既有顺序终止
- **THEN** 后端上报一次该 profile 的断开，并可在该次上报中附带退出错误信息

#### Scenario: 超限退出时断开不早于退出判定

- **WHEN** 后端因单条 stdout 消息超限而结束某个 Agent
- **THEN** 断开上报发生在该运行时被标记为已退出之后，MUST NOT 在调用方仍可能认为它运行时提前上报断开