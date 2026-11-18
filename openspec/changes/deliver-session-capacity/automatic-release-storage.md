# RB-001 自动关闭必要管理存储合同

日期：2026-11-16。状态：单 native 内部包已获独立规划门并实现合入，开发验证与交付检视范围见 [内部复核](../../../testing/runs/deliver-session-capacity/reviewer/2026-11-16-rb-internal-v1-review.md)。这不代表完整树运行时、生产启用或整个 change 已验收。

本包按主执行者分配只负责容量记录接口、SQLite 必要系统操作、schema13 及开发回归。运行时资格、已知活动、实际关闭、协议、Web、共享模块接线由主执行者负责。不修改上游、适配器安装包、本机配置或用户库，不借用户 actor，不伪造平台 target，无新依赖。

## 1. 权威与只读依据

行为权威为 `openspec/changes/deliver-session-capacity/specs/session-resource-policy/spec.md`；细化依据为该 change 的 `design.md`、`implementation-contract.md` **顶部 RB-001 当前合同**。新 RB-001 取代历史“完整隐藏活动/原子关闭窗口”门，不改变主树归属、已知工作保护、50 名额、30 分钟、失败/未知保额及管理提交后返额。

原 reviewer 初审 CAP-R020 明确：同 `(source,native,instance,runtime)` 的责任集合只代表当前一个原生目标，不能自动等同于含不同 child native 的完整主树。本候选将父子持久映射、成员快照及防迟到登记列为外部依赖，不能用两个系统操作表冒充该模型已完成。

只读核对的当前源码（主工作区原始字节 SHA-256）：

| 文件 | SHA-256 |
|---|---|
| `app/src/application/native_dispatch/capacity_records.rs` | `7d686182db30a6b46f07137cead7c1eb95f54c25ca3a97341d0061b20275e8aa` |
| `app/src/session_storage/session_capacity.rs` | `d5d7ec87cdc714da9fd23d9a16b02cf63772e8472817305d54941b8ca7e623b5` |
| `app/src/session_storage/session_capacity_schema.rs` | `d6943261afcc9878a41d4b03b5cc01f5f9ef1f5fffd646c02628554450816779` |
| 本 change `specs/session-resource-policy/spec.md` | `ba4cd175da79c120d583ab1368c376b33cbc92d97211de928505c71a8a32ddf0` |
| 本 change `design.md` | `3c7a2c8ebd775bd54eedc3514272388a363ba01039641f4ae19fb918618c2895` |
| 本 change `implementation-contract.md` | `45e4efd52dde73c647ad69c394e7dc822040723a348aaad1b757e4ef470f1c82` |

这些是规划读取快照，后续集成文档和代码变化须重新核对，不能以现有 HEAD 代替未提交版本。

## 2. 三个精确接口

当前内部模块的capture算法限定为同 `(source,native,instance,runtime)` 的未返还原容量责任，至少包含anchor；同native异owner拒绝。当前三API不接受跨child native集合，不能据本包启用生产自动回收或签完整树通过。第6节是后续runtime/树集成的独立门，不是要求本包从尚不存在的字段推导成员。若后续出现独立child加载责任，须先扩展权威树映射与capture合同，再集成启用。

接口放在 `application/native_dispatch/capacity_records.rs`，沿用 `BoxFuture`、`RegistryDeadline`、`DispatchError` 及 `CapacityRecord::same_reservation`。类型 public，字段 `pub(crate)`；不修改既有显式释放 API。`CapacityRecord` 的 `physically_closed` 是可变事实，不属于身份比对条件。

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AutomaticReleaseReason {
    IdleTimeout,
    CapacityPressure,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AutomaticReleaseOutcome {
    Requested,
    Failed,
    Unknown,
    Withdrawn,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AutomaticReleasePhase {
    Pending,
    Finished,
    Withdrawn,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AutomaticReleaseRecord {
    pub(crate) operation: EntityId,
    pub(crate) original: CapacityRecord,
    pub(crate) reason: AutomaticReleaseReason,
    pub(crate) phase: AutomaticReleasePhase,
    pub(crate) outcome: Option<AutomaticReleaseOutcome>,
}

fn begin_automatic_release<'a>(
    &'a self,
    record: &'a CapacityRecord,
    operation: EntityId,
    reason: AutomaticReleaseReason,
    deadline: &'a RegistryDeadline,
) -> BoxFuture<'a, Result<AutomaticReleaseRecord, DispatchError>>;

fn record_automatic_release_outcome<'a>(
    &'a self,
    record: &'a CapacityRecord,
    operation: EntityId,
    outcome: AutomaticReleaseOutcome,
    deadline: &'a RegistryDeadline,
) -> BoxFuture<'a, Result<AutomaticReleaseRecord, DispatchError>>;

fn finish_automatic_release<'a>(
    &'a self,
    record: &'a CapacityRecord,
    operation: EntityId,
    deadline: &'a RegistryDeadline,
) -> BoxFuture<'a, Result<AutomaticReleaseRecord, DispatchError>>;
```

`Withdrawn` 作为系统意图的终态需要与 `Finished` 区分：前者撤回自动关闭但资源继续占用，后者必要协调成功且原始集合返额。二者均不新增用户会话生命周期状态。

`AutomaticReleaseRecord.original` 从原 anchor 历史行解码，允许包含更新后的 physically_closed。调用者不能以返回物理事实更新为由更换 operation 或原责任；身份必须始终是原 id/source/native/instance/runtime。

### 2.1 begin

- 由运行时已持有的主树/容量关闭所有者调用，须在任何原生副作用前可靠返回。
- 初次调用核对 SQL 原行未 released、native 非 NULL、caller 不更换原身份、有已验证主树依据、原 source/runtime 有效，且原 Create 必要状态已经收敛。SQL 中已 physically_closed 的新意图不作为新的关闭候选。
- 冻结原始责任集合，至少包含 anchor；当前已绑定树成员和在途资源必须通过第 6 节的权威成员来源覆盖，不能从相同 native 推断不同 child native 归属。
- 新系统记录为 `phase='pending', outcome=NULL`。这只表示必要意图已登记，**不表示已入队、已发送、未发送或可安全重发**。
- 同 operation、原身份及 reason 重试读取原集合并返回当前阶段，不重新捕获成员。operation 存在但原身份/reason 不同返回 Conflict。
- 返回 Finished 时禁止再次发送关闭；返回 Withdrawn 时保留撤回结果，未来策略若再次选择该资源须登记新的 operation。返回 Pending/Unknown 时只能协调原请求，不盲目生成新关闭请求。

### 2.2 outcome

- `Requested`：仅原 ticket 已明确 `TransportFact::Written` 后记录。仅 enqueue 成功、预留成功或开始写入都不满足；不把 Requested 解释为响应成功或实际关闭。
- `Unknown`：原写入部分/结果未确认、响应未确认或必要请求记录失去确定性。原始责任继续保留；后续只能靠原请求响应、原资源关闭事实或完整原 source 收敛协调，不盲重发。
- `Failed`：已确认请求失败，保留名额及原所有者。不向“没有其他候选”之外连续关闭下一棵树来掩盖这次失败。
- `Withdrawn`：仅运行时对**原 operation 的原 ticket**证明 Zero，或证明从未入队，且资源未关，才允许调用。枚举本身不是证明；调用点必须紧邻并核对原 ticket/owner，不能用超时、异常或 outcome 为空推断 Zero。
- SQL 必须核对原 operation、anchor 身份和阶段；Withdrawn 另要求原集合仍未 released、未 physically_closed、原 source 未 closed。SQL 不声称能够从一个枚举推导真实 transport；运行时严格证据边界由主执行者实现并由独立 tester/reviewer检验。
- 任何 outcome 方法均不设置 physically_closed，也不返额。Withdrawn 只结束此自动意图；平台恢复输入仍由运行时按原票据、同 owner 和当前资格决定。

### 2.3 finish

- 只读取该 operation 在 begin 冻结的原集合；不以当前 source/native 查询代替原集合，不引入恢复后的新 reservation。
- 核对 caller 原身份。缺 operation、空/损坏集合、换 anchor/source/native/instance/runtime 均拒绝。
- 原集合全部具有必要 physically_closed 事实；原树对应资源及在途责任均可靠 released/收敛；原 Create 的必要 Finish 不得仍 owned/未知。具体跨 child 的资源覆盖须满足第 6 节。
- 同一已有 owner 的必要 SQL 事务内完成：更新 sysop Finished + 将尚未 released 的原容量行 released。COMMIT/连接关闭不确认按既有语义返回错误，不把临时内存结果签成功。
- 必要存储失败原子回滚 sysop 完成和返额；先前单独提交的 physically_closed 事实继续存在，所以对外呈现“已观察关闭、管理未协调”，occupied 保留。
- 原集合已经由原 source 收敛或成功显式释放时，核对原集合的历史闭合事实后可仅提交此 sysop Finished，不能再次返额或触及新 owner。
- Finished 重试只返回原完成记录，不复核或扫描新代际。Withdrawn 的 finish 返回 Conflict，不把后来别的操作关闭的资源写成此撤回意图完成。

## 3. 精确 schema13 DDL

保留 schema12 定义和显式 `node_session_capacity_releases` 不变，只附加以下系统操作表。DDL 不含 actor、request/用户命令、伪 target、正文、native RPC 内容或错误正文。原因和结果使用固定枚举。anchor 与成员均指真实持久容量责任。

```sql
CREATE TABLE node_session_automatic_releases (
  operation_id TEXT PRIMARY KEY CHECK(length(operation_id)=22),
  anchor_reservation TEXT NOT NULL
    REFERENCES node_session_capacity(reservation_id),
  reason TEXT NOT NULL
    CHECK(reason IN ('idle_timeout','capacity_pressure')),
  phase TEXT NOT NULL
    CHECK(phase IN ('pending','finished','withdrawn')),
  outcome TEXT
    CHECK(outcome IN ('requested','failed','unknown','withdrawn')),
  CHECK((phase='withdrawn')=(outcome IS 'withdrawn'))
) STRICT;

CREATE TABLE node_session_automatic_release_members (
  operation_id TEXT NOT NULL
    REFERENCES node_session_automatic_releases(operation_id),
  reservation_id TEXT NOT NULL
    REFERENCES node_session_capacity(reservation_id),
  PRIMARY KEY(operation_id,reservation_id)
) STRICT;

CREATE INDEX node_session_automatic_release_member_lookup
  ON node_session_automatic_release_members(reservation_id,operation_id);
```

anchor 已保存不可变原身份，sysop 不再冗余保存可失配的 source/native/instance/runtime。元数据校验须检查 anchor 实际为 native 非 NULL 的已验证主树责任、成员非空且包含 anchor、成员符合原 owner/树关系；不能只靠外键证明业务关系。多次自动或显式操作可以引用同一原责任，谁的合法管理完成先提交就先协调原额，其他操作只能幂等收敛该原集合。

不依赖部分唯一索引把 pending 等同全局用户禁用；运行时持有每树的关闭锁和原 operation。必要成员捕获、防迟到加载及同树准入冻结按第 6 节共同实现。

### 3.1 迁移与校验

- `SCHEMA_VERSION` 升为 13；现有 schema0..12 DDL/值不重写。schema12 升级只创建空 sysop 表和索引；不从显式 release 历史或现有占用反推“旧自动操作”。
- 在同一现有必要迁移事务中：DDL→必要元数据 head→PRAGMA head→全量 schema 校验→COMMIT。增加对应故障边界及 DeferredCommit/CommitUnknown 回归，遵循现有 owner/SQL close 责任。
- 初始 namespace preflight 即识别三对象；version<13 出现任何同名半成品即拒绝。current13 检查 exact DDL、STRICT、两个表及索引、main/temp shadow、main/temp trigger、未知额外索引、外键、空 identity 和业务一致性。不会“修复”用户库。
- 没有 node identity 的库要求所有新表为空；已有必要 sysop/member 历史即拒绝无身份初始化。
- 旧迁移 fixture 改动仅删除该测试自己生成的新对象后降版，不删除真实用户对象。新增正向迁移另用固定历史 schema12 DDL 从空库创建，不依赖 current13 反向降版替代证明。

## 4. 合法状态转换

下表中的 None 指 `outcome=NULL`，phase 默认为 Pending。重复同结果幂等，但仍要核对原 operation/身份；矩阵未列出的转换返回 Conflict。

| 原状态 | 允许操作/新状态 | 名额 |
|---|---|---|
| 不存在 | begin → Pending/None，并冻结原集合 | 保留 |
| Pending/None | 记 Requested / Failed / Unknown | 保留 |
| Pending/None | 证实 Zero/未入队且资源未关 → Withdrawn/Withdrawn | 保留 |
| Pending/Requested | 记 Failed 或 Unknown；同 Requested 重复幂等 | 保留 |
| Pending/Failed | 同 Failed 重复；finish 仍须真实原集合关闭证明 | 保留，直到合法 finish |
| Pending/Unknown | 同 Unknown 重复；只协调原请求/原资源，禁止降为 Requested 或 Withdrawn | 保留，直到合法 finish |
| 任意 Pending（None/Requested/Failed/Unknown） | 原集合闭合且必要状态收敛 → Finished/保留原 outcome | 同事务仅返原集合一次 |
| Finished/原 outcome | 同身份 begin 或 finish 重试；相同已存 outcome 重复读取 | 不再返额、不触及新代际 |
| Withdrawn/Withdrawn | 同身份 begin、同 Withdrawn 重复 | 不返额，不重新发请求 |

Failed/Unknown 不因重连、超时、空 outcome、新操作出现而清空。新事实表明原资源后来确已关闭时，可以 finish 收敛必要管理事实；保存的失败/未知观察不被改写成 Requested，亦不声称此前关闭请求成功。

begin 提交结果未知时，重试同 operation 查询；outcome 为空仍不证明请求从未发送。outcome/finish 提交结果未知时同样沿原 operation 协调，既有 `CommitUnconfirmed` 等语义保持。

## 5. 开发测试计划与真实边界

本包在独立 target、jobs=2 运行真实 Rust/SQLite 开发测试。运行时/活动/transport 证据若来自 fixture 必须明确标注，不签真实 Harness、实际 30 分钟、真实 50 主树或 Windows 完整进程树通过。独立用例由 tester 维护，本包不写共享 tests.md。

| ID（开发包内部） | 必须证明的断言 |
|---|---|
| ARS-001 系统主体 | 无 actor/target 的 Create-only、SourceOnly 和执行资源都能登记；不创建假用户、假 target、假显式 release；表中不存在敏感正文。 |
| ARS-002 原身份和原因 | 同 operation 原身份/reason 重试幂等；换 operation 参数、source、native、anchor、instance、runtime 或 reason 冲突。physically_closed 可变位不造成原操作身份冲突。 |
| ARS-003 请求与物理分开 | begin 只 Pending；Requested 不改 physical/occupied；Failed/Unknown 不返额；confirm_capacity_closed 只 closed_pending；只有 finish 成功才 Finished+返原额。 |
| ARS-004 状态矩阵 | 逐一测试上述合法和非法转换；Requested→Withdrawn、Unknown→Requested/Withdrawn、Withdrawn→finish 均拒绝；空 outcome 不表示未发送。运行时另测原 ticket Written/Zero/未入队证据绑定。 |
| ARS-005 原事务故障 | sysop 完成 UPDATE、返额 UPDATE、COMMIT、报告丢失、owner SQL close 故障均不提前返额；实际物理事实与管理错误分开；恢复后原 operation 返额一次。 |
| ARS-006 跨代际行为 Red/Green | A 自动完成→显式恢复 B→B physical close 但 B finish 故障→A begin/outcome/finish 重试均不能返 B；旧错误须由实际断言复现，不把缺方法/编译失败算行为 Red。 |
| ARS-007 原管理未收敛 | Create Finish owned/unknown、资源 binding 未 released、读取/审批关闭未收敛或捕获成员缺失时，finish 保额；收敛后同原 operation 成功。读取/审批事实由原 runtime 证明，不由 SQL 猜测。 |
| ARS-008 与显式/源关闭竞争 | 显式 release 与 auto 分别绑定原集合；一方合法先返后另一方只收敛原记录；原 source 关闭和原 auto owner 查询之间竞态不返新 B。 |
| ARS-009 重启与 unknown | pending/Requested/Unknown、closed_pending 和 Finished 均跨 SQLite reopen 保持；重启不沿用旧单调时钟或盲重发，原 operation 不采纳新 owner。 |
| ARS-010 schema13 | 从真正历史 schema12 DDL 建库后升级，全部原定义/数据/容量及显式关联不变；新系统表空；新对象必要边界回滚/重试、commit unknown、namespace、triggers、空 identity、FK/业务损坏均覆盖。 |
| ARS-011 主树依赖 | 父子权威映射冻结后补：多个 child native/资源和在途责任冻结、晚到登记被 fence 拒绝、最后成员未收敛不得返根额、根关闭不影响共享其他树。当前同 native 测试不冒称此项已验证。 |

适用入口：仓库 README/package.json 的 Rust 格式、Clippy、实际 cargo tests 与完整 root check/build；存储包独立适用检查由本包执行，最终主工作区完整入口由主执行者执行。迁移和已有显式释放、schema12 namespace/旧迁移 fixtures 必须定点回归。

## 6. 父子集合与发送前撤回的外部依赖

以下必须在完整规划门中明确归属与可执行接口，不能留成“系统表自然实现”的默认假设：

1. **主树权威映射**：由当前标准协议实际关系和原所有者建立 root→成员 native/原 resource/load/reservation 的必要映射，包含不同 child native 及在途责任。缺可靠归属仍拒绝子加载。没有暴露的隐藏活动不另设完整覆盖门，但平台已持有的子资源不能漏账。
2. **固定集合取得点**：运行时持有同一树的准入/关闭 gate，在 begin 的必要事务捕获全部当时成员；所有创建/执行加载/SourceOnly Replay/迟到完成都受该 gate 与 SQL 责任校验控制。必须约定哪些持久字段能查询原树成员及其代际，避免 begin 后又追加一个不在原集合的已准入子资源。
3. **SQL 收敛范围**：finish 查询这些已捕获的原资源责任而非只查询 root target/native；Create-only 和无 target 的读取责任也须覆盖。原集合出现新 owner、成员遗漏或管理未确认时保持 occupied。
4. **原 ticket 证据**：主执行者实现 Requested/Withdrawn 的唯一合法调用点，核对 operation→原关闭 ticket、frame/native/原 authority 的绑定。Unknown 不得被新 ticket 的 Zero“洗成”撤回；SQL 记录不能替代原 transport 证据。
5. **已知活动复核与输入恢复**：运行时最后发送前复核当前已知工作和审批，冻结平台新输入；只有原 ticket Zero/从未入队的可撤回失败才恢复原 authority 准入。已 Written/Unknown 的所有权继续由原关闭路径处理。

schema13 本包聚焦必要自动操作，不在本候选擅自定义或声称父子持久模型已实现。若主执行者采用额外 root/member 表或资源关联表，须先更新本节的具体读写合同、写范围和验证，再由 reviewer 定稿；系统操作的冻结和 finish 必须消费那份权威映射。当前单 native 子集仅可验证系统操作机制，不替代完整主树容量交付。

## 7. 编码与合入边界

当前仅新增本隔离 `contract.md`。后续须收到主执行者引用新规划门通过的明确编码通知，才同步**当前已修复的主工作区基线**到新的隔离副本，不能在早期 phase1 副本上覆盖后续代际/SourceClosed 修复。

拟由存储包拥有：`capacity_records.rs` 新类型/三方法；新 `session_storage/automatic_capacity_release*.rs`、schema13 定义/开发测试与必要既有仓储/迁移测试修订。共享 `application/native_dispatch/mod.rs` reexport、runtime/协议/Web 和主树映射模型由主执行者串行集成；如需要重叠，先列清写范围。所有交付为增量 patch、精确 SHA-256 清单和真实命令/失败/未运行记录；先 `git apply --check` 再应用，不提交或推送。
