# Q3 共享输入队列纯领域独立验收

结论：固定版本的独立公开 API 用例 **39/39 通过，0 失败、0 忽略、0 过滤，实际退出码 0**。结论仅限已审查的 L3 纯领域子范围；不完成原 CL 的 L5/L6、完整 WP2 或 task6.7，不代签 reviewer。

## 版本与独立性

- HEAD：`65ec7815bb0280f4519989d93a7b4ed3b4b37994` 加既有未提交源码。tester仅修改自有外部目录；经主执行明确授权在根`.tmp/q3-test-target`生成独占构建缓存，不改根产品源码、Git索引或用户数据。
- 固定产品清单：`frozen-source.json`，SHA-256 `4c126414df06228f62e954aa2d7d3eabdaabfbca7f054fcba6c79d5356d85ee1`，122项。`source-before.json`及`source-after.json`分别逐项核对，均122项匹配、0差异。
- 队列源码：`app/src/domain/input_queue.rs` SHA-256 `e5e4d8f33b2f265635bd426106beebbd58ed55a4df6e0c167c2e197a441d0e99`；开发测试 `43650392c028da53098bc58608e1ada3c10ee68ed34882fe49e4eb7f141d5aa2`。
- 合同：`61987328a83927c414fba8a2b085f0796f05792a8072ae9ad11d6187d8e1c466`。独立预期：`q003-contract-api-definition.md` SHA-256 `9e1c407e6d000b827b03bcd02482fe3d217cde18a2c75d7f83e2cbe0d966d93c`；新增初始editor语句仅明确定义中既定初值，reviewer允许原定义复用。
- 最终可入仓资产：`shared_input_queue_acceptance.rs`，SHA-256 `07f19cd4a090ed936f6abda444fd228c31bbc8fd0a806464434141e411ba2fdd`。与probe实际执行源码逐字相同，可直接复制到 `app/tests/shared_input_queue_acceptance.rs`，无需内容修改。
- tester未继承coder对话，依据固定定义编写公开消费者测试。无 include 私有源码、生产setter或源码可见性改动；只使用现有公开crate入口。39项测试均有实际断言，静态直接assert宏调用点逐项记录于`case-results.json`，helper及循环另有断言；未为运行时断言总数插桩，不把宏点数冒充实际动态次数。

## 执行环境与依赖

Windows x86_64-pc-windows-msvc；rustc 1.98.1 (48a229cea 2026-09-01)、cargo 1.98.1 (797e8a9bc 2026-08-05)。独立probe将根app作为path依赖，目标仅`shared_queue_public_api`；所有cargo调用均offline，未安装或新增registry依赖。复制app/Cargo.lock后，Cargo为外部path消费者裁去不适用的dev-only包并加入本地probe包：根261项、probe238项；共同package版本/source/checksum无变化。详细比较见`lock-comparison.json`，根锁SHA保持`87f27b619281b37015c79ceae8d95471c8a4de50f202f4dd8a6d99fa0d27e324`。

测试不连接网络、数据库、认证通道或Harness，不读取私人数据。正文均合成；超旧预算项实际构造32MiB+1字节String并逐字校验，结构正文使用未实现Clone/Debug/序列化的Payload，验证公开泛型接口无需这些trait。

## 实际命令与结果

工作目录均为 `D:/Project/acp-remote`，`CARGO_TARGET_DIR=D:/Project/acp-remote/.tmp/q3-test-target`。`<probe>`为外部绝对路径 `C:/Users/jack/.codex/visualizations/2026/10/30/01a18dc1-45b2-7f82-981e-964264221cef/session-loop-q003-tester/probe/Cargo.toml`。

```text
cargo test --manifest-path <probe> --test shared_queue_public_api --offline --locked --no-run
cargo test --manifest-path <probe> --test shared_queue_public_api --offline --locked -- --test-threads=1 --nocapture
cargo clippy --manifest-path <probe> --test shared_queue_public_api --offline --locked -- -D warnings
rustfmt --edition 2024 --check <专属目录>/probe/tests/shared_queue_public_api.rs
```

- 编译：`compile-attempt3.txt` / exit文件为0。原始预风格资产首次实际39/39成功见`public-api-test.txt`；最终资产重新编译并执行39/39，1.05秒，见`public-api-final.txt`及`public-api-final-exit.txt`=0。
- 严格静态检查：probe Cargo.toml明确启用与根app相同的clippy all/pedantic/nursery warn；`clippy-final.txt`实际exit0。最初两轮静态检查反馈helper可const及Result可消费写法，日志`clippy-attempt1/2.txt`保留；只修helper写法，无断言/预期/用例删除，随后最终39项重跑通过。
- 格式：`fmt-final-exit.txt`=0，日志为空表示无差异。未运行旧全矩阵或把主执行全量自检计为独立39项。
- 环境失败：前两次较长的外部target路径在ring的MSVC lib.exe归档报LNK1114/错误5；第一次仅工具输出摘录、第二次完整日志分别见`compile-attempt1-notes.md`、`compile-attempt2.txt`。短target成功，但未确认错误5根因；不将它们写为产品断言失败。

## 未验证及证据复用

- MAX revision/sequence没有合理的公开构造入口，因此未做独立公开动态验收，也未新增测试后门。仅读取固定开发测试中的`revision_exhaustion_preserves_body_editor_and_existing_preparation`、`sequence_exhaustion_preserves_waiting_and_never_wraps_or_reuses_token`及实现checked_add路径。主执行同快照全量日志 `testing/runs/deliver-codex-session-loop/2026-10-31-q3-full-check.txt` SHA-256 `2792fe316de54a13347f5d4668af01f241422ba20496e14dccaa5adb81c282f5`中九个开发队列测试含这两项确已运行通过。此为读取/复用开发证据，不计独立39动态数量，不代签独立reviewer静态结论。
- `Blocked`只有一个合成输入分支；没有分别证明真实active、unknown、pending、无效资源来源或ACP终态关联。Advance/Suspend是受信owner调度决定，未证明真实成功、失败、零发送或未知资源已收敛。
- 两种顺序测试证明同一`&mut`实例的线性状态机，不证明应用已按NativeKey唯一装配，也没有多线程barrier或真实多客户端竞争；同key异实例仅用于token防串测试。
- 无TTL/时钟的API和无状态查询、全新实例为空是模型边界证据；未等待真实一天、实际校时、进程重启、检查SQL/临时文件无正文或证明旧ID持久拒绝。
- 认证/撤销/读取权限、请求登记及SQL原子性、真实writer交接、审批owner、WSS/浏览器同步、真实Harness和Windows树均未运行。不执行用户拒绝的Web审计，不外发依赖或私有数据。
- 非Clone正文的正编译已验证；未额外执行Queue类型的Clone/Serialize负编译，故不宣称以类型失败探针证明所有禁止trait。正文/输出物理资源也不从一次大输入推导为无限。

未发现本阶段产品失败，无需修改或放宽固定预期。最终独立交付检视仍由reviewer负责。自有target缓存和测试材料为便于复现暂保留；没有自建服务、子进程树或端口需要清理。

## 逐 API 执行映射

以下从实际最终测试日志和固定定义逐项匹配；源码直接断言宏点数保存在JSON，helper和循环次数未充作额外用例。

| 子断言 | 实际 Rust 测试名 | 结果 |
| --- | --- | --- |
| CL-087.API01 | `cl087_api01_capacity_last_slot_orders_and_normal_ready_queues` | PASS |
| CL-087.API02 | `cl087_api02_preparation_stays_in_three_waiting_slots` | PASS |
| CL-087.API03 | `cl087_api03_targets_and_same_key_instance_token_fence` | PASS |
| CL-088.API01 | `cl088_api01_edit_identity_version_and_all_stale_mutations_unchanged` | PASS |
| CL-088.API02 | `cl088_api02_cancel_releases_slot_without_second_cancel_effect` | PASS |
| CL-089.API01 | `cl089_api01_fifo_and_active_blocking_with_independent_waiting_slots` | PASS |
| CL-090.API01 | `cl090_api01_priority_waits_for_ready_and_preserves_other_relative_order` | PASS |
| CL-090.API02 | `cl090_api02_priority_change_and_edit_invalidate_old_preparations` | PASS |
| CL-090.API03 | `cl090_api03_last_priority_then_cancel_clears_selection` | PASS |
| CL-090.API04 | `cl090_api04_normal_full_draft_and_priority_cannot_bypass_active` | PASS |
| CL-092.API01 | `cl092_api01_dispatch_has_no_original_actor_or_gate_argument` | PASS |
| CL-095.API01 | `cl095_api01_nonclone_nondebug_structured_body_moves_without_loss` | PASS |
| CL-095.API02 | `cl095_api02_over_32mib_body_and_edit_still_count_as_one_slot` | PASS |
| CL-096.API01 | `cl096_api01_node_processing_order_has_no_clock_input` | PASS |
| CL-097.API01 | `cl097_api01_observation_and_blocked_prepare_do_not_expire_items` | PASS |
| CL-098.API01 | `cl098_api01_fresh_instance_is_empty_after_old_instance_drop` | PASS |
| CL-099.API01 | `cl099_api01_cancel_before_begin_and_begin_before_cancel_or_edit` | PASS |
| CL-099.API02 | `cl099_api02_edit_before_begin_requires_current_revision` | PASS |
| CL-099.API03 | `cl099_api03_begin_rechecks_blocked_and_every_epoch_component` | PASS |
| CL-099.API04 | `cl099_api04_release_keeps_position_and_old_tokens_are_not_reused` | PASS |
| CL-099.API05 | `cl099_api05_clone_settles_once_and_foreign_tokens_cannot_read_or_settle` | PASS |
| CL-110.API01 | `cl110_api01_full_suspension_direct_new_work_and_advance_keeps_old_suspended` | PASS |
| CL-110.API02 | `cl110_api02_edit_observe_and_prioritize_do_not_resume_old_items` | PASS |
| CL-111.API01 | `cl111_api01_waiting_new_item_gets_last_slot_and_dispatches_before_suspended` | PASS |
| CL-111.API02 | `cl111_api02_new_failure_suspends_all_remaining_items_without_requeueing_failed_input` | PASS |
| CL-112.API01 | `cl112_api01_resume_retries_do_not_duplicate_or_preempt_active_new_input` | PASS |
| CL-112.API02 | `cl112_api02_resume_places_existing_runnable_d_before_old_xy` | PASS |
| CL-112.API03 | `cl112_api03_resume_invalidates_prepared_d_without_changing_new_first_order` | PASS |
| CL-113.API01 | `cl113_api01_two_linear_orders_only_one_direct_owner_and_no_fourth_waiting_slot` | PASS |
| CL-113.API02 | `cl113_api02_blocked_full_and_existing_runnable_or_prepared_item_forbid_direct_bypass` | PASS |
| CL-113.API03 | `cl113_api03_direct_active_is_retained_when_last_transport_conditions_change` | PASS |
| CL-115.API01 | `cl115_api01_other_editor_cancel_and_resume_during_active_new_input` | PASS |
| CL-115.API02 | `cl115_api02_resumed_cancel_send_order_and_stale_revision_keep_state` | PASS |
| CL-087.API04 | `cl087_api04_duplicate_waiting_or_active_id_precedes_full_and_returns_new_input` | PASS |
| CL-088.API03 | `cl088_api03_equal_body_edit_advances_version_and_mutation_errors_are_ordered` | PASS |
| CL-099.API06 | `cl099_api06_busy_and_stale_precede_blocked_then_epoch_error` | PASS |
| CL-112.API04 | `cl112_api04_resume_complete_noop_preserves_prepared_token` | PASS |
| CL-099.API07 | `cl099_api07_explicit_zero_send_settle_returns_content_without_requeue` | PASS |
| CL-095.API03 | `cl095_api03_debug_redacts_body_without_requiring_debug_generic_parameter` | PASS |
