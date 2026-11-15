# ACPI v4 独立测试定义审查

日期：2026-11-08  
身份：独立 tester；只读审查，没有运行测试或构建。  
结论：**有条件接收 R01–R08 的测试定义作为后续真实 WSS 验收方案。此接收仅针对定义可执行性；不批准 Root 合入、生产编码、写集扩大或动态验收，也不代表新增 seam 已实现或已证明被动。**

## 审查对象

审查目录：`C:/Users/jack/.codex/visualizations/2026/10/30/01a18dc1-45b2-7f82-981e-964264221cef/session-loop-q003-p3b-acpi-route-pre-review-v3`

固定输入：

- `WRITESET-v4.md` — `147B70D2E6E2FBC85C51F68841EAD47A349FC49BFB239F2816FA11B6D65EC437`
- `spec-candidate-v4.md` — `E450C05ED0FE24D87A31264137441A3EF597CD0FB9E40234B13A162EAC39976A`
- `tests-candidate-v4.md` — `88C97333952D738F60D1FC44626D4755184E0FE61A2A7B08E962EAFC8AB6C773`
- `CONTRACT-v4.md` — `6BD506F56EC4B5AE7BA318E75C004C96463D9B692CF42F8998ECE6BA6B502C69`
- 最近独立 reviewer：`ACPI-V4-INDEPENDENT-REVIEW.md`，SHA-256 `B72C7B8EA6E91218D9D620C690E8A457AFFAF6312D4AB01C6CB92AD540C019F9`

同时只读查看了当前管理 WebSocket 接收循环、outbound writer/permit、socket attachment、Node upload owner 与 preparation hook 的实际实现，以核对新增观察点与现有所有权边界。未修改候选、Root spec、源码或 Git index；未运行 Cargo、测试、构建或 npm audit。

## 判定依据

R01–R08 都给出了真实 TLS/WSS fixture、独立分支连接、输入、预期帧/关闭码和资源收敛证据。定义不把 codec/owner 单测冒充路由验收。现有实现证实了以下可复用前提：管理连接以单一 `run_authenticated` 循环接收帧并轮询 `PendingReads`；出站由原 `Outbound` channel 与唯一 writer 发送；每个排队帧持有原 semaphore permit；writer 在认证文本发送前通过 actor gate；socket attachment 结束会封存原 lifetime；Node upload owner 绑定原 socket context；preparation 有真实 worker 和身份化 join 观察。

按此结构，合同中新增的 `#[cfg(test)]` barrier 和观察点可以保持生产语义，前提是它们严格保持被动，并符合以下边界：

- barrier 只在定义的线性化边界暂停当前原 future；不创建第二 owner、替代 writer、detached spawn、模拟业务帧或重放事件。
- 等待期间不持有 upload table、lifetime、socket actor gate、registry 或其他生产业务锁。release 后必须重新执行合同要求的 stop/actor/lifetime 复核。
- observer 只读原连接/owner/receipt 身份及计数，不推动状态，不用 `pending=0` 代替原任务 join；payload、密钥和正文不进入日志。
- `BeforeInputDispatch` 必须在 Node stop 优先选择后、decode/owner 调用前；stop case 释放后重新复核生命周期，才能证明 dispatch 次数为零和唯一 1001。
- `InputAcceptedBeforeAck` 必须允许原 `ConnectionStop` 取消等待；停止胜出即结束连接，不能让测试 barrier 阻塞 Node shutdown。
- outbound 观察必须区分 queued frame slot 与已出队的 in-flight frame；后者继续持有 byte permit。饱和只用真实幂等 Data 产生 ACK，byte 注入只持有原 semaphore permit。

## R01–R08 可执行性

- **R01：可执行。** 四命令、真实 ACK 字段、状态投影、control-only 和完整服务 bundle 安装均有明确输入/结果。重复与 foreign bundle 的 API 必须是测试专用、原子安装入口；不能为方便测试新增生产可变安装路径。
- **R02：可执行。** 同块重试、同 offset 内容冲突、跳 offset 与错误阶段各使用独立连接；可由有序 WSS 接收器确认零 ACK/JSON 及单一 4400，拒绝帧后以原 owner 被动观察确认 offset 不变和接收数据清理。
- **R03：可执行。** `CpuEntered` 能暂停原 preparation worker；先检查 preparing，再释放 worker、轮询真实 ready/failed。测试明确区分 status 驱动的 ready 投影与 worker join；需要 join 时要求同一 preparation identity 的原 shutdown port 收据。等待以通知/观测加 deadline，不靠 sleep 猜状态。
- **R04：可执行。** A/B 两条真实连接分别持有 owner；foreign 查询/释放与 foreign Binary 可验证不泄漏身份、不影响 A。Close 后继续验证 A 与第三连接使隔离具有正反证据。
- **R05：可执行。** ACPI magic 分类与合法 ACPV 控制分离；坏头、方向、保留位、offset、帧长及大帧各自明确预期关闭。定义要求读取完整消息序列，不以发现单个帧就跳过后续 Close/帧顺序。
- **R06：可执行，但实现接线是硬门。** v4 修复了停止优先测试的时序：先送坏帧并在处理前暂停，之后触发真实 Node stop、观测 stop，再释放并验证复核。四命令取消由 reader 收到 peer Close、观察 pending future drop 后释放无消费者 barrier；finish-after-call 另等原 receipt 注册与 join。Binary accept 到 ACK 的取消明确由 Node stop 触发，不假定忙 reader 同时还能读 Close。若生产实现不能在指定点以不持锁方式观察/复核，case 应报告阻塞/失败，不得更换为 owner 单测签字。
- **R07：可执行。** FIFO case 在 writer 已取走首个 ACK 后断言 `in_flight=1, queued=0, byte_used=32`，再入队文本并释放 writer；顺序断言由真实 WSS 完成。frame 上限通过 256 次真实最后块幂等重试填充 queued slot，下一次事件触发 4429；byte case 是有标注的原 semaphore 资源注入。两者没有把受控注入冒称自然网络故障。
- **R08：可执行。** ACK 在 actor gate 前暂停时，由 B 真实撤销并等 commit，再释放 writer，检查无 ACK、4410、协议结束后的原 owner seal 与原连接 join。反向顺序不持 gate 等撤销，避免自锁；B 继续成功提供隔离对照。目标非法值只能使用规范 ID 格式的不存在 target 来验 `invalid_target`；语法不可解析的 ID 应按 payload 格式错误断言 `invalid_request`。

## 仍须在编码/动态验收门处理的限制

1. spec 错误映射包含 `allocation_failed` 与 `necessary_storage_unavailable`，R01–R08 未给出这两类上传业务错误的确定性 WSS 注入。可以由未来独立验收明确将其限于 owner/codec 合同，或补充最小、可复现的 test-only 注入和场景；在这项决定之前，不应声称错误表全部获得路由级覆盖。
2. barrier、passive observer、上传 Binary 路由、Binary outbound 类型和服务 bundle test API 目前不是本审查所验证的实现。每个 seam 必须在允许的写集内由编码前 reviewer 审查，在候选固定后由独立 tester 对相同源摘要执行 R01–R08；若其暂停/观察本身改变生产调度、锁持有、发送顺序或生命周期，相关结果无效。
3. R07 的“256 条 queued 加 1 条 in-flight”要求精确的真实 ACK 计数与 writer 观察；对端 Close 的读取必须区分在饱和触发前已发出的 ACK 与 4429 Close。测试必须消费和记录有序消息，不得用忽略非目标帧的 helper。
4. 最近 reviewer 对 Root OpenSpec 写集提出的 design.md 同步缺口仍然有效。此 tester 接收测试定义不关闭该 finding；spec/proposal/tests/design 的文档闭合和 reviewer 合入门仍须单独完成。

## 最终结论

R01–R08 的场景、事件顺序、真实 WSS 观测及主要资源证据在静态层面足以执行；新增 seam 在严格遵守上述 `cfg(test)`、无锁暂停、被动身份观察、取消可达和真实单 writer 条件时，不会改变产品语义。因此**接收 v4 测试定义作为后续候选验收方案（有条件）**。

此结论没有运行任何测试，也不代表源码具备所需入口、seam 正确、测试通过、独立 reviewer 放行、Root 文档可合并、产品编码获准或完整 change 已完成。完整授权状态以 Root 的独立 reviewer 与写集门为准。
