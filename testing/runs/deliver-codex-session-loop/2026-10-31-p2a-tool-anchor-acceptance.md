# P2A 工具锚点：独立公开 API 验收

结论：冻结外部版本的工具锚点子范围 **19/19 独立探针通过**；同版原公开回归 **112/112 通过**，无失败、忽略或过滤。6 项预期负编译和 5 个 Copy 类型正控制成立，严格 clippy/fmt 实际 exit0。没有发现产品阻断。以下只签实际执行子断言，不签 P2B current/scanner、首开 delivery、真实 owner/授权或完整 CL/L5/L6；同步内部 CAS/u64 极值等未动态分支单列。

## 冻结来源和隔离

独立定义 `p2a-tool-anchor-definition.md` SHA `faa72a83e9a168059a5662eefe9ff54a076bee10a0ea76c16226d670e08e259a`；合同81d0已关闭TG01定义，未改其预期。受测源为外部 `deliver-codex-session-loop-p2a-implementation/workspace`，主仓当前另包 Domain 开发未用于本次构建。

- HANDOFF SHA `7828d2fd3768615459115f3dd9d3907e999b83a8a66d24d5f5fa468ef38e4d8c`；冻结manifest SHA `9181f209916771ecc59e2b3b7f0f8703beaab59266234b8f95b2ee769f7637b6`。
- baseline manifest `5ddc610e9c4d3c6b898616e2e4c324e5ab11a7ca3a51e82d5d6618e6de1b28c4`；patch `ea8bac49e74c436482ca4457095732d9ca1d9e8e6436bb0b6257b5263d73ac49`。
- 新源码 `tool_projection.rs` SHA `4cf2b6d36a8a3095bed73de496d926cec871e97bc0addf431256be5a8d54b503`。P1 `0164d462...`、Domain `a22bb6e5...`/staging `d2a2a6ec...`，锁 `87f27b619281b37015c79ceae8d95471c8a4de50f202f4dd8a6d99fa0d27e324`。

tester 自行逐 hash 校验103文件并逐字复制到自己 `p2a-sut/`；外部普通依赖 `p2a-probe` 只引用此副本，不 include 私有源码。与baseline差异仅原ACP mod多一行声明、新模块、新开发tests三路径，原100文件不变；mod在内存撤掉声明后逐字等于baseline。执行后候选和自有SUT103/103、四交接文件摘要均未变化，见 `p2a-source-before.json` / `p2a-source-after.json`。未改root或冻结产品，不采用开发15项为独立结论。

Windows x64，rustc/cargo1.98.1；独占 `activity-target`，`CARGO_INCREMENTAL=0`。全部synthetic双工数据，没有用户数据库/凭据/配置/原生会话、网络、浏览器、Harness或Web audit。普通probe/negative各238包，237个第三方/产品条目均与产品261包 name/version/source/checksum精确一致，唯一新增各自本地probe包，省去24个开发图条目，无新增registry依赖；见 `p2a-lock-comparison.json`。

## 命令、实际退出码和日志

以下路径相对本tester目录，命令完整记录于 `run-p2a-final.ps1` / `p2a-final-exits.json`；UTC 2026-10-31T02:39:36Z。脚本运行使用进程级 `powershell.exe -NoProfile -ExecutionPolicy Bypass -File`，未改变机器策略。

| 检查 | 实际结果 | 原始证据 |
| --- | --- | --- |
| `cargo test --offline --locked --manifest-path p2a-probe/Cargo.toml -- --nocapture` | 19 passed，0 failed/ignored/filtered，0.12秒，exit0 | p2a-tests-final.log |
| `node run-p2a-negative.mjs`，逐项普通依赖 `cargo check` | 正控制exit0；6项分别exit101且匹配确切封装/借用错误 | p2a-negative-results.json、p2a-negative-*.log |
| `cargo clippy --offline --locked --manifest-path p2a-probe/Cargo.toml --all-targets -- -D warnings` | exit0 | p2a-clippy-final.log |
| `cargo fmt --manifest-path p2a-probe/Cargo.toml -- --check` | exit0 | p2a-fmt-final.log |
| 冻结SUT `cargo test --offline --locked --manifest-path p2a-sut/app/Cargo.toml --test session_loop_acceptance --test acp_message_id_acceptance --test acp_view_observations_acceptance --test view_budget_acceptance --test view_budget_allocator_acceptance -- --nocapture` | 69+6+11+20+6=112 passed，均0失败/忽略/过滤，exit0 | p2a-regression-final.log |

PowerShell将cargo正常stderr进度包装成NativeCommandError的文本不代表失败；结论基于相同shell采集的真实 `$LASTEXITCODE` 与实际测试结果。0项lib/doc-test不计功能用例。negative正控制仅正常check，不宣称严格lint；其五个Copy控制是ProjectionStep/FieldFact/RawEvidence/ToolStatusFact/ToolKindFact。负向为owner Clone E0277、Default E0277、私有raw E0616、虚构Retained E0599、DTO重建owner私有字段拒绝、AnchorView借用无法逃逸static；逐项独立源码均在 `p2a-negative/cases/`，不是用缺依赖/库整体不可用冒充拒绝。

## A–H 实际覆盖及保留边界

| 组 | 本次独立实际断言 | 未动态覆盖/不据此扩大 |
| --- | --- | --- |
| A 固定格式 | 双版512长度边界、magic/version/entity/wire、保留区、非法knowledge/origin/序号/loss、namespace/ID；status/kind×双版×8知识码×256小码共8192组合，Unknown/Uninterpreted/已知/未识别联合合法性；正确getter输出 | decoder为格式和借用事实，不验证真实来源/授权；非小枚举槽未做所有可能字节的穷举 |
| B 形状/原子失败 | 7类字段错型且前面含合法赋值，旧512字节/位置逐字不变；缺/错误initial title、initial status/content:null；错版tag不建anchor；chunk必需内容/类型；ID空/4096 UTF8可用，4097/数值/null由真实Connection先拒绝；形状失败raw事实仍回传、Incomplete且零控制wire | 未把协议入口已拒绝的ID伪称apply返回Failed；不声称验证完整嵌套schema；不是每tag每字段全部笛卡尔积 |
| C 精确转移 | R/D两列按九行有限表，NoOp四项保持；v1 initial meta:null/name缺省及rawInput/out:null；v1 update null Uninterpreted或保持；v2 KnownEmpty/Absent/pending/other；标准小码Known、未来码Known/MissingRaw及Unverified/NotRecorded；D→R保留历史loss、Uninterpreted→Known重算flags；chunk R Deferred/D MissingRaw且不patch entity_meta/status；v1拒cancelled/未来枚举整帧零赋值；真实raw含原未来字符串/1e+02/转义逐字保留 | 不解释任意字符串/大结构的current；Deferred不等于已完整支持，不用小事实推导Run或授权 |
| D 晚完成/来源 | 双版各257工具+交错文字，无淘汰；4096字节ID；首工具300000字符扩展后completed仍原position，seq与first/last准确，无256表/LRU/softfreeze；真实offer2选1、replay响应→Live、resume响应→Live，同锚点字段来源和每条raw seq/origin分别核对，mask最终7 | 未模拟完整Harness历史；DuringReplay仅RPC窗口事实，不能以mask证明整条历史来源 |
| E raw知识边界 | >预算raw Dropped但小status仍原位Known，长name MissingRaw，未来status/kind NotRecorded；后续小帧继续且loss保留；P1 Recorded后同帧anchor为腾预算真正淘汰刚录raw，标准小status仍Known；后续FIFO删raw但name事实仍Known，raw实际已不在 | Known不等于任意值可读；本包无P2B读取API，不能签“已从raw定位current/MissingRaw回退算法”。没有Retained或免费钉住raw |
| F 失基础/失败不换位 | P1 raw先真实FIFO移除旧anchor后才CreatedPartial新position、first=last当前seq/baseUnavailable；相反lease容量、workspace缺1、candidate缺1、损坏anchor均Failed且无另位置fallback/无二次observe；真实snapshot占满缩限4引用导致lookup Capacity，旧bytes/position保留；raw结果/真实水位仍在 | 缩限4引用是同一真实账本路径，不冒称完成满4096 delivery；同步apply内部CAS无公共暂停点，未人为制造并发 |
| G 费用/生命周期 | read收费2560/0，begin第二workspace差1失败且P1退款；layout ToolRead552−RawRead304=额外248≤512；旧lease320+workspace1024+候选(512+id1+payload512)=2369，精确峰值恰足成功、2368失败；旧read跨release/resource换代持1024cell+2560直到Drop，最后回root8192；foreign容器/旧read错误不污染新read，Ignored/InvalidObservation不重复推进，单次合法观察seq1；零取消/关闭wire | P2A内部中途bulk真实null、短写/poison以及u64计数极值未独立动态注入；见下方原资产/静态证据分界；未做全进程OOM恢复 |
| H 回归/封装 | 同版112项原资产实际重跑，含P1十一、发现17/权限12在69内、原view19、P0六、预算20/allocator6；6负向+5类型正控制，严格lint/fmt、锁/103源前后相同 | root的新Domain/revision组合尚未验；不将本外部限定结论直接搬到未来root集成版 |

## 静态证据、未执行与初始失败归属

完整生产源码已读，`p2a-static-scope.json`附辅助计数：唯一`.raw.observe`、唯一item_lease、唯一stage_replace/stage_insert/publish调用；唯一clone是ViewHandle，不复制Value/RawMessage/工具ID/Strategy，不持Connection/writer/Storage，不加索引/新namespace。正常路径无unwrap/expect。流程只在MissingItem走insert，lease/decoder错误终止；原raw结果与anchor失败分开。词法计数只辅助，不代替实际动态断言及独立reviewer源码检视。

同步独占`&mut ViewWindows`的apply没有公开中途CAS暂停口；u64极值、P2A内部短写/allocator null没有新增公共fixture，本轮**未独立动态执行**。原同版Domain预算20+allocator6已真实重跑并覆盖内层CAS/ABA、short/overrun/poison、定向fallible bulk真实null、零分配status，但不能将它们说成在P2A每一步都注入。P2A固定512 encoder/单次write/publish、受费scratch/lease控制流作静态证据；metadata Arc/list全局OOM仍继承既有abort边界。

没有产品失败被隐藏。原 `p2a-compile-attempt1.log` / `p2a-tests-attempt1.log` 是tester首次误写public API名/参数借用，未运行功能；`p2a-tests-attempt2.log` 为2/15通过、13个fixture从配置缺source提前失败，补显式合成source后原断言15/15通过（attempt3）。扩展为18/19不是削减失败用例。`p2a-clippy-attempt1.log`四项测试风格lint修正为局部命名/消费fixture Value，未改产品、assertion或验收预期。直接执行本地PS脚本首次受执行策略拒绝、未开始测试；后用进程级Bypass执行自己的明确脚本成功。原日志/真实退出记录保留。

## 可复用资产与未完成产品义务

主独立资产 `p2a-probe/tests/anchors.rs` SHA `b15310c25ece70fb3b8a96cef28ee909de74fa03c2d03643562b9121e4ba2fd7`；support SHA `e733dbcbb617189e698ce757685d9375ed0479b721a0966a6f339139cd8484bf`。完整资产/执行脚本摘要见 `p2a-acceptance-assets.json`，负向脚本/cases可独立重复。尚未写root验收资产，后续如集成必须固定新组合再验，不能改原冻结外部证据。

所有View/read/snapshot/workspace与双工端点由测试作用域Drop，明确旧代际测试核到8192/0。没有脱离owner任务、真实进程或用户数据库资源。内部不可达极值与真正全局OOM、满4096首开无后续输出、scanner/typed current、完整字段嵌套解释、Node来源/操作者授权/撤销、可信分页、WSS/Web安全及真实Harness/完整L5/L6均not-run。工具anchor小包不独立关闭整个P2、VPROJ-PLAN-002或完整任务3.2；交原reviewer作同版限定结论。
