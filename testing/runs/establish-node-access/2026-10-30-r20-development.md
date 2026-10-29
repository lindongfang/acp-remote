# R18–R20 集成补充

日期：2026-10-30。开发方记录，不能代签独立验收。

R20源码83项清单SHA256 `73727BCF617F3F96BCC24C4CBB7B38BA044A56856C6CB4D628869832C8B368A4`；冻结副本`.tmp/frozen-implementation-r20-20261030`逐条83/83匹配。其普通debug-embed单包`bin/acp-remote.exe`的SHA256为 `561A6024E983DD5239C09D1388CA5D256625F690EE9345E64B9654BC43AAFBE3`。

实际完整`npm run check`与冻结app普通构建退出0：Rust库144、固定向量4、Web7文件50、fmt/Clippy/类型/构建通过。原始日志`.tmp/implementation-r20-check.log`、`implementation-r20-build.log`。

## 有限增量

- R18：旧页延迟拒绝后，若另一页的replacement已进入pending/approved/consumed，按共享记录的实际状态恢复，不误展示无法继续的配对表单。原tester已真实延迟旧连接的原生4410回调，待另一页新身份consumed后释放；旧页自动使用新key/ID，未二次删除或生成第三key。原reviewer独立检视通过。
- R19：仅测试增加writer已经持actor gate、sink.send之前的栅栏。竞争revoke被实际poll50ms仍未提交，SQL仍trusted；放行后实际send成功计数在gate内增加，再允许COMMIT。真实客户端先收到对应成功结果，再4410。原tester独立通过并将NA008转PASS；普通运行时不包含该开关。
- R20：修复独立真实TTY发现的NA007-CANCEL-001。发送决定后的结果等待也监听Ctrl-C；中断先关闭配对流，再立即给device list/条件device revoke指引。已有批准可由owner list按本次指纹确认，整个分页查询最多5秒；仅确认trusted时打印稳定ID和原正常撤销命令，未知不假报取消或批准，不回滚已提交信任。原reviewer源码/helper范围通过，原tester真实延迟交付探针仍需新版本复测关闭。

## 交付边界

原tester已合并正式嵌入包、存储及生命周期证据：NA013/026/031/032通过，当前31 PASS / 1 PARTIAL / 1 FAIL；唯一代码FAIL为上述CLI提示，另一PARTIAL为真实不同SID的pair/stop执行环境。用户无需重做技术验收或安全签署。

任务2.4、2.5、3.2、3.3、4.1等已据独立判据逐项勾选，OpenSpec回读9/15。后续元数据和新报告不冒称包含在旧83项摘要内，收口另列清单。README新增构建/配置/前台启动/本机配对及Web列表撤销说明；长期定义校验、OpenSpec strict和tracked差异格式检查通过。

工作区非元数据80项与R20清单仍匹配；活动文本检查已清除新E2E探针一处行尾空格，仅历史storage报告两处原始行尾空格保留，避免改写已被摘要引用的历史证据。源码/报告新增仍须最终秘密扫描。用户原节点/配置/身份未替换，未提交、推送或归档。

## 后续独立闭环与最终检查

原提出tester已在`2026-10-30-approval-cancel-r20-retest.md`用上述固定R20普通exe完成三种真实TTY分支，均退出0；透明代理实际转发额外owner查询，提交后确认正确ID并输出完整revoke命令，提交前无匹配则只报告未确认。同key重启恢复通过，原NA007-CANCEL-001由原提出者Closed，R17 FAIL不回写。此结论不代签整个change；NA012仍缺另一真实Windows SID的pair/stop。

后续可版本化源码快照715文件、8029636字节，官方Gitleaks扫描退出0、零命中。扫描在快照根目录执行`dir .`，保留原精确规则；首次从外部以绝对扫描目录执行导致锚定路径的公开fixture例外未匹配，纠正工作目录后通过，没有放宽规则。其后新增元数据不冒称已被旧快照覆盖。再次核对R20的80个非plan/tasks/review清单项，漂移0。

长期定义校验33例/28场景/2能力、OpenSpec strict和`git diff --check`均退出0；这些只证明定义引用、结构和差异格式，行为结果以独立实际报告为准。后续最终元数据清单单列，不改写R20旧83项摘要。
