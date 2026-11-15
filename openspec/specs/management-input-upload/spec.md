# management-input-upload Specification

## Purpose

定义管理 WebSocket 的 ACPI 输入上传、命令结果、确认帧、错误关闭、撤销与取消行为。本文件是 `deliver-codex-session-loop` 的 `management-input-upload` 活动增量；`acpi-route-v10` 已通过独立 tester/reviewer 的 Rust 认证 WSS 路由编码门。上传复用节点已有能力，不新增 Harness 能力准入或私有替代；上传完成不等于队列受理、Harness 派发或执行完成。

## Requirements

### Requirement: 上传服务原子可用

系统 SHALL 在开始接受正常产品管理连接前，原子地发布属于同一节点的队列读取与输入上传能力；不得向客户端暴露部分安装后假成功的产品状态。缺失服务、节点身份不匹配或重复安装失败 MUST 明确失败，不能启动一个半就绪的正常产品入口。仅提供控制面的节点不提供输入上传，上传命令返回 `product_not_ready`。

#### Scenario: 服务无法完整发布

- **WHEN** 产品服务不能作为同一节点的完整能力集合成功发布
- **THEN** 产品启动明确失败，不开放半就绪产品入口，也不发布伪造 uploadId

#### Scenario: 仅控制面启动

- **WHEN** 节点以仅控制面模式接受已认证管理连接
- **THEN** 四条上传命令返回 `product_not_ready`，不返回伪造 uploadId；控制面状态按既有规则报告

### Requirement: 连接持有唯一上传所有权

系统 SHALL 在认证连接内维持唯一上传所有权，后续四条命令与 ACPI 数据均使用同一所有权。uploadId 仅在创建它的连接有效，不能跨连接或断线后接管。连接结束 MUST 清理尚未移交准备阶段的接收数据；已移交的数据由节点继续按原准备事实收敛，不重放、不转给新连接。

#### Scenario: 两条连接不能共享 uploadId

- **WHEN** 连接 B 使用连接 A 的 uploadId 查询、释放或发送数据
- **THEN** B 的文本查询或释放返回 `body_unavailable`，B 的数据帧以 4400 关闭且无 ACK；A 的上传不受影响

#### Scenario: 上传途中连接结束

- **WHEN** 创建上传的连接在 finish 移交前结束
- **THEN** 接收中数据被清理，其他连接不受影响；重连不能继续旧 uploadId

### Requirement: 四条上传命令使用稳定 JSON

系统 SHALL 按封闭 schema 支持四条上传命令，并保留管理命令的 requestId 与错误关联。

#### Scenario: 命令字段、标识与结果

- **WHEN** 客户端发送 `input.upload.begin/finish/status/release`
- **THEN** 使用封装 `{ "v": 1, "type": "command", "requestId": "…", "action": "…", "payload": {…} }`，且 requestId、targetId、uploadId、bodyId 均为无填充 base64url 128 位标识
- **THEN** begin 仅接受 `targetId,purpose` 并成功返回 `uploadId`；finish 仅接受 `uploadId,totalBytes` 并返回 `uploadId`；status 仅接受 `uploadId` 并返回 `uploadId,phase,nextOffset` 及阶段相关字段；release 仅接受 `uploadId` 并返回 `released:true`
- **THEN** totalBytes 与 nextOffset 是无符号 64 位整数的规范十进制字符串，仅 `0` 可有前导零，不接受符号、空白、小数或超范围值
- **THEN** 成功封装使用 `v:1,type:result`、原 requestId、`ok:true`、result 和 `error:null`；错误封装使用相同版本/类型/requestId、`ok:false`、`result:null` 和 `error:{code}`
- **THEN** 可关联合法 requestId 的无效 payload 返回 `invalid_request`；无法解析外层封装时沿用既有格式错误规则，不伪造 requestId

#### Scenario: 阶段、释放与字段拒绝

- **WHEN** 客户端读取阶段或提交缺失、多余、类型错误的 payload 字段
- **THEN** phase 仅为 `receiving/preparing/ready/failed`；仅 ready 含 bodyId，仅 failed 含 error，其他阶段省略二者
- **THEN** finish 成功仅表示已移交准备；释放后的 status 返回 `body_unavailable`，同连接重复 release 返回 `released:true`；未知或跨连接身份不泄漏归属
- **THEN** 缺失、多余或类型错误的 payload 字段被拒绝

#### Scenario: 查询反映真实准备阶段

- **WHEN** 客户端完成 begin、Data 与 finish 后查询 status
- **THEN** 各结果只关联原 requestId；准备未完成为 preparing，成功才为 ready/bodyId，失败为 failed/error，finish 或 ACK 不冒充 ready

#### Scenario: 释放与重复释放

- **WHEN** 同一连接释放自己的 uploadId 后重复释放并查询
- **THEN** 两次 release 均返回 released 为 true，status 返回 body_unavailable；其他连接仍不能访问该身份

### Requirement: 文本错误映射稳定

上传文本命令 SHALL 使用稳定结果码，不把原生服务失败降为成功或私有兜底。

#### Scenario: 上传文本错误码

- **WHEN** 上传文本命令遇到下表失败类别
- **THEN** 输入格式、方向或所有权参数无效及正文解析失败为 `invalid_request`
- **THEN** 目标不存在或无效为 `invalid_target`；上传阶段或 finish 总长度不符为 `invalid_upload_state`
- **THEN** 上传内容或偏移冲突为 `upload_conflict`；未知、其他连接或不可用身份为 `body_unavailable`
- **THEN** 分配失败为 `allocation_failed`；服务未就绪或节点运行但上传 owner 停止为 `product_not_ready`；上传必要存储不可用为 `necessary_storage_unavailable`
- **THEN** 管理认证复核失败沿既有连接关闭规则（当前 4500）处理，不伪装为上传业务结果；Harness 支持情况在实际派发时按既有规范处理

#### Scenario: 正文解析失败

- **WHEN** finish 移交的正文被节点真实解析为无效输入
- **THEN** status 为 failed/error 等于 invalid_request，不产生 bodyId，不宣称队列或 Harness 成功

### Requirement: ACPI 数据帧与 ACK 使用唯一出站顺序

客户端 Data SHALL 使用规定头、偏移与唯一出站顺序，ACK 只证明接收事实。

#### Scenario: Data 头、偏移与 ACK

- **WHEN** 客户端发送一条上传 Data
- **THEN** Binary 为固定 32 字节头及 1—16384 字节 payload；偏移 0—3 为 ASCII `ACPI`、4 为版本 1、5 为方向 1、6—7 为零、8—23 为原 uploadId 的 16 字节、24—31 为大端 u64 起始 offset，offset 加长度不得溢出
- **THEN** 只接受当前累计 offset 的新数据或最近一次成功块的完全相同重试；同 offset 异内容、跳 offset、非 receiving 或任意更旧块重放均拒绝
- **THEN** 每个成功且连接有效、预算可用的 Data 产生恰 32 字节 ACK；头同上但方向 2、无 payload、offset 为累计 nextOffset。最近块重试返回相同 nextOffset 且正文不重复附加
- **THEN** ACK 不证明网络 flush、正文 ready、队列受理或 Harness 成功；撤销、停止、传输结束或 ACK 预算失败可无 ACK，连接所有权结束并清理接收中数据，重连不能读取或重放旧 uploadId
- **THEN** ACK 与文本使用原唯一出站 FIFO，按成功入队顺序发送；32 字节计入既有出站预算并在发送或丢弃后归还，不建立第二个通道或旁路
- **THEN** 沿用 32KiB 单条入站、1MiB 出站字节和 256 待发帧边界，不新增累计正文容量上限

#### Scenario: 成功接收及最后块重试

- **WHEN** 同连接发送合法 Data 后原样重发最后一块
- **THEN** 分别得到一个完整 32 字节 ACK，累计 offset 相同，正文只附加一次

#### Scenario: ACK 与随后文本结果排队

- **WHEN** ACK 已成功入队，随后 status 结果成功入队且连接继续有效
- **THEN** 唯一出站顺序先发送 ACK 再发送该文本结果；发送或丢弃后预算回收

### Requirement: 二进制错误和关闭优先级明确

系统 SHALL 按完整 `ACPI` magic 分类 Binary，并在停止与格式错误之间使用明确优先级。

#### Scenario: Binary 分类、错误与关闭码

- **WHEN** 客户端发送 Binary 控制帧
- **THEN** 完整四字节 `ACPI` magic 起始的帧归为上传，包括短于完整头的此类帧；其他 Binary 沿既有 ACPV 共享视图规则，合法 ACPV 不进入上传，非法非 ACPI 控制沿既有格式错误关闭
- **THEN** 节点运行、认证有效且预算未超限时，ACPI 短头、版本/保留位/方向、payload 长度、offset 溢出、未知/跨连接 upload、冲突、错误阶段及上传 owner 业务错误均零 ACK、零 JSON 并以 4400 关闭
- **THEN** 运行中 owner 单独停止及上传必要存储不可用也以 4400 处理；管理认证存储复核失败沿既有 4500 规则
- **THEN** 消息或出站预算超限为 4429、认证撤销为 4410、节点生命周期停止为 1001，原传输失败保留真实关闭/结束事实
- **THEN** 数据处理前已观察节点停止或 control shutdown 时优先 1001，不再解析或调用上传业务，后续格式错误不能改写；格式错误已处理并确定 4400 后才停止时不追溯改写
- **THEN** 不确定先后竞态不承诺固定结果，一个连接不得因同一错误重复发送多个 Close

#### Scenario: 停止先于待处理坏帧

- **WHEN** 活连接已收到一个待处理坏帧，但处理前复核先观察到节点停止
- **THEN** 以 1001 结束，不执行该帧上传业务，零 ACK 和 JSON；不向已关闭连接再读取新帧

#### Scenario: 上传错误时节点仍运行

- **WHEN** 生命周期仍运行，而该连接的上传所有者已单独停止
- **THEN** 文本命令返回 product_not_ready，Binary 零 ACK/JSON 并以 4400 关闭

### Requirement: 撤销先阻断出站再完成生命周期封存

认证撤销提交后，下次发送前的权限复核 MUST 拒绝被撤销 actor，以 4410 结束；尚未越过该复核的 ACK 不得发送。已经完成发送前授权且先于撤销提交的发送可完成，不承诺追回已写出的字节。撤销事务返回前不要求同步清理全部连接资源，但原连接协议结束后 MUST 封存该连接所有权并清理未移交数据；其他有效连接和 actor 不得连带封存。

#### Scenario: ACK 发送前撤销先提交

- **WHEN** ACK 已排队但尚未完成发送前授权，真实撤销先提交
- **THEN** 原连接以 4410 结束，不发送该 ACK；连接结束后上传封存并清理接收中条目，其他有效连接仍可工作

### Requirement: 异步命令取消不产生无主任务

系统 SHALL 监督连接的异步上传结果。连接结束、撤销或节点停止取消结果观察时，不得继续执行尚未开始的命令、重复 finish、重放 Data 或伪造成功。已完成的原业务事实不因响应丢失而回滚或补执行。finish 已交给节点准备阶段的内容 MUST 由原节点准备任务继续收敛并受节点退出监督；观察者取消不等于准备任务实际结束。连接结束后旧 uploadId 不可由新连接查询。

#### Scenario: finish 已交接但响应观察取消

- **WHEN** finish 已移交准备阶段，原连接在收到响应前结束
- **THEN** 不重复准备或重放；节点继续监督同一次原准备，未移交接收数据被清理，新连接不能查询旧 uploadId，不把观察取消当作原任务已结束
