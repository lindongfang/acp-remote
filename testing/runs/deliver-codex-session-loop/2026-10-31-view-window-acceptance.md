# 纯内存视图窗口独立验收

2026-10-31。结论：本轮独立公共 API 测试 **19 passed / 0 failed / 0 ignored / 38 filtered out**，测试入口 clippy `-D warnings` 通过。未发现本轮可达公开 API 范围的产品缺陷。该结论仅覆盖 view-window-contract 的纯容器 L3 子范围；完整 CL 用例、任务3.2、Node/ACP/WSS/SQL/浏览器/真实 Harness 均未因此通过。

## 版本与独立性

依据：最终 view-window-contract SHA256 `2d8b7362a3e1c8869bbe8d8da1808b611c1a512a351aab504ae1c6b3f6f00de8`；独立 A–J 子定义 view-window-definition.md SHA256 `9bf0017621854f5296bf65b828467847624ef3d835a6c44ef303b4ee21a5241a`；implementation-contract §7 与 structured-history spec。统一 tests 的稳定 ID/标题未修改。

tester 自行读取实际公共 API、计费布局及开发测试，再使用公开 `ViewWindows/NativeSessionKey/ResourceEpoch` 编写独立消费者测试。独立断言的费用数字取自合同，不使用产品费用常量生成期望；不调用私有模型字段或开发测试 hook。已知私有溢出开发测试不计入本报告独立通过数。

生产 views.rs SHA256 `ee531f845af4050809721d88e41fbce0f0bff16235cfdb763715a5dc845ca612`；开发 tests SHA256 `61e240cf36587e8556235d7fcf512903c978d813d4f5a98db38f510e21569e41`。执行前后这两文件、native.rs、domain/mod.rs、Cargo.toml、Cargo.lock 及固定合同摘要逐一相同。完整清单为 `view-window-implementation-before.json` 与 `view-window-acceptance-source.json`。

只新增 `D:/Project/acp-remote/app/tests/acceptance/view_window.rs`，并在已有 `app/tests/session_loop_acceptance.rs` 追加对应模块声明。最终测试 SHA256 `28f2df93e92ebc159a120b21336f6983ad1043856dd11af387d5b7ecaee967d8`；入口 SHA256 `cb7e7808201a066f8f47f635c3190624c006070e304f7729939515b2efb791a7`。既有 activity/acp 独立测试摘要未改变；未改生产代码、用户数据或依赖锁。

## 命令与原始证据

Windows NT 10.0.26300.0，rustc 1.98.1 (48a229cea 2026-09-01)，cargo 1.98.1 (797e8a9bc 2026-08-05)。在仓库根执行：

```powershell
$env:CARGO_TARGET_DIR='C:/Users/jack/.codex/visualizations/2026/10/30/01a18dc1-45b2-7f82-981e-964264221cef/deliver-codex-session-loop-tester/activity-target'
cargo test --manifest-path app/Cargo.toml --locked --offline --test session_loop_acceptance view_window -- --nocapture
cargo clippy --manifest-path app/Cargo.toml --test session_loop_acceptance --locked --offline -- -D warnings
```

独占 target 延用 tester 先前构建缓存，不占父执行者 target。全部输入为合成字节/身份，不创建数据库/配置/外部进程。未运行 Web audit，未外发依赖元数据。测试结束后进程退出，内存对象按明确 Drop 次序释放；测试资产及日志保留。

`view-window-test-final.log`：19项通过，命令退出0。`view-window-clippy-final.log`：退出0。PowerShell 将 cargo 标准错误进度行包装成 NativeCommandError 外观，但 cargo 最终状态和 `$LASTEXITCODE` 均为0。首轮18项测试成功记录 `view-window-attempt1.log` 保留；随后补生产4096/32768计数边界及跨线程租约 Drop，最终19项成功。本测试源码自身的4项 clippy 风格告警、继发1项 const 告警分别保留 `view-window-clippy-attempt1.log`、`view-window-clippy-attempt2.log`；修正命名和 const helper 后通过，没有修改产品或验收期望。

## A–J 子范围实际结果

| 定义组 | 实际可观察断言 | 结果 |
| --- | --- | --- |
| A：身份、scope、代际 | 两容器相同局部代际的句柄交叉 insert/replace/finish/fail/release/snapshot 全拒绝；同source不同ID/不同source同ID隔离；同Key分别换read/load/connection/runtime，旧写入/完成/释放拒绝，旧快照仍显示旧资源/读取代际和字节 | 通过 |
| B：begin/状态 | Reading重复begin为Busy且账本不变；首个SourceRead失败保留，第二失败不覆盖，finish不洗白；新begin重建Reading；成功finish和重复finish可观察；结束后刷新撤旧句柄 | 可达公开分支通过；结束后新预留失败分支见下文 |
| C：在途快照/惰性保真 | insert后立即快照含在途数据；空快照与旧快照不可变；NUL/非法JSON/非UTF8/HTML/URI外观字节逐字相同；registry Drop后既有快照仍可借读 | 通过 |
| D：键和替换 | 无键同字节占不同位置；同字符串不同命名空间隔离；重复insert Conflict，缺key replace MissingItem；完整替换保持工具初始位置与后续顺序；新读取不继承旧键；4096字节多字节key/nativeID合法、4097拒绝 | 通过 |
| E：精确账本 | 根8192仅节点；身份1024+UTF8长度、窗口1024、条目512+key+bytes、空快照256/有引用256+64N均精确；目标/节点字节恰好与少1边界；非法超生产/usizeMAX/不足根limits拒绝 | 可达边界通过；内部checked极值见下文 |
| F：流式淘汰 | 10000条流后仅保留9997–10000，水位9996/计数9996；Finished仍非history_complete；其他目标内容不受清理；被淘汰key更新MissingItem；单条超限Capacity且Incomplete不能finish洗白 | 通过 |
| G：租约与替换双份 | 有/无旧快照时，候选峰值差1字节均拒绝，原条目/位置/其他条目/账本不变且无淘汰；峰值恰好成功；旧快照保留旧版本；淘汰后仍被两个快照持有的对象不退账，逐个Drop准确退元数据与最后正文 | 通过 |
| H：跨代际与64身份 | 同Key旧快照+新window共享目标字节及条数限额，逐一换各epoch不获第二份预算；64个无当前window的旧租约身份阻挡第65个；部分Drop仍占名额、最后Drop后collect_released可回收 | 通过 |
| I：快照容量与零泄漏 | 两条目快照384字节、2引用的恰好/差1字节/差1名额；反复失败账本不变；旧数据仍保留；空快照非零费用；真实默认每目标4096与节点32768同时计算entry+references；租约跨线程Drop后join确认准确退账 | 通过 |
| J：布局/副作用检视 | 固定64槽、LinkedList逐节点释放、闭合枚举namespace、线性键查找、有界水位计数、精确boxed引用slice；快照非Clone且仅返回借用，不暴露Arc；候选/快照先reserve后分配；模块只依赖std/native/错误派生，无SQL/文件/网络/取消/审批调用 | 独立静态检视符合；不冒称运行时Node副作用通过 |

## 稳定 ID 状态与未覆盖

| ID（标题沿统一tests不变） | 本轮 L3 子范围结论 |
| --- | --- |
| CL-006 | 当前目标共享容器、scope/资源代际、旧句柄拒绝、lease释放通过；浏览器重连/Node资源 owner 未验 |
| CL-012 | 可靠键原位替换、顺序、缺项与跨代际不猜合并通过；协议结构适配/轮次未验 |
| CL-023 | 工具完整对象替换/旧快照/峰值失败原子性通过；真实工具协议和执行未验 |
| CL-024 | 未知/不可信字节保真与namespaced key通过；结构分类/前端渲染/资源安全未验 |
| CL-025 | 容器无保存调用仅静态检视；产品保存或原生会话行为未执行 |
| CL-026 | 缩小字节预算、真实4096/32768计数、租约/快照计费和失败通过；外层队列/在途响应预算未验 |
| CL-027 | 容器release/rebuild不复活旧window通过；真实原生读取及历史权威未验 |
| CL-042 | 首开立即快照含在途数据通过；Node订阅/WSS未验 |
| CL-049 | 流式淘汰/有界水位/租约共享预算/快照计费通过；标准ACP全量重放接线及分页未验 |
| CL-050 | 缺失/容量/读失败与完成分开、finish不洗白通过；真实回放中断/协议完成未验 |
| CL-051 | 临时位置、namespaced key、scope/read/resource隔离与MissingItem通过；真实跨加载来源身份未验 |
| CL-052 | 容器仅显式finish，失败sticky与新begin重建通过；协议完成证据适配未验 |
| CL-062 | release旧窗口后重建仅代表新读取的模型语义通过；原生压缩/删除未执行 |
| CL-063 | SourceRead失败不称完整、旧快照不变和新read重建通过；真实原生读取错误入口未验 |
| CL-064 | 换代/释放/64身份最后lease回收模型通过；权限撤销、真实资源容量和回收进程未验 |

这些ID均仅记录子范围，不将完整用例改为Passed，也不把原L5/L6义务降级。没有触碰统一tests的稳定ID/标题。

两项明确的动态覆盖限制：

1. 已结束同Key begin 的“新1024字节窗口预留失败”：固定limits、独占`&mut self`且原1024窗口已成功计费时，撤旧后预留等量账本空间在可达正常公开路径上必然有额度。测试实际覆盖成功刷新撤旧句柄及首次begin窗口预留失败后的空身份回收；不把后者冒充前者。独立静态确认实现先`slot.window=None`再reserve，新reserve错误会向外返回且不会恢复旧window。内部账本损坏/分配故障没有公开注入，本轮未动态触发。
2. `u64::MAX`读取代际/位置、`usize::MAX`内部加法与计费退账损坏保护：公共API没有设置内部序号/账本的入口，也不能用超大切片伪造合法内存。独立静态检查checked_add/checked_mul/checked_sub及错误传播，记录为静态检视；不依赖开发私有测试冒称独立运行覆盖。公开极端limits拒绝有真实运行。

生产默认字节上限值已实际断言，字节边界运行使用明确缩小限额，未为证明128MiB拒绝而分配巨型对象；同一账本路径的真实默认4096/32768计数边界已实际运行。未知系统分配器OOM不在本轮故障注入范围。后续接线仍需独立验证结构适配、授权/逐块撤销、慢订阅/外层复制预算、ACP完成证据、SQL正文隔离与真实Harness。
