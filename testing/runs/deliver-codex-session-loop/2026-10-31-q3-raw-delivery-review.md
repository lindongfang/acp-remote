# Raw JSON 基础层候选v1：限定交付复核

2026-10-31。**PASS：同源基础层实现及独立RJ01–30验收通过，可按既定顺序纳入集成。** 这是本候选的最终限定结论，已包含独立动态证据，非仅静态结论。根仓库先集成R3、后集成raw，并在最终组合源完成工程检查；这些集成检查尚不能由本报告代签。

## 版本与证据亲核

- 合同9f2b236f、固定定义d172289c保持原字节；冻结源码清单 `7b1a6d69f0c931adf5d3e3bf1010f5fe85bd5877dfac0d374e67d80a30e65287` 127项已由reviewer逐项核验。
- 独立报告 `session-loop-q003-raw-tester/acceptance-report.md` SHA256 `98cfa126c50631bd27c861a452cb0964a00dadbefa423289fb0984f927a50516`；逐项结果 `cbb210aaa347145b4d264f739ee70d01d6af6eb814561163be10057385ab15c1`；evidence-manifest `920663ebdde965be73a36624cf522c90816c1b61e4eee4cece571ba66d650460`。
- 亲核证据清单24/24文件同hash；before/after各132项（127冻结源+5实际构建输入），期望、前后及当前独立workspace实际文件均匹配，无缺失/差异；锁文件87f27b61不变。实际构建输入包括原toolchain、三份Web dist和JWS fixture，未把它们冒认已在127清单中。
- 唯一入仓资产 `delivery/app/tests/raw_json_acceptance.rs` SHA256 `1853768ef419f0838f543cc6c050c0ecb932f961e5f86638bbf3f5b81f2ce43a`，已读全部函数、辅助子进程及trait/泄漏断言，before/after及delivery相同。逐30条ID均唯一、PASS，原定义行完整对应，函数名和行号与最终资产准确匹配。无产品源overlay或新增依赖。

## 实际结果与充分性

亲读 acceptance-frozen.txt（45f6e605…）及其真实exit0记录：主harness **33/33**，为独立固定RJ函数30+原管理模块内置3。RJ23/24/25/26额外各自运行同一可执行文件的精确单项子进程，四个均1/1、exit0；内部256KiB线程正常join。120秒仅收敛自有测试子进程，不是平台性能/内容门槛。

大正文超过32MiB、262145叶数组、131073键对象均完整比较而非摘要；20000层纯数组及混合对象、64组共享clone/owned/drop和深重复键/孤立surrogate/畸形语法失败清理真实通过。未知大数词法、严格scalar与decoded重复键、首错次序、精确整数边界及-0、浅借用/精确双端迭代、compact四空白、五句柄/全部错误canary均实际执行。小栈测试oracle不构建递归Value。

原公开JWS/ACP四目标实际4+6+19+11=40/40，原管理库3/3，ACP库69/69（含开发raw8，单列不充独立30）；严格all-target all/pedantic/nursery -Dwarnings及fmt实际exit0。已亲读原日志/退出记录，未声称reviewer亲自重跑。缓存hardlink回退复制警告与功能结果分列，不是失败。首次Clippy的测试资产提示及修订历史保留，局部allow维持原验收分支；最终资产冻结后重跑通过。

原管理strict_json.rs通过相对path原字节编入integration test，运行8层成功/9层TooDeep；这是原私有模块源码在测试crate复用，未扩大公开API。其三个原cfg(test)测试保留，管理原调用点及源码前后同hash。共享索引/平面Drop/单次扫描复杂度/checked及try_reserve由先前同源静态报告cf31726a佐证，未用动态耗时冒充复杂度证明。

## 集成条件与剩余边界

同源纳入上述唯一1853768e资产和已审6路径实现；不改原RJ定义、旧ACP资产或原产品接线。先R3后raw，在实际组合源执行适用fmt、严格clippy、check/build与回归，并冻结最终组合清单。若组合改变本候选产品字节或oracle，判断影响并独立补验，不能将本PASS无条件搬到新版本。

真实OOM仍not-run；Allocation仅变体/文案动态与显式reserve静态，不保证所有依赖分配可优雅失败。此阶段未迁移RawMessage/Strategy/transport/P1/P2，旧32MiB帧、96层/131072-token限制和缓存门槛仍待后续整体替换。**不关闭CL-095/103整体、不勾任务3.1/3.2/6.3，不证明实际NDJSON/outbox、无上限共享缓存、reader/writer、Harness/WSS/浏览器或独立安全/供应链审查。** 用户禁止的Web audit未执行；无外发、安装、提交或发布。
