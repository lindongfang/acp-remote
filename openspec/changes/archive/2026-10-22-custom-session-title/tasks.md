# Tasks

## 1. 命令与权威合同

- [x] 1.1 [wp:WP1] 确认 Access 无资源存储前置变更已完成，登记 session.title.set 与 core.session-title.v1；完成判据：共享 fixture 覆盖字符串标题、trim/空值/null/512 边界、未协商拒绝和旧 title 形状，Rust codec 与 PV1 通过。
- [x] 1.2 [wp:WP1] 更新 design D5 七份权威文档、命令目录和标题来源说明；完成判据：删除禁止改名条款，明确 Owner 两类值与有效标题，不新增过时 Export/设备权限或虚报 Agent capability，独立检视通过。

## 2. Owner 元数据与远程回源

- [x] 2.1 [wp:WP2] 增加 custom_title 与有效标题领域投影、维护端口和持久化代码说明；完成判据：Agent 更新/清空不覆盖、无 Agent 标题兜底、同文确立自定义、并发后写及运行中改名测试通过。
- [x] 2.2 [wp:WP2] 实现 Owner 接受与一次事务状态/完整摘要/终态，接入 Sync/Node Link；完成判据：重复 requestId 不重复修改，参数冲突明确拒绝，Access 无标题/命令/收据落盘，结果丢失只回源查询的测试通过。
- [x] 2.3 [wp:WP2] 增加版本化 migration 和漂移断言；完成判据：旧库 Agent title 保留、custom_title 为 NULL、新建与升级列清单一致、失败整体回滚与重开幂等，PV1/PV2 和独立检视通过。

## 3. 双击编辑与端到端

- [x] 3.1 [wp:WP3] 统一头部/列表/搜索有效 title 与双击原地编辑，维护组件/状态注释；完成判据：只双击进入、未命名不当真值、无额外按钮或恢复操作，三处标题一致测试通过。
- [x] 3.2 [wp:WP3] 实现 Enter/外点保存、Esc 取消和 composition 规则；完成判据：Enter/blur 不重复派发、Esc 的 blur 不提交、非法输入不发命令、其他设备更新不覆盖编辑输入，PV3 通过。
- [x] 3.3 [wp:WP3] 实现失焦导航、当前页错误与离页放弃输入；完成判据：请求绑定原会话、迟到结果不覆盖新会话、刷新只读现状不恢复进度或自动补发、不保留跨页草稿测试通过。
- [x] 3.4 [wp:WP3] 主检出执行 Main E2E；完成判据：plan 全部标题情景及 Owner/Access 重启均有可读证据，Access 存储无标题/命令/收据，PV1/PV2/PV3 全绿且独立检视通过。

## Workflow follow-up

- 实现与独立检视通过后同步 delta 再归档，随后基于该提交实施 scoped-offline-page。
