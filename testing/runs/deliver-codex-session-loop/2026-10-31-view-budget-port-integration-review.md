# 预算端口验收资产入仓独立复核

2026-10-31。已实际完整读取原tester`view-budget-integration.md`（fe969c929429e357d7fe1f1f2d3562bf3e3ae60c1849cb181160ed7e46e640af）、三资产清单、真实运行/clippy日志及三份exit JSON；对原外部probe三源与根三个文件分别执行只读no-index diff并逐hunk复核。

**集成限定通过，原纯Domain验收结论仍有效。** 实际差异只有两个test binary的support path、100000→100_000、两个const helper、五个局部变量重命名、一个不再使用的NativeSessionKey冗余clone去除。没有改变测试数、输入/预期、已收费租约、Drop/原子操作顺序、allocator guard或故障尺寸；没有新增allow/ignore。clone仅是外层原生身份值，原函数后续不再引用局部t，未删任何ItemLease引用。

独立重算根三资产全部匹配：

| 文件 | SHA256 |
| --- | --- |
| app/tests/view_budget_acceptance.rs | 2b504bc9d89feaaedf50ae4b48094a9ecf1e46f301314507a882e1a8b696470d |
| app/tests/view_budget_allocator_acceptance.rs | c9c7aa38019c02caf5824660be248992e5f93c2bdd29a59a36f4e1228e011f3a |
| app/tests/acceptance/view_budget_support.rs | bcd680e034916be05460ed387a15edce38513ba85b6807b19d88bc27eb7d998a |

根真实运行20+6=26项、0失败/0忽略，原日志包含6次bulk null、精确Layout及publish指针不变/realloc0、零分配status；cargo test、root all-targets clippy -D warnings、三资产rustfmt检查exit JSON均为0。allocator保留在独立`view_budget_allocator_acceptance`二进制，没有并入原session_loop_acceptance。首次28项测试风格lint失败保留，不误称产品缺陷或删日志换绿。

本reviewer同时重核原预算manifest10项，全部仍为a22bb6e5/d2a2a6ec/d6等冻结摘要，原19及依赖/工具链未动。原外部45/14负/16正证据不改；本次26是实际根集成归属，不重复扩算为更多产品能力。原19、负编译未在本次风格回归重跑，因相同产品/预期及原资产不变而复用原同版证据。

此结论允许Domain README仅把该子阶段“待独立验收”改为已通过并保留不代表ACP/真实owner完成的限制；文档改动需另记录新摘要，不能回填原10项冻结快照。完整根npm check/build若随后重跑，另记实际新组合范围。本报告不签P0、ACP P1/P2、任务3.2整体或真实Harness/WSS。
