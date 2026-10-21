# Tasks

## 1. Wire 与权威合同

- [x] 1.1 [wp:WP1] 登记 resource.stateless-origin.v1、远程来源流消息与运行期 ACK 语义；完成判据：schema 和共享 fixture 覆盖订阅/重放/ACK/重置/未协商拒绝，Rust codec 测试和 PV1 通过。
- [x] 1.2 [wp:WP1] 定义 resource.caught_up、readCursor/消息增量覆盖和 command.status 来源定位；完成判据：冻结 closed object 字段和读取屏障合同，同请求与跨主体冲突 fixture 通过，不新增未经定义的签名字段。
- [x] 1.3 [wp:WP1] 更新 design D7 文档、AGENTS 与 ADR-0013；完成判据：移除 Access 先落收据/可持久恢复映射的现行要求并保留 Owner 保证，文档与机器定义逐项对应，独立检视通过。

## 2. Core 运行期与回源端口

- [x] 2.1 [wp:WP2] 拆除持久 imported 目录依赖并定义纯运行期端口；完成判据：mock 测试覆盖列表、来源检查、撤销、世代清理与 owned 资源分域，core 依赖闭包不增加。
- [x] 2.2 [wp:WP2] 接通 Owner 状态查询和来源身份校验；完成判据：原 requestId 去重、客户端丢失标识后不自动补发、跨 Owner/主体拒绝测试通过，Rustdoc 写明端口生命期。
- [x] 2.3 [wp:WP2] 定义原子读取边界与增量覆盖信息；完成判据：消息半截、结构化历史与实时事件竞态测试能证明无重复拼接和无静默遗漏，PV2 与独立检视通过。

## 3. 存储白名单与旧数据清理

- [x] 3.1 [wp:WP3] 移除 imported 资源/命令/收据/游标持久表及写入端口，维护代码内迁移说明和漂移脚本；完成判据：升级库和新库白名单一致，owned 事件/epoch/幂等与节点信任逐项保留。
- [x] 3.2 [wp:WP3] 实现旧代理记录、日志及 SQLite 副本清理的启动门禁；完成判据：故障注入时不监听，重开可安全完成，不出现远程哨兵，重复打开不变化，PV1/PV2 与独立检视通过。

## 4. Node Link 与 Sync 适配

- [x] 4.1 [wp:WP4] 实现有界内存 catalog/attachment 和 Owner 回源读取，补充模块内生命期说明；完成判据：断开和撤销取消任务并清除对应来源，不误读本机路径，多 Owner 隔离测试通过。
- [x] 4.2 [wp:WP4] 接通来源重放屏障、运行期 ACK 与新 Sync 流；完成判据：追平和实时无缝衔接、越界/跨来源 ACK 拒绝、慢客户端不阻塞及 Access 重启测试通过，无落盘收据调用。
- [x] 4.3 [wp:WP4] 接通纯在线命令观察与 command.status；完成判据：Owner 已提交回复丢失后只查询，不重复执行，旧端明确降级、owned 流不变，PV1/PV2 与独立检视通过。

## 5. 客户端与跨节点验收

- [x] 5.1 [wp:WP5] 增加 TS 新流解码与来源状态，删除远程摘要/游标/命令的持久缓存路径并维护内存生命期注释；完成判据：Rust/TS 共用 fixture，浏览器存储无远程资料，来源断开即清除显示。
- [x] 5.2 [wp:WP5] 实现原子读取重建和实时 delta 覆盖合并；完成判据：messageId/eventId/deltaIndex 重复与乱序场景不重拼，断线模型清除后即使仍有 C 也先读取重建、刷新/来源重置各有测试，PV3 通过。
- [ ] 5.3 [wp:WP5] 主检出执行 Main E2E 和存储审计；完成判据：plan 中全部跨节点情景有可读断言证据，数据库/日志/临时文件/浏览器无远程资源或进度，PV1/PV2/PV3 全绿且独立检视通过。

## Workflow follow-up

- 完成实现、独立检视、规范同步和门禁后归档；随后基于本变更提交开始 custom-session-title，再实施 scoped-offline-page。
