# 公共 ACP 客户端

本模块实现 raw JSON、完整配置、按连接协商的 v1/v2 策略和受监督的单 reader/单 writer。协议层没有进程、数据库、Actor 授权或 Node/WSS 装配；这些数据类型与来源品牌都不是业务授权。

`Connection::spawn` 返回可克隆操作句柄、唯一 `EventReceiver` 和唯一 `IoOwner`。调用路径是 reserve → Reservation.encode → enqueue(应用私有 WriteFence) → Ticket/Received；响应与实际写入报告分开。Reservation 和 Ticket 的 Drop 有明确取消责任；错误 enqueue 归还所有权。`ExtensionFields::empty()` 返回 Result，调用方必须处理配置分配错误。

reader 在 writer 背压、外部 SQL 等待或 consumer 未 next 时继续接收。实际 poll_read 返回含 LF 的批次冻结本地 receipt 及原 RPC/read-window 资格；锁外 parse 不借后来的发送或恢复窗口补资格。Stop 封新 poll，保已取得完整帧，排空后才发布终止错误。shutdown 使用同一绝对期限实际 join；等待方取消或原 runtime 关闭后仍可由原 Core 槽继续收敛。joined 不代表原生执行成功。

每次真实 poll_write/poll_flush 都经过外部私有短围栏。零发送、部分帧 Unknown、完整帧接受、flush/Written 和 RPC response 是不同事实；部分异常封连接，未知不重发。外部 IO 提供者必须独占且不在后台或 Drop 自主首写。实际管道与 mock/buffered 写层证据分别记录。

RawMessage.fields 使用 raw_json 浅借用，无 Value DOM 兼容入口；完整未知字段、大数、转义和数字原词法保留。公共 ACP 链删除旧 32MiB/96 层/token/列表条数及 Message/Tool 输出身份长度门槛。保原 RPC 控制、双方64同时pending、Session/Option4096和原 list/replay/listing cwd/cursor 参数检查；这些是平台兼容差异，不是 SDK 上限。累计 server ID 不设寿命阈值，seen 防重不清空。pending_count 是可接受新动作的 live 关联数；关闭后为0，原不确定关联仍保在 shutdown 的 unresolved_requests。

SessionSetup 要求有版本和来源的完整 cwd/MCP/additionalDirectories 快照，Known(empty) 与 Unknown 分开；有序 env/header 和未知 raw 扩展不缩水。协议不扫描本机私有配置或验证路径授权。不支持的版本/能力仅限制对应路径。固定 SDK 2.1.0 stable schema：v1 `1c57095acf9c0844054dd1fdc3624c121b3fdc24209ecc987fd6748c705fdef5`，v2 `28c7251e820511da257cdb55d30e2c30bbbe59226305b04e3b751dc655e0778d`。

`view_observations` 与 `tool_projection` 为旧可选预算显示适配器。公开入口必须真实 Received、核相同连接来源；它们的 ViewWindows/history/anchor/key 预算尚未整体消除，不能成为真实 IO 必经门槛，也不声称长键完整历史投影。开发旧同步单元仅验证同生产私有数据算法/旧布局和计费；新增真实客户端用例另验公开品牌，测试不提供 fake Received。

对外约束见 [ACP 平台约束规格](../../../../openspec/specs/acp-platform-constraints/spec.md)，详细内部契约见归档 [public-acp-runtime-contract.md](../../../../openspec/changes/archive/2026-11-16-deliver-codex-session-loop/acp-runtime/revision2/public-acp-runtime-contract.md) 与 [empty-constructor.md](../../../../openspec/changes/archive/2026-11-16-deliver-codex-session-loop/acp-runtime/revision2-supplements/empty-constructor.md)。协议夹具和受控子进程的真实管道仅验证本模块协议与 IO 收敛边界，不替代真实 Harness、Windows Job 进程树、派发账本、持久审批或 Node/WSS 集成验证。

独立子历史使用当前连接初始化时双方声明的 `io.github.lindongfang.acp-remote/child-history` 版本 1。策略在 v1 的 `clientCapabilities._meta` 或 v2 的 `capabilities._meta` 发出声明，并分别从响应的 `agentCapabilities._meta` 或 `capabilities._meta` 读取确认。固定只读方法为 `_io.github.lindongfang.acp-remote/child-history`。当前连接返回 JSON-RPC method-not-found 后，后续查询直接报告能力限制。未实现扩展的来源仍可使用已保存的子快照及原生自动投递的子事件。

原连接按 RPC ID 接收每批结果。解码器验证根、子、快照、序号与游标，跨块拼接完整 ACP 通知并保留原始字节。根资源持有查询任务，获准的管理读者共享任务，各自复核权限。完整子快照与轮次索引在同一事务发布，续页通过平台快照令牌读取。受控 ACP v1/v2 的正向测试使用节点的管理 WSS 入口。扩展合同见[历史导航设计第 8 节](../../../../openspec/changes/archive/2026-11-21-deliver-history-navigation/design.md)。
