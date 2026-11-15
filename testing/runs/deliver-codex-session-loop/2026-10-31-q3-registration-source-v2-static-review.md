# R3 candidate-v2 同源静态复核

2026-10-31。静态修复复核通过；**完整交付结论待独立动态验收与实际集成工程检查**。

亲核 candidate-v2/source-manifest.json SHA256 `5eeac76995a33c7e2ed410d3100b117052a405e67dcfd32b8052c15637163ba3`，136/136文件匹配。与v1逐项比较仅 `app/src/application/queue_owner/service.rs` 改变，新 SHA256 `1cbd530dead9f908ee4402a31e397b4b42892346efde56d658f041070edfb89f`；亲读完整实际diff，仅为reconcile绑定修复，没有其他产品/API变更。原v1源和问题报告保留。

REG-IMPL-OWNER-001 的静态反例已消除：最终tasks/state锁内按请求target/id重新核当前pending，再从同一pending取得probe/worker；有worker时解锁后只inspect该probe，未有worker时在注册围栏内建立与该probe/operation匹配的ticket/guard/task。前段owner查找不再决定后段可接管的pending。请求A不能因为锁间pending换成B而读取/占用B。

原 START-001、SHUT-001–004 在v1中已修的相关源本次全部同hash，静态修复结论沿用：Storage supervisor保留/清理、Core持句柄、关闭串行、Stop只补已D的E及最终状态/claim同围栏。它们尚须对应独立实际证据，不因本静态报告自动关闭。

已亲读 tester `shutdown-application-and-generation-supplement.md` SHA256 `797839b5dbee994ffb304c8f5db3b072fd8d498d5d5db899f6fafa191868f671`：SHUT04真实BA后不新D及重启Interrupted、SHUT05只补同E不重复D、SHUT06真实旧service/新runtime代际回归，定义可接受。其将无await短缝隙静态与真实代际动态分列正确；不得为制造命中加入生产yield。OWNER-001新增定义和实际并发回归另待tester交付。

后续同源验收仍须包含冻结v2实际SQLite+实际owner/InputQueue的P/F/M/C/QEXT、BA/SA/AU关闭并发与等待方取消、factory真实COMMIT前后drop/Weak及原39回归；精确资产SHA、逐用例结果及未执行边界必须亲核后再签限定交付。当前未见仍未修的静态阻断，不将开发18/18或clippy报告当独立功能结果。
