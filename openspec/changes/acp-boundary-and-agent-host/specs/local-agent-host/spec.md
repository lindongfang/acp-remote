## Purpose

把本机 ACP 子进程托管为 `AgentCatalog`、`SessionBackendFactory` 与 `SessionEndpoint` 的实现，定义其目录暴露、会话与标识映射、turn 接受与事件交付、能力门控、交互转发、失败与取消、进程树清理、stderr 采集、凭据注入边界与空闲回收的可观察行为。

## ADDED Requirements

### Requirement: Agent 目录暴露与可用性判定

系统 SHALL 从本机配置的 Agent profile 派生 Agent 目录条目，并在该 profile 的启动前提（命令可解析、凭据引用可用）不满足时把对应条目标记为不可用并给出原因；目录查询 MUST NOT 启动任何 Agent 进程。

#### Scenario: 未启动进程也能列出 Agent

- **WHEN** 本机登记了两个 profile 且当前没有任何运行中的 Agent 进程
- **THEN** 目录返回两条条目及各自可用性，且过程中没有创建任何子进程

#### Scenario: 凭据引用失效只影响该条目

- **WHEN** 某个 profile 的凭据引用在 keystore 中缺失
- **THEN** 该条目为不可用并给出明确原因，其余 profile 的条目仍按自身状态返回

### Requirement: 会话创建与 ACP 会话标识映射

系统 SHALL 使用 core 已分配的会话标识发起一次 ACP 会话建立，并把 ACP 返回的会话标识与该 core 会话标识建立唯一映射；同一 core 会话 MUST 只对应一个运行中的 ACP 会话，重复建立 MUST 被拒绝。

#### Scenario: 新建会话完成映射并只启动一个进程

- **WHEN** core 以一个已解析的 workspace 与会话创建请求调用后端
- **THEN** 启动一个 Agent 子进程、完成 `initialize` 与 `session/new`，并记录 core 会话标识与 ACP 会话标识的唯一映射

#### Scenario: 相同 core 会话重复创建被拒

- **WHEN** 对同一个 core 会话标识再次请求创建
- **THEN** 返回冲突类错误，不启动第二个子进程，也不覆盖既有映射

#### Scenario: 会话恢复只接受已存在的映射

- **WHEN** core 请求打开一个本进程从未建立过的会话引用
- **THEN** 返回明确的不可用错误，不按猜测的标识向 Agent 发送请求

### Requirement: turn 的同步接受与流式事件交付

系统 SHALL 在向 Agent 发出 prompt 后同步返回「已接受」结果，并把随后的 ACP 通知按 Agent 发出顺序、以结构化事件交付给 core 的事件 sink；事件交付 MUST 保留类型判别子与结构化内容，并同时提供逐字节原始文档。

#### Scenario: prompt 先被接受，事件随后到达

- **WHEN** 一个 prompt 被接受后 Agent 依次发出多条 `session/update`
- **THEN** 调用方在事件到达之前已收到「已接受」结果，随后事件按 Agent 发出顺序逐个到达 sink

#### Scenario: 结构化事件不被文本化

- **WHEN** Agent 发出的更新包含 tool call、diff 或 terminal 内容块
- **THEN** 交付给 core 的事件保留同一结构化内容与类型判别子，并携带逐字节原文

### Requirement: 交互请求的转发与回传

系统 SHALL 把 Agent 发起的交互请求（权限请求与已宣告的 elicitation）作为需要外部解析的交互交付给 core，并使该请求在 Agent 侧保持未完成直到 core 给出解析结果；MUST NOT 代答、MUST NOT 按默认策略或授权范围自动允许或拒绝、MUST NOT 用超时伪造结论。

#### Scenario: 权限请求等待外部解析

- **WHEN** Agent 发出权限请求且 core 尚未给出解析结果
- **THEN** 该请求在 Agent 侧保持未完成，且没有任何自动回答被回传给 Agent

#### Scenario: 解析结果按原标识回传

- **WHEN** core 以「允许一次」「拒绝」或「取消」之一解析该交互
- **THEN** Agent 收到对应的 ACP 结果，且回传所用的请求标识与原始请求一致

### Requirement: 能力协商与可选调用门控

系统 SHALL 保存每次会话建立时 Agent 宣告的能力集合，并只在该能力确实被宣告时调用对应的可选 ACP 方法；未宣告的可选能力 MUST 返回显式不支持，MUST NOT 先调用再失败，MUST NOT 用默认值补齐。

#### Scenario: 未宣告的可选能力不被调用

- **WHEN** Agent 的 `initialize` 响应未宣告某个可选能力，而 core 请求该能力对应的操作
- **THEN** 返回显式不支持错误，且没有任何对应 ACP 消息被发往子进程

#### Scenario: 已宣告的能力按真实结果返回

- **WHEN** Agent 宣告了某个可选能力且 core 请求对应操作
- **THEN** 请求以该能力对应的方法发出，其结果按 Agent 的真实应答返回，不被改写为「不支持」

### Requirement: 模式与配置选项的读取与写入

系统 SHALL 把模式与配置选项的读取映射为 ACP 对应方法的真实结果；当 Agent 未宣告相应能力时，读取 MUST 返回空结果而非编造候选，写入 MUST 返回显式不支持。

#### Scenario: 未宣告时读取返回空而不是编造

- **WHEN** Agent 未宣告模式能力而 core 请求当前模式状态
- **THEN** 返回空候选列表且当前模式为空，不生成默认模式

#### Scenario: 未宣告时写入被显式拒绝

- **WHEN** Agent 未宣告配置选项能力而 core 请求设置某个配置值
- **THEN** 返回显式不支持错误，不静默成功也不修改本地状态

### Requirement: 取消与 turn 终态唯一

系统 SHALL 支持取消当前 turn，并在 Agent 确认或进程终止后把该 turn 收敛到唯一终态；取消 MUST NOT 产生第二份终态事件，MUST NOT 阻塞其他会话的执行。

#### Scenario: 取消后只有一份终态

- **WHEN** 一个进行中的 turn 被取消
- **THEN** 该 turn 只产生一次终态（取消或失败）事件，且该会话不再产生该 turn 的后续更新

#### Scenario: 取消不影响其他会话

- **WHEN** 一个会话的进行中 turn 被取消
- **THEN** 其他会话的进行中 turn 继续正常产生事件

### Requirement: 失败路径的明确结果与 turn 不设超时

系统 SHALL 对启动失败、短请求超时、Agent 异常退出、非法 JSON 以及乱序或未知请求标识的响应给出明确失败结果与可区分原因；`session/prompt` 所代表的 turn MUST NOT 因超时被终止，只有显式取消或进程退出才结束它。

#### Scenario: 启动超时失败并回收

- **WHEN** `initialize` 在 10 s 内没有完成
- **THEN** 返回启动超时错误并回收该进程树，不留下可继续写入的会话

#### Scenario: turn 不因超时被杀

- **WHEN** 一个 prompt 长时间未完成但 Agent 仍在正常产出更新
- **THEN** 系统不终止该 turn 也不杀掉进程，只有显式取消或进程退出才结束它

#### Scenario: 异常退出成为明确事件

- **WHEN** Agent 进程在 turn 进行中退出
- **THEN** 该会话收到明确的失败或退出事件，后续写入被拒绝，其进程与子进程被回收

#### Scenario: 未知响应标识不被误配

- **WHEN** 子进程返回一个与任何未完成请求都不匹配的请求标识，或返回无法解析的 JSON
- **THEN** 该响应被记为明确的协议错误，不解析为任何会话的结果，也不使既有未完成请求被错误完成

### Requirement: 进程树清理

系统 SHALL 在资源所属主机上清理完整的 Agent 子进程树：Windows 使用由 Daemon 持有并设置 `KILL_ON_JOB_CLOSE` 的 Job Object，Unix 使用进程组或等价机制；系统结束该 Agent 或关闭该句柄后，孙进程 MUST 不再存活。

#### Scenario: 关闭 Job 句柄后整棵树停止

- **WHEN** 关闭 Daemon 持有的 Job 句柄（或在 Unix 上终止进程组）
- **THEN** 该 Job 内的父进程与孙进程全部停止，且该判据作为常驻回归测试在目标平台执行

#### Scenario: 强制结束 Agent 时孙进程不残留

- **WHEN** 系统要求强制结束一个仍有孙进程在运行的 Agent（短请求超时、异常退出处理或显式关闭）
- **THEN** 孙进程与父进程一同停止，不留下孤儿进程继续占用资源

### Requirement: stderr 有界采集与结构化计数

系统 SHALL 把 Agent 的 stderr 采集进上限为 256 KiB 的环形缓冲并把丢弃字节数与采集总字节数记入结构化日志（stderr 内容本身不得进入日志，只按上限保留供诊断回取）；超出上限 MUST 丢弃最旧内容并记录丢弃计数，MUST NOT 无界缓存，MUST NOT 把 stderr 内容当作 ACP wire 解析。

#### Scenario: 超限丢弃最旧并计数

- **WHEN** Agent 写入的 stderr 总量超过 256 KiB
- **THEN** 只保留最近的 256 KiB，产生一条丢弃计数日志，且 stdout 上的 ACP 解析不受影响

#### Scenario: stderr 不进入协议通道

- **WHEN** Agent 在 stderr 上输出任意文本
- **THEN** 该文本不作为 ACP 消息被解析、转发或回传

### Requirement: profile 来源与凭据注入边界

系统 SHALL 只按本机配置端口提供的 profile 启动 Agent 进程（参数数组、不经 shell 拼接），并只注入 `env_allowlist` 与 profile 环境绑定声明的交集加必要进程环境；凭据值 MUST 只在启动前经凭据解析端口取得，引用失效或 keystore 不可用时 MUST 失败关闭；Node/Device 私钥与 token MUST NOT 出现在子进程环境、日志、事件或错误消息中；profile MUST 来自本机配置端口，MUST NOT 读取启动配置文件。

#### Scenario: 白名单是注入上限

- **WHEN** 一个 profile 的绑定集合包含不在其环境白名单内的变量名
- **THEN** 该变量不被解析也不被注入，白名单内且已绑定的变量按绑定注入

#### Scenario: 引用失效时失败关闭

- **WHEN** 启动前解析发现某个凭据绑定不可用
- **THEN** 进程不启动并返回不可用错误，磁盘、事件与日志中没有任何凭据值

#### Scenario: 不注入节点与设备密钥

- **WHEN** 任意 profile 成功启动
- **THEN** 子进程环境中不存在 ACP Remote 的 Node 或 Device 私钥与 token，日志与错误消息中同样没有

#### Scenario: 不使用启动配置文件中的 profile

- **WHEN** 启动配置文件里存在与库内 profile 同名或同命令的条目
- **THEN** profile 只来自注入的配置端口（本 crate 不读任何配置文件），同名文件条目不会成为 profile 来源，且目录查询确实经由该端口取 profile

### Requirement: 空闲回收与进程关闭顺序

系统 SHALL 只在该 Agent 没有任何进行中的 turn 且空闲时长超过配置的会话空闲超时（`0` 表示不因空闲关闭）时关闭其进程；关闭 MUST 按顺序停止写入、结束未完成请求、等待关闭 grace（5 s）后强制终止完整进程树，并确保不遗留无所有者的后台任务。

#### Scenario: 无活动会话且超过空闲超时时关闭

- **WHEN** 会话空闲超时非零、该 Agent 没有进行中的 turn 且空闲超过该值
- **THEN** 其进程树被关闭，且该运行时的既有会话映射不再被复用（后续对同一会话的 `open` MUST 显式失败，而不得把已关闭的进程当作活跃端点）；目录条目可用性 MUST NOT 因回收而改变（仍只由「命令可解析 + 凭据可解析」决定），下次请求按需重新启动（新进程代）

#### Scenario: 零值不因空闲关闭

- **WHEN** 会话空闲超时为 `0`
- **THEN** 进程不因空闲而被关闭，只在显式关闭、Daemon 关闭或进程退出时结束

#### Scenario: 关闭顺序有界且不留后台任务

- **WHEN** 对一个仍有未完成请求的 Agent 执行关闭
- **THEN** 写入被停止、未完成请求以明确结果结束、超过 5 s grace 后进程树被强制终止，且没有遗留的后台任务继续持有该进程
