# WP2 catalog/lifecycle 冻结包独立检视

2026-10-30。**BLOCKED（CAT-R001：登记幂等探测在可变路径/配置解析之后），不签最终产品。** 独立reviewer，只读候选与主D权威合同，未继承开发自评，未改产品/plan/tests。§10设计/用例门槛此前PASS不等于此实现通过。

## 冻结与实际证据

候选根 `C:/Users/jack/.codex/worktrees/codex-session-catalog/acp-remote`。`.tmp/wp2-catalog-logs/catalog-v1-manifest.json` SHA256=`ABE3FEFC1A4170414B8F2B4FCE577AAA91ED822EDF4D36FA33E5172353537A57`。开始/结束实际核 **44/44源码清单、11/11交付文件** 均匹配；8/8日志摘要匹配。对照主D已有2.3的input/approval兼容实际差异，确认schema扩展、显式新增列及共享命名空间处理，不把原2.3通过无条件外推。

实际核读开发日志：storage52/history application22/JWS4及fmt/clippy成功。alltests-v1是真实 **216过/1失败**，失败node::tests::management_websocket_closes_oversized_transport_frames_with_4429为Windows10054；报告明确它属于此前快照且未宣称最终all-target全绿。本reviewer读取实际失败正文，不隐藏NET-I001，也未重跑网络以制造通过。候选node/ws未在11文件交付范围；主执行另有原基线复现，归NET独立问题闭环，不能据此要求catalog包改node/ws或宣称全检查已通过。

本reviewer实际以真实owner运行已有冻结编译物：

- `.tmp/catalog-target/debug/deps/acp_remote-266e97892ace37dd.exe session_storage:: --test-threads=1`：exit0，52/52，9.02秒。
- 同exe `application::history::tests:: --test-threads=1`：exit0，22/22，8.98秒，含HIST-R001及预算/强刷/关闭回归。

testbin SHA256=`F8D4381DF52F8D8BE38ACCE6A92A7B31A9051947387347E233A7FCD1171899EA`。未独立重编译或重跑JWS/全目标；上述74项为纯SQLite/异步owner开发测试重跑，不是真实资源、gate、WSS或Harness验收。测试仅自有临时数据库，未操作用户原生会话/配置/密钥。

## CAT-R001 — P2：已提交登记的原样重试仍依赖当前路径/配置可解析

位置：domain/catalog.rs CatalogResolver::harness/workspace；application/catalog.rs CatalogRepository登记签名；session_storage/catalog.rs register_harness/register_workspace。

两个登记端口只接收私有字段的ResolvedHarness/ResolvedWorkspace，因此外部调用方必须先经过canonicalize/read_dir/hash_file/configRef allowlist查验，才能进入存储私有replay(actor,request,action,digest)。§10要求原样重试返回已提交原结果，即使目标随后变化；若第一次登记后可执行文件/目录已删除、暂不可读，或原configRef已撤下，重试会在构造Resolved值时直接InvalidContent，无法拿到原Command/ID/revision。同actor/request跨动作或不同payload时也可能被可变IO错误抢先，无法由全局ledger给出既定冲突。

当前registration_has_real_target_unverified_provenance_and_replays_after_state_change测试只是再次调用仍可解析的area.harness()，没有撤去文件/目录/配置引用；通过不能覆盖此路径。下层replay在事务内先于目标状态校验是正确的，但上层缺少以原始封闭参数进行只读幂等探测的入口，后续gateway不能仅靠commandId查询补齐未知受理结果。

最小修复：提供原始typed登记参数的探测/受理服务，先按当前actor与原request/原payload摘要查询全局ledger，命中返回Replay/冲突，不触发新的路径/配置IO；仅Fresh才解析并校验，最终事务再重查幂等与容量后原子写入，不能以probe结果跳过首次路径检查或并发CAS。可以借鉴审批prepare/probe模式，但不要把客户端原始字段冒充Resolved权限。回归应含：登记成功→删除自有文件/目录或撤下configRef→同请求原样重试仍返回原结果；异参数/跨动作仍冲突，首次新登记仍严格验证，且没有额外定义/审计/revision增加。修复后原reviewer按新冻结摘要关闭。

## 已核符合与内层边界

- ledger目标模型区分Harness/Workspace/Session/Approval，登记session_id null、真实target无假session；input/approval保留原关联结果，通过显式列名适配新schema。所有动作actor/request共享唯一约束和100000上限、同ID异action摘要冲突，不挤掉原事实；device.revoke独立规则保持。
- 登记定义/Command/audit/node revision同BEGIN IMMEDIATE事务，未报告能力为unverified，证据写入核固定program/config摘要；定义路径/configRef resolver不执行版本探测。CommandResultMetadata只给当前原actor及作用范围元数据，不自行发行null内容租约，实际读取分类/gate/代数/租约仍须gateway落实。
- create原子creating，claim核原actor代数/状态/bindingVersion，只发私有不可Clone permit；report核claim/dispatching/当前状态与绑定，create成功才loaded，resume只匹配旧scope/原ID并单调推进version，旧permit/迟到回报不能覆盖重启未知。缺资源可靠事实不能靠ActorContext产生实际owner授权。
- release必须history_flushed/resources_closed/references_reclaimed全部真才released，否则failed/releasing并锁新输入；这些bool是运行层内部事实端口，当前未接任何Web/ACP入口，未发现本包把客户端bool当授权。最终owner必须从真实强刷、原生关闭、引用回收取得事实并维持安全临界区，本包本身不能证明它们为真。
- cancel受理将Run置cancelling并事务失效审批，不以命令受理解除前台；可靠Cancelled结果与原输入状态分开。Unknown保留输入delivery_unknown，reconcile_unknown_run只改变Run为可管理interrupted且保留原Command未知，实际“资源已不存在”必须后包owner确认。
- recover与原input/approval重启事务协调，未完成生命周期Command转delivery_unknown、会话interrupted；旧claim不能恢复派发。现开发四动作×三窗口中的第三窗实际是report已COMMIT后重启、预期Completed，而非外部结果已发生未持久化；后者在SQLite层与claim后无report窗同状态，可复用其未知断言，但**不能把该测试称为已执行真实外部结果窗口/派发计数验收**。
- 审计只存平台ID/固定action/outcome/时间，不带路径、定义参数、正文或恢复字段；NativeBinding无Debug且恢复key白名单，正常路径无unwrap/expect/panic（unwrap_or为明确可选缺省）。存储有语义错误，SQL细节不外泄。
- 新schema1→未发布schema2布局仍同事务升级并保留节点身份、clients/撤销代数/审计/revision；独立52项重跑包含保留与故障回滚。旧中间schema2拒绝打开而非重建，不是清理草案库授权。未来已发布布局变化仍须版本化迁移，不能援引本轮未发布例外。

## 交付限制

CAT-R001 Open；独立tester结果另担其实际范围。允许只按11交付文件新快照集成，**不得覆盖C旧基线lib/Cargo/platform/node/ws或主D新ACP目录**；模块声明合并保留新目录，集成后完整检查不能省略NET-I001关闭与最终同版结果。真实发行者/gate/writer、进程资源三事实、WSS投影、当前rev列表冻结、租约与前台UI都未由本包实现，不签最终任务/CL完成。

## 同日追加：catalog-v2 CAT-R001 原问题复查

新manifest SHA256=`1879B7FF362CBD66D2ED0DB6DA1E6A6282DFCE68B967B3F10BB2BE4649BC71E9`；首尾实际44/44源码、11/11交付文件匹配，6/6新日志摘要匹配，旧ABE3FEFC...及原失败日志保留。本次仅5个已有owner文件变化，其余原已审实现按逐摘要复用，不重签不存在的运行层能力。

**CAT-R001 Closed；catalog/lifecycle内层候选限定PASS。** 原始typed HarnessRegistration/WorkspaceRegistration进入CatalogRegistrationService，先由存储probe按当前actor及原payload/action摘要查询全局ledger，Replay/冲突返回前不触碰resolver；Fresh才解析路径/config，随后原登记BEGIN IMMEDIATE事务再次核actor、去重、容量和原子定义/Command/audit/revision。低层Resolved登记方法已注明不能直接接网络原始请求，实际gateway需接此service，不能重新倒回“先resolver再登记”。

实际读取并独立运行四新增回归所在存储集合：真实删除自有workspace、改变/删除自有exe、撤下原config allowlist后原样重试返回原Command/ID/revision，零额外resolver调用/审计/revision；全局跨动作/异参数冲突在resolver之前，Fresh仍严格验证；两个Fresh probe后的并发最终事务只写一个事实。真实owner执行同testbin `session_storage:: --test-threads=1` exit0，56/56通过；不是模拟网络重试或真实Harness。开发history22/JWS4/fmt/clippy日志仍只按其新固定范围引用，未重跑全目标，不把原NET-I001失败改成全绿。

CAT原阻断解除，尚需独立tester同版结论及主执行集成检查。仍只可集成11文件，不能覆盖旧C基线lib/Cargo/platform/D新ACP；NET修复与最终全检查、运行层owner/gate/租约和资源三事实的限制继续保留。
