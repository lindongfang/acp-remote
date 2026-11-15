> Q-001（2026-10-31）：本文保留为既有实现子合同，非本轮新接线依据。与当前specs/design冲突的正文、输出、实体/快照容量预算、重放门槛、签名期限及派发再验原actor均已停用；旧测试仅说明原范围。非冲突协议事实可复用，但须按新阶段独立复核。

# Domain同步借用与读取版本小合同候选 r8

2026-10-31；仅独立预审输入，未编码。只实施with_window、绑定stamp及统一revision；不含ACP/current iterator、16384读取预留、输出队列或授权。本稿不改变r4/r5/原Domain费用与冻结资产。

## 1. 最小工程API

```rust
#[derive(Clone, Copy)]
pub struct WindowReadStamp<'h> { /* private: &'h ViewHandle, revision: u64 */ }
pub struct BorrowedWindow<'w, 'h> {
    /* private: &'w Window, frozen: WindowStatus, stamp: WindowReadStamp<'h> */
}
impl ViewWindows {
    pub fn with_window<'h, R>(
        &self, h: &'h ViewHandle,
        f: impl for<'w> FnOnce(BorrowedWindow<'w, 'h>) -> R,
    ) -> Result<R, ViewError>;
    pub fn validate_read_stamp(
        &self, h: &ViewHandle, stamp: &WindowReadStamp<'_>,
    ) -> Result<(), ViewError>;
}
impl<'w, 'h> BorrowedWindow<'w, 'h> {
    pub fn status(&self) -> WindowStatus; // frozen，非live mutex读取
    pub fn stamp(&self) -> WindowReadStamp<'h>;
    pub fn items(&self) -> impl ExactSizeIterator<Item = ViewItem<'_>>;
}
impl WindowReadStamp<'_> {
    pub fn revision(&self) -> u64;
}
```

新增ViewError::ReadChanged；Stale仍表示容器/目标/资源/读取不匹配，SequenceExhausted表示revision已封闭。Reading/Finished/Incomplete均可读取，Incomplete不是只读失败原因；其状态必须在frozen header如实展示。

BorrowedWindow不Clone、不公开Window/Arc/ReadStatusCell；items只借用已有bytes/key。Domain端口本身0新堆分配、0 item Arc clone、0新owned-body引用/租约，items账本不变。因此窗口4096对象或节点32768对象时，不因新增snapshot引用额度而失败。固定栈guard/header/stamp大小须实际编译核对；不是持久元数据池。callback额外分配或输出属于调用者独立预算，不能把其任意行为记作Domain“0分配”承诺。

HRTB只让body借用存在于callback内；R不能携BorrowedWindow、ViewItem、bytes/key借用、iterator或捕获它们的future逸出。stamp只借用外部h，允许作为固定元数据返回，不携body。没有“创建全snapshot再借它”的实现路径。

## 2. Stamp身份与ABA

stamp私有保存原始&ViewHandle，而非scope/item裸地址或仅(gen,rev)。验证须：

1. 先按现有h规则在当前ViewWindows找到window，校验私有scope Arc身份、NativeSessionKey（含source）、ResourceEpoch及read generation。
2. h与stamp内原handle按同一完整same_read比较，不得只比两个u64。原handle相同view的合法clone可通过；另一容器即使target/resource/gen/rev数字全相同也Stale。
3. 再短锁该read的Meta，未sealed且revision相等才成功；已变化ReadChanged，sealed SequenceExhausted。

stamp对h的借用令原scope Arc仍由活handle持有；不可能在旧stamp有效时先释放该scope分配再复用地址，避免裸地址ABA。无新增body/预算身份引用、无全局token表。真正owned/跨网络CurrentCursor不在本包：后续cursor必须持有或受生命周期约束地绑定原ViewHandle，再加stamp/anchor position/字段坐标；不能只序列化revision当跨容器/目标/资源/读取通用凭证。owned handle只属控制身份，不能声称它持有body、保证原窗口仍存在或授予读取权限，其元数据费用由后续包装承担。

## 3. 统一ReadMeta与可见变化

现有ReadStatusCell的Mutex<ViewState>替换为私有Mutex<ReadMeta>：

```rust
struct ReadMeta {
    state: ViewState,
    revision: u64, // 新read从0开始，只checked_add，不复用
    sealed: bool,
}
```

窗口及跨线程StagedItem/BoundedWriter共享同一cell；revision不能放在只有&mut ViewWindows才能访问的独立字段，否则Drop-fail不会推进它。ReadMeta/Mutex/原Charge继续由原WINDOW_BYTES=1024覆盖并活到最后stage/workspace引用Drop，实际size_of/峰值须证明；费用或布局需增加时先复审，不默改预算。

每个真实可见变化与其revision推进必须在同一短Meta临界区提交：

| 路径 | 必须推进的变化 |
| --- | --- |
| 原insert/staged publish insert | item/position实际发布；字节相同的新对象也算变化 |
| 原replace/staged publish replace | 当前item身份替换，即使key/bytes相同 |
| 原insert与stage_insert的FIFO reserve | 每次实际移除前缀及水位/计数更新，哪怕最后reserve返回Err |
| fail/Drop未发布stage/overrun/build失败等 | 首次使state变为Incomplete；所有路径共用cell内helper |
| finish | Reading→Finished；Incomplete仍拒绝，已Finished再finish无可见变化 |
| 无可见变化 | workspace/lease/snapshot成功申请与Drop、只收费退账、不改变已sticky Incomplete的重复fail：不推进 |

不得只给新API计版本，遗漏原insert/replace或错误后的FIFO。release/换代使旧h失效，允许清理即使旧read sealed；新read独立cell与不复用generation，旧stage Drop只能改旧cell，不能推进新read版本。

## 4. 锁与提交顺序

严禁持Meta Mutex调用callback、用户closure、等待任务/线程，或Drop StagedItem/Item/Charge。也不在持它时取得NodeBudget/TargetBudget账本锁；保留现有node→target预算锁序。可见内容变更由&mut ViewWindows序列化，但state可能由另线程stage Drop修改，二者都要用上述Meta。

内容变更采用“准备资源→短锁checked推进并转移容器所有权→解锁→释放旧对象”：

- 分配/预留候选、Arc及必要列表节点在锁外；插入可先准备单元素LinkedList，再在临界区零分配append转移，不能在锁内触发用户析构。
- replace用mem::replace取出旧Arc，锁内不Drop旧值；FIFO pop取出Arc后连同水位更新完成，解锁才Drop它。被移走列表节点的std释放路径与实际临界区须源码审查，不携Item/Charge析构进入预算锁。
- fail/finish只在短锁内checked推进与state赋值，不访问预算。StagedItem Drop先完成此状态步骤，退出锁后再按原RAII清理其字段。
- 不允许先解锁预增rev，再无条件发布：另线程可能在间隙把read sealed。必须在同一短临界区检查sealed/推进/实际可见变更，保持读取与封闭规则。

这不是新增万能锁或外部进程所有者；不允许拿Meta锁覆盖整个方法的分配、回调或清理。现有std全局OOM abort边界不变，不新增依赖/unsafe引用计数。

## 5. with_window线性化及副作用边界

&ViewWindows保证本次callback期间items/位置/水位不被安全Rust的&mut操作改变，**不保证ReadStatusCell.state不变**。算法固定为：

1. 校验h取得当前Window借用；短锁Meta，若sealed则SequenceExhausted，否则一次复制(state,revision)，解锁。
2. 以该副本的state加不可变window元数据构成Frozen WindowStatus和stamp。callback内status()永远返回这个冻结副本，不再次读live cell；items同样来自当前不可变window。
3. **无任何Meta/预算Mutex持有**地执行f一次。可在callback中Drop同cell的旧stage；它能够完成，不自锁。
4. callback返回R后再短锁Meta：sealed则SequenceExhausted；revision与起始不同则ReadChanged；一致才接受R。此比较是成功读取的线性化点；解锁后再返回Ok(R)。
5. 失败时先退出所有锁再Drop R，R的析构同样可能Drop stage；不能在锁内销毁它。不自动重调callback。即使state碰巧相同，也比较revision，防中间可见变化被抹平。

另线程可在步骤4解锁后立即fail；Ok表示读取在比较点一致，不承诺直到网络发送都最新。validate_read_stamp同样只在自己的短锁比较点有效，不能当长时间发送围栏或授权。

**callback不能执行需撤销的外部副作用**：不在其中发送网络/SQL提交/允许审批/发布成功页，不持此借用await。允许有界读取、写入调用者已计费的私有候选/缓冲，以及正常资源清理（包括drop stage）。若ReadChanged/SequenceExhausted，私有候选必须作废/清空，不得当成功交付；Domain无法回滚closure已经做过的任意副作用，也不声称类型系统能禁止所有I/O。真正输出在Ok后按后续owner/授权/版本协议提交，后续阶段负责。

## 6. checked耗尽与可审测试入口

所有可见变更先计算revision.checked_add(1)。成功才连同变更提交。需要变化但已MAX时：置sealed=true；若尚非Incomplete则state=Incomplete(SequenceExhausted)，若已有Incomplete保留首原因；不改变items/position/watermark，不回绕，不再产生成功with_window/stamp验证/发布。封闭动作不需要一个新的有效版本：所有旧stamp在sealed后均被拒绝，不能在MAX旧stamp下读到新状态成功。

Drop/fail原为无返回值的路径也必须执行封闭逻辑，无panic；正常返回错误的路径为SequenceExhausted，Drop只做安全状态封闭和原有退账。无可见变化的重复fail不消耗revision，不能借重复no-op把正常读取故意耗尽。

测试极值只能用本模块私有#[cfg(test)]窄入口设置当前read revision（例如仅MAX-1/MAX闭合枚举），不导出生产setter或任意scope/状态/ledger写权限。测试注入不是实际长期运行可达性证明；公开A/B行为与内部极值证据分列。

### 6A. sealed后的逐API结果

sealed关闭的是该read后续构建与一致读取；已有不可变正文仍可作为明确标注Incomplete的诊断数据读取。以下规则不新增权限或改变原费用：

| API | 当前read已sealed时的行为 |
| --- | --- |
| with_window / validate_read_stamp | SequenceExhausted；不调用callback，不接受旧stamp |
| window_status / snapshot / item_lease | 按原读取、查找及容量规则可用；状态如实Incomplete，snapshot/lease仍收费；不推进revision，不提供stamp一致性承诺 |
| stage_workspace / insert / replace / stage_insert / stage_replace / publish / finish / fail | 对已匹配当前read返回SequenceExhausted；不发布、不FIFO、不新建工作区、不改变首失败原因 |
| 已有BoundedWriter::write_all | 在复制前检查sealed，返回SequenceExhausted；不改变written或正文；remaining/written及workspace.scratch仍是本地候选访问，不发布 |
| abort_stage | 消费并清理候选；外来或过期read仍Stale，匹配但sealed返回SequenceExhausted |
| StagedItem Drop / 内部无返回值fail | 不panic、不再改sealed/state/revision；照常退账 |
| release / collect_released / begin | 按原失效和回收规则可用；新read独立revision=0，不解除旧cell的sealed |
| usage / target_usage / identity_count / 已有snapshot或lease读取、Drop | 原行为；不推进revision |

错误优先级：先保留各API原有参数与跨read身份校验；取得当前匹配cell后，sealed检查优先于Incomplete、Conflict、MissingItem、候选poison/short-write及容量申请。没有匹配当前read的操作不能凭sealed旁路Stale。已持有BoundedWriter只检查其原cell，不检查更新一代read。

stage_insert按原顺序先验证工作区/当前read身份，再验证key长度；超4096字节先返回InvalidKey，已sealed时不再修改失败状态。合法key才检查sealed，sealed优先后续checked成本溢出及容量/冲突；不把key校验提前到原跨read校验之前。insert/replace/item_lease原slot前item_cost顺序保持。未sealed坏key仍按原首次失败规则处理，包括首次变化遇MAX时封闭并返回SequenceExhausted。

未sealed时的no-op：已Finished再finish返回Ok；已Incomplete的重复fail返回Ok且保持首原因；两者即使revision=MAX也不封闭、不推进。finish遇Incomplete仍Incomplete。首次真实变化在MAX封闭并返回SequenceExhausted；后续显式fail/finish按上表返回SequenceExhausted，Drop仍正常清理。

测试入口固定为私有cfg(test)枚举`RevisionBoundary::{Penultimate, Last}`及私有`ViewWindows::set_revision_boundary_for_test(&mut self, handle: &ViewHandle, boundary: RevisionBoundary) -> Result<(), ViewError>`。仅对当前未sealed read把revision单调设为MAX-1或MAX；stale按原规则拒绝，sealed返回SequenceExhausted，试图从Last降低至Penultimate返回Conflict且不改变任何状态。不接收任意整数，不改变sealed/state/scope/ledger，不提供unseal或复位。独立极值测试通过隔离副本中同模块cfg(test)子模块访问；生产或普通依赖编译不得可见。

## 7. 独立可测交付门槛

1. 空/4096小项/节点32768项：with_window元数据/bytes正确，调用前后usage/items不增；无snapshot、item Arc clone或新堆分配。借用/iterator/future逃逸负编译与允许返回stamp正控制。
2. 两容器相同gen/rev/目标/资源、不同target/source/resource/read、release→begin及原容器Drop后另建：错stamp/h均Stale，原scope仍由h生命期固定，无裸地址ABA。
3. callback内同步Drop同cell未发布stage及另一线程精确在callback中Drop：不死锁，guard.status仍冻结，最终ReadChanged。已Incomplete的重复fail无可见变化可成功。旧generation stage Drop不影响新窗口stamp。
4. 原/staged insert、replace同bytes、reserve已FIFO后Err、overrun、Drop-fail、finish/fail逐路径revision推进；无变化路径不推进，水位事实仍真实，原first failure保持。
5. 在postcheck之后才发生state改变：第一次Ok有明确线性化点，后续validate_read_stamp失败；不得冒称第一次Ok一直保证最新。
6. MAX-1最后一次合法变化→MAX，再变化sealed；各Drop/公开方法/读取拒绝及正常release回收，无wrap/panic/预算泄漏；极值仅私有cfgtest。
7. 原view19、预算26、P1独立断言同版回归，不改预期/费用。实际ReadMeta/cell/guard/stamp布局、锁序/析构和分配源码证据，动态与静态范围分列。

写范围候选仅domain/views.rs、views/staging.rs及其开发测试和必要API文档；没有ACP、SQL、network、current/scanner/输出队列或新授权。所有新增断言当前not-run，待独立预审才可实施。

本轮只读基线：views.rs a22bb6e5e9a0e7fdca3b66e9e3dc18025a37ab57832ea95b44e509d189255f26；views/staging.rs d2a2a6ec3756777a5758d05ffd51e7a78f9f62fb5158283545854c77b00b0f09；domain/native.rs 8d01cef25c7ba12ffd2aff94cc78fc8bfbe41d604feaf2312f13fe47cadf9abd。未改冻结代码、旧合同或独立资产，未网络/浏览器/audit。
