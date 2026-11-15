# revision2 固定 QEXT / EVT / EXT 验收映射

2026-10-31，独立tester定义，尚未实现或运行。与 `v2-fixed-definition.md` 同组，绑定主合同 `d4a2840699b69a689fb3a43a39b7874609a38851052cd094a2e68c6364f1e74c`、owner合同 `32d1f7728bcd20afb860575e6dcb673070acba398d1e83cc62c61211e00b1314`、DDL `ba49b4012f96c42220744c0dd9c975035408f0649209eb77ac983aef04fc8a1d`。旧候选保留原字节；本文将其未冻结前提替换为revision2合法API，不造生产setter。

QEXT是纯队列L3；EVT/EXT是实际SQLite+QueueService+InputQueue L4子断言。跨表编号可以由同一实际测试覆盖，报告逐编号映射**同一测试名**，不得重复计测试数。真实协议、writer、原生成功/失败及完整L5/L6都未由此证明。

## 固定前提与屏障

- 使用v2主定义的合成A/B/T1/E1、不可Clone/Debug Body、真实双SQL连接。P目标通过Storage::start_queue_service_with_repository获得同一实际service和配对NativeQueueDriver，包装器默认委托真实仓储。
- 通过service.validate_body/register/consume正常受理W、X/Y等，item来自节点实际RegistrationId。不得直接导入QueueInput到实际owner、替换owner map或修改私有active。
- local W：受控environment=Ready(E1)，实际W管理受理成功后 `driver.prepare(T1)`→`begin_selection(&preparation)`→合法ActiveKey；无任何原生字节，driver没有body/first-write接口。
- 无Run W：environment.resource_epoch(T1)=Some(E1)、readiness=Blocked；`driver.open_external_work(T1,E1)`取得opaque ExternalWorkKey。它不是队列item/Run，不占三个等待位。
- BA：包装器完成真实inner.begin_consume并由第二连接确认Applying后扣住返回。SI：实际D完成、wrapper进入settle但真实COMMIT前扣住；SA：inner.settle真实COMMIT后扣住返回。控制器到达/放行/SQL可见性建立先后，不依赖sleep。
- 失败事件用 `driver.observe(T1,QueueOwnerEvent::Settle{active,disposition:Suspend})` 或 `ExternalForegroundFailure{work}`；observe成功返回建立实际owner已处理的本地顺序。环境队列路径另外用poll_target消费同型事件，D前排空已入队观察并重取readiness。
- C/D同短锁无await。任何“COMMIT已成功、但C未处理”的停点都属于BA，**不存在C已处理而D未apply的可插事件缝隙**，不人为增加yield。
- Pending的inspect允许当前暂定全文借用并明确Processing(applied)，不能伪称Stable。no writer live始终Blocked；Ready只为上述受控算法夹具。

## 固定新增纯队列用例

| 子编号 | 操作步骤 | 预期结果与主CL |
| --- | --- | --- |
| REG-QEXT-01 | 原正常X/Y/Z，suspend_waiting() | 返回3，全部挂起、failure_suspended=true；ID/revision/正文/submitter/editor/顺序不变，无active/Run新增。关联CL-091/110。 |
| REG-QEXT-02 | 对上述再调用；resume后再调用；另空队列调用 | 依次返回0、3；空返回0仍置failure模式。没有复制/版本变化；checked序号不降低。关联CL-110/112。 |
| REG-QEXT-03 | 正常X/Y/Z优先Y并prepare；suspend_waiting；旧token begin/release；resume后prepare | 新挂起3；旧token StaleToken；优先清除，恢复后第一项X，不保留旧Y优先；拒绝不影响新准备。关联CL-090/099/110。 |
| REG-QEXT-04 | 合法active D+token、等待X/Y；suspend_waiting；prepare及访问active | 返回2、X/Y挂起；D元数据/body/token保持、prepare Busy；不结算D、不证明停止。关联CL-091/110/113。 |
| REG-QEXT-05 | 挂起X/Y后Submit new_d Blocked；Edit X、Prioritize X、prepare/begin new_d并Advance | new_d非挂起并先选；X保持挂起、优先Suspended；Advance后X/Y仍挂起。关联CL-110/111。 |
| REG-QEXT-06 | 原settle(token,Suspend)与新方法复用路径；保原39资产原字节回归 | 精确active释放一次，其余挂起/优先清除/失败模式符合原语义；旧clone不消费新active。39结果另报、不可回填旧报告。关联CL-099/110/112。 |
| REG-QEXT-07 | 非Clone/Debug完整结构Body通过挂起/恢复/移出，检查返回正文；等待含prepared对照 | 结构逐字段一致、不转文本、不增加trait/字节预算/时间依赖；prepared也计等待且挂起失效。关联CL-095/097/110。 |

序号/外部work serial MAX不自然可达，独立公开测试不造setter；若实施有封闭cfg(test)极值控制，由同crate F目标单列，不能把原39或公开EXT动态当MAX证据。

## 固定可信事件与管理D处理序

| 子编号 | 实际步骤 | 精确预期 / 对应主定义 |
| --- | --- | --- |
| REG-EVT-01 | local W或无RunW，X/Y等待；new_d在BI与BA两个可达分支暂停；先observe失败成功，再放begin回报/D/E | 只挂当时X/Y；new_d后加入保持非挂起。live Blocked为Queued；受控Ready的DirectSelected只列算法证据。对应CL-110.REG01。 |
| REG-EVT-02 | 原候选“C已处理、D未执行” | revision2不可达，排除独立执行/计数；真实COMMIT完成但owner未处理归EVT01-BA分支。不是需求缺失或用例删弱。 |
| REG-EVT-03 | W活动，X/Y等待；new_d已D成为第三等待项，SI/SA暂停；observe失败再放E | X/Y/new_d均挂起；管理历史Queued不自动恢复。对应CL-111.REG01。 |
| REG-EVT-04 | new_d已E稳定Queued后再observe W失败 | 与03最终挂起集合一致；SQL快慢不能改变D/事件真实顺序结果。对应CL-111.REG01的无pending对照。 |
| REG-EVT-05 | X/Y已挂起且W可信活动；Resume在BA扣回报；先Fail(W)，后D/E | Resume实际apply在失败后，恢复2项、X先Y、failure模式结束。对应CL-112.REG01。 |
| REG-EVT-06 | 同前，Resume先D恢复2项，SI暂停；再Fail(W)后E | 当前X/Y再次挂起、failure_suspended=true；历史Resumed{2}不能清掉新失败。对应CL-112.REG01。 |
| REG-EVT-07 | 正常未失败X/Y，Resume D为0；随后Fail(W)，再同Resume登记重试 | 历史Resumed{0}；X/Y因失败挂起；Existing重试不再resume，不清新失败。对应CL-112.REG01及CL-017.REG01。 |
| REG-EVT-08 | Fail W先挂旧项，new_d后D；SA真实提交后返回CommitUnconfirmed，reconcile两次 | 仅发布相同历史Queued/DirectSelected，D/事件都不重跑；旧项挂起和new_d资格保留。对应CL-112.REG03、CL-076.REG01。 |
| REG-EVT-09 | new_d先D、Fail W随后；settle COMMIT前失败，inspect自身Applying，reconcile | 只补同outcome settle，new_d仍属于失败时等待集合；不重新apply/重播W/恢复。对应CL-112.REG03、CL-076.REG02。 |
| REG-EVT-10 | local W被有效key settle后，受控新D direct；active_key取得D；再投旧W Advance/Suspend | 两次旧事件Registry(StaleObservation)，D/等待/Processing状态不变；D实际key仍有效。对应CL-110.REG04。 |
| REG-EVT-11 | 正常shutdown/join后重建service，旧preparation/active/external key到新driver；另有未join任务时再次start | 旧能力拒绝且不改新owner；未join时claim不释放，不能替换owner。旧SQL回报在join后不可达仅作静态防御核对，不假造公开回报注入器。对应CL-099.REG04/05。 |
| REG-EVT-12 | A/B时捕获Ready，SQL期间environment ActivityChanged为Blocked或E2，D前已入队；放回报 | D先消费已入队事件并读取最新readiness，不沿旧Ready选Direct；旧resource选择被最后围栏拒绝；live无writer始终Blocked。对应CL-099.REG03、CL-113.REG02。 |

EVT01与03反向都必须实际运行；不能只验证一种时序或将全部新项统统豁免/统统挂起。Resume05/06同理。Fault/reconcile不会另创协议事件watermark，不把内部序当ACP归因证明。

## 固定无 Run 外部工作覆盖

| 子编号 | 实际API操作 | 预期 / 复用映射 |
| --- | --- | --- |
| REG-EXT-01 | open_external_work W；X/Y等待；new_d BI/BA；observe ExternalForegroundFailure W先于D | W无InputQueue item/Run，X/Y挂起，new_d非挂起第三项；方法即时不等SQL。复用EVT01无Run分支。 |
| REG-EXT-02 | 无RunW，new_d先D、SI/SA时失败 | 三项全挂起，E不恢复。复用EVT03无Run分支。 |
| REG-EXT-03 | 无RunW与Resume前/后、以及no-op Resume后失败 | 恢复与再次挂起按本地D/事件顺序；旧resume重试不清后来失败。复用EVT05–07无Run分支。 |
| REG-EXT-04 | W Failure已消费；new_d后来加入；再次W Failure、Completed | 均Registry(StaleObservation)，new_d不挂起，完整投影不变；仅ResourceEpoch相同不能再次生效。对应CL-110.REG02。 |
| REG-EXT-05 | W未结束时再open→Busy；有效Completed消费W且不挂/不恢复；同E1新open W2，再Failure；重放W | 新W2 serial不同且可作用当时等待；旧W仍拒绝，不以资源寿命禁用合法后续工作。对应CL-110.REG02，并补成功完成对照。 |
| REG-EXT-06 | 旧owner/runtime合法key对新driver；当前resource换E2并处理ActivityChanged，再旧E1 key失败；合法新E2 key | 旧key StaleObservation不改新queue；旧资源key失效不证明旧work停止/资源收敛，readiness仍Blocked。新key只能由当前driver合法mint。对应CL-099.REG05、CL-110.REG03。 |
| REG-EXT-07 | local active D+等待X/Y时合法无Run外部失败；对照当前资源不符与重复key | 只suspend_waiting，不清D、不返资源/不清pending；不匹配完整key拒绝。active_key D前后相同且有效。对应CL-110.REG03/QEXT04。 |
| REG-EXT-08 | EXT01/02/03与BA/SA应答未知、SI前回滚交错，观察Applying完整StoredApplying并reconcile | 事件和管理D分别一次；只核原operation/outcome，不重播旧外部key、不额外挂新D、不清后来失败。复用EVT08/09、CL-112.REG03。 |

对opaque key的runtime/owner/item/revision/resource不能任意逐字段改造来跑“公开负向”；公开测试使用真实旧/异owner/异resource工作key，并核getter正绑定。额外逐字段伪造仅可由明确同crate负向测试分列，不宣称来自公开生产调用。

## 同gate覆盖与统计安排

- P真实公开目标覆盖EVT/EXT可达矩阵、SQL后COMMIT包装、实际owner控制/读取/关闭；F同crate覆盖事务前真实故障、封闭极值及内部防御；M/C覆盖schema5新增和旧迁移全部适用断言；QEXT独立L3目标可与原39同版回归，但统计分开。
- 预计Rust测试名按固定子ID命名；同一测试覆盖多个别名行时JSON映射相同test名。执行报告列实际tests运行数、逐行状态、分支、直接断言及证据，EVT02明确不可达/不计数量。当前全部为定义，没有任何pass计数。
- final source冻结时验证原39资产07f19…及历史报告3331737…不变；包含新suspend_waiting的根源作为新版本验收，不能沿旧snapshot的39通过代签新版本。
