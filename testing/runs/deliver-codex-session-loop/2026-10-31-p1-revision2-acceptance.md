# P1 raw观察 revision2：独立公开API验收

结论：固定revision2上的原8组基础探针及原P1-CODE-001三项负例，实际 **11/11通过**，无失败/忽略/过滤；同版现有69项公共合同与预算20+6项，合计95项回归通过；外部类型负向9项确切拒绝，clippy/fmt通过。P1-CODE-001三项原预期的动态复验依据已成立，交原reviewer最终关闭。仅签本文已执行公共路径，不把内部极值、完整P2/entity current、实际node来源授权或完整L5/L6签完成。

## 固定版本和独立性

输入是root `testing/runs/deliver-codex-session-loop/2026-10-31-p1-revision2-source.json` 十份冻结来源。main SHA256 `0164d462fa3bd16740a664a02407fc67a13176ddfb2b1f73dfba8e4381ee225a`；开发tests `0017c3742eeb53dfb62496d7addf6ab593c57fa7d68a9424479dfb04a0bb548f`；其他8源与原冻结一致。tester自行核对前后10/10均匹配，`p1-revision2-source-before.json`、`p1-revision2-source-after.json`；十源实际副本及原manifest保存在本目录 `p1-revision2-source-copy/`。未改生产、其他tester资产或Git索引。

原1e38快照、三项反证及混合时段结果另见 `p1-initial-acceptance.md`。原十源在root `.tmp/session-loop-p1-revision1/`；本tester重新逐项核对10/10符合原manifest，证据 `p1-initial-source-preserved.json`。原8组和三项测试的预期没有为修复调整，原失败日志/旧锁保留；修订后新增布局小程序只测公开size，不更改原11断言。

本次使用**外部普通依赖** `p1-probe` → 产品app，不include源码或强制依赖cfg(test)。真实Connection+合成双工peer提供Incoming，模拟外层owner持续分流；不是实际节点/认证/Harness。输入全部合成，不读用户数据库/密钥/原生会话。不运行网络、浏览器或Web npm audit。

Windows x64，沿项目rustc/cargo1.98.1；独占 `CARGO_TARGET_DIR=<本目录>/target-management`，最终进程 `CARGO_INCREMENTAL=0`，避免本机缓存hard-link告警。源码/工具没有改变全局环境。产品Cargo.lock SHA `87f27b619281b37015c79ceae8d95471c8a4de50f202f4dd8a6d99fa0d27e324` 未变；probe与negative各238包，其中237个均为root261包中的相同name/version/source/checksum，唯一新增为本地probe自身，tokio同root为1.54.1，无额外registry包/版本，见 `p1-lock-comparison.json`。

## 实际命令和日志

```powershell
cargo test --offline --locked --manifest-path <本目录>/p1-probe/Cargo.toml -- --nocapture
cargo run --offline --locked --manifest-path <本目录>/p1-probe/Cargo.toml --bin layout
cargo check --offline --locked --manifest-path <本目录>/p1-negative/Cargo.toml
cargo test --offline --locked --manifest-path app/Cargo.toml --test session_loop_acceptance --test view_budget_acceptance --test view_budget_allocator_acceptance -- --nocapture
cargo clippy --offline --locked --manifest-path <本目录>/p1-probe/Cargo.toml --all-targets -- -D warnings
cargo fmt --manifest-path <本目录>/p1-probe/Cargo.toml -- --check
```

| 检查 | 实际结果 | 原始证据 |
| --- | --- | --- |
| 原11项公开API/顺序断言 | 11 passed，0 failed/ignored/filtered，2.66秒，exit0 | p1-revision2-tests.log |
| 独立公开布局 | Read304、Stats56、RawObservation32；Read+scratch=1328≤收费1536，exit0 | p1-revision2-layout.log |
| 外部负向接口 | 9个预期编译错误，exit101：E0277×4、E0599×1、E0616×2、E0308×1、借用不能逃逸static×1 | p1-revision2-negative.log、p1-revision2-negative-classification.json |
| 适用原合同回归 | session_loop_acceptance 69/69；budget行为20/20；allocator6/6，exit0 | p1-revision2-regression.log |
| 严格clippy / fmt | 最终均exit0；fmt正常静默 | p1-revision2-clippy.log、p1-revision2-exit.json |

真实退出码由同一shell保存于 `p1-revision2-exit.json`，UTC 2026-10-31T01:23:07.3406845Z。PowerShell包装cargo进度stderr的NativeCommandError文本不作为产品错误，判据为实际退出码/编译错误/运行结果；此最终clippy没有先前缓存hard-link warning。layout二进制的0项unittest和doc-test0不计作功能用例。

## 原8组逐项已执行及未执行细分

| 组 | 本次实际公共路径 | 未执行/不能据此扩大 |
| --- | --- | --- |
| R1 双版完整raw | 真实双版Connection→observe→snapshot；完整通知含外层_meta/params扩展、1e+02/-0、UTF8/转义/NUL、未知tag/块、tool patch；按独立offset和原payload逐byte检查，真实v1 null-ID入口有记录 | 不解释typed current或实体合并；不是实际Harness原始数据 |
| R2 origin/seq | Replay记录、响应后Live不断号；v2 DuringResume；otherSession/response/v2 State Ignored不改变seq/usage；新代际初seq0；真实v2缺snapshot导致原Connection失败 | u64 seq/count末端无法从普通API自然触达，见下方静态条目 |
| R3 滚动与恢复 | 默认13000小记录保存到tail13000；另205帧各≥256KiB、payload51.25MiB后仍可snapshot读尾seq205；单条显示超6000目标budget后Dropped seq1、下一小条seq2/position1；sticky原因及finish拒绝Incomplete | count满的完整snapshot真实Capacity，只证明保存进度，不签首次交付；P1内部“预留已淘汰后真实null”未独立注入，由同版Domain allocator原资产回归覆盖内层，不冒称P1 wrapper注入 |
| R4 pump/零控制 | 超限后真实peer Permission/v2 State/目标B/标准response均处理；P1 outbound计数字节完全不变，无cancel/close/prompt；独立权限handler仍可一次safe cancelled；v1同名state_update作为未知显示 | synthetic owner仅证明接口可持续驱动，不是实际node公平调度/actor撤销/后台审批生命周期 |
| R5 预算/Drop | 独立计算raw137/item681、read1536/frame768，真实峰值5042差1失败/恰好成功；snapshot320、旧read/新window同账及最后Drop；公开布局304/56/32 | 本次没有P1 per-frame scratch真实null/构建途中Drop/短写hook；同步observe内部无公开暂停点。不以归零证明全局分配均可恢复；新增字段源码无raw/Strategy clone须与布局证据共同看 |
| R6 header负面 | 0..34每个截断、16个tag/reserved字节损坏、origin0/seq0/零/极大/短长长度/尾随与双record拼接拒绝；双版×3origin合法组合；借用指针不复制；正确header包非JSON只说明格式可读 | parse_header不是JSON验证、来源证明或授权，公开header可见字段也不是proof |
| R7 封装/陈旧 | 当前错kind→InvalidObservation且零stats/usage；foreign容器/旧代际→Stale且新read保持Reading；非Update先Ignored；Incoming/Strategy/Connection/peer Drop后snapshot仍拥有原raw；三项001误用顺序逐项重验 | 当前Incoming没有私有连接token；两个外层owner误对同一handle各begin的全局单一owner约束不由此wrapper强制，不能由非Clone推断已装配单一nodeowner |
| R8 回归/边界 | 实际95项原合同/预算回归、普通依赖9负编译、clippy/fmt、10源前后相同；v2 null-ID是真协议失败，不计P1显示Dropped；begin缺1字节workspace失败且不泄漏费用 | P0完整35项由其原执行者报告，本次不重复代签；完整P2及L5/L6未验 |

三项P1-CODE-001在原冻结中均实际Err(InvalidObservation)；新固定版分别返回Ok(Ignored)、Ok(Ignored)、Err(View(Stale))，stats/usage及新window状态的原断言也通过。代码顺序修复与动态结果一致，不靠放宽错误类型或移除误用负例取得绿灯。

## 静态证据与精确保留限制

- `last_sequence.checked_add`、drop-count checked及closed分支在固定main源码存在：耗尽只阻止本read显示登记、保持显式CounterExhausted；**未从普通公开API动态触达u64极值**。开发内部9例不是本tester的独立动态证据，不以其代签。
- 本模块borrow已有raw/fields，未持有RawMessage/Value/Strategy或独立SessionId，也没有Connection/writer/Storage/控制端口；source/layout与独立费用边界共同支持P1先收费。其global allocator OOM继承root abort边界，不宣称所有heap异常返回Capacity。内层fallible bulk/null与零分配status在同版预算独立资产6/20回归中实际执行，但不伪称在P1 observe每一步都注入。
- **4096 None-key对象时首次交付仍有缺口。** 本次真实满条数后snapshot再需4096引用，受同一items=4096限制而Capacity；无按None-key获取单条lease的P1入口。不能把publication/window_status尾位置当内容已送到订阅者。字节路径205大帧能snapshot读出只证明另一条可达路径，不覆盖该限制。后续delivery/有界读取或引用余量需所属阶段解决，原spec“首开无后续输出”义务保留。
- 同Connection/source/权限与单一owner由外层保证，当前public Incoming字段校验只是形状一致性，不能抵御伪造来源授权。无node/WSS调用者装配，所以不宣称已按真实actor授权或撤销。
- VPROJ-PLAN-002、VP-S05/S13工具锚点/原位current、scanner/typed patch、可信轮次分页、浏览器安全呈现/资源访问、真实Harness与Windows树及完整L5/L6均pending/not-run。P1全raw不是永久不做实体语义的产品决定。

## 可持续验收资产与清理

外部 `p1-probe/Cargo.toml`、`src/lib.rs`（原8组）、`src/ordering.rs`（原三项001）、`src/bin/layout.rs` 可重复运行；`p1-negative/`专门为期望编译失败资产。精确文件摘要在 `p1-revision2-assets.json`。若后续入仓，测试owner须按Rust模块路径接入lib/ordering并实际编译，不能直接把外部crate路径改成生产绝对include；本轮没有写root资产。

双工端点和所有ViewWindows/read/snapshot在各测试作用域释放，指定用例另显式release/collect并核回root8192/0items；没有spawn子进程、脱离owner任务、临时数据库或端口遗留。测试代码运行时分配的合成fixture/raw期望属于外部探针，不冒充被测P1预算。未提交、推送、发布或外发审计。
