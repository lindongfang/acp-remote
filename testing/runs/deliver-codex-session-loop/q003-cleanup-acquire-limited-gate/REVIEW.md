# 最小关闭修复提案：有限静态编码门

结论：Pass-limited-proposal-static。可以在新隔离候选实现“同一 Storage owner 先登记 acquire、caller 取消后保有原任务、成功无人接收时显式关闭并等待原 ACK”的有限修复。此门不是新 ABI 的最终批准，不批准 Root 安装，不确认完整物理关闭，也不关闭 Q3-COMP-CLEANUP001 / Q3-COMP-STARTUP001。真实实现与独立产品测试均为 0。

输入为 session-loop-q003-cleanup-independent-diagnosis-v1，freeze SHA256 99177c081abbed12996c76dd25893ed624a90bf922775ee205f9e7d9c6df2428，858 成员已独立逐字节长度及 SHA256 核同。其 789 源是诊断 787 + 两诊断测试叶，不能与交付 A789 或当前 Root639 混同。本审查只读该固定 workspace、提案、报告、实际日志及锁定 SQLx 源。未重跑产品或修改任何既有资产。

## 已有证据及限度

实际 module01 为原模块 29 项中 28 绿 / 1 红，exit101；红点是原首次 remove_dir_all raw32。诊断后续只读独占打开的结果会随窗口变化，不能取代首次失败。

实际 barrier02/03/04 每轮都是 1 红 / 1 绿、exit101，合计三次真实 SQLite worker 屏障红与三次保有 acquire 后显式 close 对照绿。红支确实 pool_closed=true、size=0 而 worker 尚在回调、主库/WAL/SHM 各 raw32；绿支先证明 pool.close pending、size1，再释放 worker、等待 acquire 与显式 close，唯一首次 remove 成功。对照只是保有成功 acquire 的库机制控制，没有真实 Node/Trust/registry receiver 移交，不能签产品修复。未强制窗口的 80 区全绿、旧 v1 屏障挂起后停止的夹具错误都保留，不能删去或当修复结果。

冻结 SQLx 五叶及 Cargo.lock 校验表独立核同：0.8.6 poolInner.rs:242-297 内部 acquire timeout 包住取得连接后的 await；poolInner.rs:346-378 connect timeout/error 和 after_connect 错误另有普通丢弃 / close_hard；poolConnection.rs:86-89 显式 close 与 :134-171 / :199-209 普通 Drop 后台归还不是同一收据；sqliteConnection.rs:219-241 显式关闭才等待 Shutdown，close_hard 仅 drop；sqliteWorker.rs:323-329 在 ACK 前释放 conn/shared。原缓存 crate 未由本 reviewer 重新解包，归档来源的逐字节比对是诊断包已存证据。

## 编码范围和必须保持的不变量

1. 单一 Storage 责任。复用 / 扩展当前连接 owner 的实际所有权边界，acquire 第一次 poll 前就注册原任务。任务、获取登记、取得的原连接、原关闭收据必须属于同一个 Storage；不在 Trust 构造或每次 acquire 中另建可独立报 Complete 的 owner。既有 registry/Approval 原 receipt 身份、ACK 与 join 语义必须保留。
2. 真实接线。当前 storage.rs:526-527 只把 pool 克隆给 TrustStore，trust.rs:161 起仅持 SqlitePool；client_by_id:545-555、client_by_public_key:526-539、revision 与 pool.begin 入口须改为实际经过共享获取责任。NodeRequestAuthority.current → RegistryOwnerScope.while_open → policy.require_current_actor 这条链必须真实覆盖，不能只改显式 registry 查询。按本次实际 Storage 消费者逐入口盘点，同 scope 运行期裸 pool executor/transaction 不得默默绕过新保证；其它启动 / 后台路径保留明确未覆盖边界，不借此要求无关业务重写或万能 executor。
3. 移交无空窗。registry request_registry.rs:61/89/142/146 当前 lease.attach(pool.acquire().await?.detach()) 的 await 前后必须承接同一个 acquire 登记，直到原 connection attach 或 close ACK + task join。接收端销毁、发送失败、封 scope 与移交竞争均不能先回收登记再放出连接。成功交给调用者后的查询、事务取消和 Drop 也不能退回无收据普通 PoolConnection::Drop；已登记连接转入既有关闭所有者后才可释放 acquire 段责任。
4. 状态不可洗白。获取中、已移交、正在关闭、原实际 ACK 与已 join、未确认必须可区分；caller Drop 只放弃结果，不 abort 所有者任务。晚到的成功只能关闭，无人接收时不能继续已取消的授权或写事务。明确未启动 worker 的失败可结束登记；无法证明的内部超时 / 错误不因返回 PoolTimedOut 或任务 join 而推断安全。
5. 关闭依赖顺序。先封新获取，沿原 Node 顺序停止业务并推进占用连接归还 / 关闭，同时让旧 acquire 可以取得 permit 或准确失败；不能“等所有 acquire 完再关已持有连接”，也不能先 pool.close 导致内部取消再算成功。等待原实际关闭收据及任务 join 后完成 pool 关闭；原 Node deadline 不重置、不延长。调用 close 的 future 被 deadline 丢弃后，原 owner 保有任务及失败知识，不能 abort / 清零后伪报 Complete。
6. 结果必须消费。node.rs:432-451 当前 storage.close 返回后只读 registry_close_status。新获取责任的 Pending/Unconfirmed 必须进入同一个 Storage/Node 最终关闭判定，不得只存到新 owner 日志而外层仍成功。可保留现有错误类别；不得弱化既有 shutdown_observation、原断言或关闭尾。

## 必要设计缺口 CLDB-DESIGN-REV001（Open，完整关闭修复门阻断）

FIX-AND-EXPECTATIONS.md:13 已正确承认：SQLx 内部超时或报错可能先丢 worker，外层任务拿不到原连接 / 原关闭收据。现提案尚未给这些分支正常确认关闭的具体接口。允许先编码上述有限成功路径和 sticky Unconfirmed；不能把这作为全分支正常关闭设计已完成。

错误来源需在候选中精确归类：等待 permit 前、idle ping / before_acquire、connect / worker establish、after_connect、显式 close 错误。不能只依据统一错误名推断“worker 没启动”。已丢失原 worker 收据时留下原任务关联的未确认事实并传播到 Node；重做 query、新 acquire、新 pool、新 close 不能替原 worker 补签。需要正常确认这些分支时，必须另交原 worker 建立 / 退出收据的最小实际设计，独立审查后才能实现其下层或依赖变化；当前不授权 SQLx 依赖补丁。

同时明确范围：storage.rs:337 connect_with 在 Storage 构造前，不能假定已经存在本提案的 owner；poolConnection.rs 的 return_to_pool 自带 ping / 后台任务，poolInner.rs:503 起的 maintenance 可在关闭事件丢弃内部等待，pool.close 本身的连接关闭不等于可观察每个错误。必须盘点当前实际启用选项及运行期路径，不能声称外层 acquire 的 owner 已覆盖池内部所有工作。可以保持这些分支 Open，不阻止有限 post-open 修复；若宣称 Storage 全部物理关闭则该缺口必须解决。

## 最少独立观察与验收门

CLDB01-07 接受为待实现的测试定义，产品执行仍 0。不需要为静态门补大量文件，但固定候选前需要实际最小接线：同 Storage owner 的只读获取登记 / 移交 / 原 ACK / join / sticky 未确认观察；真实 SQLx 取得连接后、registry attach 前的可控屏障；真实 Trust authority 取消链。观察点只暂停或读状态，不替业务获取 / 事务 / close，不提前提交、不制造 ACK，也不暴露敏感正文。

CLDB01 产品版、CLDB02 真实 Trust、CLDB03 await-before-attach 必须各自在 worker 屏障未释放时证明完整关闭不能成功，释放后证明原 ACK + join 与唯一首次物理检查。CLDB04 补真实 SQLx 内部 timeout / close error，拿不到收据时预期 Unconfirmed；不能用抛错 mock 代替 worker 消失证明。CLDB05 必须实际新建 worker 尚未 acquire 返回，CLDB06 要有同 owner Trust/registry permit 竞争与原 budget 下无循环，CLDB07 重跑原首红 / 原模块与受影响 Trust/Node。既有断言、首次 remove 一次、原预算均保留；不能加 sleep、删除重试、pool.size 轮询或 fake completion 取代关闭证明。

限定授权：可实现上述有限 owner/接线及开发回归，随后送固定候选作 ABI/差异静态门和独立测试。未批准实际产品修复通过、当前 Root 安装、依赖变更、schema9、C/D 扩围或旧问题关闭。Root 七叶 schema 成功预期修正的独立复核另候新包，本结论不回填该门。