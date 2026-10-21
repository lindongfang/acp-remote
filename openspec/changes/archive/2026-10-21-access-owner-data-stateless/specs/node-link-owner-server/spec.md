## MODIFIED Requirements

### Requirement: `resource.ack` 的单调性与归属

协商无持久中转能力后的 resource.ack SHALL 只表达当前连接及来源会话的运行期累计处理进度，不表示 Access 已落盘。cursor MUST 单调不减且不超出本连接已发位置；回退、跨会话或 epoch 不匹配 MUST 返回 nodelink.protocol.sequence_invalid。ACK MUST NOT 触发 Owner 提前删除保留期内事件，未协商新语义时不得以旧落盘 ACK 伪装兼容。

#### Scenario: ACK 回退被拒绝
- **WHEN** 连接先 ACK 到 originSequence “40”，随后提交 “35”
- **THEN** Owner 返回 nodelink.protocol.sequence_invalid，当前运行期水位不回退，也不存在 Access 落盘承诺

## ADDED Requirements

### Requirement: 来源重放结束有明确屏障

协商无持久中转能力后，Owner SHALL 为每次有效来源订阅给出明确追平位置，且先发送该位置及之前应重放的事件，再发布追平通知和之后的实时事件。位置 SHALL 绑定当前临时路由与 generation，旧路由通知不得被当作新订阅追平。Owner 保留期或 epoch 不满足请求时 MUST 明确拒绝并说明重建原因。

#### Scenario: 重放与实时并发
- **WHEN** 订阅期间 Owner 同时产生新事件
- **THEN** 已捕获屏障之前的事件先发，追平通知准确指向已完成位置，后续实时事件不越过屏障

#### Scenario: 旧 generation 通知迟到
- **WHEN** 已重新绑定的会话收到旧 generation 的追平通知
- **THEN** 拒绝或丢弃旧路由帧，不推进新订阅状态

### Requirement: 无 Access 记录时 Owner 保持请求权威

Owner SHALL 继续按原节点身份和 requestId 的幂等键保存接受与终态；Access 不保存命令引用不改变此保证。查询 SHALL 校验当前节点信任与请求归属，MUST NOT 要求 Access 提供已删除的本地交付序号或命令记录，也 MUST NOT 因来源节点重启自行重新执行副作用。

#### Scenario: Access 无请求记录但携带原标识
- **WHEN** 已认证 Access 重启后重新查询原 requestId
- **THEN** Owner 根据自身记录返回 accepted/terminal 或明确不存在，不要求 Access 恢复磁盘记录
