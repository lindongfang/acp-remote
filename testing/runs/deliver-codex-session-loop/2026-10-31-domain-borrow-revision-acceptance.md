# Domain借用与revision：独立验收

结论：固定101源版本上，**9项公开行为+3项独立allocator+8项隔离私有测试，共20项全部通过**；15个负编译案例确切拒绝且stamp返回正控制通过；原106项适用回归全部通过，外部all-targets clippy/fmt exit0。未发现本阶段新增产品阻断。仅签下面逐分支覆盖，不代签P2A/current/scanner、输出授权/WSS或完整L5/L6。

## 来源、资产和隔离

行为依据：原DBR01–08定义SHA `28319d55ca866d8b4e83b030ea5f649d847268224577625d3e5a6cf17e1cea87`，加父执行者/reviewer明确的stage_insert参数优先级消歧；最终r8合同SHA `5ebc06dca886ad7e76834b8a564414cd034ced08b2dc7c4e483d0e2917aef172`。未改原冻结定义或原19/预算26/P1资产预期。

源码输入为root `testing/runs/deliver-codex-session-loop/2026-10-31-domain-borrow-revision-source.json`，SHA `0f09eafdbea65522a628fb177eb07ab3e955baab92dd998c5710374e8e439e91`。tester自行逐项核对101份，前后101/101一致：`dbr-source-before.json`、`dbr-source-after.json`。关键产品源为views.rs `01e8ad736724f71a26fab128f819f41f1f65b359b06face964aa5d117c4eb933`，staging.rs `ad3bf8644505a1f9656e58bc94580e146ca52702351570eda9b56c4e88cc8443`。没有改root产品、测试、锁、Git索引或用户数据。

`dbr-prepared`是外部普通依赖，公开行为/allocator/负编译均通过产品公开API，不include源码或强行开启依赖cfg(test)。隔离副本`dbr-copy`复制上述101源；最终100份逐字一致，唯一原文件变化为views末尾3行cfg(test)相对模块入口及分隔空行，完整diff `dbr-private-overlay.diff`，核对 `dbr-copy-source-after.json`。私有测试仅用既定`RevisionBoundary::{Penultimate,Last}`，无任意整数/scope/state/ledger setter。

副本构建的build.rs还要求Web index，不在101源中。按父执行者明确授权，仅在copy新增标明synthetic的`clients/web/dist/index.html`编译夹具，并新增独立cfg测试文件；额外清单`dbr-extra-fixtures.json`。未复制/修改root dist，不改生产build规则，不跑npm或Web/HTTP测试。只执行Domain私有filter，其余398个lib测试过滤；不以合成index代签真实Web。

Windows x64/rustc-cargo1.98.1。最终独占本tester `target-management`、`CARGO_INCREMENTAL=0`、`CARGO_BUILD_JOBS=1`；未复用主target。根锁SHA `87f27b619281b37015c79ceae8d95471c8a4de50f202f4dd8a6d99fa0d27e324`直接作为两个新probe的种子，不沿旧副本锁；两probe各238包，除自身本地包外237个的name/version/source/checksum与根261包完全一致，tokio1.54.1、无新增registry包/版本，见`dbr-lock-comparison.json`。

## 实际命令与数量

```powershell
cargo test --offline --locked --manifest-path <本目录>/dbr-prepared/Cargo.toml --tests -- --nocapture --test-threads=1
cargo test --offline --locked --manifest-path <本目录>/dbr-copy/app/Cargo.toml --lib --no-run --message-format=json
cargo run --offline --locked --manifest-path <本目录>/dbr-prepared/Cargo.toml --bin owned_private_watchdog -- <隔离test-exe> 8
cargo check --offline --locked --manifest-path <本目录>/dbr-prepared/negative/Cargo.toml
cargo test --offline --locked --manifest-path app/Cargo.toml --test session_loop_acceptance --test view_budget_acceptance --test view_budget_allocator_acceptance --test acp_view_observations_acceptance -- --nocapture
cargo clippy --offline --locked --manifest-path <本目录>/dbr-prepared/Cargo.toml --all-targets -- -D warnings
cargo fmt --manifest-path <本目录>/dbr-prepared/Cargo.toml --all -- --check
```

| 层 | 实际数量/结果 | 证据 |
| --- | --- | --- |
| 普通依赖公开行为 | 9/9、0failed/ignored/filtered | dbr-public-final.log |
| 独立allocator二进制 | 3/3；首开及重复借用0alloc；4096/32768满额；真实bulk null命中一次 | 同一log的allocations段及tests/allocations.rs |
| DBR04 owned child | 1个子测试内部覆盖同步Drop、跨线程屏障Drop、失败R析构；正常退出/回收 | 同一log的嵌套child段；该child是公开9项之一的执行，不额外充作第21项 |
| 隔离私有极值/布局 | 8/8、398 filtered；native watchdog核精确8项并wait回收 | dbr-private.log、dbr-private-final-watchdog.log |
| 类型负向+正控制 | 15/15均cargo exit101及预期诊断；正控制/最终恢复exit0 | dbr-negative-complete-results.json、dbr-negative-complete-exit.json及dbr-negative-final-*.log |
| 原适用回归 | P1 11/11、原合同69/69（含view19）、budget行为20/20、allocator6/6，共106；exit0 | dbr-regression.log、dbr-check-exit.json |
| 最终外部clippy/fmt | 均exit0；没有更改生产lint规则或加allow | dbr-clippy-final.log、dbr-fmt-final.log、dbr-final-exit.json |

`dbr-final-exit.json`记录最终public/private-build/private/clippy/fmt全0，UTC 2026-10-31T02:46:45.4079821Z。负编译是单案主动切换源码、更新mtime强制编译，逐个验确切错误；不是把编译失败通通算通过。反序列化案另加入同root版本serde1.0.229，随后正控制恢复通过；合并清单保留14+1的原始独立日志。lib/bin的0项unittest不计功能通过。

## DBR01–08分支结果

| 定义组 | 实际验证 | 静态/范围边界 |
| --- | --- | --- |
| DBR01 满额借用 | 空/Reading/Finished/Incomplete；ExactSizeIterator顺序/len/key/bytes；同目标4096及节点8×4096=32768时初次/重复0alloc、usage/items不变；满额snapshot对照Capacity | 0 owned item Arc clone由固定实现直接借window.items的源码核对支持；没有私下增加strong-count测试开口，不以0alloc单独代签0 Arc clone |
| DBR02 HRTB | guard/item/bytes/key/iterator/Vec/future/外槽借用逃逸拒绝；guard不可Clone；stamp不可裸构造/From/Deserialize、借h不能先Drop；允许stamp Copy/Clone及作为R返回，也可返回仅携stamp的future；返回后能独占修改windows | callback主动复制自有Vec仍是调用者预算；没有宣称类型系统禁止任意I/O或所有future |
| DBR03 身份/ABA | 两容器相同gen/rev/target/resource数值仍Stale；不同source/native、旧read/release→begin、原容器Drop后重建；合法h clone通过，错误身份先Stale | runtime/connection/load完整ResourceEpoch比较及scope Arc身份的每字段覆盖属源码证据；安全公开API不能伪造“只改一个私有字段且gen不变”的h，未另加任意scope setter；不靠概率地址碰撞证明ABA |
| DBR04 锁与R | callback内同步Drop同cell、另一线程精确屏障Drop均返回ReadChanged，不死锁；guard header冻结；内部可再读status/usage/nested with_window；失败R析构同cell stage恰一次、费用回基线；已Incomplete重复Drop no-op成功 | 每个危险挂起在自有child内、父10秒watchdog；Domain锁/析构源码另核，没拿任意外部owner锁设计当本包保证 |
| DBR05 统一revision | 原/staged insert/replace同bytes仍各+1；Reading→Finished、first-fail、Drop/overrun/poison/short-write/CAS失败；workspace预算差额及checked溢出失败、原replace容量失败旧值保留；pin两old lease后原/stage FIFO两次移除仍Err，精确+2eviction+1first-fail；独立73123-byte真实null发生在FIFO后，+1eviction+1first-fail，后续可继续 | 未枚举所有allocator尺寸/线程交错及所有std Arc/list全局OOM；真实null和纯逻辑Capacity分别运行/报告，不互相代签 |
| DBR06 post点 | post成功返回stamp后，R中stage随后Drop使stamp ReadChanged；旧generation stage在新read callback内Drop不影响新rev0；callback只执行一次 | 没有实际网络/SQL/页发布；Ok只保证post比较点，未来发送期间最新/撤销由后续owner实现，Domain不回滚任意callback副作用 |
| DBR07 MAX | 原insert及staged publish的MAX-1最后合法变化→MAX；下一真实变化sealed；Drop在callback中封闭使post优先SequenceExhausted；no-op MAX不封闭、首原因sticky；Last→Penultimate Conflict不改状态；sealed诊断snapshot/lease可用或按原Capacity/MissingItem、构建/writer/publish/abort拒绝及回收；stage_insert foreign坏key→Stale、matching sealed坏key→InvalidKey、matching sealed合法key+usizeMAX→SeqEx、未sealed坏key首次state变化撞MAX→SeqEx | 全部来自私有闭合hook注入，不声称自然执行2^64次。准备名含pending_final_contract的函数已按最终5ebc合同运行，不代表仍未执行 |
| DBR08 布局/旧回归 | 私有独立size测量、106适用回归、15负编译、外部fmt/clippy、root101前后同版、copy仅允许overlay | 根全工程check/build由父执行者另报；本报告不使用其结果代签我们的具体Domain用例，也不签P2A |

## 布局与锁的证据

独立cfg测量：ReadMeta16、ReadStatusCell48、Window160、stamp16、BorrowedWindow120、MutexGuard16字节；Window+cell+两usize控制头=224≤既有WINDOW_BYTES1024。guard/stamp是固定栈值，无新持久池；旧窗口/status-cell费用仍到最后workspace/stage引用Drop。布局数值不是进程RSS或所有allocator对齐开销总量，继承原保守对象预算。

静态与动态共同核对：ReadMeta三事实同Mutex；with_window先短锁取(state,rev)、解锁回调、再短锁比较，`checked?`导致的R销毁在guard作用域外。header用pre状态副本，不读live state。所有正文可见更新在同一短Meta区checked+转移；Arc/列表节点准备在外，replace旧Arc与FIFO取出Item在外释放；没有持Meta拿node/target账本锁或Drop Item/Charge。LinkedList pop本身可能释放std节点，不能伪称锁内完全无任何dealloc；关键是Item/Charge析构与预算锁在锁外，原全局OOM abort边界不变。

## 无效尝试与修正保留

没有通过删用例/改预期修产品。以下都是执行环境或探针驱动问题，原日志保留，不算产品失败或通过：

1. fresh target-dbr首次构建ring归档MSVC LNK1114，无法覆盖原archive、错误代码5（不是DLL初始化结论），`dbr-public-first.log`。当时0测试；改用原专属缓存target-management和jobs1后正常运行。
2. 隔离copy初缺Web index导致build拒绝，`dbr-private-build-stderr.log`；随后仅按授权新增synthetic编译夹具，产品build规则未改。
3. PowerShell宿主拒绝运行ps1 watchdog，未改变执行策略；改用正常编译的本机Rust测试owner，固定私有filter/自己的exe/30秒kill-wait，正常核8项并回收。未通过cargo父进程去遗留测试孙进程。
4. 最初Copy-Item保留旧时间戳导致Cargo继续命中正控制缓存，14次exit0的`dbr-negative-*.log`不是有效负编译证据。最终每案WriteAllBytes+更新mtime得到确切拒绝，日志名含`final`。合并15案时曾有PowerShell嵌套数组计成2组，原分组尝试另存；只修结果汇总展平，15案原始诊断不变。

## 仍未验与交付

本阶段20项动态独立案例、15案编译拒绝及106回归已经执行。仍未穷举任意线程调度/锁中毒、全局allocator耗尽/OS故障；正常Arc/LinkedList分配全局OOM仍可abort，不把bulk null注入说成全进程OOM恢复。Meta poisoned恢复规则未新增，未另注入其穷举组合。

借用小端口提供满额内部遍历条件，不等于实际订阅输出/WSS/权限撤销/浏览器首开或原生资源已经装配；stamp不是授权或长期发送围栏。P2A在另独立版本，当前101源没有它，实体current/工具anchor/scanner、可信轮次分页、实际节点/真实Harness及Windows全树和完整L5/L6继续pending/not-run。未访问用户db/private材料或执行网络/audit。

持续资产为`dbr-prepared/`，精确清单`dbr-assets.json`；额外编译夹具单列，根101副本和唯一定义overlay可审。所有child成功wait回收，私有runner只运行固定Domain过滤；纯内存fixtures及TLS allocator开关均已复位，无运行中后台任务、DB或端口。未提交、推送或发布。
