# WP1.4 原生模块导入与全部feature编译

2026-11-10，主执行/coder。工作区D:/Project/acp-remote，HEAD 65ec7815bb0280f4519989d93a7b4ed3b4b37994；本次16文件源码摘要见SOURCE.json，修改前字节副本保留，13文件导入清单见CHANGES.json。未改Git索引，未提交/推送/发布/归档。

## 实际修改

- application/native_dispatch的13个文件移除45个编译器报告的未使用导入。逐文件排除use super导入区后进行token对照，13/13其余代码一致；未删除能力或运行逻辑。
- 全部feature编译暴露五类缺失名称（lib 1条、lib-test 25条编译诊断）。shared_view_test_support改为直接导入tokio::time::Instant，避免非test的feature构建依赖node模块cfg(test)别名。
- approval_records测试专用方法使用ApprovalReplyId完整类型路径。
- native_dispatch模块仅在test且审批dev/acceptance feature开启时，对crate暴露已有ApprovalRepairMode、ApprovalRepairScope、ApprovalReconcileOutcome，供现有节点测试使用。不扩大常规产品API，不改变恢复算法或断言。

## 验证

隔离CARGO_TARGET_DIR=.tmp/acpi-route-v11-checks；均使用锁文件和离线依赖。

- cargo check --manifest-path app/Cargo.toml --all-targets --all-features --locked --offline：初次失败日志all-features.*保留；修复后三者引用问题消除，最终exit0（44.75s），见all-features-fixed.*。这是编译成功，不代表所有feature对应测试已运行。
- cargo clippy --manifest-path app/Cargo.toml --all-targets --locked --offline --message-format=json -- -D warnings：最终exit101，lib377、lib-test361诊断；相较本轮前422/406各减少45条。二者重复，不相加为独立缺陷数量。最终完整诊断见clippy-final.jsonl和stderr。
- cargo fmt --manifest-path app/Cargo.toml --all --check：exit0。
- git diff --check：exit0，换行警告保留。
- openspec validate --all --strict：11/11通过。

本轮没有新增产品行为，未为导入清理新增镜像测试，也未宣称重新执行真实Harness或Web。先前队列30/30仍属其受测快照，本轮以全部feature实际编译核验引用正确性。剩余strict错误、完整Rust执行、独立tester/reviewer、真实Harness/Web和资源清理仍待；1.4不勾，总体9/28。npm audit按用户要求不执行。旧v11冻结及报告不回填为本轮源码通过。