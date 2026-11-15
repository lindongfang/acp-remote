# 未冻结 R3：关闭所有权过程审查

2026-10-31。结论：原 `take(tasks)` 关闭算法存在两个实质性问题，必须修复并通过独立真实屏障验证；本报告不是 R3 交付通过，不修改 revision2 合同或既有测试预期。

## 阅读点与边界

只读隔离实现 workspace，未改产品、根仓库或历史证据。首次读取 `app/src/application/queue_owner/service.rs` 的 SHA256 为 `8c4205f5dd940036f076b02e64124b8423e0855453aad0a03bcfd35157196db7`，780–870 行仍为先 `mem::take` 全部任务、在关闭 Future 局部逐一等待、最后按局部 remaining 是否为空决定 joined。新 shutdown 互斥字段已创建但该方法尚未使用。

实现者同时正在修改文件。随后读取 `queue_owner.rs` SHA256 `7458863e818cde2afb5debd285e9db5843cc7e4c94c379b9c47272e9f90a76c4` 已出现 `Arc<TaskRecord>`、槽内 JoinHandle 和独立 AbortHandle。两次阅读不是一套冻结交付源，不能以其跨文件中间态判断已编译或已修复。

`storage.rs` 阅读点 SHA256 `3648642e9757bb70511f5ee0067341e92fde402ef781e9d2922312ecc55542a8`：queue_close 只串行 Storage.close，不能串行公开 QueueService.shutdown。Storage 已将相同 supervisor 保留到 stop_and_join 完成之后再清理，早先 REG-IMPL-START-001 的修复仍待独立 Weak 生命周期动态验证。

## 问题与最小反例

| 编号 | 严重性 | 反例与影响 | 关闭门槛 |
| --- | --- | --- | --- |
| REG-IMPL-SHUT-001 | P1 | S1 取走任务并等待真实 BA/SA 工作 Future；S2 同时调用公开 shutdown，看到 Core.tasks 为空，自己的 remaining 也为空，因而返回 joined=true、清 owner 并释放 runtime claim，而 S1 的任务尚活着。 | 关闭调用串行；joined 与 claim 释放由真实全部任务结束和封闭注册共同决定，不以局部空集合代替。并发第二关闭不得绕过仍在等待的第一个关闭。 |
| REG-IMPL-SHUT-002 | P1 | S1 已取走任务并在 await；调用者取消 S1 Future，其局部 TaskRecord/JoinHandle 被 drop，Core 中没有句柄。Tokio JoinHandle 的 drop 会 detach；之后 Storage.close 可能把空集合当作已 join，潜伏任务仍可继续。 | 句柄与取消入口持续由节点持有。任一 shutdown 等待 Future 被取消后，后续同一 supervisor 仍能 abort 并实际 join；仅增加关闭互斥不能满足此条件。 |

本机缓存 Tokio 1.53.3 `src/runtime/task/join.rs` 18–35 行明确区分：drop JoinHandle 会 detach；等待 `&mut JoinHandle` 可取消安全。后一保证要求句柄本身仍有所有者，不能证明局部句柄随 shutdown Future 一起销毁是安全的。

## 注册围栏：已看到与尚需验证

当前 consume 在 tasks 锁内检查 Running 后安装 pending/注册任务；reconcile 同样在该锁内检查；spawn_audit 在该锁内检查 Stopping/Stopped 后才注册。对这些路径，没有据此另报“Stop 后必然漏注册”的已证漏洞。审查要求是保留这一同步关系，并在修复后检查最终 drain、Stopped 和 claim 释放仍使用一致围栏。

launch_startup 本身尚无相同状态检查，须结合 Storage 在首个 await 前持有 queue_lifecycle、安装 supervisor、只启动一次的完整调用路径判断，不能只看该私有方法断言存在可达竞态。后续同源交付必须核对所有 spawn 注册点和关闭路径；不允许新任务在已确认 join 后进入。

## 独立验收补充

已亲读 tester `registration/shutdown-waiter-ownership-supplement.md`，SHA256 `d0527ac1917a88e497dd4086b169574ca7ef0857bf5a0091fecff8e96ec6ae0b`。其 REG-SHUTDOWN-01–03 方向通过定义审查，尚未运行：

- 使用真实 SQLite 仓储与实际 QueueService/InputQueue，BA 为 begin COMMIT 后扣回报，SA 为 settle COMMIT 后扣回报，AU 为独立 audit Future 等待；第二连接确认真实持久事实。
- 工作 Future 内独占 Arc 生命周期探针、测试端仅 Weak。任何 joined=true/close 完成，都须对应工作 Future 探针不可升级；只看 wrapper 对象存活或调用数不够。
- 公开 shutdown Future 首次实际 poll 返回 Pending 后，分别并发第二关闭、取消第一关闭再调用 Storage.close。测试转发同一 poll，不在生产插 yield，不用 sleep 猜取走任务时机。
- 核实 claim 不早放、第二 factory 拒绝、无第二 runtime/lease；释放原门闩后无迟到 D/E/SQL 推进。尚未 join 必须保留 unresolved 责任，最终取消后真实收敛。
- 首次 poll 若已 Ready，应记录合法 Ready 分支及 Weak 条件；不能将其计作并发 Pending 或取消等待动态覆盖。

修复设计可采用 Core 持有 TaskRecord/句柄槽并在锁内借用等待，或等效取消安全所有权；本报告不指定实现框架。串行、取消安全、注册封闭和真实 join 缺一不可。两问题保持待关闭，须绑定后续冻结源、独立测试资产和实际结果；当前没有功能通过结论，也没有放行 WP3 writer/WSS 或下一产品阶段。
