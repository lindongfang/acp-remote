# R1 schema11 有限独立验收报告

2026-11-13。结论：**R1-01 至 R1-11 在本批固定源上独立验收通过**。旧schema10 Red恰1项在版本头10≠11失败；重新编译schema11后34项逐完整限定名实际通过（release19、catalog8、schema7），每项恰1 test、无ignored、无超时、stopInvoked=false、原Windows Job自然收敛为0。此结论不代签R2/R3、真实Harness或完整WP4。

## 固定身份与程序门

- Root/实际副本516叶canonical：`b9de84e397f31d3f7882c807b07ef28b47fe4774f93c6819cacce035498951aa`；index：`ac76c7ad84c1d4333fc64a710647e60aeb96015eb4407f996a4bf6adc91fbff4`。Root最终逐叶再次核验、Red finally恢复与Green副本末核均无漂移。
- 源清单：`../2026-11-13-r1-complete-facts-development/SOURCE-FROZEN.json`；HEAD仅作索引：65ec7815bb0280f4519989d93a7b4ed3b4b37994。测试副本只追加私有模块声明（0c8757f53b5ae0ee6ea22cc2bce580014a34807a4c93724c8e8b951eda8be4a7）及独立oracle，产品无额外改动。
- 正式最终动态程序门：`../2026-11-13-r1-review-followup/RF04-DYNAMIC-FOLLOWUP-GATE.md`，SHA256 b308a55ca5b6407e517e5133a5b66c00a3070f6b6c4dd112c2384a9635d6f7e6；三程序SHA见PROGRAM-SHA256.json/PREPARED.json。
- Rust1.98.1；Cargo test --lib --no-run --locked --offline --message-format=json；从本次Cargo artifact绑定PE后实际--list（950项，选34），并逐项--exact --test-threads=1 --nocapture。命令/原始stdout/stderr/result均在本目录。
- Green编译监督单列：Cargo0/non-timeout；supervisor125；仅vctip；Stop=true/natural=false；原Job清理active0。该异常真值保留，不是产品动态自然空。Green PE：`01ae2538961e77ea03201b612a87d1e408251edd30035b7e737f9d4f649eec04`；Red PE：`fdb6b935fc448acf2bc44c5df12504f185b6e3d88655c4761344be3e1a3dbf70`。未复制旧PE/target，使用Root让渡的独占target D:\Project\acp-remote\.tmp\q3-source-r1-matrix-20261113。

## R1逐ID结论

表中编号对应下方实际完整限定名及green-NN原始记录，不是测试过滤简称。

| ID | 状态 | 实际测试编号 | 断言与覆盖 |
| --- | --- | --- | --- |
| R1-01 | 通过 | 04, 13, 23 | 真实文件fresh，pragma11、metadata count0、Source四对象；合法身份写入后双11。 |
| R1-02 | 通过 | 06, 07 | 精确9成功及原release各回滚点；旧身份/事实/布局、Source零残留。 |
| R1-03 | 通过 | 08 | 精确10含完整release/security/approval事实；仅新增Source布局及预期schema头。 |
| R1-04 | 通过 | 04, 01, 03 | 原Storage关闭、share_mode0独占探针、原preflight并join、新Storage重开，全量快照相等。 |
| R1-05 | 通过 | 10 | 四个Source DDL各before/after，共8点；MigrationInterrupted、精确10全事实/布局回滚。 |
| R1-06 | 通过 | 10 | SourceMetadata/Pragma/Validation/SourceValidationAfter/Commit共5点；精确10恢复。 |
| R1-07 | 通过 | 12, 14, 15, 16, 17 | 现有有限结构/namespace/完整FK形状与坏引用负例，拒绝且保留。 |
| R1-08 | 通过 | 11 | 真实deferred FK COMMIT错误787/ForeignKeyViolation；完整精确10回滚。 |
| R1-09 | 通过 | 01, 09, 05 | 真实COMMIT成功后报告丢失；完整11实际文件关闭再新Storage重开，全部快照相等。 |
| R1-10 | 通过 | 01, 02, 03, 08, 13, 18, 19 | 空11与空10升级；Source非空无身份拒绝；单例受控多/冲突身份拒绝且保数据；release全事实。 |
| R1-11 | 通过 | 06, 07, 08, 04, 22 | exact9/10/11分离；synthetic8有真实metadata8，原catalog故障双头8与Source零残留；完整受影响组重跑。 |

## RF04实际文件证据

- fresh文件pragma11且metadata实际0行；写入合法身份后metadata为(1,11)，再原Storage关闭、独占文件probe、原preflight并join、新Storage重开；全部DDL/列/每行每列/pragma/metadata快照完全相等。
- CommitUnknown输入为精确10且metadata(1,10)，含2条release行、撤销actor、审批和audit等完整旧事实；实际COMMIT已成功、返回报告丢失，原owner停止并独占probe后，新raw scope看到完整11双头及Source四对象，再新Storage重开全量快照完全相等。旧表仅schema_version预期改变，其他每列事实保持。
- 无身份精确10文件Source四对象为0、必要事实空，实际Storage成功升级11并重开；metadata一直0行，未伪造(11,11)。这条真实路径也证明10检查没有查询不存在的Source表。
- 受控ignore_check_constraints仅fixture连接ON后插singleton2/schema10，立即OFF并查询0；原DDL不改。实际Storage拒绝UnrecognizedSchema，独占probe后完整快照（含两条冲突身份）逐项等值。
- 13次物理share_mode(0)独占探针、4例自然自清理、所有原owner join由实际断言完成；本次data目录为空，runner/target锁已释放。会话RPC producer为0的依据是本批只构造Storage/schema/preflight，未构造NativeRuntime/ACP客户端。
- 全量可读JSON在evidence/final-green-01..04；包含sqlite_schema完整type/name/table/sql、所有表（含sqlite_sequence）的列/行，NULL/BLOB及text NUL用类型前缀/hex保真。EVIDENCE-SHA256.json绑定事件，RESULT.json为机器摘要。

## Red与fixture失败记录

Red仅在独立副本反转已许可storage常量与schema Source接入两叶；保留所有新断言，唯一fresh/reopen选择器实际0 passed/1 failed，明确left10/right11，自然Job空；finally恢复两叶并核516叶。

首轮Green曾在CommitUnknown导出fixture阶段报main.node_meta缺表，尚未进入迁移；根因是内存连接OPEN_MEMORY被VACUUM内部ATTACH继承。最小修正显式file:绝对路径?mode=rwc并断言实体文件/记录database_list，随后v2在Cargo成功后即时Jobactive1被监督阻断。v3在原deadline内增加最多5秒纯观察，仍有active1并保留Stop=true/natural=false；未将该次编译计为自然退出。单次诊断证实原Job首尾仅vctip残留，最终动态门才放行固定Red PE的list/Red1，以及Green唯一编译后list/34。attempt-v2、attempt-v3和单次诊断原始证据均保留；attempt-v1保留完整旧日志/程序/绑定，未删除或冒充通过。

最终执行使用final-red/final-green新标签；旧attempt均不覆写。Red使用诊断唯一artifact固定PE，未重编；Green仅编译一次。

## 实际执行清单

| 编号 | 实际--list完整限定名 | 结果 |
| --- | --- | --- |
| 01 | `session_storage::native_release_schema::tests::file_acceptance::rf04_file_commit_unknown_is_actually_committed_then_new_storage_reopens` | 1 passed / 0 failed / 0 ignored |
| 02 | `session_storage::native_release_schema::tests::file_acceptance::rf04_file_conflicting_node_meta_is_readonly_rejected_and_preserved` | 1 passed / 0 failed / 0 ignored |
| 03 | `session_storage::native_release_schema::tests::file_acceptance::rf04_file_exact10_empty_identity_upgrades_without_querying_missing_source` | 1 passed / 0 failed / 0 ignored |
| 04 | `session_storage::native_release_schema::tests::file_acceptance::rf04_file_fresh_storage_close_preflight_and_new_storage_reopen_preserve_all` | 1 passed / 0 failed / 0 ignored |
| 05 | `session_storage::native_release_schema::tests::release_commit_report_loss_keeps_complete_schema11` | 1 passed / 0 failed / 0 ignored |
| 06 | `session_storage::native_release_schema::tests::release_precommit_faults_roll_back_to_exact_nine` | 1 passed / 0 failed / 0 ignored |
| 07 | `session_storage::native_release_schema::tests::schema11_upgrades_exact_nine_and_preserves_old_layout_and_data` | 1 passed / 0 failed / 0 ignored |
| 08 | `session_storage::native_release_schema::tests::schema11_upgrades_exact_ten_and_preserves_release_rows_and_indexes` | 1 passed / 0 failed / 0 ignored |
| 09 | `session_storage::native_release_schema::tests::source_commit_report_loss_keeps_complete_schema11_and_release_fact` | 1 passed / 0 failed / 0 ignored |
| 10 | `session_storage::native_release_schema::tests::source_ddl_and_head_faults_roll_back_exact_schema10_with_release_fact` | 1 passed / 0 failed / 0 ignored |
| 11 | `session_storage::native_release_schema::tests::source_deferred_commit_failure_is_actual_sqlite_foreign_key_787` | 1 passed / 0 failed / 0 ignored |
| 12 | `session_storage::native_release_schema::tests::source_partial_and_unrecognized_layouts_are_rejected_without_repair` | 1 passed / 0 failed / 0 ignored |
| 13 | `session_storage::native_release_schema::tests::source_schema11_allows_a_valid_empty_identity_database` | 1 passed / 0 failed / 0 ignored |
| 14 | `session_storage::native_release_schema::tests::source_schema11_rejects_broken_parent_references_without_repair` | 1 passed / 0 failed / 0 ignored |
| 15 | `session_storage::native_release_schema::tests::source_schema11_rejects_each_operation_parent_foreign_key_shape` | 1 passed / 0 failed / 0 ignored |
| 16 | `session_storage::native_release_schema::tests::source_schema11_rejects_each_registration_parent_foreign_key_shape` | 1 passed / 0 failed / 0 ignored |
| 17 | `session_storage::native_release_schema::tests::source_schema11_rejects_main_and_temp_source_triggers_and_temp_shadows` | 1 passed / 0 failed / 0 ignored |
| 18 | `session_storage::native_release_schema::tests::source_schema11_rejects_non_empty_source_tables_without_identity` | 1 passed / 0 failed / 0 ignored |
| 19 | `session_storage::native_release_schema::tests::source_schema11_rejects_valid_source_rows_without_identity` | 1 passed / 0 failed / 0 ignored |
| 20 | `session_storage::node_catalog_schema::tests::catalog_foreign_key_damage_is_necessary_integrity_failure` | 1 passed / 0 failed / 0 ignored |
| 21 | `session_storage::node_catalog_schema::tests::catalog_history_without_identity_is_missing_metadata` | 1 passed / 0 failed / 0 ignored |
| 22 | `session_storage::node_catalog_schema::tests::every_catalog_precommit_fault_rolls_back_original_eight` | 1 passed / 0 failed / 0 ignored |
| 23 | `session_storage::node_catalog_schema::tests::fresh_migration_is_valid_and_reopening_keeps_layout` | 1 passed / 0 failed / 0 ignored |
| 24 | `session_storage::node_catalog_schema::tests::fresh_namespace_shadow_and_partial_objects_are_rejected` | 1 passed / 0 failed / 0 ignored |
| 25 | `session_storage::node_catalog_schema::tests::invalid_catalog_ordinals_return_before_pool_acquisition` | 1 passed / 0 failed / 0 ignored |
| 26 | `session_storage::node_catalog_schema::tests::lost_report_after_real_commit_keeps_complete_current` | 1 passed / 0 failed / 0 ignored |
| 27 | `session_storage::node_catalog_schema::tests::required_catalog_trigger_and_external_index_are_rejected` | 1 passed / 0 failed / 0 ignored |
| 28 | `session_storage::schema::independent_schema5::cl004_reg01_all_pinned_zero_to_four_upgrade_to_five_without_any_old_fact_loss` | 1 passed / 0 failed / 0 ignored |
| 29 | `session_storage::schema::independent_schema5::cl004_reg02_each_registration_migration_fault_rolls_back_whole_transaction` | 1 passed / 0 failed / 0 ignored |
| 30 | `session_storage::schema::independent_schema5::cl004_reg02_real_commit_unknown_is_readable_from_second_connection_at_five` | 1 passed / 0 failed / 0 ignored |
| 31 | `session_storage::schema::independent_schema5::cl004_reg03_five_new_table_definition_and_identity_history_are_validated_readonly` | 1 passed / 0 failed / 0 ignored |
| 32 | `session_storage::schema::independent_schema5::cl004_reg03_half_schema_five_and_true_future_are_readonly_rejected` | 1 passed / 0 failed / 0 ignored |
| 33 | `session_storage::schema::independent_schema5::cl004_reg04_actual_strict_fk_shape_and_operation_constraints` | 1 passed / 0 failed / 0 ignored |
| 34 | `session_storage::schema::independent_schema5::cl004_reg04_full_u64_revision_blob_roundtrip_and_signed_time` | 1 passed / 0 failed / 0 ignored |

## 范围与限制

只验收R1 schema11有限矩阵与受影响catalog/schema组；未运行全workspace、strict clippy、前端或npm audit，未启动真实Harness，不证明R2 discover/create或R3认证/WSS。未改Root产品/测试叶、Git index、既有ACL或用户数据；仅按清单复制约9.94MB编译源/配置，复用离线依赖与让渡target。RF01/02/03与RF04的最终关闭由独立reviewer按本次同源证据复核，既有历史报告和任务checkbox未改。
