# WP1.4 队列锁作用域与未使用导入修复

2026-11-10，主执行/coder；工作树D:/Project/acp-remote，基线HEAD 65ec7815bb0280f4519989d93a7b4ed3b4b37994。本轮增量七文件摘要见SOURCE.json，原字节副本以before保存；未提交、未改变Git索引。

改动：queue_owner/tests.rs五处显式drop(state)改为局部词法作用域，在SQL await前结束锁的作用域。原来已有显式drop，不宣称发现生产死锁；保留全部原测试动作与断言，经差异与去除空白/块括号/drop(state)后的token对照确认。另仅删除三个task_slot模块的未使用shutdown trait导入和三个Storage子模块的未使用Row导入。无产品行为、协议、生命周期顺序或验收预期变化，无新增allow和依赖。

验证：Windows当前用户非沙箱owner，testing/ci/windows-owner.ps1；独占target=.tmp/acpi-route-v11-check，TEMP=.tmp/acpi-route-v11-runtime-unrestricted。

- cargo test --manifest-path app/Cargo.toml --lib --locked --offline application::queue_owner::tests:: -- --nocapture --test-threads=1：作用域修复后30/30；最终六处导入修复后再次30/30、882 filtered、exit0（queue-final.*）。
- cargo fmt --manifest-path app/Cargo.toml --all --check：exit0。
- git diff --check：exit0，既有换行警告保留。
- cargo clippy --manifest-path app/Cargo.toml --all-targets --locked --offline --message-format=json -- -D warnings：最终exit101。lib 422错误、lib test 406错误，配置间诊断重复，不能相加为独立问题数。五处await_holding_lock已不再报告；六处未使用导入已移除。原始及最终JSON诊断、stderr与退出码均保存。

remaining-diagnostic-counts.json是编译器消息按lint code分组的数量，包含lib/lib-test重复，不等于去重后缺陷数量。剩余主要包括dead_code、unused_imports、significant_drop_tightening和too_many_lines；不为全绿自动删除未接线能力或提前释放承担串行责任的锁。

范围：本轮不是独立tester/reviewer签字，也不是完整Rust、真实Harness/Web验收；1.4未完成，整体9/28。v11 SOURCE原封不动保留，其证据只属旧source set；本轮七文件变化不回填旧清单。完整集成冻结和必要回归仍待后续执行。npm audit按用户要求未执行。