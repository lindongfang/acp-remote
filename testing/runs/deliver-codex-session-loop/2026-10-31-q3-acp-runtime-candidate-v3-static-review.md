# 公共ACP runtime candidate-v3固定源静态复核

2026-10-31。**固定v3静态无剩余阻断，八项修复逻辑闭合；最终交付及问题关闭仍待同源独立完整证据。** Root未合入/组合检查未执行，不以开发82例代签。

## 输入和范围

- 源清单：`98393e3f0a5b39d0db9c98f456ec891af8602c4ea3014ed233476d2af622cc85`；190文件逐hash全部匹配。
- build-inputs `3fe26f29438d580a5867ac9580ceb00c2eb6f9b00573d2955279e446a68eb8f5`，三份实际dist匹配；源内toolchain、JWS fixture和lock亦被190清单覆盖。
- HANDOFF `44fb64a349dfdbdd79d1c6d0178c4e63b557cbe95867adc226a74ae900d7ca26`、delivery `233ac97a899a6bfa04f628c17ec6135cfbdf6a17d2ec40230796f4972e668f2e`，delivery10份逐hash匹配。
- 权威from-baseline-exact.patch `3cbc956760c0b56749809a3544fd8179ce32dc90516a8335af1fe480432fce6c`；reviewer在真实根目录实际执行git apply --check退出0，未apply/改索引。作者patch-verification另记独立副本check/apply0、190零差异，未冒称reviewer也apply过。
- 对原182基线仍恰22路径、全ACP：14改8增。对v2恰model/runtime_tests/transport/reader/state五路径，逐hunk已读。raw_json基础、Cargo/依赖/lock、SQL/Node/WSS/管理及原独立oracle均未变；v1/v2冻结原字节保留。

绑定64cc/8bb5、empty7f682592及f27b/077a，不增加产品规则或public API。

## 八项同源静态结果

| 稳定ID | v3确认 |
| --- | --- |
| ACP-IMPL-PROMPT-001 | known resource浅查uri:string和text/blob至少一个string，保持anyOf而非exclusive；一支合法另一额外字段错型可保，unknown原片段不丢；无新URI访问/长度/非空限制。 |
| ACP-IMPL-SEEN-001 | State.note_server_request在Closed仍记seen，重复先拒；仅live核64并建立Waiting，不制造Closed回复grant。后续完整帧继续drain。 |
| ACP-IMPL-DEBUG-001 | Response自定义类型脱敏Debug，RemoteError任意code不再由Response/Incoming Debug输出；公开code和原raw可读不变，ToolNamePatch等敏感类型保持掩码。 |
| ACP-IMPL-REJECTED-001 | rejected由Option变VecDeque，route拒绝push_back、take原签名pop_front；valid object原文按FIFO全保，invalid JSON不伪RawMessage；首terminal不覆盖、尾合法完整帧继续处理，无新quota/原文日志。 |
| ACP-IMPL-WRITER-001 | 真实W Err/Flushed仍在原State锁内分别记Unknown+Stop/Written，后Stop不能改写；实际accepted和FrameAccepted同锁，非法fence后置检测与传输事实分开。 |
| ACP-IMPL-READER-001 | State.stop/finish统一wake Core共享Notify，expire直接Stop退出仍会唤醒writer，避免自主terminal等外部shutdown救活。 |
| ACP-IMPL-READER-002 | 真实R Err/EOF原poll短锁内先Stop，无锁外封口窗口；Stop后禁新读写poll，旧已取LF照常drain。 |
| ACP-IMPL-LIFECYCLE-001 | collector正常和shutdown fallback均借Core IO slots逐个实际join，同deadline/cancel-safe；collectorErr/None不假true，drain后才发布terminal。 |

八项当前均仅静态闭合，状态保持Open直到最终独立同源对应测试/证据到齐。原四过程缺陷的旧动态未执行事实仍保留，不把新绿写成旧红。

## 责任、数据与文档边界

已复核scope私有Arc身份按ptr_eq、Strategy同源、Received真实reader构造且不可外部突变；每poll/flush fence、真实read receipt/LF与窗口generation、锁外大parse、Stop排空及borrowed shutdown槽所有权。正常启动两IO和collector登记后才开started。旧四问题修复无回退。

raw浅编码/compact、未知大数/escape、完整有来源setup、Known/Unknown、有序env/header、跨版能力限制沿合同；既有控制额度不扩到正文/new/resume。P1公开scope-first、P2调用同真实入口；开发inner只复用私有生产算法，无fake Received/public兼容入口。旧ViewWindows/anchor/key预算保持非必经遗留，不签完整无限视图。

Rustdoc已区分：RawMessage.parse仅语法/根object，write_json仅小标量编码；Reservation.encode无IO/错误不转责任；enqueue EnqueueError退还原inputs；next队列后terminal错误一次再None。没有再将enqueue所有权承诺错误放到next。

## 独立资产迁移预读

已逐hunk读当前prepared的P0/P1/P2及两个helper差异：36函数数目/原统计、布局、计费、来源、nullable及顺序断言保留；Message/Tool旧长度冲突按CF改，五个Received不可伪造迁移函数仅替换输入构造，未改变其原assert。protected audit文本计数10/10、10/10、3/3、3/3、4/4与所读差异相符。test-only Legacy桥真实spawn、reserve/encode/enqueue/next，P1/P2拿真实Received，不靠伪品牌。

这仍是prepared资产审查，正式最终hash/实际36逐函数日志及6负例尚待冻结交接。负例须先同imports正对照0并核E0308/E0277/私字段等准确span和语义，不能任意失败充数。已读runner与positive/attacks准备，最终诊断另核。

已读owned_pipe/protocol_child，来源仅自有Rust协议child经OwnedProcess unbuffered stdio，真实握手/配置/输入/嵌套审批/取消/IO关闭与Job报告；不能称真实Harness或多层后代树已验。已要求若报告包含标准session/close，须实际发该请求（不能把owner.shutdown当协议close）；最终以实际更新资产/日志为准。

## 原红与剩余门槛

已亲核第四指定1.98.1红：rejected_fifo资产`2bcef84dc9409fbdc9f6fbf95c6c99f86a065f7c062bd4917fc7e74e9432ab80`、日志`a388d01cb5fc0fb00fa3be07fae31c30d250042af51a01770f879367a750636c`。V1分支retained=1应2实际失败，尾合法帧/首terminal先过；未虚称该红继续跑到V2。其余三项正式红及1.99误cwd纠偏已在四项发现报告记录，原文不回填。

最终需指定1.98.1/source cwd完整独立矩阵、八问题对应真实/静态分列证据、6负例/正对照、36迁移、实际owned child与严格工程检查，随后root窄合/冻结新组合并实际默认check/build。严格等时/锁内或parse短缝/OOM/极值不能假动态覆盖。本报告不签必要账本/actual R3接IO、持久审批、原生Run因果、Node/WSS/Web、真实Harness/Job树或完整WP3。
