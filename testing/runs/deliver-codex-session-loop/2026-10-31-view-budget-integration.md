# 预算验收资产root集成风格适配

2026-10-31。仅三个获授权测试文件修改，**root集成20+6=26项实际通过，0failed/0ignored；root全目标严格clippy -D warnings及三文件rustfmt检查均exit0**。冻结预算生产清单10项前后仍全部匹配，没有修改产品源码、依赖、原19预期或外部原冻结证据。

初次root严格clippy的28项测试风格失败保留于原.tmp/session-loop-budget-integrated-clippy.log，并在budget-integration-before/strict-clippy-failure.log备份；root集成初次26绿日志及三源适配前逐字副本同目录保留。旧formal报告a87b08f8...和外部probe/日志/SHA全部不改，本报告为root集成新增归属，不回填旧证据。

## 精确语义diff

- 两测试文件的100000改为100_000，数值和全部预算/数据/allocator注入尺寸不变。
- support的epoch/key变为const fn，函数体不变。
- 三个局部序列中五个变量重命名：两lease、两stage、一个key字符串，所有调用/断言及Drop次序保持。
- 首个测试中未再使用的NativeSessionKey局部值直接move进begin，删除一处冗余clone；未删除已计费ItemLease或任何实际引用/账本断言。
- 没有新增allow、ignore、测试删除、预期或流程调整；globalallocator、六次null、零alloc/status、峰值/CAS/Drop/旧代际的实现和断言保持。原显式support path保留。

已逐hunk比对budget-integration-before三源与root最终文件，上述以外无变化。

## 实际命令与退出码

独占CARGO_TARGET_DIR沿原tester/activity-target，本次进程CARGO_INCREMENTAL=0；Windows/rustc1.98.1/Cargo锁与原formal同版，没有Web audit或网络外发。仓库根执行：

```powershell
cargo clippy --manifest-path app/Cargo.toml --locked --offline --all-targets -- -D warnings
cargo test --manifest-path app/Cargo.toml --locked --offline --test view_budget_acceptance --test view_budget_allocator_acceptance -- --nocapture
rustfmt --check --edition 2024 --config skip_children=true app/tests/view_budget_acceptance.rs app/tests/view_budget_allocator_acceptance.rs app/tests/acceptance/view_budget_support.rs
```

真实退出码分别在budget-integration-clippy-exit.json、budget-integration-tests-exit.json、budget-integration-fmt-exit.json均为0。原始日志budget-integration-clippy-attempt1.log、budget-integration-tests26.log保留。PowerShell对cargo正常stderr进度的NativeCommandError文本不当作编译错误，使用同shell采集LASTEXITCODE及工具完成exit0。

运行20行为+6allocator全部通过，六次真实bulk null命中、公共layout、指针不变/realloc0以及零alloc status仍在root日志实际出现。独立allocator二进制保持隔离，不将其globalallocator链接进原session_loop_acceptance二进制。旧19/14负编译/16正控制未在此风格回归重复；产品10项同SHA未变且原冻结外部证据不变，保留原formal同版归属，不把26扩成重复45。

## root最终资产

| 文件 | SHA256 |
| --- | --- |
| app/tests/view_budget_acceptance.rs | 2b504bc9d89feaaedf50ae4b48094a9ecf1e46f301314507a882e1a8b696470d |
| app/tests/view_budget_allocator_acceptance.rs | c9c7aa38019c02caf5824660be248992e5f93c2bdd29a59a36f4e1228e011f3a |
| app/tests/acceptance/view_budget_support.rs | bcd680e034916be05460ed387a15edce38513ba85b6807b19d88bc27eb7d998a |

清单budget-integration-assets.json；生产/合同/锁与原19等10项核对结果budget-integration-production-after.json全部true。测试进程结束，没有自建数据库、端口、后台任务或用户数据操作。全局OOM/内部不可达极值/真实ACP等边界继续沿原formal，未新增产品能力通过结论。
