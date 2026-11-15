# 来源会话入口：当前差异与下一实施批次

2026-11-13。只读盘点，不是实现、程序门或独立验收通过。精确输入与当前文件摘要见同目录 `IMPACT.json`。本轮不启动另一套 Cargo、不复制工作区、不修改受测产品，也不应用旧补丁。

## 当前真实缺口

- ACP `strategy.rs` 已有 `new_session` / `list_sessions` 及 discovery 合同测试；这证明协议客户端有对应入口，不证明节点产品已接通。
- 当前 `application/native_dispatch/mod.rs`、`node/session_services.rs` 没有旧候选的 Source session 服务装配；13 个 Source 新文件尚不存在。
- `management_ws/session_protocol.rs` 的封闭动作包含配置 catalog、输入、审批、释放和 view；没有原生会话发现、创建及外部原生 ID 读取动作。`catalog.*` 是配置发布流程，不能当作会话列表。
- `node/native_views.rs` 当前读取以已有平台 target 为入口。CL-061 要求外部会话按原生 ID 查看、且浏览不以执行接管为前提；当前路径尚不满足这一完整要求。
- Web `ManagementConnection` 没有上述会话发现/创建入口。Web 共享视图的局部通过不能填补此缺口。

行为依据为 `specs/structured-history/spec.md` 的“基础原生会话发现与读取”、`specs/harness-management/spec.md`、`implementation-contract.md` 及 CL-061/062/063/065/085；任务承接 3.1、4.1、4.2，未完成判据不变。

## 旧候选可以复用什么

保留的 repair-v2 v4 候选固定 945 叶，清单 SHA-256 为 `8c6a799614e4d95f425e40323d47ff63bdd95a5dc88a63e9fc70ece1efac353f`。相对其 932 叶基线共有44个变更文件；当前 Root 中10个仍等于旧基线、21个已经变化、13个尚不存在。没有文件等于旧候选最终版本。必须在当前代码上移植有效差异，不能整文件覆盖或以旧 `git apply --check` 通过结论授权本次安装。

旧实现可作为同一 runtime/receiver、真实 Source 归属、discover/create、原始结果 Doc 传输及必要元数据的参考。旧作者开发结果只在其固定版本和范围内有效；未接入 WSS/Web，也未完成无 target 的外部 ID 读取。

旧 `SOURCE-CODE-REVIEW-v4.md` 和 `SOURCE-V4-INDEPENDENT-REVIEW.md` 均不允许合入，`SSF-SQL-RECONCILIATION001` 保持 Open。最新可见 v14S 门仅授权 compile/list，未授权动态测试；不得把准备报告当成已经执行。

v14S `queryTest` 使用裸函数名加 `--exact`，实际 overlay 将它挂在 `node::tests::independent_source_acceptance` 内。恢复该程序前，需先由测试方核实际 libtest 列表并绑定完整限定名，避免重复出现零测试。这里只记录静态不匹配风险，未执行列表或修改历史冻结程序。

## 必须先解决的集成冲突

当前 schema 10 已用于 `node_native_release_operations`；旧 Source 候选也把自己的表加入 schema 10。不能把两个不同布局都宣称为同一版本。下一实施方案应在当前迁移链上增量设计，保留既有释放布局、身份、审批及旧数据；若采用后继版本，应先冻结对应 DDL、结构验证和故障回滚用例，不直接修改或迁移用户数据库。

当前审批、环境继承、Doc 生命周期、原始资源关闭和 registry connection owner 已有后续修复，21个漂移文件必须逐个对照。Source SQL 原任务/receipt 的保留、reconcile 及 source claim 释放前置须保留，不能为减少工作量删掉故障安全条件。

## 下一批次的实施顺序与退出条件

1. 收齐当前 WSS 19入口与 Web WV-13/14/15 的独立结果；保持这两批受测产品不变。
2. 在当前基线上固定 Source 移植方案与写集。复用旧独立七组定义及真实故障观察口，测试方补齐 Register/Query caller cancellation 与 no-SQL-start 的真实可观测性，reviewer 对差异增量审查。
3. 移植来源服务和后继 schema 增量，在唯一原 runtime/receiver 上实现 discover/create；标准不支持直接报错，未知写结果不重发。开发 Red/Green 后，由独立测试方验原 SQL 关闭责任，原问题须由独立复核关闭才能合入。
4. 再补认证 WSS 封闭动作、Doc 原始结果消费及 Web 选择入口；分别授权列表、按 ID 读取和执行，不能以伪造 target/read proof 或第二 runtime 绕过缺口。
5. 完成外部 ID 读取及真实 Harness/Web 的最小闭环，然后补 A/B、撤销、停止恢复等受影响场景。合成浏览器和受控协议对端的结果分别记录，不替代真实 Harness。

这是一份后续实施依据，未新增产品规则、未放行编码、未关闭 finding、未勾任务。整体 tracked 仍9/28；`npm audit` 仍按用户要求不执行。
