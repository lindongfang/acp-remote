# Provider failure 与 source Permission：组合 v3 编码门槛

结论：**限定工程设计/ABI 编码 PASS**。ND-PROVIDER-ABI001、002、003 均按下列固定补充 **Closed-design**；允许作者在独占 copy 实施真实平台/Native/SQL 路径并冻结新候选。不关闭 NDPREP-NOOWNER001 的产品缺陷，不签 API06/WIP、Native80、新证明实际成立、SQL执行或最终交付。原全部冻结/首红保留，无需新增用户产品决定。

## 固定版本和核查范围

有效组合为原 provider draft `d56c0c177e0f8578cafedf4a1fef50b51f1ddc19824763bcad1f3bcbdfa69e05`（9成员）、addendum-v2 `75385018d6c9bfef14d4a1a81e65c693fe4c48892913432becc2daa591977a34`（1成员）及 combined-design-v3 `f61d394e1ab799fc6a3ec8d6b6180b31aaa1ff0404fea2048136f7dd39211e80`（26成员）。上述成员逐 SHA256 均亲核；v2 有效 ADDENDUM `485ef8a99041ab2214889ac2c82345d78cb232bc92a46925d1af848fc2fe4f5d`，v3 CONTRACT `13b8f4f9a8cd0fa0e01de21aa1962685e315b6b202ef6c52f25400b105049f8e` 完整读取。

输入绑定固定 API06 `5442d93177b1825ea1c28cd091fa49a95c145c7a765e6cabc41640747f2e7b59`；本轮核的是包内实际原代码与拟议 ABI/DDL，不把全 API06 或作者 live copy 冒作正式候选审查。参考本 reviewer 首审 `b5f1a55acc2438c5234ce9ea2783bd0ca7b02f2c859982d35e3a24fb611860b8`。包内原 process owner、source、supervisor 代码及本次 Permission 原 model/transport/state/adapter、Generic ports、observation/control/closure DAO 的相关分支已实际核对。

真实 v14 CreateProcessW2 首红的关闭可达性仍需新实现同源后重验。本轮没有运行 SQL、进程、编译或测试，没有修改 Root、作者、tester、原资产或既有报告。

## 原三个断点的精确闭合

**ABI001**：`prepare_tracked(spec)->Result<PreparedProcessSpawn,ProcessOwnerError>` 固定在 permit.enter **之前**，只做可表示性/真实准备分配，无 OS/await。可恢复错误返回同一未消费 permit 的 BeforeEntryFailed，由原 controller 不可逆 revoke；不拿错误字符串推 NoProcess。enter 后 into_owner_driver 为纯 infallible move，立即移交原 entered token、adopt 原 owner，才执行 driver.run。原 driver 非 Clone，unused Drop 消耗未来 create 权；running Drop 先处置或移交真实局部 owner，再 publish 原 terminal。Create/assign/resume 与 stop 同阶段围栏，不留成功 Create 后只有 DTO 的窗口。

实际实现必须保留该顺序和同 owner identity。request_stop、任务取消、DriverTerminal 或通知均不等于进程/IO/SQL join；Rust allocator abort 不能冒 typed Allocation。新 trait/enum 的原 Running 证明在桥接和缓存过程中不得丢失其同 entered-controller 绑定，application 不得 import 平台句柄类型或以 owned.is_some 充证明。

**ABI002**：删除 original_job_closed bool。私有 NoProcessCreatedProof 必须含原 OriginalSetupClosedProof 与 DriverTerminal；SuspendedUnassignedClosedProof 必须复合原 suspended root 实际 exit、never-resumed 与同原 setup/Job 全关闭证明。跨 inward port 的 opaque OwnedNeverCreatedClosure/OwnedSuspendedUnassignedClosure 保完整证明和原 entered binding；SourcePhysicalClosure 不再降为 Copy 摘要丢证明，SQL重试保 Arc 原物。错误/取消/超时/关闭未确认则同 owner/句柄保留 Unconfirmed。

实施时 OriginalSetupClosedProof 必须由原实际 handle/管道的关闭结果 mint；不得仅因 OwnedHandle Drop 被调用就虚构关闭确认。成功 Create 的未 assigned suspended root 必须查原 root handle 退出；assigned 或 resumed 分支仍走原 Job 全树，不能仅 root exit 或另建空 Job。字段不进 Debug/log/DTO。

**ABI003**：增加原 SourceSlot 的显式私有 launch outcome：Reserved/PreparedBeforeEntry/EnteredOwnerAdopted/DriverRunning 或 Failed。adopt 只接管责任，不等于创建/运行。Spawned/take_streams/prepare/start/init 只接受原缓存 OwnedRunning 及同 Running/entered 证明；OwnedFailed、被取消、停止或保留 owner 的失败重入均不能走 bootstrap。BeforeEntryFailed 原 permit 单独退役，custom EnteredUnconfirmed/NoOwned 保 Unknown，不授权新 launch/握手/重发。Running 也不直接 mint Loaded，原配置/协商/resume/SQL 流程仍须真实完成。

## 组合 schema6：精确批准的候选形态

批准实施目标是完整 `schema6-combined-proposed.rs` **`fe155c0d82b5aa587dee8d5000f5dc5e959f00e95974c4db07a886e536a2e0ad`**；对应完整 SQL `478262bbeb09bbda8441584bd78eb41bd9fc80093ae5c4f0d9ded2fa39c4daf6`。不是把含绝对候选路径的 no-index patch 直接应用生产。

已实际静态提取、比较全部14 DDL：73b5e8 原形态→a4fce905 provider 只变 ordinal1 source 与2 resource；provider→fe155 combined 只变 ordinal14 control。三者均11表/3索引；两个 SQL 文件与相应 Rust 原定义逐条加分号拼接完全相同。没有执行数据库。

原 CORE/SESSION DDL 节与当前 Root 实际相同；product_schema.rs `e3d2d32c0e3ec32f1c6ef4ed4a971f31c4f203bd8ca6d67063f81b4f14aa6c00`、registration_schema.rs `4bc3bb41a8b40456ac1bccf6bb331f1549b5e4e60b048a106504d4ba0efddf67` 与 Root 逐字相同。dispatcher 因候选 latest6 与 Root5有职责差异，不能宣称整 dispatcher 文件无差；旧1–5定义未变。

- EnteredNeverCreated 仅 entered_before_create/create_rejected、no_process_created、NotPrepared、JobNULL；source 全 Scope/bound/init/wire/evidence 空，resource 全 load/transport/loaded 空，不得伪造 epoch。
- SuspendedRootExited 仅 OwnedConverged/OwnedBeforeIo/NotPrepared/JobNULL，原 Windows owner、全 pre-IO NULL 条件和完整原 root+setup 证明。AssignedJob 仍 WindowsJob+真实 active0。
- 两新 pre-IO 分支在**同 close 事务**检查原 source Reserved/Unknown、原所有可靠 reserved resource 精确集合、记录标志与 absent pre 项，禁止冲突 Released/Spawned 等状态；8个 no-native-effects 查询都绑定同原 instance，结果必须0。跨 instance 保持隔离。该 SQL 只作必要事实围栏，不证明 OS 无进程；每个 effect/集合变化/SQL错误均拒绝，不修复或删除。
- SourceClosed/Released 必须精确比较原 before 与投影 after，COMMIT/关闭不确认保同证明/行/claim，不因 task join 放资源；旧 schema6 布局不符 exact preflight 拒绝，无 silent repair/schema7/旧私有内容回放。
- control 只增加 `(permission_cancelled,shutdown,resource=None,work=None,rpc=Some)` 且 target/owner/load 同NULL、原 source/scope FK可靠。resourceNone native_cancel/priority、mixedNULL、permission缺RPC、unsupported带target/Selected等仍拒绝。原 frame/transport/phase/原typedRPC约束不变。

schema6 独立 expected fixture/ordinal/787/preflight 的新版替换 **须原 tester 另提 exact diff、独立 gate**；本报告不改其原件。旧1–5、body/key、max1、Future77、回滚/物理close、product_*计数及旧120预期不得顺带弱化。

## Source-scope Permission 的真实构造与限制

实际 model PermissionRequest 是私有品牌、可 Clone 的原对象，没有独立 ReplyCell；原 Connection.reserve_permission_response 调用原 Core.State.server 的 scope+digest+Waiting/Reserved/WriteCell 仲裁。actual accepted 才 spend，actual Zero 才可能恢复原 Waiting。不能从 SQL、bare SID、typedRPC 或 Raw 重建同等权。

实际 API06 adapter 对 permission reserve 使用 require_loaded_control，observation 只给已映射 target 保存品牌；因此 KnownPre/loading 与 Unbound 的责任补齐有真实工程必要。v3 的 source-owned PermissionSlot 在原当前 lease ack **之前**保原品牌和来源；其后分配/SQL失败 seal源、保 slot/lease，不得 ack成已应用却丢责任。target facts 仅持同 Arc。原初始 association 由同 source stage fence 下真实当前 map 得出 KnownResource(originalPre) 或 Unbound，不能凭新同SID/便利FK改挂新epoch。

新增 `reserve_source_permission_cancelled(&Source,&original,&SourcePermissionCancelBinding)` 有实际可调用路径：私有 binding 只能由已登记的 whole-source closing safety operation、原 slot、同 original scope/source/close_intent 构造；公开 getter 不铸授权。adapter 检查同实际 AcpSource、品牌 scope、typedRPC及原 source绑定，再调用同 Core.reserve_permission_response(Cancelled)，原 Reservation/Frame/enqueue/Ticket/error ownership 继续使用。无需 Loaded/Run/伪 BoundSetup，也没有 source-only nativeCancel 或 Selected/allow 方法。

KnownPre 原 resource binding 去掉 Loaded 前置的同时仍须原 Source/config/Pre/请求事实匹配；保存 resourceSome control 必须原 resource reserve 可靠。缺 row/确认未知只能在 whole-source closing 安全 SourceOnly 分支，不为满足 FK造假。Unbound 原责任保持 source 隔离，target close/priority 不得拿它或其它资源的 slot；closed/retired sameSID不能新挂旧 grant。

未知主题/选项不自动拒绝。当前 permission-approvals/spec SHA `935bd6039aa07fc33e3706ba50d20766cbd58022ac8dc57450547bec2854ea3b` 明确原可验证未知选项允许人工选择，并允许协议必要的安全取消；请求可以来自加载、后台或无Run。新 supplement 只在明确 closing intent 安全 Cancelled，不增加人工裁决/批准持久化/页面动作。不因 foreground idle、快照或 Run完成自动失效；不改普通已确认权限语义。

## actual poll、SQL gap 与关闭责任

source-only ControlFence 检查同原 source gate、slot Arc/obligation/control claim、scope/native原identity、closing且未physicalClosed、同 close_intent；只有 Shutdown authority。真实 io.poll 仍必须经原 ProtocolState 每poll仲裁，heldLF/unack不旁路、prefix不能插帧；无 DTO/bool/fakeScope grants。原 resource ControlFence、Priority可靠E、nativeCancel无RPC、Unsupported原品牌、park/c737期限仍保持。

已明确授权的安全 Cancelled 可在必要 SQL 坏时真实尝试，但 gap保留、普通 Prompt/Load不得放行。Written不等于 SQL可靠或 native工作结束；terminal SQL确认才正常退役响应保护。Zero 重试须原真实 Zero+Stored、旧 worker joined、原 slot仍pending、同 source closing意图/currentScope、原 serial检查，新预算受原close截止夹住；Unknown不自动重答，保到真实 source收敛。reserve前错误不凭空造Zero，reservation消耗用原CancelledControlReservation。

whole-source closure inventory 额外包含所有 source permissions 的原qualification/来源/claim/terminal/retired/fingerprint，即使没有 target/dispatch。先 seal并实际 join原观察/control/metadata生产者，再冻结精确集合/做必要SQL；target关闭只能负责对应 KnownResource，不能退役Unbound。真实 whole-source物理收敛可按原closure规则退休 unresolved response责任，不能伪 Written。NoOwned/未知、task join或通知都不够。

CombinedSupervisor 的 last_shutdown_report 为只读最近实际完成报告；等待必须先登记原progress通知再检查，在一次 false 后等真实 driver/owner/task/metadata变化。无变化不忙循环、不 self-notify充进度；Storage等待被取消仍保同supervisor/claim/JoinHandle，不能提前关闭SQL释放责任。永久无法证明的custom NoOwned可以诚实Pending且可取消等待，不要求伪成功返回。

## 后验门槛，不冒签执行

原 provider12子断言已明确真实OS与cfgtest actual cut：missing exe必须实际CreateProcess0；assign/resume故障必须先持原真实 suspended/assigned对象，失败点不能写proof；caller waiter drop与driver drop/任务失败分开；原root/Job/IO事实由真实对象证明。原v14红保持，不因新typed接口改写为“旧已关闭”。

独立tester须在实际API后固定精确子断言/函数映射并同源验证：KnownPre初始化前/加载中无Run/unknownSID品牌ack前留存；unknown subject无自动动作；whole-source Cancelled与target/priority隔离；同Core race只能一次；实际Zero+Stored重试及旧serial/预算；partial/flushUnknown；必要SQL ABORT时safeIO与gap；callerDrop；所有source/target阶段closure及无owner负例。加上14逐ordinal/实际787、schema exact preflight/阴性NULL和native-effect矩阵、原Windows全树/registry/pool与受影响原资产回归。

编译或作者开发绿不能代替这些独立实际分支；formal80仍0。最终SourceSlot完整证明、实际closeHandle/任务取消、generic桥接以及未确认时同责任保留，将按新冻结代码再次审查。本 gate 不授权并写 Root/Shared，最终生产桥仍待候选固定后串行合入及真实组合检查。
