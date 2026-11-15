# revision6 独立门禁交接、变更表与验收入口

本文件是作者的工程答复，不是独立关闭结论。本轮tester outputs-v3/80定义固定于inputs-v5.json；gaps-v3为0000abe57f8ab03d7045cf97725519ea9058edbae97f009c86f115296bf7e9e3，reviewer revision4正式报告cc5f2ac27592ec05f625a8ea38450560d7505bfabe1359e3ade504add7774ebf。旧v1定义/报告保持原字节，新版需要tester/reviewer分别复核，不代签。

## 逐问题工程解答

| 稳定问题 | revision6完整工程变化 | 必需同版预审/后续真实验证 |
| --- | --- | --- |
| ND-API-001 | interfaces §1的PromptReservation私有检查包装拥有原Reservation，构造核Some RPC/kind/scope；control不实现该trait，无unwrap/伪ID | NDT-002：正编译+非法包装/Clone/Deserialize负编译；真实prompt/control反例 |
| ND-API-002 | EncodedPromptFrame::byte_len_checked，只从同实际frame bytes.len checked得含LF总长；Frame move到真实enqueue | NDT-003：非Clone大body、retain次数、编码次数、实际writer总字节 |
| ND-WIRE-001 | 主合同§2/3、interfaces §2/4：一次claim组合factory，首await/IO前保supervisor，同pool/closer受信仓储构造；R3同D ContentOrigin；Prepared/Direct/Job探针完整字段和私有mint；prepare/adopt/start观察源 | NDT-004/014/030/032/036/080：actual register→consume→real SQLite/R3/ACP，无假Ready/行数据造proof |
| ND-OBS-001 | 主合同§6覆盖所有Incoming/Response子类及terminal无seq；observation无target，事实真target；无目标仍连续ack、unresolved source隔离；requires_action Active且raw保留；snapshot receipt不等因果 | NDT-020/040/044/066/067/073：整连接混排无target事件、多目标、未知state、无Run活动 |
| ND-OBS-002 | RequiredGapKey实际owner/scope/resource/generation+RequirementId集合；成功只消费全字段相等的一项，旧代际不清新，SQL间隙不是ack水位 | NDT-049/050/054：A成功B失败乱序、旧scope worker迟到、未知提交inspect、真实writer仍被B阻 |
| ND-SQL-001 | 主合同§5+新DDL：arm原子persist selection+attempt；proof/inspection/StoredFacts完整字段；codec/跨表/终态/幂等/close/restart责任明确；取消不可达真实Ticket Unknown→Written正例 | NDT-010–022：真实SQL故障/类型/回滚、closed不重开、未知确认与真实终态区分 |
| ND-SEM-001 | 用户明确答复已同步root新spec；inputs-v5.json核当前3政策hash（历史semantic-rebase保v3）；主合同§8精确v1/v2 Stopped(reason)+Suspend旧项/手动继续，error仍可靠失败，end_turn成功，优先中断独立，unknown不猜终止；DDL新增stopped | tester另版更新NDT-061/070与主tests增量；原Pending是历史，不回写。reviewer核授权/spec/新oracle一致才解除本问题 |
| ND-TEST-001 | interfaces §5冻结只观察/暂停真实read-capture/emit/pump apply/ack屏障及同锁poll同步见证；不加生产yield/setter | NDT-042动态held LF和实际writer；NDT-043静态guard持续+并发回归，二者范围分列 |
| ND-RETRY-001 | 主合同§4、interfaces §2：SelectedContext沿Job→Running→SelectedRetry唯一move；真Ticket Zero+精确StoredZero+同active/resource才next_attempt，保planned_run_id，不重复D/回队/假Direct | NDT-019正路径必须有原waiting项真实第一次Zero、第二次实际成功writer；Pending/Unknown/Written/SQL未知/资源换代均负例 |

## 同版验收分组

不改tester稳定ID/标题/预期；以下是工程接口落点，独立tester自行维护用例源码与结果；实际Loaded证据链及最小source/resource表另见resource-ownership.md，同版补审需覆盖reserve前零spawn、真实resume/flush后才mint、callerDrop保Job/IO、共享Job关闭顺序：

| 定义组 | 本revision可执行入口/观测 |
| --- | --- |
| NDT-001–004 | 新Storage组合factory，真实两仓储wrapper；inputs-v5/历史rebase/freeze摘要；私有capability负编译夹具 |
| NDT-010–022 | schema6真实仓储/migration，原Closer的Before/AfterCommit及close hooks转发，完整inspection；只在tester自有库 |
| NDT-030–038 | actual R3 register/consume、同D来源账本、E Pending、内部native capture与当前完整config；管理并发不绕授权/不重actor |
| NDT-040–054 | prepare_observed，真实lease/ack/gate，5类cfg(test)观测边界；实际writer accepted/flush；Gap集合query只读视图 |
| NDT-060–073 | 真实原始v1/v2帧、精确scope/RPC/messageId、origin、状态区间；不调用finish_work伪造v2结果；早响应仍保transport |
| NDT-080–085 | 组合supervisor的factory/source/job/SQL槽；原IoOwner/ConnectionOwner实际join；Drop/concurrent close、wallclock与monotonic分开 |
| NDT-090–092 | 原39队列、R3、公共ACP全部回归与真实check/build；本L4不代Node/WSS/Web/Harness/Windows Job树 |

新增只读测试诊断（cfg(test)/同crate fixture）：NativeCoordinator::diagnostics(target)仅回runtime/owner/resource、准备/active/transport阶段、gap键/RequirementId与fingerprint、source quarantine计数、实际任务槽数和join状态；无body、原始配置、setter或生成proof。测试必须从真实factory走register→consume，不可用diagnostic字段重新装配Job。关闭hook复用原ConnectionOwner，禁止新建第二close控制器。

R3管理SQL包装器精确故障点和native repository新增故障点：BeforeCommit/AfterCommit(Reserve,Arm,RecordTransport,RecordObservation,RecordConvergence,Close,InterruptPreviousRuntime,ReserveSource,RecordSourceSpawned,RecordSourceBound,ReserveResource,RecordSourceInitialized,BeginLoad,RecordLoadTransport,RecordLoaded,RecordResourceState,RecordSourceClosed,InterruptResources,RecordSourceLoss,CloseUnlaunched)。AfterCommit报告丢失是“确认丢失”夹具，不能声称真实SQL COMMIT未发生；真实SQL错误另用已拥有测试连接/文件故障。每个错误记录方法/点/实际次数，不含SQL参数正文；No await区间不能注入异步屏障。

## 交付前仍需满足的门槛

1. reviewer确认本revision全部工程答复与完整接口/DDL可编码；tester复核同版80定义（64原ID+16资源组）及已更新Stopped oracle，引用当前input hash及用户原答；不以作者本文件关闭独立问题。
2. 最终root306已重基准并逐项匹配；后续任何源变化仍须另冻结影响清单，不混用历史writer/248源。变更写范围由root再分配，本次作者未改root/其它候选。
3. 编码才引入真实模块/API/hooks/migration；本文是草案不是已有能力。不得在门槛前将任务勾完成，也不执行DDL/用户库。
4. 编码后冻结实际source+锁摘要，tester正式L4执行与reviewer交付检视分别完成；root再合入并跑实际完整入口。Node/Web/Harness/完整Windows树与资源策略证据后续按负责范围另验。

无待决产品语义：ND-SEM001已获明确答复并有权威spec新hash；尚未完成的是**独立同版门禁及下游实现/动态验证**。不把这些未完成项记成产品已通过。

## 作者实际检查

已执行：最终root306逐字节snapshot/hash、before/after entries相同、v4 freeze/19成员未变、NDTv3及reviewer正式004报告与最终Root报告证据核对；设计文本/DDL字段与闭合stage矩阵自查见static-review-v5.json。未执行SQL解析/迁移或编译。

未执行：实现、Rust编译、SQL语法运行/迁移、NDT动态用例、Node/WSS/Web、真实Harness/Job树、安全/供应链审计。无新增依赖、安装、网络或锁文件改动。自查不代独立预审。

## revision6 新资源阻断逐项答复

- ND-RESOURCE-001：lifecycle-ports.md给完整NativeLifecyclePort/OwnedNativeSource/SourceLaunchPort/ConfigurationPort关联类型、factory P/L/G等式、原Init/Load Reservation+Frame+Ticket+Receiver/Ack/Gate错误归属；PreSource/PreResource只SQL元数据，实际prepare后同SourceSlot一次BoundScope和record_source_bound再start；应用不importACP/downcast。
- ND-RESOURCE-002：独立SourceLossProbe/record_source_loss及source_losses/loss_targets表，只从实际P.next Terminal产生，无Received/sequence，保存Unknown不解除Lost/资源责任；Convergence类型与DDL均不含Loss，release端口不能接它。
- ND-RESOURCE-003：LaunchController唯一permit.enter与revoke线性竞争，NeverLaunched消费未来启动权+SQL quiescence/完整inspection；close_unlaunched原子关闭精确reserved集合，不造IOjoin/Job0；enter后错误仅实际cleanup或Unknown。
- reviewer附带文字：同步prepare所有失败按BeforeAdopt/AfterAdopt typed归属统一，adopter拒绝返原IoOwner；无IO任务的阶段不隐含asyncjoin，安装中shutdown等待installation_complete。

以上是作者工程答复，不是独立关闭。本轮80个NDT定义仍全部未执行；请tester/reviewer对同revision新hash独立复核，最终writer c737/旧29组合306源已在inputs-v5重基准；仍须本revision80NDT/reviewer同版预审后才编码/正式验收。

## ND-RESOURCE-004：全阶段Pre地址与真实closure证据

closure-contract.md逐格覆盖reserve→enter→adopt→takeIO→prepare前后→memory Bound/SQL bound未知→start/init→setup前→load准备/可能写→SQLLoaded，列真实owner/可选字段、合法proof、持久入口及拒绝情况。NativeResourceRepository关闭地址改原Pre，SourceClosureProof不要求Scope/setup；真实stage证据与原SQL quiescence/精确集合决定能否closed/released。整源原子close源+全部同instance资源，单目标共享路径为UnloadedReservationAbandoned（严格无可能目标IO）或真实TargetNativeConverged，不杀其它目标。

NDT-105/112/114应按原80ID/标题另版补全各格正/负验证；作者不修改tester定义，不把矩阵自查当测试运行或独立关闭。Source已Bound/resource Pre、recordBound提交未知及BeforeAdopt错误必须有真实Owned清理后Pre持久闭合正例；不得造Scope/wire/loadRPC/空IOjoined，EnterUnknown仍只Unknown。


## revision6 ND-CONTROL-001 答复与新门禁

新权威附件[control-ports.md](control-ports.md)提供完整NativeControlPort/关联原请求/无RPC cancel/同Frame和Ticket、R3 Prioritized可靠E hook→唯一handoff、原Lease→自动unsupported、ClosingAuthority→当前安全取消链；Prepared之前纯capture、错误返原物、真实终态与late-cancel barrier、必要SQL故障保gap、11表3索引中的独立非Received控制facts及完整失败表。人工Selected/SQL审批owner/独立cancel Web受理不扩入，Unknown权限不自动cancel/deny。

新input-snapshot/external-v6记录tester v4 outputs358888f3/gaps25c5及reviewer v5报告4d9c7785。004仅合同层已获独立认可，CONTROL001等待同版tester/reviewer，不由作者自行关闭。80定义ID/标题和主tests123未改；NDT037/052/071/073等原定义的具体真实可达路径与新增子断言建议在附件§8，由tester独立维护。运行0、无编译/SQL执行；根最终306摘要必须再次实际逐项核，旧v5所有成员完整保字节。
