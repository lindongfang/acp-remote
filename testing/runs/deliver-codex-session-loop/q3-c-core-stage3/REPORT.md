# C 核心 stage3：两项 P2 有限修复候选

本包基于固定 stage2 的完整 795 源文件，只修 C-STAGE2-REV001、REV002。两个实际失败回归已转绿；全 target/全 feature 编译与格式检查通过。原 reviewer 尚未对本版复查，两问题仍待其关闭；本包不声称独立产品验收通过。

## 输入与隔离

- stage2 freeze：1c66b8e03ffc61985d63e786c91ccadbf673743877fdb29dec8991d908d4406a；原 826 成员全部核验后复制，交付前再次核验，无漂移。
- reviewer freeze：ada0021c9fe3163b04d3d64dda53fee2aeabdec1783f6d1130c4a21779e2348c；3 成员全部核验。inputs/REVIEW.md 是本次实际修复依据，原 v3 A–E 编码门继续有效。
- 工作仅在 workspace-stage3、candidate-stage3、logs-stage3 及自有重建目录。Root、stage2、tester 写入均为 0。仍基于 stage2 的90de781，不混入 A 的另一组合。

## REV001：已结算返回先核原请求链与完整预期 effect

最终写事务读到已结算记录时，现在调用 Evidence::settled_fact。该方法先逐项比较原 request、actor、register_generation、action、registered_at、intent_digest，并拒绝 Registered；不会把会合法改变的 state、cipher、admission、结果列整行对照。

合法 rejected 通过 Command::from_row 原有闭合形态校验后可只读返回。合法 committed 先走原 fact/Profile/Binding 必要关系校验，再对照本次解密 intent 导出的 Keys：创建动作的 harness/source/profile、原创建请求、payload format、digest 和原 cipher 字节均核同；绑定动作核 target/workspace/harness/source/profile、launch_path、canonical 和有效 selection 关系。既有 Profile::load 和 Command::fact 继续负责 format、创建/选择请求、引用与 digest 关系；没有略过这些 SQL 校验。

最终已结算分支不强求 admission/time 等于当前尝试的 generation/at；开发正例特意给不同值。恢复 Evidence::verify 复用相同关联与 effect 校验，同时保留恢复对本次提交 generation/time 的额外要求。仍 Registered 的逐字段完整原记录相等检查保持原样。

开发回归包括：实际 settled_fact 的合法 rejected 正例及6项原登记字段损坏负例；实际 Keys matcher 对三种结果的正例和异 harness/source/profile/revision/target/workspace/结果种类负例。后者使用纯事实对象，只验证调用链中真实 matcher，不能记成 schema9 committed SQL 正/负例。完整 committed mutation 仍由获准后的 F 独立用例执行。

## REV002：普通 gate 等待受原 scope/deadline 取消

CatalogSqlFence::enter 新增私有 scope 参数，只将真实 serial.lock_owned 等待置于 d.run(scope.while_open(...))。管理 start 传已核同的 actor 原 scope，两个配置 port 的共用 selected 路径传同 registry 原 scope。未决操作的 CatalogFailure 在获得 gate 后仍按原路径完整返回，没有被 while_open 转成无观察的普通错误。

保持原 gate、原 deadline、原 operation lease 和持 gate 操作的同步保护/解密责任。显式 recovery_guard 完全未改变，seal 后仍可等待并确认原 receipt cleanup，不把恢复职责一刀切取消。

新增 cfg(test) GateWaitObservation 是锁等待位置的只读 AtomicUsize 观察；没有控制器、暂停/放行接口、ready setter 或 F hook。回归使用同一真实 registry/fence，先持原 gate 和 lease，轮流使 Register/Apply/Query/Resolve/ResolveRead 进入真实锁等待，核 waiting==1 且 serial 仍被占用；随后 seal，要求250ms内 NodeStopping。期间不释放原 gate；核同 retained 仍为原 operation、SQL slots 为空。再独立释放持 gate 方原 lease，在 gate 仍持有时观察真实 close_connections 完成，证明 waiter 已释放自己的 lease。此测试覆盖共享入口 seam，不冒充完整授权管理/双 port/schema9 调用验收。

## 实际验证

| 项目 | 结果 |
| --- | --- |
| 修复前红灯 | 保留 stage2 行为的机械提取/观察副本上，17 pass、2 fail；分别为登记字段损坏被接受、seal 后 gate 等待超时 |
| 修复后开发测试 | 21 pass、0 fail、665 filtered；原18项加3项本次回归 |
| cargo check --all-targets --all-features | offline、locked，退出0 |
| cargo fmt --all --check | 退出0 |
| strict clippy | 退出101；stage2 381条，本版382条；按代码/消息/主文件/原文多重集对照，新增3、移除2均 dead_code，新增其他类别0 |
| 增量补丁 | 实际 apply --check、apply 均0；重建后全部795文件逐字节一致 |

strict 仍未通过，私有 C 核心未接默认 D 工厂；没有 allow、公开性扩张或虚假生产调用。编译测试中曾出现的测试 import/borrow 错误已修正；最终结果仅以上述 final 日志及 exit-codes-final.json 为准，未把中间失败记成通过。

相对 stage2 共7路径修改，其中5个为两处实现及其测试，另外 adapters/acp/mod.rs 和 session_storage/mod.rs 仅是 cargo fmt 要求的模块顺序调整。没有改 schema、migration、原 owner 算法、D 构造/读接口或依赖。schema9业务CRUD、真实DPAPI、完整committed mutation、正式原34+补充12、F安装、默认D工厂均未执行。

## 包结构与复现

source/ 是795源完整候选；before/ 是7个原文件；changes-from-stage2.patch 只适用于固定stage2 source。应用时使用 git -c core.longpaths=true -c core.autocrlf=false apply，避免本机换行/长路径改变字节。source-manifest.json、changes.json、patch-verification.json 记录精确输出和实际重建；input-manifest.json、inputs/、input-integrity.json 记录精确输入；validation.json、strict-summary.json、logs/ 记录真实执行范围和红绿证据。freeze.json 覆盖所有交付成员，不含其自身。
