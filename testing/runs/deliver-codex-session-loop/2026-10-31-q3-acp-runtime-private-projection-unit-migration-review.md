# 开发投影单元的私有数据算法迁移边界

2026-10-31。**限定准许提案，不是实际重构实现或测试通过。** 公开ObservationRead::observe/ToolProjectionRead::apply仍必须接真实&Received并先验证ConnectionScope；不增加Incoming兼容公开入口。

已存在开发同步单元直接构造/修改Incoming以验证raw一致性、布局、预算和优先级。这些可以调用从真实生产路径提取的私有observe_inner/project数据算法；若需cfg(test) apply_inner，只能薄组合相同私有生产函数，不复制一套测试专属算法。它们可以构造普通数据，不得构造fake Received、来源setter/public bool grant或生产测试hook。

公开scope-first校验必须在调用该私有算法之前，不能因单元便利先走Ignored/布局分支而绕过来源拒绝。开发单元清楚标为私有数据/legacy预算证据，不冒称公开来源校验已过；新增实际spawn/真实Received正反来源开发例，独立资产仍由tester经真实公开路径迁移和验收。原公开伪造攻击面的compile-fail与静态证明继续，不由私有单元替代。

最终候选仍须审查私有调用图、可见性、未增加From/Deserialize/工厂及真实公开测试。该实施细节不改变64cc合同/产品规则或既有编码范围；coder不可据本许可修改独立旧oracle。
