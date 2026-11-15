# R1 固定源独立验收后验结论

2026-11-13，独立 reviewer。**RF04关闭；R1-01至R1-11在以下固定源的有限范围通过独立验收。** RF01/02/03先前关闭仅针对开发测试finding；本次独立tester确实执行当前源的Red1及Green34后，R1动态前置条件现已满足。既有R2 Source有限写入门与Root独占Doc窄切片门可按各自边界生效，不需另等同一授权。此结论不签R2/R3、真实Harness、完整WP4或未来源码。

## 固定身份及实际后验

- 产品516叶canonical SHA256：`b9de84e397f31d3f7882c807b07ef28b47fe4774f93c6819cacce035498951aa`；HEAD `65ec7815bb0280f4519989d93a7b4ed3b4b37994`仅辅助索引；Git index `ac76c7ad84c1d4333fc64a710647e60aeb96015eb4407f996a4bf6adc91fbff4`。reviewer在最终动态后独立逐核Root/copy全部516叶，零额外差异，私有module/overlay保持门内SHA，index未变。
- 实际执行程序：private `26efc8e8e7a06e01b2933d659f08a0928813ed4f2992d9cabfd2ba1e149543de`，runner `3c2fee4017e09a2e3565f79ef5a489cf7f87bf6cd888f29e79e6436a7361608a`，wrapper `b321060b54828439c1776d75c9e6738d928ac8ddaba1eec1a28191302b81027f`，与最终动态门一致。
- Red实际PE SHA `fdb6b935fc448acf2bc44c5df12504f185b6e3d88655c4761344be3e1a3dbf70`于Green覆盖同target路径前已独立实核，绑定诊断原Cargo唯一artifact及精确两叶反转；实际fresh选择器恰1 failed，明确left10/right11。list与Red原Job均自然归零、Stop=false；finally恢复516叶。没有再次重编Red。
- Green本次Cargo artifact的PE SHA `01ae2538961e77ea03201b612a87d1e408251edd30035b7e737f9d4f649eec04`已独立读取实际PE核对。实际list 950项，选34（release19含私有4、catalog8、schema7）；reviewer逐项读取原stdout/stderr和supervisor/result，全部恰1 passed、0 failed、0 ignored，完整限定名一致，原Job均natural=true、Stop=false、active0、non-timeout。没有零选择器、过滤简称或历史通过替代。
- Green唯一编译Cargo0/non-timeout，原Job首尾只有同PID52964/vctip/HasExited=false，supervisor125、natural=false、Stop=true，owned cleanup active0；依最终门单列为编译helper监督异常，原真值保留。该异常不用于冒称产品动态自然退出；所有实际list/test均无例外自然空。

## R1判据与有限证据

| 判据 | 本次实际Green编号 | 结论依据 |
| --- | --- | --- |
| R1-01 fresh | 04、13、23 | 真实文件fresh pragma11、metadata实际0行、Source四对象；合法身份后双11。没有伪造无身份metadata11行。 |
| R1-02 exact9 | 06、07 | 精确9与release三回滚点、原身份/事实/布局及Source零残留。 |
| R1-03 exact10 | 08 | 原10逐表逐行事实和release/security/approval数据/索引保留，只增Source对象与预期版本头。 |
| R1-04 reopen | 01、03、04 | 原Storage/raw/preflight关闭join与独占probe后新Storage重开，全量快照一致；Storage边界无RPC producer。 |
| R1-05 DDL故障 | 10 | 四DDL前后8点，MigrationInterrupted，精确10布局/头/事实回滚。 |
| R1-06 head/validation故障 | 10 | 另5点，双头与旧事实精确恢复。 |
| R1-07 结构/坏引用 | 12、14—17 | 已固定有限namespace/partial布局/逐FK形状/坏引用，拒绝且不修复原数据。 |
| R1-08 deferred COMMIT | 11 | 真实SQLite FK787、全事务回滚至精确10。 |
| R1-09 CommitUnknown | 01、05、09 | 实际提交11、报告丢失；真实文件raw确认后新Storage重开。 |
| R1-10 identity/release | 01—03、08、13、18、19 | 无身份10/11合法空、Source非空拒绝、冲突双身份拒绝且全快照不变、旧release全事实保留。 |
| R1-11 exact8/9/10/11 | 04、06—08、22 | synthetic8真实metadata及双8、各fault零Source；精确旧版与最新重开分开，受影响三组全部重跑。 |

reviewer独立比较四私有用例的JSON事实：fresh身份前后/new Storage全量一致；CommitUnknown从60表到62表、18行旧事实保持，旧表每列每行仅node_meta.schema_version由10变11，旧sqlite_schema条目全部保留，提交后/新Storage快照完全一致；无身份exact10升11并reopen metadata始终0行；冲突身份 `(1,11),(2,10)` 被拒绝后完整快照完全一致。36个本次Green事件、13个share_mode(0)独占探针、4个自然cleanup均实核；自有data为空、runner/target两锁释放。

机器后验为本目录 `REVIEW-R1-ACCEPTANCE.json`，SHA256 `e666863fae661a57e8caa24552cc3d8c5906349c2a7ce9c29340051f52ace112`：含34项实际限定名/结果、11判据映射及190个所读证据文件SHA；所有EVIDENCE-SHA256引用实际哈希重核通过。其41个历史+当前事件中，只有36个final-green事件计入上述Green文件事实，不把历史失败事件计通过。

tester原报告 `2026-11-13-r1-file-acceptance/REPORT.md` SHA `f7c7a52cd07e90b6db4d92c0d942279c878a1e634964bb16dc30474d404cb2e1`，RESULT SHA `aa33d049d38566ab0511f6c43e1f60b9e098102abe87538405fa4ac8ba490faf`。旧fixture失败、v2/v3编译监督受阻和单次诊断原证据保留，不改签其历史结论。reviewer未构建、list、动态运行或改产品；验收执行由独立tester完成，本结论来自原证据后验。

R2最新方案绑定 `49d2e026f840fa98530d528703291a89de052f8e6ff2d2fc887d65adfbfb9c18`；Source执行者须先固定该已验R1基线再按 `R2-SOURCE-WRITE-GATE.md` 写有限V01/V02/V03，Root按 `R2-DOC-SLICE-GATE.md` 独占Doc叶。不得改六个R1叶或共享tester matrix target，不将本次R1通过冒称Source producer或M01…M07完成。
