# WP2 历史保存开发阶段记录

日期：2026-10-30。范围为任务2.2内层历史/保存及开发测试，未签任务最终Web或真实Harness验收。最新输入事务阶段在先前input-development记录，不将其156项数量冒作本轮。

## 实现与固定版本

类型化条目/来源/位置/版本在domain/history；有界协调器、单在途保存、停止端口、worker及关闭责任在application/history；SQLite事务预留位置、原字节载荷、版本CAS、完整性及读取pin在session_storage/history。先分配源位置，允许崩溃放弃未用预留区间而不复用；旧条目更新保持身份/首次位置，晚旧版本不回退正文。

触发与容量分开计量：64KiB只累计新接入结构化快照的原始payload，不含管理信封或保守元数据收费；版本替换不倒扣该轮累计，开始flush只领取属于该flight的量，旧ack不清新dirty。容量则包括仍持有的current/inflight唯一版本payload、完整元数据及固定保守开销，跨Run按会话累计，并共享原子节点预算。500ms/64KiB、各强制刷新及3次/5秒/250ms故障规则按合同实现；停止通过OutputControl端口请求，未冒称实际Harness已停止。

不可变旧output对象只在无条目/读取pin引用时回收，不回收尚未受理的input对象。pin读取/登记与版本替换/回收使用BEGIN IMMEDIATE事务串行。完整性事实按session/run分别保留，新Run不擦除旧Run故障。

最终13文件清单为本目录`2026-10-30-wp2-history-source.json`，复制自冻结manifest且原字节SHA256 `EF2FE20EEC101129B4342870106BC81CC61F8F9E4E891A347C20BAD0298C94FF`。主执行复制前逐项验证主目录与manifest的mainSha256一致、新目标未存在，复制后13/13匹配；其中实质差异11文件，两文件与2.1相同。未改用户已有产品修改、索引或提交。

## 独立问题与修复

旧候选虽然开发测试通过，独立review发现HIST-R001：worker先算空闲期限再登记notify_waiters等待，可能丢首条输出唤醒并延至60秒。旧候选未合入主目录。修复在任何检查/调度前pin并enable同一通知future，保留广播；增加强制原窗口的两条回归及多个forced-flush等待者回归，带有界timeout，不以随机压力代替确定性交错。

原reviewer实际复核新两文件并运行三条回归后关闭问题，见[review](2026-10-30-wp2-history-review.md)。原manifest、首次失败/中断及Clippy诊断保留历史原文，不把旧候选签成当前通过。

## 已验证

- coder最终原始日志位于隔离工作区`.tmp/wp2-2.2-hist-r001-logs/`，verification.json索引fmt-02、clippy-02、owner、regression-02、history、test、diff及exit文件。history34、完整Rust190库+4向量及格式/Clippy/差异均exit0。
- 独立tester固定13文件前后摘要，真实owner下34项专项及2项独占外部probe通过，逐13组边界与真实/替身范围见[acceptance](2026-10-30-wp2-history-acceptance.md)。原审查服务403属于当时未执行，修复候选重试恢复后才产生上述结果。
- 主目录集成后的`npm.cmd run check`实际exit0：Web50、Rust190+4及类型/格式/Clippy/生产Web构建通过。日志`.tmp/wp2-integrated/history-check.log` SHA256 `13EEC0384EC871214507B13869605496DA8EFE20BB02BC1956139A0596E1E013`。
- 随后主目录`npm.cmd run build:rust`实际exit0，普通Windows exe已构建；日志`.tmp/wp2-integrated/history-build.log`及history-build.exit。没有启动用户节点或实际数据库。

## 限制及后续

OutputControl仍是内层端口/明确替身，尚无真实ACP停止、安全接线、Web实时基线或网络读取租约；关闭预算单元断言不等于整条15秒节点关闭/Windows Job Object验收。骤退测试是真实测试子进程exit93与位置不复用，不能扩写成真实Harness中断副作用验证。产品CL-023–027/042仍待后续最终集成验收，task2.2不提前勾选。

schema1稳定升级继续支持；schema2尚未发布，本次中间2.1-only草案布局会被新严格结构校验拒绝，不能自动重建或删除已有库。若保留中间测试库，应保留原件并另做迁移/取证，不能借“草案”清理用户数据。当前只操作专用测试库。

同一隔离coder接着实现2.3审批域及事务，主目录产品保持本13文件版本，仅汇总证据。分页合同修订仅使未来wire与已确认前端参数规则一致，不声称本轮已实现分页或提前交付03完整父子历史。
