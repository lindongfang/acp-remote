# P1验收资产根集成独立复核

2026-10-31。已实际完整读取原request-tester`p1-root-integration-acceptance.md`（a62ec1274f40334ab609c4cbdb6ddf793d62a9c05761d9ab2ad1bab23b584041）、assets/exit清单、真实root测试与clippy日志及style diff；另对外部原lib与当前root主测试做只读no-index逐hunk比较。

**入仓组合限定通过，P1已执行公共子范围及001关闭结论保持。** 真实差异仅显式ordering相对模块路径、两个Copy统计局部变量alpha改名、闭合Version1/2的`as u8`换`u8::try_from(...).unwrap()`。结果/头偏移/预期、输入数据、费用、操作顺序和Drop不变；测试中的转换对闭合值1/2等价，不增加产品路径。没有allow/ignore或断言删除。

重算6项资产与原失败日志备份全部匹配；root主源`00146db42db0c6a78fe9f888e0d9cf68af0e43606ae5d95ed546656ffd6f9d0f`；ordering仍`7c68b790f6e6df7debb0e0229c9b537de5a8d029c47b2f20198d15ffed452238`原字节，外部lib仍2db0c260。原风格失败日志93810c37与备份一致，不被最终绿日志覆盖。

亲读`p1-root-tests.log`：11/11、0失败/忽略/过滤，包含001原三Ignored/Ignored/Stale；实际保真/峰值/FIFO/原位失败与满4096 snapshot Capacity提示均保留。`p1-root-clippy.log`全目标严格完成；`p1-root-exit.json`记录格式化、test、all-target clippy、root fmt与diff check全部0。fmt正常静默，不补造内容；Git行尾提示不冒称空白错误。原产品revision2 manifest10项本reviewer重算仍全匹配0164/0017/a071等，无产品逻辑或依赖变化。

旧95回归/9负编译/layout是同版前次独立报告证据，本次不重复计为再执行；本次只证明入仓11及工程边界。主随后完整check/build仍需以实际新组合记录，旧P0 full不代签新P1。内部u64/部分wrapper null/暂停等未动态分支、满条数首次读出、实际来源/唯一owner、P2锚点/current及完整3.2/L5L6限制全部保持，不因资产入仓扩大结论。

后续全量检查状态补记：已亲读主`.tmp/session-loop-p1-check.log`失败尾，库实际389过/1失败，失败为既有`node::tests::owner_ipc_lists_and_revokes_clients_without_a_web_identity`启动HTTPS时AddrInUse/Windows10048；该次build阶段未执行，不能说全工程通过。实际读node.rs的free_address先bind本机:0、只返回local_addr后销毁listener，NodeRuntime稍后再绑，存在未保留端口的竞争窗口；这只是解释候选，不能单凭fixture形状关闭实际失败。后续定向/全量复验由主另记，不能用本报告针对11项的通过代签这次全量失败。

## 根组合最终全量复检

之后按独立预审，仅IPC-only cfg(test) case使用localhost:0，未改产品启动。node当前b959b31c，撤回该唯一注释/地址hunk整文件精确回已验c97a0f51；其fixture单验1/1和完整复验由`p1-node-port-fixture-review.md`记录，原10048失败继续保留。

已实际读取根`2026-10-31-p1-full-check.txt`/`full-build.txt`：Web50、lib390、JWS4、平台12、session_loop_acceptance69、P0新6、P1新11、预算20/allocator6均通过（公共集成各二进制合计112），无失败/忽略；fmt/all-targets clippy完成，Web/Rust实际build完成，主执行记录两个完整入口actual exit0。此新组合已包含P1入仓11，旧P0日志不再被用来代签P1。

完成时本reviewer重核P1 revision2 frozen10项仍全匹配0164/0017/a071等；根P1两测试源仍00146db4/7c68b790，与已核外部适配仅样式/path差异。node cfg fixture为独立窄例外，未回填原10快照。故**P1已执行公共子范围的根集成与此次全工程检查可限定收口通过**，但不是P1所有私有故障分支/完整3.2通过。

私有u64/部分wrapper分配故障、满4096首次内容读出、实际连接source/唯一owner/操作者授权、P2A/B全字段current/scanner/原位工具完整义务、轮次/分页/安全渲染及真实Harness/WSS/L5L6仍沿原报告待办。本次全绿不证明这些未装配能力，P2A只有独立方案/用例门槛，不借此声明已实现。
