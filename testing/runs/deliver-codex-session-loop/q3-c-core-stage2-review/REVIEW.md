# C stage2：实际有限核心独立静态检视

结论：本版有限核心静态检视存在两项 P2 未闭环，不能签本版通过。原 A–E 设计编码门不撤销；coder 可在既有有限范围修复并提交新固定候选，独立 tester 可继续准备精确 F 提案。本报告不授 Root 合入、schema9 生产迁移/安装、默认 D 工厂或完整产品通过。

目标为 candidate-stage2/freeze.json SHA256 1c66b8e03ffc61985d63e786c91ccadbf673743877fdb29dec8991d908d4406a。本 reviewer 独立逐项核同全部 826 成员及 source-manifest 的 795 源输入，均无漂移；manifest SHA256 4043c1a25b552b3a72e3ed2a82fa80d2df8bef025a18ff294cd4ec7d40fe1062。读取实际新核心、原共享连接 owner、两份 traits/proof、原 ABI/合同/schema9 及 S09 精度定义；没有以 coder 实现摘要代审。文件位置以下均指这份冻结 source，不指 Root 活动文件。

## C-STAGE2-REV001 / P2 / Open-static：最终已结算分支跳过原候选关联核同

位置：app/src/session_storage/node_configuration/management.rs:693；上下文为 666–708，Command::fact 在 151–240。

apply 已在第一事务持有完整 before 并在锁外解密得到 intent。第二事务却先执行 `if row.state!=CatalogState::Registered{return row.fact(c.sql()?).await}`，随后 694 行的 `row!=evidence.before` 仅覆盖仍 registered 的情况。Command::fact 校验读取行自身的结果关系，既不接收 before 也不接收已解密 intent，因而不能证明已结算像与原候选相同。

一个约束可合法满足的防御向量：AfterDecrypt 后，以受审 fixture 将同 request/actor/action 改成完整 committed，保留原 digest/同 cipher 移动，但令 command.profile/result 与新 profile 行共同指向另一个 profile ID；profile.created_by_request_id 仍同请求。Profile::load 与 fact 的内部关联可以全部满足，而实际 cipher 内 profile ID 仍是原已解密 intent.profile。693 行将返回另一 profile 的普通 committed 事实，未比较本次 intent 的预期键；同理，已结算像中的 registered_at/register_generation/digest 改变也不被此分支对照原 before。这里是静态可达的受控损坏防御问题，不称另一合法 apply 穿越共享 gate，也未声称实际 SQL 已跑过。

依据：CORE-ABI D 的原 snapshot 与最终重验、E 对原 request/effect 的完整关联，以及已经独立批准的 S09-C：只有完整匹配的结算事实可只读返回；身份、关联或结果异值属于损坏防御，不能靠 state 已结算洗绿。query 读取普通既存事实与此处持旧候选继续执行的分支有不同输入义务。

修改建议：保留合法已结算只读返回分支，但在返回前核不可变原请求链字段，并按本次已解密 intent 核完整预期 effect。创建动作核 harness/source/profile、原创建请求、format、digest 和 cipher 原字节；绑定核 target/workspace/harness/source/profile、launch_path/canonical 及原有效 selection 关系。rejected 也须核原请求链与闭合形态。不要机械对 settled 使用 `row==before`，因为 state/cipher/必要结果列本就应改变；也不要强求合法先结算的 admission/time 必须等于本次尝试 at。可提取专用核同逻辑，不能仅移前现有整行相等判断而拒绝全部合法 settled。

回归义务：F 精确正文批准后，保留合法 committed/rejected 返回正例，另列合法约束下的异结果键/原登记元数据损坏负例；仍 registered 的逐字段负例和真实同 gate 串行竞争分别计证。若暂停点仍持原 BEGIN IMMEDIATE，fixture mutation 未到，不提前关闭/移动 hook 伪造到达。

## C-STAGE2-REV002 / P2 / Open-static：共享 gate 等待未受原 owner seal 取消

位置：management.rs:522–526（register/apply/query 共用 start），node_configuration.rs:76–81（resolve/resolve_read）；共同等待在 catalog_sql.rs:41–44。实际 operation 的 scope.while_open 在 management.rs:755 起、779 起、803 起及 node_configuration.rs:83 起，均晚于 gate 等待。

调用先获取原 catalog_operation_lease，再等待 CatalogSqlFence::enter 的 serial mutex。enter 只运行 d.run(lock_owned())，不等待原 scope seal。因此当原 owner seal 而另一个操作仍持 gate，尚未进入 gate 的合法调用不会因 seal 立即结束；必须等 gate 释放或原 deadline。这个等待尚未进入后面的 scope.while_open。旧连接 owner.shutdown 明确等 leases==0，故该 waiter 还持有原 lease；本报告不声称必然比持 gate 操作多拖延 shutdown，更不称未知被错误清除，但该段 async 等待缺少冻结 ABI 要求的原 owner 取消路径。

依据：CORE-ABI C 要求原 scope.while_open 和单 deadline 取消原 async 等待，seal 时不能再沿旧资格推进；真实 RegistryOwnerScope::while_open 已提供 wait_sealed 的 biased 取消分支。现有开发 shared_gate 测试只验证 Pending/释放后进入，不覆盖 owner seal 时仍占 gate 的等待。

修改建议：将取得 gate 的等待一并置于同原 scope 的 while_open 与同一个原 deadline 下，保留 CatalogFailure 而不压丢原操作观察；Drop 必须释放该 waiter 的 operation lease。resolve/resolve_read 同样处理。不要新增 deadline、owner、ready setter 或换 gate；不要为“取消等待”撤销已持 gate 操作的真实同步保护/解密责任。recovery_guard 的显式原 receipt 清理允许 seal 后等待，不能一刀切取消已授权原清理恢复职责。

回归义务：用实际同 registry、同 shared gate 的到达证据，使第二调用确实等待 gate；在不先释放 gate 的条件下 seal 原 owner，证明第二调用按 NodeStopping 收口并释放自己原 lease、无 catalog SQL 进入。测试持 gate 的原责任单独正常清理，不用关闭未知的 setter，也不将仅 future Pending 当已到 gate 的证明。

## 已核实的有限静态结构

- CurrentCatalogActor 私有字段/构造；真实 NodeCatalogAuthority 取同 NodeRuntime request_authority 与 storage.request_registry，verify_owner 比较真实 scope；current 走现有 NodeRequestAuthority，最终写事务仍核 main.clients trusted/auth_generation。纯数据 draft/ID 无授权作用。
- registry 初建时一次 Arc<CatalogSqlFence>，Clone/provider 只共享；管理与两个配置 port 共用此 fence。未决记录会阻新命令/新配置读，没有新 SELECT 清旧债务或换 provider 另起 gate 的路径。
- 外层原 operation lease 覆盖锁外同步 protect/unprotect；每个 SQL 槽用原 owner 单独 lease。pool acquire 后 attach/detach/包装没有 await 裸连接窗口。取消/Drop 由 CatalogConnection 同步 handoff 原 begin_owned_close，并保同 original receipt；没有新的 runtime/thread/owner/pool。
- register 两短事务之间归一化/保护；apply 第一短事务读取完整 Command/identity，关闭后解密/完整 codec，第二事务重验仍 registered 的所有字段与 cipher 原字节。必要 effect、同 cipher/digest 移入 immutable profile、命令清 cipher/必要结果在一个 COMMIT；无重保护。最终 settled 分支的缺口单列 REV001。
- BEGIN 之前/COMMIT 首次 poll 前保守记录未知；只有实际 COMMIT/ROLLBACK 成功更新相应知识，finish 立即 handoff 同槽并等原报告。必要写 COMMIT 汇总取业务写槽，未被后续 RecoveryRead 的 COMMIT 覆盖。
- reconcile 先以原 receipt.confirm_actual_cleanup 核 actual+joined；保留原必要写及证据，只对原 request 做只读 verify。register 不存在/同 registered，apply 同 before/完整相同 effect 的结算像，均按 Evidence 核；不重新 apply。恢复读自身 close 未确认仍保原恢复读 receipt；owner 已 seal 时可确认原 cleanup，但禁止新恢复 SQL/plan。没有恢复 setter 或启动新 Storage 洗未知。
- 配置读短事务完成后才解密，以同 context 核 envelope/request/actor/action/generation/profile/harness/source；ACP raw 数字、字段顺序/未知 extra 与 UTF16 code units 保留，重复 decoded key/未知 Node 壳拒绝。缺 wire recipe 不伪造 Known 空值。目录仅对既有 workspace 数据核同，尚不证明 D 当前真实文件系统或 driver 品牌。

这些是代码静态事实，不是业务 CRUD/真实 DPAPI/完整上传已跑过。D factory、目录 owned 观察、driver 同 owner、global audit 及生产 consumer 仍按原门 Open。AuditWarning 当前明确 Unavailable，不把未做的全局审计写成 None。

## 实际已有证据与未验证

原冻结 raw logs 中开发测试确为 18 pass / 0 fail / 665 filtered：profile codec5、intent3、context1、Command SELECT 形态1、原 owner scratch SQL7、真实 Node 配对后撤销1。最后一例在 schema8 上明确得到缺 catalog 表的 StorageUnavailable，随后 revoke 拒绝；这不证明 register/apply/query/resolve 的 schema9 业务 SQL。

check 与 all-targets/all-features check raw tail 均 Finished；fmt 原日志为空，退出 0 沿冻结 validation 记录。reviewer 没有重跑任何检查。独立对两 strict 原 JSONL 重算诊断 multiset：baseline298、candidate381，新增83全部 dead_code，移除0；因此 strict 仍红。没有为消警添加假生产调用、allow 或把私有 core 扩公开。当前没有默认生产装配，严格检查与装配继续 Open。

schema9 安装/真实业务 CRUD、真实 DPAPI、原34+补充12正式独立执行、F 控制器、VM COMMIT poll 故障均为0，不能由18开发绿代签。后续 F 提案须对准实际固定候选，以私有自有数据库安装精确测试 schema/挂点，区分真实 gate/slot/VM 到达、受控 mutation 与产品调用；提交新修复候选后须更新同源输入。F 门尚未到，本报告不预先批准其 DDL/hook，也不因其未到延迟本次真实代码发现。

仅写本 reviewer 新子包；产品写入0，测试执行0，Root 安装0，用户数据操作0，提交/推送0，网络/npm audit0。旧冻结包不回写。所有问题由原 reviewer 对新固定版本复查后再关闭。