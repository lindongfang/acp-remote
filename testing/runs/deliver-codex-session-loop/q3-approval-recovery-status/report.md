# APO003/P2 独立测试恢复报告（2026-11-02）

结论：本次恢复没有重新运行用例。已从真实落盘stdout/stderr/JSON复核前次v2的7个不同函数各一次：6通过、Closed一例失败；两条P2已各一次通过，无必要重跑。APO003整体及P2原resolver lease身份/hold组合仍未通过。Closed首红保留，新的精确验收提案等待reviewer，不修改旧结果或验收预期。

## 固定版本与独立性

作者candidate-v2 freeze 90de43e0f289ad0691142613a164c59d2990cd89c71e7a5a7cf601e11e4102fd，逐成员1658全匹配；源清单2c67da9d94392015ab18e9850efdb66fff2e952b0dede9b1729f7258bb10fa0f，781源。独立intake freeze c7f821cbfe808dedd78fb06c75d50d38ceb962a4985030086d9dff1c927182a1，799成员全匹配；原review门ef5e6f018ab6920263cb7f2da6ffa207fdd8741eff9fc9051ecc2295920cbffd同本次保存副本。

v2相对接收源仅app/src/node.rs增加私有cfgtest挂载，新增独立验收叶app/tests/approval_owner_acceptance/association_cases.rs，782源清单 9d59618e393299ab36cb1d60678704405a2ca2e00bf42e5e8b665e1ccc9fb44f 全实核；未改生产逻辑、原48预期、新public API或Close类型。v2实际freeze 3809b78f9680c78551bdbdaacf3b6e813c94da12899cb9c13c380c8b6d693ed4（832成员）。lib独立真实链接fresh=false/exit0，EXE dce7c481d3e554caf672572c0d110922d56de6fcfcc4d59c46bc99a5e4df5dcd；729个真实lib测试列举，每次exact只选择1。编译诊断40warning/0error；fmt-check0。受控协议子进程复用同源a1已实际fresh=false构建的dev child 5e27a7e5381db93a5efba26527715e0fb9ce1cf607b3f265e1e305597da9dd00，child源码未变，未假称v2另编。

## 实际执行

| phase | 独立函数 | 真实结果 |
|---|---|---|
| n01 | apo003_trusted_same_original_upgrade_reads_do_not_write | 1 passed, 0 failed |
| n02 | apo003_same_slot_concurrent_registers_one_association | 1 passed, 0 failed |
| n03 | apo003_second_connection_remaps_before_begin_exact_recheck | 1 passed, 0 failed |
| n04 | apo003_caller_drop_preserves_original_owned_association | 1 passed, 0 failed |
| n05 | apo003_original_source_closed_cannot_revive_after_mapping | 0 passed, 1 failed（exit101） |
| n06 | apo003_p2_actual_resolver_report_loss_next_register_recovers | 1 passed, 0 failed |
| n07 | apo003_p2_actual_resolver_report_loss_explicit_repair_recovers | 1 passed, 0 failed |

命令和原逐次summary见execution-matrix.json及原v2 logs/n01…n07.json、stdout/stderr。各次同shellowner true，初始独占TEMP为空，固定EXE/child SHA相同，日志均sourceUnchanged=true；恢复时对782源重新全hash。原a1首Trusted另有1次红，实际为SourceClosed后像SQL误写main.node_native_sources；v2只在测试改为实际main.node_native_source_instances，原日志/现场不动。累计跨两版8次调用、7个不同函数、6绿2红，不用累计掩盖最终Closed红。

Trusted/Concurrent/Remap/CallerDrop均通过实际原Node、原Source/请求/choice/raw保持；错误Source拒绝、只读及Recheck不升级、合法映射升级一次、必要保存与Selected真实对端/SQL、无Run及后续Spent拒绝。Remap使用第二实际SQLite连接在pre-BEGIN切点改变映射并拒绝旧映射；CallerDrop在原调用future pending时真实丢弃caller，原Source所有任务仍完成。它们不是held-close组合。

## Closed首红及提案

APO003-CLOSED-ORACLE001：n05在association_cases.rs:234断言readonly==(0,0,0)失败，实际(0,0,1)。到达原Native第一次joined/metadata=true、SQL Source生命周期closed、Node.shutdown Ok、主DB首share0成功及3sidecar absent；business提前panic使后续mapping-cannot-revive及firstremove未完成，保留8项自有合成取证现场，不追认函数通过。

恢复只读取原n05库，初次沙箱ACL拒绝后获精确只读命令许可；查询只取计数、reply kind/origin/transport及无用户决定引用，不读密钥或正文，不改库。association0/command0/decision0；唯一回执cancelled/source_closing/written且decision_operation_id为空，真实受控对端approval-cancelled标记存在。数据库读取前后SHA cf5dc6092851585e4523eebde841c66b40bd62bb999ee95fae158c5199c4f70a完全一致。这是晚期首红归因证据，不是原用例补执行。

精确提案closed-proposal/freeze.json 3484c9b0c8016e9e1f2ba8ef2f20be6309869290b547f078d1bffe361996b362，patch 4674c0c0db6392e7260f38bf0bbd0f8be66a0efce4ae8c8ad9a1ff1a92995cad；仅测试叶。Source实际关闭后先要求association0/command0/decision0/Selected0/attempt1/dispatch0，唯一原审批回执Cancelled/source_closing/written且无人工决定，保原response_id建立postclose baseline；再逐步验证read/list/inspect、RecheckOnly、明确拒绝再次register后精确快照不变，Selected始终0。非Closed全零预期保持。只做rustfmt直接工具链0、git apply --check0；未安装、未编译或重跑。proposal内首rustup shim默认home ACL失败亦留证，不作编译结果。

## P2两条有限观察

n06=NextRegister及n07=ExplicitRepair均实际1/1通过：首错误精确SqlCloseUnconfirmed{commit_confirmed:false}，当时association/command/decision/reply_attempt全0，Selected未发送；原Source/Approval保留，只读不升级；随后合法register或显式RepairMissingFacts恢复，同原目标保存后才真实Selected，最终SQL facts(1,1,1)。但注入调用仍为全局fail_next_result_after_ack，日志明确global_fault_lease_identity_independent=false，不能证明故障实际消耗者就是目标resolver原lease。故只记P2可观察恢复的partial证据，不关闭APO-RESOLVER-PRE-CANDIDATE-RECOVERY001的完整独立验收。不能从globalfault/Barrier.arrived/错误/通知推原ACK或身份。

## 关闭与资源分轴

七次均记录首Native joined=true、metadata=true和Source SQLclosed=true。n01–n05 Node global Ok；n06–n07 Node global Err为原报告丢失预期，不能写Node/Storage正常关闭。七次均首DB排他share0成功、-wal/-shm/-journal不存在；这不替代Node Err为正常ACK。通过六例实际firstremove=true且TEMPAfter0，Closed firstremove=false/forensic_retained=true。恢复不重删或修复旧现场、不清全局fault。最后Get-Process所查cargo/rustc/acp/acceptance/dev-child为空，命令exit0；Win32_Process命令行盘点受ACL拒绝，有限范围如实保留。

## 未验证与交接

Closed精确提案待原reviewer核实；获门后仅新独占副本、新清单/EXE实际重测才能闭合。00a362两类型重导出获窄许可但本测试副本仍未安装，BeforeClose/AfterCloseAck与原resolver稳定lease身份未证明，held负面、释放恢复、SourceClose及callerDrop组合均未运行。需要原lease只读关联口仍须独立精确门，不构造ACK/identity。独立正式48、完整审批/普通Nodeconsumer、完整产品L5/L6、同最终源码default/all-features/check/strict全量未在本恢复执行；作者的自评与strict298/554/603不作为我方绿。Root集成、真实外部Harness、安全/供应链独立验收亦未代签。

写入仅在独立tester目录；未改Root/author/reviewer，未联网、提交、推送或触碰用户数据。原freeze与真实失败保留。
