## MODIFIED Requirements

### Requirement: 远程命令保持稳定身份与结果

可重试远程命令 SHALL 保持稳定请求标识及来源身份；Access MUST NOT 因连接断开重新执行无法确定结果的副作用，也 MUST NOT 持久保存命令状态或结果引用。命令受理、完成、失败与结果不确定 SHALL 分别呈现；Owner 的资源范围、授权及实际能力限制 SHALL 在执行前校验。Owner 负责请求防重复执行，调用方仍持有标识时查询原结果；标识丢失时只重新读取当前资源。

#### Scenario: 接受后连接中断
- **WHEN** Owner 已接受消息而 Access 未收到结果即断线
- **THEN** 持有原请求标识时恢复查询或重放原结果，不产生第二次 Agent 执行；无法确定时显示结果不确定，不以新标识重发

## REMOVED Requirements

### Requirement: 无正文交付与回源历史
**Reason**: 用户取消 Access 的持久化资源索引、收据与来源进度，原先先提交收据再广播及依据本地游标恢复的要求不再成立。
**Migration**: 用下面的纯在线交付要求替换；升级清除历史 imported 资料，保留 Owner 权威日志和连接信任，旧协议不再以落盘方式兼容。

## ADDED Requirements

### Requirement: Owner 持久化后在线转发

Owner SHALL 先持久化事件再交付。Access SHALL 验证来源身份、路由、序号和摘要后有界转发，MUST NOT 写入资源索引、交付收据或来源进度。历史 SHALL 在线向 Owner 读取，Owner 不可达时明确不可用而非返回空历史。ACK SHALL 只承诺当前运行期流控处理，不承诺 Access 已落盘，Owner MUST NOT 据此提前清理权威保留数据。

#### Scenario: 事件重放与重复投递
- **WHEN** 同一 Owner 事件在重连后再次到达
- **THEN** 来源事件身份保持不变，客户端按来源身份去重，Access 没有远程资源或进度落盘

#### Scenario: 离线后重新可达
- **WHEN** Owner 恢复连接
- **THEN** 重新获取清单并建立临时路由；调用方有来源位置时回源重放，没有时重建当前状态和历史

#### Scenario: 旧节点要求落盘 ACK
- **WHEN** 对端没有协商新的运行期 ACK 语义
- **THEN** 远程资源能力明确不可用，不发送声称已进入 Access 可恢复索引的 ACK
