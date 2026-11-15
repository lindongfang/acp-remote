# R2 有限接线增量检视

2026-11-13，独立 reviewer。被审 HEAD `65ec7815bb0280f4519989d93a7b4ed3b4b37994` 加实际读取工作区叶，读取身份见 `SOURCE-R2-WIRING-READ.json`；所读22个产品叶均与 R1 516叶冻结清单匹配。Git index SHA256 `ac76c7ad84c1d4333fc64a710647e60aeb96015eb4407f996a4bf6adc91fbff4`。未构建、list、执行测试、复制工作区或改产品。

## 当前结论

R2 定义及有限 V01/V02/V03 首批方向接受；`DECISIONS-v1.md` 修订v3（SHA256 `ccdb765caf94b0ac953f3c9064bc83354eedfd82a72bca1c72a29b24a524e567`）仍有下列两项具体接线阻断，**当前未放行产品写集**。M01…M07 保留为后续必需合同，不要求新增全排列；V01…V03 不能代签 R2 完整交付。

## 已可固定的最小选择

1. **单 document owner**：`DocumentShutdownSlot::install` 明确拒绝第二次登记，`Storage::register_document_shutdown` 直接转入该槽，故 sibling第二attach不可行。接受一个 `DocumentDisposalOwner<NodeDocumentPayload>`（Queued/Source枚举，`RetainedRaw` 委派原bytes），维持单Storage receipt。Queue仅把 `QueuedBodySnapshot<B,P=B>` 的原 `RuntimeCore<B>`/`OwnerCell<B>` 与 `DocumentPayloadSlot<P>` 拆开，`snapshot_queued_body<P: RetainedRaw+From<B>>` 在原Queue受权边界内部clone后into；不开放任意外部mapper或伪Queue source。必要写点为 `queue_owner/service.rs`、`queue_owner/document_read.rs`、`document_source.rs` 私有sealed实现与 `node/document_transfer.rs`，原Queue authority/epoch/dispatch语义不变。Root已明确采纳此项，不走composite/多登记。
2. **SQL原对象生命周期**：schedule前安装Node-owned invocation/operation identity、typed receipt容器、可join原task，再启动SQL；worker取得真实connection后同步记原owner。`OriginalSqlCloseReceipt` 只能由实际 `RegistryConnection::begin_owned_close()` 返回的waiter产生，不能在schedule前伪造。接受先在同entry同步保存 `waiter.receipt()`、再await原报告的顺序；取消/Drop亦须像当前 `CatalogConnection::handoff` 一样保存原close对象。原task、事务knowledge、原close、worker/app join事实分开，query不修复，reconcile先join/confirm；不复用当前 `SqliteNativeDispatch::finish` 的单一错误映射冒充完整SQL inventory。
3. **CPU关闭基础方向**：transport专属原slot与Installed gate、注册和seal同锁，reader/writer join后封住注册再join CPU，只有原JoinHandle真正Joined才reap，方向可用。必须留真实identity/start/join小事实，不能永久保每批handle/大buffer，也不能借document Disposing identity伪称CPU。完整input reserve/extend/framing/frame复制/drain与大对象析构须纳入CPU owner；EOF/stop/cancel/panic不能把input root最终drop回reader。

## 剩余有限阻断

### R2R01：CPU顺序等待与M06接收进展冲突（阻断）

位置：被审v3第1节reader等待本批blocking完成后才再poll；`TEST-DELTA.md::R2-M06` 要求真实decode/Vec-growth屏障持住时，向原接收链新送独立控制/响应并观察进入。当前 `transport/reader.rs` 的parse/route/`Strategy::session_listing`在同一链；仅搬到blocking再让reader等待，不能证明该M06进展，屏障会阻挡后续新字节capture/路由。

最小闭环：coder/tester固定“进入”实际事件与唯一接收链。不得拿屏障前已入队的旧事件代替屏障后新送控制。可将有序framing/raw route的有限CPU工作与Source长typed decode/Vec-growth工作拆开，后者原task先登记，由原response owner持有并独立join，reader不等待它才继续读取/路由；原raw/cut/scope/RPC/kind关联和Source最终outcome仍等其确认。若采用另一有界结构，须说明单receiver、顺序与原close/CPU/retire纳管。只需修这一事件合同与接线，不扩M06矩阵。

### R2R02：source级配置入口仍是假设（阻断）

位置：被审v3第1节把无target配置解析写成“若不能就是阻断”；当前 `ConfigurationPort` 仅 `AcceptedTargetProof`，`ReadConfigurationPort` 仅 `ReadTargetProof`，`SqliteNodeConfiguration::SELECT_CONFIGURATION` 还强依赖target selection/native ID。不能以source slot提取已定义替代这条实际缺失入口。

最小闭环：固定source级、当前actor/registry/config revision关联的真实配置proof与解析API；在原 `SqliteNodeConfiguration` 的新窄adapter复用原profile保护上下文/完整配置decode，按合法source/harness/profile/workspace/launch_path读取，不构造target/native ID/ReadTargetProof。`ensure_source_slot(SourceInitializationPlan)` 可窄提取当前source共用/cleanup/metadata closer/view/approval安装；source初始化使用该真实proof，target路径仍保原current/scope校验。补列 `source_ports.rs`、configuration adapter与必要mod声明的有限写叶，不静默扩大产品包。Root已同意这个source-slot方向，具体proof/API由coder落报告后再核。

## 剩余判据与限制

以上两项落到同一修订报告后，只做该有限差异再审即可决定首批写集；无须重开Source require_empty端到端矩阵。实际实现需在源码/测试/PE固定后做Red和三条真实纵向Green，随后由独立tester逐步执行既定M01…M07。此处是方案审查，不签未来实现或历史测试。RF04文件验收首轮Red已有实际版本10≠11失败，但Green首例在fixture读不到node_meta，尚未进入迁移断言，RF04仍Open；tester最小fixture修正必须重新绑程序SHA审门。

## 最终方案增量结论

已读方案最终实际SHA `116f5a082b39fc45a0a29a97fdab8580a1e6ffae686da1731dff0a6011159ec2`（实际标题仍v5，口头修订号不作身份）。R2R01/R2R02均限定关闭方案finding：长typed job与原raw/framing链拆开、原Source.jobs先安装后ACK且application-defined typed port不反依赖ACP；source级当前actor/registry/profile proof及原配置adapter已固定。Root明确选定opt-in新增SessionsDeferred，通用默认不变；Doc单owner窄切片Root独占。书面有限实施门见 `R2-SOURCE-WRITE-GATE.md`，SHA `3eb420a4ba4e42b4bb70d305fde3e50e8407ed1e9c7ac846ddfbbbc30f1e301f`；R1独立验收完成后可实施该写集。本节取代前文“当前未放行”的时间状态，不代签实现/测试/完整R2。
