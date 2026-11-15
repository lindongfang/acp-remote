# revision2最小补充：逐操作预算与合法测试ABI

2026-11-01。只设计。基线freeze `777a273e2a570ab8b61f7aba69defa1e638da3d5c46b2d8e6be8b760f9f18637`全部455成员原字节保留，Root306仍为fa004a90/f01458de固定源。此次不编译、不写产品代码/SQL、不改ND正在编码的workspace，不运行真实测试/私有会话/外网审计，不提交或改原83oracle。

## 有效覆盖清单

| 文件 | 当前权威 |
| --- | --- |
| read-and-source-api.md | 取本包**完整覆盖页**：A1 ViewRuntimeOptions.native_read_policy、A5 Coordinator provider参数、A9每新实际读取取fresh options、A10 fixture/controller精确ABI；A1–A8其余规则不变 |
| engineering-contract.md、send-and-credit-api.md、native-assembly.md、issue-disposition.md、case-mapping.md/json | 原revision2冻结字节不变；B6测试点如何取controller/arm/release由本包A10给精确操作，安全pause位置不改变真实sender事实 |
| 当前八份产品spec及123 CL、原83 SV steps/expected | 原负责规格/独立tester定义保持，本补充不取代产品规则/预期 |
| NativeDispatch | 继续v6 9d1349…+receipt ec30e1bf…正式overlay及其独立gate；本包只读旧冻结引用，绝不碰其编码workspace |

## SV-PLAN007的具体补口

A9新增NativeReadOptionsProvider::for_new_operation、私有真实NativeReadStartContext、MonotonicReadOptionsProvider与NativeReadMetadata::selected_options只读观察。Coordinator不再在服务启动保存绝对Instant给未来所有读复用，而持可信provider；新operation在真实binding完成、reserve前取当时单调now，**恰一次**选择该操作的write/At或Unbounded；原NativeReadOptions形状/ACP映射不变。

join已有operation零取新预算、不延长/换RPC；终态后的desired_refresh后继才另取fresh预算；provider/checked_add失败无新RPC。不是input/queue/body TTL，不偷偷将At变Unbounded，不自动重试可能已写操作。SV038/039/049仍按原期待执行，选项采样和actual reserve/enqueue映射分别核。

## SV-PLAN008的具体补口

A10给OwnedTestArea::create/install_tls/path/cleanup、FixtureTaskOwner::new/handle/shutdown、FixturePeerWire::from_duplex/from_child_owned及原物错误归还、真实peer.write_bytes/next_node_frame/close、FixtureSigningClient::generate/public_key、真实fixture.scope/owner、controller的具体getter/arm_pause/observe_once/arrived/release与单次fail_next/triggered/disarm。

duplex直接move真正tuple并返回可驱动peer；Windows child原OwnedProcess先adopt同stableowner再take_io，stderr/Job/IO同owner真实join，不把duplex冒Job。area仅创建唯一新child，cleanup实际join后核canonical父子/marker/边界，拒绝路径替换/reparse，不接删除ROOT。自有TLS材料由已有dev-dependency测试执行者生成后写自有area，library facade不加新正常依赖。实际配对/AuthProof仍唯一Actor来源，无public bool/Scope/proof setter。

pause只在真实安全点、全部短锁外；actor gate内点只observe，真实IO Pending不伪造。AfterFlushFactBeforePublish是真flush Ok并释放actor gate后、publish前可pause，恢复核seal/nonce；BeforeCommitCursor也在registry锁外。gate Drop/release/owner cancel均有同arm_id责任，不能挂关机或释放新arm。七个命名新fallible点只一次注入AllocationFailure/RandomUnavailable，与global allocator/raw parser/native proof无关；这验证受控fault分支，不宣称真实系统OOM或容量预算。

## 独立确认与未执行

输入依据为原tester revision2/api-review-v2.md及planning-review-v2.md（SV-PLAN007/008），本包inputs.json按实读字节固定。原tester已在消息中说明SV034的stale缓存是仍保留时允许分支，不要求永久旧正文；原83steps/expected仍完整保留，以其独立v2定义为准。讨论不是gate，两个问题仍标“修订提出、待原tester/reviewer同版复核”。

本包仅做文件SHA/前后基线/覆盖页机械差异检查；执行数0。下一步交原queue_tester复核007/008及定义映射，再由独立reviewer同版决定编码门槛；不以本作者freeze或static shape自签PASS。
