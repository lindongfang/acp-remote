# P1-CODE-001 revision2独立复核

2026-10-31。只读产品及明确的初版备份，不改独立用例。实际读取根`2026-10-31-p1-revision2-source.json`并重算10项全一致：main `0164d462fa3bd16740a664a02407fc67a13176ddfb2b1f73dfba8e4381ee225a`、开发测试`0017c3742eeb53dfb62496d7addf6ab593c57fa7d68a9424479dfb04a0bb548f`，其余8项仍初版摘要。

**P1-CODE-001 Closed；当前已执行raw公共API子范围限定通过。** 静态窄修及原三断言同版动态复验均成立，原始证据已实际读取。不是P1合同每个内部故障分支全验，也不是整体3.2完成；具体未执行分支保留在末节。旧初版1e38报告和实际root-lock三失败保留，不覆盖原结论。

逐hunk比较初版备份与当前：生产只有observe一处修改，先从event借用session/typed State，OtherSession或协商v2 State先Ignored；匹配显示事件再window_status验证原handle、context验证raw形状/一致性；原raw tag为v2 state_update仍保守Ignored；最后才closed/checked seq和构建。没有String/Value/Strategy clone、新分配、预算/格式/Drop或Connection逻辑变更。v1同名未知tag不被当v2控制吞掉。此顺序修复原三负例的确定路径，没有放宽matching current的InvalidObservation检查。

开发测试原一项把OtherSession期为InvalidObservation，与已批准Ignored定义冲突；现在只改fixture为相同目标但错kind，错误预期仍InvalidObservation，并新增OtherSession、v2 State、matching Stale三顺序回归。没有改原独立三断言。开发9/9、严格clippy仅作输入，待独立记录，不与之前跨改写结果混用。

初版backup10项本reviewer全部重算符合原manifest，新版10项全部符合新manifest；未改strategy72ad/transport93bd、Domain a22/d2、锁87f或a071合同。实际payload/header/收费/owner来源未装配及满4096 snapshot Capacity的范围限制沿初审报告，不因修复顺序而改变。

初审时尚待原tester同版完整公共API、真实协议/长流/预算/负编译、布局及可测故障/不可达极值分类、前后源码/资产/锁与退出记录；最终已读证据见下文。源码写期间的原negative/regression/clippy不计入本版；若后续集成独立资产，另记根组合归属。无真实Harness/owner/WSS/完整3.2通过结论。

## 最终同版证据与001关闭

已实际完整读取原request-tester正式`p1-revision2-acceptance.md`并重算SHA256为`a992f3c34b1b17bbc73762afb66f3f2d87ef35c2e2643d83317995186a82f77c`；完整亲读普通探针src/lib.rs、原ordering.rs、negative源及其原始日志/分类、layout、95回归与clippy/fmt退出记录。原ordering源码仍`7c68b790f6e6df7debb0e0229c9b537de5a8d029c47b2f20198d15ffed452238`字节未变，三项现在分别Ok(Ignored)、Ok(Ignored)、Err(View(Stale))，原stats/usage/新窗口Reading断言也全部通过。结合生产唯一hunk，001由原提出者关闭。

固定版实际11/11，无失败/忽略/过滤；包含原8基础与原3修复反证。另原session_loop_acceptance69、预算20+6=95真实回归通过；layout Read304/Stats56/Raw32且Read+scratch1328≤1536；负编译9项落在预期Clone/Serialize/Deserialize/From、setter不存在、私有字段、借用型raw非拥有Arc及生命周期表达式（4×E0277、E0599、2×E0616、E0308及lifetime），不是导入错误。matrix/layout/regression/clippy/fmt真实exit0，negative按预期101；0项layout unittest/doc-test不计功能数。原PS进度stderr包装不作错误或通过判据。

本reviewer对最终10资产重新SHA256全部匹配；对普通probe/negative两锁各自独立解析name/version/source/checksum：均238包，其中237包精确来自根261包，只新增各自本地探针，无新registry版本。最终tokio1.54.1与根锁一致；首轮旧锁及跨改写结果继续排除。新版source-before/after均为0164/0017等10项匹配，完成时另核根仍须相同；不把纯路径依赖的normal build当cfg(test)私有入口。

公开路径实际覆盖：两版完整通知含外层_meta/未知字段/数值与转义逐byte、逐raw origin/seq和Replay→Live、不因响应归零、P0真实v1null、Ignored控制及另一目标、预算失败后持续peer与零自动控制字节、差1/恰好5042峰值/旧read+snapshot跨代到最后Drop、严格头所有截断/版本/保留位/长度/尾随/借用、原三误用顺序。真实13,000小条保存进度达到tail13000/retained4096；另205×至少256KiB的字节路径可snapshot读取尾seq205。两者证明范围不同。

## 明确保留的验收与交付限制

1. 8f8e R2/R7内部seq/drop u64极值未从普通API动态触达，仅独立静态检查checked/closed；开发私有测试不代签。R3的P1 wrapper已FIFO后的真实null、R5 per-frame scratch null/构建中暂停/短写hook也未在wrapper独立注入，只有同版Domain六项allocator回归证明内层。**本次不能宣称全部八组每一分支都已动态执行**；这些规划分支仍保留后续证明义务，不通过措辞永久删除，也不为伪造通过新增公共counter setter或无限故障入口。
2. 默认4096 None-key对象时完整snapshot真实Capacity，没有按None key单条读取入口；count路径只签持续保存/status尾，不签首开订阅读出。205大帧可读尾不能替代该场景。实际delivery需后续有界读取/引用余量等设计及count已满无后续输出验收，完整3.2义务保留。
3. 当前无私有连接proof/实际单一nodeowner装配，非Clone不证明全局唯一begin；字段一致性不是实际来源/操作者授权。真实owner/公平调度/撤销/WSS/Harness与完整L5/L6未验。
4. P2工具锚点/原位current、VPROJ-PLAN-002、scanner/typed语义、可信轮次分页和安全渲染/资源访问仍待交付。003仅观察层定义/逐记录origin可证，不冒签未来实体聚合。

独立11及negative目前外部可持续资产；若入仓，须保持普通依赖/模块路径与原断言并按实际根组合重新验证。当前限定结果不把原95回归重复算新能力，根完整入口后续新增P1时另记，旧P0 full日志不能代签本阶段。
