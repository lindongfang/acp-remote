# Source 会话入口增量实施方案

2026-11-13。状态：待独立测试差异定义及方案检视；不是编码放行或验收结果。承接当前任务 3.1、4.1、4.2，不改变其完整完成判据。

## 输入与隔离

- 行为以当前 change 的 structured-history「基础原生会话发现与读取」、harness-management「能力准入与证据状态分开」、native-session-recovery 及 implementation-contract 为准。用例保留 CL-061/062/063/065/075/076/077/082/085 等原 ID；Source discover/create 子范围不能代签无执行绑定的外部 ID 读取。
- 当前 Rust 基线是 `../2026-11-13-native-view-error-mapping-v1/SOURCE-GREEN.json`：515 叶，canonical SHA-256 `5f076a551d81c3b3ed28b18a0c6bc3d66772969170178962ec3b745c9aaf9942`。开始产品写入前重核；当前 WSS 验收期间不改其受测产品。
- 旧候选、当前 44 叶影响分类及逐叶摘要见本目录 `IMPACT.json`：10 叶仍等旧基线，21 叶漂移，13 叶不存在。旧候选不是安装授权，禁止整文件覆盖漂移叶。
- 主执行承担 coder；独立 tester 维护下述 TEST-DELTA，独立 reviewer 审当前方案、用例及实际交付。当前三条并行验收占满代理容量，Source 测试定义在其中一条完成后复用空出的 tester；不再复制整工作区或另开构建。
- 本目录仅存本批方案、差异定义和结果引用。旧候选与失败日志只读；不迁移用户数据库，不改 Git index，不增加依赖。

## R1：schema 11，保留已交付的 schema 10

当前 schema 10 已包含 `node_native_release_operations` 和 `node_native_release_actor_lookup`。旧 Source 候选的同名版本号不兼容，故 Source 布局新增为 11，绝不重用版本 10 或改写旧版本 DDL。

Source DDL 的精确参考是旧固定候选下 `workspace/app/src/session_storage/source_sessions_schema.rs`，文件 SHA-256 `de8a8122773418f8547d1c0cf3896260a8009fa4e05a3f07f96d28aff584f4b8`。采用其 `DEFINITIONS` 中按原顺序的四个定义：

1. `node_source_session_registrations`：显式来源、完整启动选择、登记者代际、动作、cursor、意图摘要和登记时间。
2. `node_source_session_operations`：原 runtime/connection/RPC、实际 transport/response/ack、必要错误与终态事实。
3. `node_source_session_runtime_operations` 索引。
4. `node_source_session_created_owners` 部分索引。

本批不把聊天正文、列表 raw/typed 响应或凭据写入这些表。旧文件中所有 `current < 10` 的 Source 版本判断移为 `< 11`；当前 native release 的 `< 10` 判断保持不变。Source 结构验证与空身份检查在当前迁移链上增量接入，保留当前 preflight worker、pool close 和 registry connection owner，不能从旧候选覆盖它们。

迁移继续使用当前 `BEGIN IMMEDIATE` 原事务：验证原布局 → 执行原有必要步骤 → 在 `<11` 时依序执行四条 main-qualified Source DDL → 更新 `node_meta.schema_version=11` 和 `PRAGMA main.user_version=11` → 验证完整布局 → 实际 COMMIT。新建和 1..10 升级走同一链，原 release 表/索引/数据/身份/撤销/审批不丢失。版本 11 重开只验证，不重建或补猜缺失结构。

开发测试增加四条 DDL 各前/后、metadata、pragma 故障检查点；保留真实 deferred-FK COMMIT 失败和 commit-report-loss 两轴。提交前错误必须回滚到精确原布局及双版本头；报告丢失则保留实际已提交的完整布局，重开不得重发会话 RPC。错误布局、trigger、额外索引、temp shadow、外键破坏、Source 部分布局或双版本头不一致应拒绝，而不是自动修补。

受影响测试必须区分「验证 9→10 步骤」和「验证最新版本」：当前 `native_release_schema/tests.rs` 的 `exact_nine()` 不能从 11 新建后只删除 release 对象就伪称 9；应使用精确旧布局或在自有 fixture 中移除全部后继对象。保留 release 故障回滚到 9 的断言；成功穿过全部迁移后验证最新版本、release 数据与旧布局保留，并单独覆盖精确 10→11。不能仅全局替换 `(10,10)` 为 `(11,11)`。

R1 预定产品写集：`storage.rs` 的版本常量、`session_storage/mod.rs` 的模块声明、`session_storage/schema.rs` 的增量接入及故障点、新 `source_sessions_schema.rs`，以及其开发测试和确受影响的 release fixture。独立验收预期由 tester 维护，coder 不自行修改。

## R2：在原 runtime 上移植 discover/create

在同一 Node 的原 configuration Arc、Native runtime、Registry owner、Source launcher 与唯一 ACP receiver 上增加 Source 操作服务。业务对象、应用端口、SQLite 行、ACP raw/typed 结果及页面 DTO 分层；不新增第二 runtime/receiver，不伪造 target、Native ID 或 ReadTargetProof。

- 受理前检查当前 actor、原 socket、显式来源和完整 profile/workspace 选择。必要登记未可靠提交时零 `session/new`；登记后由 Node 拥有任务，取消 caller await 不丢原 slot。已接管的操作不因派发时重验旧 actor 而改变语义；新 query/read/reconcile 检查当前权限。
- discover 只调用标准列表，create 只调用标准新建；最高共同已实现版本按现有策略协商。不新增证据准入、厂商白名单、私有扫描、自动降版或替代会话。真实不支持与平台未实现分别报错。
- 原 reserve 的 scope、typed RPC 和 response kind 才能接收结果。创建身份只来自实际响应；Written/Unknown 不是创建成功，重试同操作、query、reconcile 或重启不重发 RPC。
- 原 raw 与 typed 响应通过真实 result owner 接 Doc，保留未知字段和 opaque cursor。新 producer 单独证明 provenance、线性交接、逐块当前权限、seal/ACK/最后析构与关闭；旧 Queue/Native 的绿不能代签新 producer。
- 长列表解码、Vec 增长和最终大对象析构不得占短 lifetime/actor 锁或 ACP/socket 接收循环。旧候选没有证明 CPU 卸载，不能直接宣称已满足；移植前必须选定现有 Node-owned 工作/retire 槽并由测试定义真实观测点。

原 `SSF-SQL-RECONCILIATION001` 保持 Open：SQL inventory 必须登记原 task 与原 receipt，caller 返回后仍保留；query 只读且不能以新 SELECT 洗掉旧 Unknown/Err。显式 reconcile 先 join 原 task、确认原 receipt，再以保存的 typed proof 做必要事实修复，不发送 session RPC。Source close 等原 producer/IO/tasks/SQL 关闭及必要事实收束后才释放 claim；未知保持保护。`no-SQL-start` 必须有实际调度前的可观测事实，不能以 inventory 暂无条目推断。

R2 具体叶以 `IMPACT.json` 的44叶为候选上界，逐叶移植并记录实际差异。21个漂移叶仅修改必要接点，当前审批、环境继承、Doc关闭、Source关闭和连接所有权修复保持。若需要额外产品叶，先说明依赖并增量审查，不静默扩大写集。R1/R2串行，浏览器与WSS tester不写这些产品路径。

## 最小开发与独立验证顺序

独立 tester 在本目录增加 `TEST-DELTA.md`，引用旧 `MINIMUM-MAPPING.md` 的七组风险并补当前差异：schema10→11、Register/Query caller cancellation、真实 no-SQL-start、原 SQL 三故障轴及新 result owner 的工作/析构槽。不重复编造整套 CL ID，也不缩掉完整义务。reviewer 对当前增量确认后才开始对应产品写入。

1. R1 开发 Red/Green：精确10保留release行升级11、旧版本链、11重开、结构负例、逐DDL故障、真实COMMIT失败/报告丢失；随后运行受影响schema/registration/approval/release回归。
2. R2 最先运行真实 child discover、真实 create response、WrittenUnknown+重复操作三例；每例核原标准调用次数、实际结果、无伪target及原任务关闭。先通过少量可执行链路，不先堆大矩阵。
3. 独立七组按实际入口扩展，优先原 SQL 三故障轴、Register/Query取消、no-SQL-start和关闭claim保护；从实际 libtest list 绑定完整限定名，禁止裸名 `--exact` 导致零测试。测试 Job 必须核数量及自然清理，编译产物与编译监督结果分别记录。
4. Rust 使用仓库规定1.98.1、locked/offline和独占target。按实际源码差异增量构建；不复用不匹配PE，也不因报告更新重编译。审查复用同源同范围证据，故障重跑必须对应具体修复或新假设。
5. 当前固定版本独立结果与交付检视闭环后，才决定R1/R2集成及原finding的继任独立复核；旧Open历史不回写为Closed。全任务判据未满足保持原checkbox待办。

## R3：认证 WSS、Web 和外部原生 ID 读取

R1/R2不修改当前封闭动作。后继R3先独立冻结动作/DTO及其权限、去重和Doc生命周期，再接当前认证WSS和Web入口；列表、按ID读取与执行接管分别授权。无列表但有标准按ID路径时明确限制，不因缺列表拒绝其余标准操作。

现有 native view 从平台 target/read proof 进入；外部ID读取必须新增真实来源读取资格和对应原runtime资源链，不能创建假的执行绑定补洞。查看允许按实际标准能力激活/加载进程，但不造Run；重放、执行恢复、权限请求和历史完整性分别呈现。外部刷新需重新读原生事实，失败时缓存明确为旧视图。

R3之后执行CL-061/062/063/065的节点/页面组合及CL-085真实Harness外部创建、更新、查看最小闭环，后续再补A/B、撤销和停止恢复。合成浏览器、合同对端、真实Harness分别记录，不能互相代签。完整task 3.1/4.1/4.2以及最终验收仍由各自全部判据决定。

## 当前退出条件

本文件只解决移植依据和实施顺序；独立TEST-DELTA、方案检视、产品实现与真实执行均尚未完成。当前tracked仍9/28。`npm audit`继续按用户要求未执行；无新增用户产品决定、术语或ADR。
