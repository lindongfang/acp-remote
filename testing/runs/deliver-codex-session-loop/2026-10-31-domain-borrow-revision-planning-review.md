# Domain同步借用与revision编码前独立复核

2026-10-31。只读合同、最终独立定义及当前Domain实际API，未改产品/测试或执行新能力。

**r8限定Domain编码门槛通过，未见剩余具体定义阻断。** 只允许views.rs、views/staging.rs及其开发测试/必要API文档；不放行ACP/current/scanner、16KiB读取预留、输出队列、持久化或授权。独立DBR01–08全not-run，实际交付另审。

依据：根`domain-borrow-revision-contract.md` SHA256 `e75e54d7bcc15efb18cab339fbfcd19deed852b7d7f15d0f056860ca0954430e`；独立`domain-borrow-revision-definition.md` `28319d55ca866d8b4e83b030ea5f649d847268224577625d3e5a6cf17e1cea87`；inputs `e3673a0f6522d100e9543ba4ff1c5a30862e3f8705fd6317c722077ad15ede71`。完整r7/八组已实际读取，再核r8仅标题及私有hook单调拒绝Last→Penultimate的精化。原r7文件/输入保留，不混用。实施前Domain基线仍a22bb6e5/d2a2a6ec/native8d01cef2。

## 借用与身份

with_window以高阶生命周期约束（HRTB）限定正文/条目/key/iterator及捕获它们的future只在callback内活着；R可返回只借外部h的stamp或固定值，不能延长对Window正文的借用。不Clone guard、不创建snapshot、不Clone item Arc或持owned-body租约，满4096/节点32768时不争新增引用额度。callback主动复制正文或分配输出是调用者独立预算，类型系统不会也不能禁止所有I/O/复制，合同未冒作此承诺。

stamp私有借用原&ViewHandle，完整same_read比较含scope Arc身份、NativeSessionKey/source、runtime/connection/load及read generation；不只比两个u64。活h保住旧scope分配，防旧stamp存在时裸地址复用的ABA。合法h clone可用，错容器/目标/资源/read在sealed/revision之前Stale。stamp不持body、不授予读取授权，不是可直接序列化的网络cursor。

## 一致性与锁

统一ReadMeta把state/revision/sealed置于原cell同一Mutex，原/新publish、相同bytes的新item身份替换、每次实际FIFO及首次状态变化都在同一短临界区推进。仅费用申请/退账、重复sticky fail或Finished finish等no-op不推进；错误后的FIFO不能丢版本。prepare资源在锁外，容器所有权转移与checked版本在锁内，旧Item/Charge/候选在解锁后析构；node→target预算锁序保持，Meta不取得这些锁。

with_window预锁复制state/rev后解锁，callback看冻结header和不可变items；callback及其返回R析构均不得持Meta/预算Mutex。回调内/另一线程Drop同cell stage可改变live state，post短锁比较检测ReadChanged/sealed，R失败时锁外Drop且不自动重调。成功仅承诺post比较点一致，不能当网络发送期间最新或撤销/授权围栏；callback只可写已付费可作废的私有候选/做正常清理，不能提前发布成功页或执行不可撤销副作用。

## sealed、优先级与测试钩子

r8§6A逐API表可执行：一致读/新构建写入sealed后SequenceExhausted；window_status/snapshot/item_lease仍按原诊断查找及费用规则可用，不把sealed优先泛化成全部读禁令。原参数与跨read身份检查不能粗暴搬到统一sealed检查之后；当前匹配cell后的Conflict/MissingItem/Incomplete/poison/short及容量申请按表让sealed优先。既有writer只查原cell，旧stage/Drop不污染新read；abort仍消费清理，release/collect/begin仍可回收。

MAX时下一真实变化封闭，不回绕、不改变item/position/watermark，已有Incomplete保首因；旧stamp在sealed后全部拒绝，因此不需要假造一个新有效revision。未sealed MAX的重复fail/finish仍no-op；一旦sealed则显式fail/finish按表拒绝，Drop只退账。私有cfg(test)两档hook只能单调设MAX-1/MAX，Last→Penultimate明确Conflict且零状态变化，sealed不能复位；不提供任意整数/scope/ledger setter或生产可见开口。

## 独立验收与实施约束

DBR01–08覆盖满额0alloc/0owned引用遍历；生命周期负编译及stamp返回正控；完整身份/ABA；同步与跨线程Drop/失败R锁外析构；旧/新API真实delta、错误FIFO、同bytes替换/no-op；post线性化；隔离cfg副本MAX与全API sealed矩阵；原view19、预算26、P1十一同版回归及布局/锁/分配源码。死锁负例用可回收独立child+watchdog，不以留死锁线程的超时假清理。新增真实分配故障分支若未执行须逐项not-run，不用预算Capacity代签null。

Meta/cell/guard/stamp与列表节点/临时峰值仍需实际布局证明在原WINDOW_BYTES1024和已有头费用内；std pop节点释放与Arc/Charge析构必须读实际临界区，不能仅凭返回Arc推断无析构。当前门槛不提前证明实现无锁序问题或所有分配安全。若与P2A并行，按既有plan隔离或串行冻结，不能在对方正式验收期间改其Domain依赖而复用跨改写日志。

DBR-TG01/02仅定义层关闭，无新增用户产品决定。此端口解决满条数下同步借阅的内层条件，仍不等于首次客户端交付、输出预算/授权、P2锚点/current完整义务或整个3.2完成；真实owner/WSS/Harness/L5L6仍待后续。

## stage_insert参数优先级实施中消歧

主执行在源码尚未冻结时询问r8§6A的InvalidKey边界；本轮只确认合同解释，没有开始最终代码审查。stage_insert先保留原same_read/当前h身份校验，再对当前匹配cell验证key长度。即使sealed，key超过4096仍InvalidKey，不再fail或修改状态；这个参数特例不延伸到之后checked成本/容量。合法key匹配cell后sealed先于这些运算/申请及Conflict等。因此foreign+坏key仍Stale；matching sealed+坏key是InvalidKey；matching sealed+合法key及usizeMAX payload是SequenceExhausted，均不发布或额外改状态。

insert/replace/item_lease原本位于slot之前的item_cost顺序保持，不把stage_insert键校验移到其原跨read检查之前。未sealed坏key仍保原失败状态副作用；首次真实state变化遇MAX时的封闭规则保持。这是原“参数错误保留、sealed优先于容量”的工程精化，不增用户产品分支、不扩大写范围，也不预签尚未冻结的实现。已通知独立tester将上述对照纳入DBR07。
