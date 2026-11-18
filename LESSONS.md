# LESSONS

本文件是错题本，每晚或每轮复盘写入。每条按固定格式记录，便于对照修复与后续升级。

同一规则在三次不同事件中出现后，晋升到 [docs/code-standards.md](docs/code-standards.md)，并从本文件删除。迁移完成后，正式规则只保留在 code-standards 中；再次发生同类问题时，只更新其中的对应规则和复发记录，不重新写回本文件。

**真实测试夹具的 release 标记必须有 RAII 兜底**
- 现象与后果：两个 `read-first-held` 原生 view 用例正常结束后没有写 release；dev child 一直停在 `session/load`，节点关闭时原生 replay 与 Storage 用户长期保留，曾表现为 `StorageDeadline` 和 `FinalResultSelected [Completed, Completed, Completed, Deadline, ...]`。
- 修复 commit：未提交工作区；`app/src/node/session_services/queue_read_development_tests.rs` 新增 `NativeViewFixtureRelease`，正常返回和 panic 展开都会释放夹具。
- Done right: 凡是阻塞真实子进程或原生调用的测试 release 标志都绑定 RAII guard；清理逻辑不能用“测试通常会走成功路径”作为前提。
- Seen 1× (2026-11-12)

**Node 停止不能只 seal session facade，必须 shutdown 原生 runtime**
- 现象与后果：`OwnedNodeSessionServices::seal()` 只封存上传、文档、队列与正文 facade，没有调用 `NativeRuntimeHandle::shutdown()`；即使 Node 已要求停止，原生 runtime 及 Storage 用户仍可能存活，close deadline 后错误收敛为未确认。
- 修复 commit：未提交工作区；`app/src/node/session_services.rs` 新增实际调用原生 shutdown 的关闭门，`app/src/node/lifecycle.rs` 使用同一 deadline 协调管理连接和 session services，并保留 deadline 后的原 cleanup/join 责任。
- Done right: Node owner 的停止路径必须显式关闭其原生 runtime；deadline 只能发布预算失败，不能丢弃 cleanup、把未 join 当已释放，或提前通知 Storage 用户完成。
- Seen 1× (2026-11-12)


**活动事件的验收必须绑定该事件处理后的状态**
- 现象与后果：容量验收曾用较早的缺字段事件和较晚恢复的活动状态拼出“缺字段后仍保护”的结论，后续 start 可以掩盖前一次错误清除工作。
- 修复 commit：未提交工作区；`runtime_child_source_clock_probe.rs` 在原活动锁内采集同 receipt 的投影后状态，`capacity_l6_real_capabilities.rs` 按原收据和工具 ID 核对；L6-CAP001 的受控 Red、8项内存 Green 和独立复核保留在本 change 证据中。
- Done right: 验证事件的效果时，将事件身份和它自身处理后的状态一起固定；阶段末的当前状态只能作为附加检查，不能替较早事件补证。
- Seen 1× (2026-11-17)

**撤回后的重新候选检查必须保留原操作历史**
- 现象与后果：真实容量验收入口在确认关闭操作已 Withdrawn 后，复用“没有任何历史关闭操作”的首次资格检查，导致 CP011/CP032 后续流程必然被原撤回记录阻断。
- 修复 commit：未提交工作区；`capacity_l6_real_boundary.rs` 使用专用检查，绑定原操作、成员、来源与代际，拒绝其他未协调操作，并核对新活动结束后的接收及候选计时。L6-ENTRY001 保留旧调用链、受控 Red 和3项本地回归。
- Done right: 复用状态检查前核对它的历史前提；从撤回或失败状态继续运行时，验证原记录和新的资格，不通过删除历史或放宽首次检查来绕过。
- Seen 1× (2026-11-17)
