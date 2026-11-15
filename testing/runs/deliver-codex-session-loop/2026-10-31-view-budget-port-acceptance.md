# Domain视图预算端口独立验收

2026-10-31。限定A–I子范围独立验收通过：**新公开API20项、独立allocator二进制6项、原视图19项，共45 passed / 0 failed / 0 ignored**；另有14个精确负编译及16次新API正向控制通过。外部crate all-targets clippy `-D warnings`、新资产rustfmt检查通过。未发现本轮范围产品缺陷。

这是纯Domain字节/预算端口结论，不是ACP结构投影、DrainOnly、渲染、权限、真实owner/pump、WSS或完整L5/L6通过。不读取JSON语义、不签分页/真实Harness/全部heap OOM恢复。root生产及原19资产本轮只读，所有新增资产和报告均在tester独占外部根。

## 固定版本和资产

依据独立定义e0ae135e...、根合同d6ee1d36...；冻结清单为root `testing/runs/deliver-codex-session-loop/2026-10-31-view-budget-port-source.json`。独立执行前后逐一核对其10项均相同，见view-budget-before.json和view-budget-after.json：views.rs `a22bb6e5e9a0e7fdca3b66e9e3dc18025a37ab57832ea95b44e509d189255f26`、staging.rs `d2a2a6ec3756777a5758d05ffd51e7a78f9f62fb5158283545854c77b00b0f09`、开发staging/tests.rs `7533fa57...`及Cargo锁/工具链/合同均匹配。原独立view_window.rs保持SHA `28f2df93e92ebc159a120b21336f6983ad1043856dd11af387d5b7ecaee967d8`，外部测试逐字复制此文件，没有改原19预期。

可持续外部普通依赖crate位于`view-budget-probe/`，只直接依赖产品path，无新依赖/feature/生产hook。新文件摘要清单view-budget-acceptance-assets.json：

- tests/budget.rs：`bfafcf959d0085856fad2640d64d31c452f3a95a3e2609a6f21a407482c2547f`，20项公共行为/账本测试。
- tests/allocations.rs：`348c79758ba640f7d0c3dd894fbd9e33c485071f4c07c98689ff21c7a9188e64`，单独测试二进制6项；scoped allocator不影响其他二进制或生产。
- tests/support/mod.rs：`249f352aa76733229276035c0154df6793cb82ea6be17a39f6a3ff0b89c4b9d9`；原19另在tests/view_window.rs。
- run-budget-negative.mjs及每个negative/positive源、编译JSON保留，最后活动src/lib.rs恢复为普通正向导出。

产品锁261包，外部可达图238包，237包均来自产品锁相同name/version/source，唯一新增是probe自身。未使用的产品dev依赖被裁减，不是新增版本，产品Cargo.lock没有变化。普通dependency不是include产品源码或强制依赖cfg(test)。

## 命令、实际退出码与环境

Windows x64、rustc 1.98.1 (48a229cea 2026-09-01)、cargo 1.98.1。独占CARGO_TARGET_DIR为原tester activity-target。最终清洁记录关闭本次进程incremental，避免本机缓存hard-link复制警告；不修改全局Cargo/环境配置。

```powershell
$probe='C:/Users/jack/.codex/visualizations/2026/10/30/01a18dc1-45b2-7f82-981e-964264221cef/deliver-codex-session-loop-tester/view-budget-probe'
$env:CARGO_TARGET_DIR='C:/Users/jack/.codex/visualizations/2026/10/30/01a18dc1-45b2-7f82-981e-964264221cef/deliver-codex-session-loop-tester/activity-target'
$env:CARGO_INCREMENTAL='0'
cargo test --manifest-path "$probe/Cargo.toml" --locked --offline --tests -- --nocapture
cargo clippy --manifest-path "$probe/Cargo.toml" --locked --offline --all-targets -- -D warnings
```

最终运行`test-final-clean.log`有3个真实测试二进制：allocations6、budget20、view_window19。空src/lib.rs单元测试0不计功能通过。view-budget-runtime-exit.json记录真实工具完成exit_code=0（shell显式exit LASTEXITCODE），不是根据stderr外观推断。最终clippy-exit-recorded.log及view-budget-clippy-exit.json为同shell直接采集exit0，工具完成也为0。PowerShell将cargo stderr正常进度行包装成NativeCommandError文本不代表cargo失败。

首轮17行为/6allocator/19回归日志及含缓存hard-link warning的中间45日志保留；没有产品失败被抹掉。补目标隔离/纯lease/UTF8键边界至20行为后最终45实际重跑，均为当前冻结产品版本。汇总为view-budget-acceptance-summary.json；全部命令离线，无Web audit/浏览器/网络外发。

## A–I实际覆盖

| 组 | 本轮独立结果 |
| --- | --- |
| A 单条lease | 多项窗口只借匹配一条，320字节/1引用逐份计费；Missing/InvalidKey零新租约；字节/items恰好与差1；纯lease在release后只保留身份+旧item+lease而不保活window1024，最后Drop退账 |
| B workspace/cell | n=0/1/7680精确512+n，0长仍512，恰好/差1与usizeMAX拒绝；未授stage的workspace正常Drop不标失败；旧workspace跨release/new begin仍保旧cell1024，多份到最后Drop才退，node/target共享、64身份仍占名额 |
| C writer/边界 | 完整/零长/分段写；overrun本次零写、written不推进、永久PoisonedStage；ShortWrite无发布；满写仍不可见；只成功insert推进position；4096 UTF8键可用、4097 Structure且零淘汰；容量/checked失败显式未完整 |
| D 峰值/move | B2049+old516+lease320+work512+candidate516=3913，恰好成功/差1字节或1item失败；replace零淘汰并保旧值/位置。消费lease不加第二320，snapshot/其他lease仍保旧blob；publish后S/L退账，最后旧引用才退正文；指针/分配实测见下 |
| E CAS/ABA | 多candidate已实际支持且全部收费；两候选同旧item先发布者胜，另一Conflict；grant前已换item亦Conflict；stage中旧项被FIFO淘汰再同key/相同bytes插入仍拒旧候选，真正private强Arc身份不是key/地址裸比较 |
| F scope/Drop | 跨容器同局部generation、同容器错target、read/load/connection/runtime变化，foreign work/lease/stage拒绝；Drop/abort仅标原cell，不污染新Reading；旧snapshot冻结状态不变；abort理由sticky、旧h abort消费并Stale且费用清完 |
| G FIFO/status | 10000次stage insert持续滚动最新窗口，不引入DrainOnly；duplicate reserve/publish Conflict不偷换replace；stage不可见半成品；真实bulk分配失败发生在FIFO淘汰2项之后，status保eviction2/through2/next3/retained0，后来insert成功位置3；低预算snapshot失败时status依旧可取且1万次零alloc |
| H 公开封装 | Clone/move/借用逃逸/private buffer/不能提取writer数据/snapshot不是写token等14项精确负编译，每项后正向控制，见下 |
| I 原回归/布局 | 原19逐字资产同版全过；公开size_of与实际allocator请求/无copy转换核对；domain依赖仅native/std，不接ACP/JSON/SQL/文件/授权或执行副作用 |

## 真实allocator null、容量与512/1024布局证据

allocator仅属于外部allocations测试二进制。const thread-local固定Event数组记录请求，不在分配回调里分配/格式化/查询账本；RAII guard只在当前线程指定调用期间启用，唯一预先识别的bulk Layout(size,align1)首次命中返回null，其他调用转System。不会耗尽系统内存，不注入Arc/list元数据OOM，不用生产hook。

实际六次null均记录命中1并验证Capacity、Incomplete及账本/旧值/水位：scratch10007；insert payload12347和key3071；replace payload12347和key3071；FIFO已淘汰2项后的payload12347。key失败前已分配的payload还实测对应dealloc指针，确认部分构建退账/释放；replace失败原值不变，未淘汰。六次来自四项故障测试中的两个双分支，不冒称六个不同测试函数。

独立公开size_of：workspace144、staged360、lease128、parts104、WindowStatus96。workspace≤512，staged+parts464≤512，lease≤256元数据部分。实际root创建请求[16,80,512]合计608≤根8192；begin目标/窗口请求[64,200,48]合计312≤身份1024+window1024。最后48字节是ReadStatusCell Arc分配，由源码字段（Mutex<ViewState>及Charge）/Arc控制块核对；WindowStatus96不是私有ReadStatusCell32，不混淆两者。私有Window/Cell字段布局以源码/实际元数据请求核对，未把开发私有size_of测试冒作本tester公开反射。release后旧cell1024的保守费实际长存/最后退账由公开usage断言直接验证。

选定scratch3079/key2099/payload12347各出现一次精确align1 Layout请求，payload与key发布后借读指针等于原分配指针，期间realloc0，实测没有复制第二份blob或key；writer容量固定与完整长度另测。固定std源码独立读过并hash保存view-budget-stdlib-source.json：RawVec grow_exact将len+additional设为cap；Vec满len时shrink_to_fit不分配，into_boxed_slice移走buffer；String try_reserve_exact/into_boxed_str沿同一Vec链。结合当前源码reserve先于fallible分配、满capacity填充，再移动发布，确认不会靠分配后发现超cap才补费。该结论限当前固定工具链/布局，不宣称进程RSS等于账本或所有allocator物理开销逐字相同。

## 14项负编译与16次正控制

run-budget-negative.mjs实际执行30次普通`cargo check --lib --locked --offline --message-format=json`：初/末正控制2次及每个负项之后14次正控制，合计16次exit0。负项各exit101且只有预期目标span/诊断，完整源SHA与编译JSON见view-budget-negative-results.json及probe对应文件：

- ItemLease/StageWorkspace/StagedItem/BoundedWriter不满足Clone：4项E0277。
- 消费lease/workspace、publish后stage、abort后stage不能再用：4项E0382。
- bytes和scratch不能逃出拥有对象：2项E0515。
- staged.data和lease.item私有：2项E0616。
- writer无into_inner：E0599。
- ViewSnapshot不能作为stage_replace的ItemLease：定向E0308，证明该接口不接受snapshot写身份，不以无关类型错当封装成功。

正控制实际调用workspace/stage/write/publish/status而不执行，排除库整体缺失、语法/依赖/工具失败或强制cfg(test)造成假绿。负编译不会复用运行期测试二进制的globalallocator。

## 证据限制与交付

动态覆盖可公开触达的usizeMAX scratch/候选长度拒绝；u64读取代际/position极值、ledger损坏/poison等不可由当前公开API设置，仅独立静态核对checked分支与无回绕，不冒签开发私有故障测试。没有做进程级全局OOM/OS故障实验；Arc/LinkedList稳定std abort边界沿d6保留，实际null只证明上述选定fallible bulk失败可返回。布局测量不是Miri/全状态证明；并发异步owner/真实协议/权限装配不在本阶段。

完整CL ID/标题不改；CL-023/026/049/050等只签定义所列L3子范围，完整产品与L5/L6仍未验。所有新增文件仅外部资产，根生产/原测试10项摘要前后一致；测试无DB/密钥/端口/外部进程服务，无用户数据，cargo/Node进程结束。probe/日志/target为持续回归有意保留，globalallocator只随独立测试进程存在。后续若集成，建议行为测试与allocator二进制分开，避免globalallocator干扰既有测试；本轮未自行写入root。

plan持久摘要：固定a22bb6e5/d2a2a6ec/d6，独立普通依赖20新行为+6allocator+原19=45/45、0ignore；真实bulk null6次、status零alloc1万次、privateArc CAS/旧cell1024/峰值/move及14负/16正编译通过；外部clippy/fmt通过、10冻结源未改；内部不可达极值仅静态、全局OOM与真实ACP/owner产品未验。
