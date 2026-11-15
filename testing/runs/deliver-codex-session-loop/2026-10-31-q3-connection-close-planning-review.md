# 组合关闭缺陷与v3限定编码门禁

2026-10-31。**REG-INTEG-SQL-001（同tester REG-INTEG-CLOSE-001）确认是产品资源关闭责任缺口，阻断当前组合交付。限定方案与已有独立红回归预审通过，允许在原隔离workspace编码v3；不签修复完成。** 旧v1/v2、原154限定报告和根172失败版本均保留，原六个已修问题不自动重开；本问题另行验收关闭。

## 真实失败与根因

根组合172项清单 `890243a0cf085b153d7b2f9b0b82b678abdc4e03b4bd951a641dd52a5c6cfe7d`；实际owner全量日志 `.tmp/q3-regraw-full-check.txt` SHA256 `e479cc013a95416cb724c475d7c1cab42cfbf8b3b91bf1b9677b271bc3c65f5e`：库478中475过、F02/F03/F15在fixtures.rs:39即时remove_dir_all遇Os32。Web/fmt/clippy先前通过不覆盖此物理资源问题。

已亲读独立纯SQLx对照源和原日志：普通DetachedDrop即时删除84/96失败，对照显式close与pooled-close-on-drop为0/96；实际BEGIN/INSERT后abort并join调用任务，pool.close后立即同步Windows share_mode(0)探针，detached96/96仍占用，pooled0/96，另连接确认未提交INSERT=0。此对照不改产品或原fixture，不靠sleep/retry；因此不是简单为fixture加删除重试能解决的问题。pooled对照通过只说明该实验，不证明它的内部超时满足全部关闭契约。

进一步亲读实际Storage回归 `storage_physical_close_acceptance.rs` SHA256 `95748e82937065b7cd11f08a9b11d3fc1d7ea0fc2ea3c11cd54613a65cfce34a` 及红日志 `c5524f612288d784feabe9317f24dec06d7de98f50624bf738dfcb5808c3eacc`：REG-CLOSE01正常仓储返回后Storage.close，24/24独占探针失败；REG-CLOSE02实际节点owned begin的真实Before/AfterCommit取消，两分支也失败，同时分别保持无operation/Applying事实。失败自有DB保留，未靠反复删除抹平信号。该两例是固定独立验收基线，正常与取消路径均可使旧实现变红。

本轮按诊断技能做只读独立复核；复现/单变量实验由独立tester实际执行，reviewer亲读而未冒称亲自重跑，不重复占用tester target。已有真实红反馈足以进入精确产品修复，故不再新搭重复实验。

## 锁定primary证明与排除的简化方案

- sqlx-sqlite0.8.6 connection/mod.rs219–234：close消费连接、发送Shutdown、drop外侧引用，再等待worker确认；worker.rs323–328先drop(conn/shared)才发ack。普通Drop不等待该确认。
- sqlx-core0.8.6 pool/connection.rs113–114的detach释放pool所有权；pool/inner.rs97–114只能等待自身计数/permit，不能覆盖已detach的worker。因此当前registry finish的COMMIT/ROLLBACK之后仅drop，Storage.pool.close不能保证这些连接已物理关闭。
- close_on_drop的take_and_close在connection.rs16/157–175有5秒CLOSE_ON_DROP_TIMEOUT，超时会释放guard；consuming PoolConnection.close未来本身取消也会drop局部floating guard。二者都不能直接当作“真实ack已经完成”的全部证明。
- 根Storage实际max_connections=1。保pool permit还可能阻断SQL屏障期间使用同pool的可信resolver；F17静态Authority不能冒充这种实证。保留detach既有并发语义、补独立关闭owner，避免为此修改pool数量/队列事件规则。

## 本次固定工程合同与编码许可

亲读 `connection-close-owner-supplement-v3.md` SHA256 `d9b1cf3f355e685012daaf22e3db205f8b3c6dba6baf3d2708d7068c6c4a0370`，其机制可实现，且足够窄：

1. 新私有ConnectionOwner/ConnectionLease/RegistryConnection/CloseSlot只负责本registry的物理连接。首await前登记lease（包括等待acquire），seal后拒绝新lease；acquire失败/取消、BEGIN失败也归还。
2. 保detach；正常完成、错误、outer deadline与Drop取消都将连接在同state锁内转为tracked close槽，再减lease，无先计数0后注册任务的缝隙。正常路径/Drop无panic或未登记spawn。
3. CloseSlot持JoinHandle，owned任务不套SQLx5秒或operation deadline、不被等待方abort，只等待真实SqliteConnection.close确认。operation只可取消等待，不能取消关闭责任；等待取消不take/drop handle。多等待读取同结果，只有真实join后回收完成槽。
4. Storage原queue stop/join→registry seal、lease0及所有槽真join→pool.close。取消一次Storage.close仍保owner/slot，后续可继续drain。完成条件和最后注册共用围栏。
5. 原COMMIT/ROLLBACK结果与关闭状态分离。已COMMIT不能因close错误伪装rollback、改受理资格或重D；错误为独立typed Unconfirmed。Storage.close签名保持()，crate内Pending/Complete/Unconfirmed getter让Node原关库处报告静态关闭失败。原deadline仅限制等待并报告未闭，不abort关闭job；不得带SQL原文/正文。
6. 新测试hook仅cfg(test)，固定BeforeClose/AfterCloseAck一次性pause；若模拟确认错误，必须先真实ack再模拟结果丢失，不冒称真实IO故障。不能新增生产开关、注入任意连接或改旧hook/oracle。

许可写范围：专用registry_connection_owner私有模块及开发测试、session_storage/mod.rs、request_registry.rs、storage.rs，以及Node原关库结果一处typed检查/开发测试；不改schema/DDL、原独立资产、Raw实现、actor/时效/回执业务或max1配置。v3新候选保留旧冻结字节，不在根172版本并写。私有helper具体布局可自主选择；不能借修复扩成通用数据库运行时。

## v3交付验收门槛

上述95748两红例原oracle必须在新源变绿，并覆盖正常COMMIT/ROLLBACK/BEGIN失败、BI/BA未知/取消后真实物理独占及必要事实。新增CloseSlot pause下的operation等待取消、第一次Storage.close等待取消后第二次继续同槽、seal/acquire/最后归还竞态、完成槽实际join后回收、已COMMIT加确认失败不重D/Node typed失败，由tester按本d9b1合同独立固定与执行；不得以开发自测代签。

必须回归原默认并发F、紧F02/F03红循环、原154语义、SQL authority下事件可达及Raw30；再在新冻结候选与根实际组合重跑适用工程检查/默认并发全库。不得靠线程串行、sleep、删除重试或提高容量/时间阈值通过。新物理关闭oracle与旧Future内Weak各证明不同层，均保留。实际资源关闭与模拟确认错误分列。

本次是明确编码门禁，通过后进入v3代码/实测验证；没有剩余需要用户产品决策的阻断。REG-INTEG-SQL-001仍open，修复后须新源SHA+独立结果+root组合证据复核关闭，原172组合当前未交付通过。
