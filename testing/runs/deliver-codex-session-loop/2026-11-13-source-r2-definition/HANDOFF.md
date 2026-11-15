# 给主执行与 reviewer 的 R2 交接

2026-11-13。已完成：独立静态读取当前规范、rebase 方案、当前端口、旧 v4 候选和独立 review；在本目录定义 3 条首批真实 child 纵向例、7 个有限必需后续合同，以及实际入口/最小观察缺口。

交审文件：`TEST-DELTA.md`（验收预期）、`ENTRY-AND-OBSERVABILITY.md`（实际接点与缺口）、`INPUTS.json`（读取输入身份）。当前状态 `pending-review`，不是编码授权。请 reviewer 对 R2 范围与首批顺序、M02调度前证据、M03三故障轴、M05/M06新 producer工作/析构槽和M07关闭claim逐项给结论；reviewer通过后才做对应产品写入。

本轮未执行：Cargo 编译、libtest list、测试、迁移、产品运行、vendor Harness、WSS/Web、复制工作区、产品修改、finding复核。`SSF-SQL-RECONCILIATION001` 保持 Open；独立 R1 的交付状态由主执行绑定，不能从当前已有 schema11代码推断通过。旧 `MINIMUM-MAPPING.md` 原件尚未定位，本定义明确采用已读旧 PLAN/CORE-COVERAGE 和当前 REBASE-PLAN 的七组，不声称原件完整覆盖。

建议实施顺序：先保留R1独立门并冻结R2写集/原槽接线→V01真实discover→V02真实create response→V03WrittenUnknown重复不重发→M02/M03/M07原取消/SQL/关闭→M01/M04/M05/M06剩余有限合同。V01…V03可先形成小可运行链路，不能越过其他必须项宣称R2交付。
