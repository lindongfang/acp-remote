# R3 candidate-v1 同源静态审查（未通过交付）

2026-10-31。亲自逐项核对 source-manifest.json `14999d88b245985ce2dbcef0a54c8ad5b4e5d1d1d2653ccad26deecfe0a9cf33`：136/136一致，无缺失。124基线全部保留；16产品路径变更/新增，另4项Web dist/fixture入清单作为构建资产。三条schema5 DDL与冻结合同去注释/空白规范化后逐条完全一致。

已读实际application/domain端口、queue owner/service、真实SQLite仓储、schema/storage及增量。静态确认 START-001/SHUT-001–004的修正：Storage保supervisor跨await并在真实join后清slot；Core持Arc<TaskRecord>，JoinHandle在槽内借用等待，取消关闭不丢槽；专用互斥串行join；停止/任务注册/最终Stopped及claim释放同tasks锁；取消旧Stopped不回退；启动回报不覆盖停止；Stopping不从未applied的Applying mint新D，仅补已D的E。均仍需tester独立动态，未关闭原问题。

开发 registry-pre-freeze.txt 实际18/18，但含旧 management_ws 测试1项，queue/registry专属为17项；不能将18都称本阶段新增。开发结果不代签独立验收。

## REG-IMPL-OWNER-001（P1，交付阻断）

冻结 `app/src/application/queue_owner/service.rs` SHA256 `0c02328e37c3d91727392a2c7a233f585ec17d67b60272c4b9fd87740f6ea0f4`，reconcile 707–775行：先按请求A定位owner，再独立锁state取probe，再独立读worker；最终tasks+state锁下却只取当前pending、未核对其仍是A，就写worker/ticket并注册捕获旧probe的任务。

多线程合法交错可在各短锁之间完成A并开始B：旧调用可能读到B的probe，对reconcile(A)错误检查/恢复B；或捕获probeA后给B写worker=true，而注册的任务仍使用A。若B原来因提交未知而worker=false，A的错误/析构guard均按operationA过滤，不能恢复B；B随后被假worker阻止正确恢复。没有await不能证明同步代码不会在多个运行时线程间交错。

最小修复：最终同tasks+state锁内按请求target/id核验当前pending，一次从该pending取probe/worker。worker=true时释放锁仅inspect这个已核验probe；false时在同边界写ticket、构造匹配该probe/operation的guard并注册任务。前段定位只能作为候选，不授予之后接管任何pending的许可。不需新增公共入口、产品规则或生产yield。

实现者已确认机制并准备新revision，v1冻结字节须保留。独立定义追加旧A不可接管B、B仍可由自身probe恢复一次及并发reconcile不串ticket；真实公开并发回归与无await短缝隙静态审查分列，不伪称确定性命中。修复后需核新冻结源和准确delta，再结合独立结果判断。candidate-v1不能作为最终交付通过版本。
