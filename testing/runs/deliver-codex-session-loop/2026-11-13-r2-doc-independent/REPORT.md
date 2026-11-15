# Doc 窄切片独立验收

2026-11-13。固定7项全部实际通过，所有原Job均自然退出、Stop=false、active0、无超时；未编译。

源517叶canonical：`21137d04eb1cfcdce87342d363d29ccaa9734b2f75f716aaec760fe2cd8061f1`。Root与隔离源执行前后全部逐叶相符，index未变。固定PE SHA：`7d8789785a2728360850daa858140b7beaeaa7948da66a0c2b806fcd0af57fc1`，与本批Cargo唯一libtest artifact一致。原API Red只读复核：恰2条E0308，三个API叶精确回置R1，其余509个共同叶与Green一致（包含本次固定测试与Node实现），不计运行时Red。原Red清单512叶未包含Green清单中的Web dist三叶、rust-toolchain.toml和testing/ci/windows-owner.ps1五个支持叶，不能将其描述成517叶完整Red绑定；具体清单与原诊断见RED-READ.json。

本runner首次静态前置误将Red清单按517叶全量对照，在任何list/test进程启动前即闭锁，原错误保留为PREFLIGHT-ERROR-v1.json。查明是上述五个支持叶未纳入原Red清单后，仅修正前置身份范围，没有改断言、产品、原Red日志或固定PE；随后只启动本轮一次list及7项exact动态。本报告修订仅澄清这一证据边界，不重跑测试或改动原动态证据。

独立检查断言：From原allocation ptr/bytes/len；同一生产owner真实payload slot；snapshot scope/current/bytes与retire；原Queue raw到实际socket及final credit；edit/cancel seal；同actor另一socket拒绝；descriptor u64十进制和null；completed/empty任务reaping；publication failure保持flush而不commit。六个Node例均断言原shutdown/join/registry/pool、唯一disposal sealed/joined、share_mode0数据库探针及目录锁释放；原stderr确认joined/metadata_confirmed和area removed。

| 编号 | 完整限定名 | 实际结果 | 进程结果 |
| --- | --- | --- | --- |
| 1 | `node::tests::document_transfer_development::document_d1_completed_and_empty_jobs_are_reaped` | 1 passed / 0 failed / 0 ignored | natural=true; Stop=false; active0 |
| 2 | `node::tests::document_transfer_development::document_d1_descriptor_canonical_decimal_strings` | 1 passed / 0 failed / 0 ignored | natural=true; Stop=false; active0 |
| 3 | `node::tests::document_transfer_development::document_d1_original_queue_raw_to_real_socket_and_final_credit` | 1 passed / 0 failed / 0 ignored | natural=true; Stop=false; active0 |
| 4 | `node::tests::document_transfer_development::document_d1_publication_failure_retires_without_later_command` | 1 passed / 0 failed / 0 ignored | natural=true; Stop=false; active0 |
| 5 | `node::tests::document_transfer_development::document_d1_real_edit_cancel_seal_without_waiting_for_credit` | 1 passed / 0 failed / 0 ignored | natural=true; Stop=false; active0 |
| 6 | `node::tests::document_transfer_development::document_d1_same_actor_other_socket_cannot_take_the_consumer` | 1 passed / 0 failed / 0 ignored | natural=true; Stop=false; active0 |
| 7 | `node::tests::document_transfer_development::document_d2_wrapped_snapshot_preserves_original_allocation_and_scope` | 1 passed / 0 failed / 0 ignored | natural=true; Stop=false; active0 |

每个label独立owned TEMP/TMP/p1/rpc，wrapper在初始化原owner前设置，执行后各目录自然为空。复用reviewed wrapper SHA `b321060b54828439c1776d75c9e6738d928ac8ddaba1eec1a28191302b81027f`，不改ACL、不清用户数据；只读Root产品和R1原证据。命令、原stdout/stderr、supervisor、Job结果、cleanup均在本目录。

范围仅Doc七叶窄切片；不签Source producer、完整R2/R3、真实Harness或WP4。未执行编译、全workspace、前端或npm audit。
