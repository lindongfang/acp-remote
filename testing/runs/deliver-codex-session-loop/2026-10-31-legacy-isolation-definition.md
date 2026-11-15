# 旧正文/强刷路径构建隔离独立子用例

2026-10-31。依据 legacy-isolation-contract SHA256 `77521442e75e30ba64725efee8320abc07fc00ee6cef6a3ad934f5bf77d4fb88`（由c7a34b87、ff8b569f版依次增加child --exact及三个机械文件白名单）、implementation-contract §1/4和现有schema4/新管理接口。本文件只定义验收，**全部新增构建隔离断言not-run**，没有修改产品/原用例/DDL/用户数据。本阶段只隔离生产构建可达性，原数据保留；不签完整change、WSS、审批owner或真实Harness。第一次搬移编译暴露错误物理test路径和生产旧domain私有字段dead_code，属于机械适配尚未闭环，不计产品功能验收；不得用移动前366绿签移动后结果。

稳定用例ID/标题不改：CL-001为真实构建/测试，CL-002为边界与差异核对，CL-003为新旧层依赖，CL-004为迁移/管理事实保持，CL-025/026/075/076为新输入正文隔离及管理失败不重派的既有子断言。本页不把旧强刷业务重新定义为当前产品规则；保留的旧测试只是历史回归。

## 1. 固定输入与测试所有权

已只读保存 `legacy-isolation-before/source.json` 和对应逐字源文件；`old-tests.json`列所有旧测试函数名。正式执行前按父执行者冻结源码清单重新核对，记录从该准备基线到最终代码的允许差异，不能用旧报告冒签移动后的组合。

独立测试在原tester外部根维护：普通依赖小crate、每个source片段/诊断JSON/命令、锁依赖版本比较及结果报告。独占CARGO_TARGET_DIR；不运行浏览器/Web audit、没有网络外发；不访问用户实际库/配置/凭据。SQLite回归仅运行既有测试自有临时副本。保留所有旧失败日志/版本归属，不回填其结论。

## 2. 普通外部依赖正向控制

建立独立crate，`edition=2024`、独立`[workspace]`，唯一直接依赖 `acp-remote = { path = "D:/Project/acp-remote/app" }`；不用include源码、仓库test module、dev-dependency、补丁或feature把产品依赖编译成cfg(test)。使用`cargo check --lib --offline --message-format=json`；外部锁用产品Cargo.lock作为解析起点，仅允许加入此小crate自身package，比较第三方name/version/source与产品锁一致，锁稳定后用`--locked`。依赖解析、工具/环境失败单列受阻，不能计负向通过。

有效控制源至少如下，编译而不运行，不打开库或派发：

```rust
#[cfg(test)]
compile_error!("this probe must be an ordinary non-test dependency build");
pub use acp_remote::application::product_commands::{
    InputOwner, MemoryInputs, ProductInputService, ProductRepository,
};
pub use acp_remote::domain::{ActorContext, EntityId};
pub use acp_remote::domain::{activity::SessionActivity, native::NativeSessionKey,
    requests::ProductRequestIdentity, views::ViewWindows};
pub use acp_remote::session_storage::product_commands::ProductStore;
use acp_remote::storage::Storage;
pub fn product_factory(storage: &Storage) -> ProductStore { storage.product_commands() }
pub fn identity() -> EntityId { EntityId::from_bytes([7; 16]) }
```

预期：普通依赖真实编译成功，Storage/新ProductStore实际工厂与新ProductInputService等类型仍存在，不能通过把整个库cfg(test)掉来实现隔离。检查调用命令、Cargo配置/环境没有强制`--cfg test`；正向控制本身拒绝test cfg。只记录必要的配置判定，不转储敏感环境。每个负向源都以此已成功控制为基础，避免依赖丢失/全库不可用造成假阳性。正向存在性不授予网络权限/permit，也不证明新API运行行为。

## 3. 逐入口负向编译矩阵

每次仅加入以下一个目标，单独记录source SHA、cargo退出码、compiler-message.error.code、primary span及message，之后恢复正向控制并确认依赖仍可用。可以同crate串行替换lib.rs复用锁/target，不并行写同一个源。不得把本应失败的目标和正向控制一起当作一组“cargo报错”笼统通过。

| 编号 | 待加入控制源的单个目标 | 预期边界错误 |
| --- | --- | --- |
| N01 | `pub fn legacy(s: &Storage) { let _ = s.sessions(); }` | E0599，Storage无sessions方法 |
| N02 | 同上仅改为`s.histories()` | E0599，Storage无histories方法 |
| N03 | 同上仅改为`s.approvals()` | E0599，Storage无approvals方法 |
| N04 | `pub use acp_remote::application::InputRepository;` | E0432/对应不存在或私有导出诊断，必须指向InputRepository |
| N05 | `pub use acp_remote::application::InputService;` | 同上，必须指向InputService |
| N06 | `pub use acp_remote::application::history::HistoryCoordinator;` | E0432/E0433/E0603，旧history模块不可达 |
| N07 | `pub use acp_remote::application::approvals::ApprovalService;` | 同上，旧approvals模块不可达 |
| N08 | `pub use acp_remote::session_storage::SessionStore;` | E0432/E0603，旧兼容re-export不可达 |
| N09 | `pub use acp_remote::session_storage::history::HistoryStore;` | E0432/E0433/E0603，旧SQL history入口不可达 |
| N10 | `pub use acp_remote::session_storage::approvals::ApprovalStore;` | 同上，旧SQL approvals入口不可达 |
| N11 | `pub use acp_remote::session_storage::legacy::SessionStore;` | E0432/E0433/E0603，新legacy路径也不能绕过隔离 |
| N12 | `pub use acp_remote::domain::history::HistoryItem;` | E0432/E0433/E0603，旧强刷领域模块正常依赖不可达 |
| N13 | `pub use acp_remote::domain::approvals::ApprovalBinding;` | 同上，旧必Run审批关联领域模块正常依赖不可达 |

判据：13个源均能被Rust正常解析，错误精确来自指定缺失/私有API，且无无关错误。语法错误、找不到acp_remote/外部依赖、泛型/参数不满足、链接器/工具失败、lint当error都不计隔离通过。若Rust版本诊断代码有差异，必须解释同一目标span和语义，不接受任意非零退出码。原公开路径N01–N10/N12/N13在准备时源码确实存在；N11专门检查新移动路径不泄漏，不冒称移动前已有该模块。并独立检视所有pub use/工厂/feature别名，避免只堵原路径却换一个生产公开入口。

## 4. 普通构建与旧回归完整性

按当前README/实际Cargo配置，在固定组合上运行普通`cargo build --manifest-path app/Cargo.toml --locked --offline`、仓库Rust fmt/clippy all-targets和`cargo test --manifest-path app/Cargo.toml --all-targets --locked --offline`。可复用同版同范围有效CI证据，但移动前日志不可代替。正常依赖负向和cfg(test)保留各有独立证据，不能只运行cargo test证明生产边界。

旧测试静态清单已保存，当前56项、0ignore：

| 文件/原真实测试命名空间 | 数量 | 允许映射 |
| --- | --- | --- |
| application/history/tests.rs，application::history::tests | 22 | 不变 |
| session_storage/tests.rs，session_storage::tests | 12 | session_storage::legacy::tests |
| session_storage/history/tests.rs，session_storage::history::tests | 12 | session_storage::legacy::history::tests |
| session_storage/approvals/tests.rs，session_storage::approvals::tests | 10 | session_storage::legacy::approvals::tests |

用实际测试二进制`--list`逐名称映射对比56项，检查源码没有删除/新增ignore/削弱断言；再确认实际执行数量与通过/失败/忽略，而非只列出测试。不要求整个库总数固定，因为新测试可增加；旧集合不能减少，不能用新增测试凑数。历史审批应用没有独立test模块，相关断言由旧SQL审批10项保留，不虚构一组旧应用测试数量。

特别执行真实父测试`...legacy::history::tests::abrupt_child_exit_loses_only_uncommitted_tail_and_never_reuses_reserved_positions --exact --nocapture`。启动测试子进程前，确认父进程未设置`ACP_WP2_HISTORY_CRASH_ROOT`，避免父测试早退；该变量只由父用例向自己拥有的child传入。保留源码对child exit93和`VISIBLE_POSITION=1`的原assert，并实际通过这两断言。全限定路径须变成`session_storage::legacy::history::tests::crash_after_reservation_child`；re-export不改变真实测试名称。零匹配会exit0，不能当成功；父断言恰能拒绝这种假绿。父用例同时检查reserved next_position65、只保留已提交用户输入、新位置不重用。它创建/回收自己的child并有10秒超时杀/收尾，不能留下测试进程。

## 5. 原文搬迁与源码白名单

保存的旧session_storage/mod.rs是生产旧业务来源；最终legacy.rs中的旧实现体（use、常量、trait impl、SQL与辅助函数）应逐字保留。只允许旧模块声明增加明确path、旧tests声明适配，以及新mod里cfg(test)包裹/兼容re-export。实际比对展示删去/增加的声明hunk与剩余逐字块，不能用大范围空白/注释归一化掩盖SQL或逻辑变动。

application/mod.rs只增加旧history/approvals与InputRepository/InputService相关use/声明的cfg(test)，新product_commands声明保持；旧应用文件业务原文保持。Storage只对sessions/histories/approvals增加cfg(test)，product_commands工厂、共享fault latch、身份/信任/目录锁/迁移逻辑保持。session_storage/history/tests.rs仅允许child --exact路径字符串机械替换，exit93/marker/数据库断言原文不变。测试路径真实变化可映射，不改业务名称/预期。

追加的三个文件仅允许以下机械变化：session_storage/history.rs原`#[cfg(test)] mod tests;`增加`#[path="history/tests.rs"]`；approvals.rs同理使用`approvals/tests.rs`，保留实际12+10旧SQL断言文件/内容，不能通过误加载父tests.rs得到重复/替代数量。domain/mod.rs仅给旧`pub mod history`、`pub mod approvals`各加cfg(test)；根EntityId/ActorContext/纯基本值和新native/activity/requests/product_commands/views声明/实现不变。禁止用allow(dead_code)保留生产旧模型或改业务方法消除告警。该domain/mod before逐字快照另保存，已核对SHA `6d13968c8f95fbcfa5f629a37435782b1743e3f1d1226d48e4663142bb7469d5`，不覆盖先前基线文件。

固定对比schema.rs、product_schema.rs（CORE/SESSION/schema4 DDL）、application/session_storage product_commands、请求签名适配器、domain/native/activity/requests/views/product_commands、Cargo.toml/Cargo.lock和已运行SQL资产。若除白名单路径机械变化外有业务/依赖差异，报告证据失效/额外重验，不能自动纳入本阶段。保留的根纯数据值与通用错误不能仅因可import而误判存在持久化工厂；旧domain/history与approvals本身必须普通依赖不可达，同时检查其未充作新DTO/授权对象或反向连接旧SQL。

## 6. 新数据库/正文隔离组合回归

在最终隔离组合实际重跑schema迁移24项和当前管理输入31项（管理tester维护的固定版本/证据，不改其copy或预期）。分别记录执行数/版本：24证明新旧布局/身份撤销/正文等事实无损，31证明新管理受理/claim/派发/故障逻辑未被替换成旧InputService；二者不互相代签。

至少明确保留管理验收`m_new_path_never_changes_legacy_body_or_security_metadata`实际分支：自有库既有合成正文/旧management_commands/security_audit不变，新一次输入通过真实ProductInputService→ProductStore路径，Turn/Item/旧command/history-pin没有新增；对端计数与未知不重派事实保持，关闭库后DB中无新BODY marker。schema24与管理31使用的源码/锁和当前组合一致；不同冻结版不能直接复用。管理安全清理/容量/水位CL-080–084完整义务仍按spec后续验收，当前事务子阶段未清理不等于整个change排除。

## 7. 缺口及放行条件

没有新增用户产品决策。child --exact路径及显式SQL tests物理path、旧domain门控已由77521442合同补齐；未发现其余需改变产品边界的设计阻断。此次是已确认隔离边界下的必要构建适配；旧56项/child93与marker/body/DDL判据没有削弱。

仍需工程执行前落实：父冻结最终移动/门控代码与完整清单；reviewer核对上述56项真实命名空间、逐字迁移白名单及正/负探针判据；外部crate锁/普通cfg控制完成后再判API；确认Windows owner前置使实际全测试可运行。缺工具/环境或未执行记受阻/not-run，不能替换为LX或静态成功。若实际迁移必须改白名单外业务/测试预期，暂停该hunk评估并单独定义，不猜测授权。

交付报告应列13入口逐项诊断、正向控制、56旧名称映射/ignore与实际运行、精确child93/marker断言通过依据、源码/锁前后、迁移24/管理31组合、失败/未执行、临时crate/进程/副本清理。只有上述实际证据齐备才签本阶段构建隔离，不签任何未接真实产品能力。
