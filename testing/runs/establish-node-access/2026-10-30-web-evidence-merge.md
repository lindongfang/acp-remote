# 正式嵌入Web独立证据合并：NA013/026/031/032

本报告是独立证据逐断言合并，不冒称本tester重新运行其他tester的浏览器。本次核读 `2026-10-30-r6-e2e-acceptance.md` 最新正式R16/R17及R18追加、原始汇总JSON、存储独立报告、原本tester r2/r3真实页面协调报告、原reviewer独立动态报告和当前未变管理模块测试。

**NA013、NA026、NA031、NA032按当前有限Steps/Expected均PASS。NA007明确FAIL，NA007-CANCEL-001/P2/Open等待原tester R20复查，NA012仍PARTIAL。** 更新后的当前统一计数为31 PASS / 1 PARTIAL / 1 FAIL。不要求用户重复技术验收，不增添真实硬件断电、每个浏览器所有私密模式、所有BFcache组合或每个并发命令成功等门槛。原报告内过时Open/等待R15段落不覆盖最新实测追加。

## NA013 / NA032正式包证据

原独立E2E tester正式R16/R17前后核对83项、exe副本摘要及实际返回index/JS/CSS字节在exe中，R17三资源也对应冻结dist。原SC003问题不再阻止这些**新的**正式包记录；不倒称旧r9debug网页已被exe固定。

- NA013：同data、相同canonicalOrigin、同一真实A profile/key/clientId/nodeId；listen先127.0.0.1再100.119.123.57。仅自有Edge进程域映射/禁代理，不改系统代理；securityState secure，Network remoteIPAddress分别确为loopback/Tailscale地址。两路径未认证info/list/subscribe均4400无管理交付，可信原A三操作成功且自动认证。没有把URL/DNS含Tailscale当成实际路径，也没有用最初代理失败或吊销查询失败代签。
- NA032：R16正式单包真实Windows、SQLite、DPAPI、owner named pipe、WebCrypto/IDB、受信TLS及真实Tailscale地址，两个独立普通profile真实TTY配对/明确y批准；A观察B新增revision，A撤B，两个B页面4410、原B key新证明4401；同data重启A有效B拒绝、owner正常关闭。R17保存/双页旋转、R18旧原生4410晚到已consumed replacement分别补受影响实际路径，其余未变后端机制复用。
- 资源清理在原报告latest-cleanup中逐个匹配PID/port/pipe/lock；本次不接管它已关闭的profile。未把同主机真实Tailscale地址连接扩成另一机器/VPN远端隧道验收要求。

## NA026真实批准失败联合链

R17 ordinary binary38AA9D...3E6C2上，自有SQLite仅对device.pair.approve审计插入设置RAISE(ABORT) trigger。真实Web认领、真实TTY核对后y，CLI退出1/storage_unavailable；clients/audit/revision与故障前相同，无半信任。页面cancelled/clientId=null且未AUTHENTICATED；该失败key正确签新鲜auth proof仍4401。

删除自有trigger后同browser key重新配对/TTY批准，获得稳定ID、实际info/list/subscribe；刷新/同data重启保持ID/key并认证。另用仍新鲜且刚在pair/status真实200验证过的配对status proof调用长期auth，4401，不拿已过期claim的失败冒充跨用途隔离。以上正/负对照及回滚/恢复联合填补本表原NA026缺口，不代签NA007提示失败。

## NA031有限断言合并表

| 当前原断言 | 独立已执行证据与边界 |
|---|---|
| 不可导出CryptoKey、真实IDB读回签验后认领 | browser-storage实际浏览器正常/私密读回签验及实际认领；正式E2E实际生产页配对/新WSS持有证明；本testerR17页privateKey.extractable=false、pendingClaim=null。 |
| 写失败不认领、不假成功 | 原storage tester实际初始化put失败、健康lookup后put失败不发claim、私密模式写失败阻断；当前存储异常路径及Web Locks/stale保存新增分别有review和实际双页补验，不把后来新增gate当成旧put故障已在新版重跑。 |
| 私密模式行为及完整退出 | storage追加r6真实InPrivate完整配对/认证、关闭该profile全部进程后重新进入DB不存在；临时存储行为明确，不宣称识别所有私密模式。 |
| 已有坏key不静默新key | 原storage实际已有坏记录导致失败，无替换；新Vault仍严格load/parse/自检，新增锁与current-key比较未放开坏record自动替换。 |
| 刷新、普通profile全部进程退出再打开原key/ID | 正式R16 A普通profile8→0、同目录在TS重开，原key/clientId/nodeId不变，真实认证/三操作成功；R17再次全关后重开通过。不是InPrivate丢库或仅关tab。 |
| 清站点/丢失提示及新origin | storage tester清真实可信store后新unbound身份，条件性“先本机撤销旧设备”提示实际复测；NA-STORAGE-001已由其关闭。新HTTPS origin实际无迁移私钥/信任。原复查不要求浏览器识别已完全丢失的旧ID。 |
| 同origin自动原身份复用、临时断线及节点恢复 | 原本tester真实页面停止/重启自动认证；正式E2E正常重启/同profile重开和身份连续补充；本testerR17受控pagehide/pageshow调用真实生产恢复链认证且实际分页。E2E那次组合stop10秒未观察到热重连仍保留为未观察，不用它覆盖已有成功正常重连记录。 |
| 4401/4410终止旧身份并新key再次配对仍需批准 | 原本tester实际4410/4401→新key/真实再次批准；正式R16/R17两页4410后仅delete1/generate1、同新key、两页真实签验通过。NA-E2E-001由原tester正式包原路径关闭，不再沿用早期Open。 |
| 晚到旧拒绝遇另一页新已消费身份 | R18真实已验证连接原生isTrusted CloseEvent暂存，另一页真实新配对/TTY批准且consumed后释放原回调；旧页自动读新key/ID认证，无第三key，无二次删除新身份。原tester实测并保持问题Closed；不伪称pending/approved所有状态穷举。 |
| 节点不匹配保留错误/绑定、不签proof、不循环 | claim-origin独立真实TLS既有pin替代节点key/nodeId/origin/坏hostProof，签名与proof0、原记录不变；本tester r2/r3未验证close及终止恢复真实页面受控事件、原reviewer页面协调9场景与当前协议错误优先级证据。 |
| 主动断开/离开页面取消重连和认证 | 原本tester r2真实IDB延迟、合成pagehide/pageshow，隐藏后释放无新连接/UI覆盖、恢复单流程；r3限流/终止恢复调度；原reviewer实际main受控DOM/事件/clock延迟promise覆盖manual/生命周期。明确不是实际浏览器BFcache全部导航实验。 |
| 结果未知命令不自动重发 | 原reviewer实际ManagementConnection签名认证后fake socket/timer使node.info无结果超时，只发1个command；恢复入口无原请求重放，仅新认证/读取/订阅。本tester本次核对client.ts与reconnect.ts和冻结R19相同字节，再独立运行同一现有测试1 passed/14 skipped，真实WebCrypto＋fake socket/timer，未冒称真实device.revoke物理丢包。原用例要求一般命令不重发，未额外强制所有action分别制造网络丢包。 |

由以上原用例所有有限部分合并判NA031 PASS；不是只把NA-E2E-001一个问题Closed就升级整例，也不是将侧tester“本包不代整例签”误读成还存在未定义的新必需用例。存储早期浏览器记录作为具体已观测平台/故障机制子项保留，**完整退出后当前固定包自动认证**已由新的正式R16/R17链补足，旧debug资源不固定问题没有被隐去。

## 本次实际补验与可追溯快照

本tester执行 `npm.cmd --prefix clients/web run test -- tests/management.test.ts -t request.timeout`：exit0，1 passed/14 skipped，200ms。执行前核对live与冻结R19管理client/reconnect分别相同：

- client.ts：9F06BDF7D97599C1205050431A0CF967465FDB810CD8D41DA7015E5489F44246。
- reconnect.ts：33644C4EA9DD9160B709D41C67BD8BC6274B27F9AA7F57C0C372356E8FB6D230。

仅此定向测试为本次新增执行，无重新运行节点或浏览器，无自有持续进程。主方R20完整144+4/Web50及新ordinary binary为交接信息，本tester未执行，不用它关闭NA007。

| 核读证据 | SHA256 |
|---|---|
| r6-e2e-acceptance.md最新正式链追加 | F5DAE2EAA0041D2574F9ED45B932367BB29FFA8EDF8525661C8EF3E0389E17AF |
| browser-storage-acceptance.md（含私密与原问题关闭） | BAF2744599735B07898950395BFCDBCF5EDE2EBD6B62735DD3903E0BBD789AD6 |
| approval-cancel-boundary-acceptance.md（R17 FAIL） | ED608964CF78CB61E8FC6C7C314F7FA00F9EA47943EFEEFDDEB5A44241E85358 |
| r17/evidence/latest-summary.json | 78DA0B509AFEE55E14DCA1A7F97B1818A571672D9FC13D474F5F0DB1EB81BAD4 |
| r18/evidence/late-summary.json | DB9777FE78AE52E6DCBA35930F397B09ADF29B8EA23F15A4DEB29D4977BCB900 |
| r17/evidence/NA026-summary.json | 40A7EF04EE62194103EBA72D375CB3D751B89E41A86C8CA9EEAAE858B6C5075D |
| 本次unknown-result-client.log | 7BA30DA7215B81C115FBE257BDB4BC9D42402CCA3A50666CD61079C877F8EB0E |

## NA007实际FAIL，不以修复声明替代复查

原side tester NA007-CANCEL-001/P2/Open：真实R17 TTY approve已转发并COMMIT，代理持住真实approved事件不交付；独立DB trusted＋批准审计已存在，再真实Ctrl-C。CLI实际退出0xC000013A，没有device revoke提示；信任保留、原浏览器null-ID重启恢复成功。前置Ctrl-C正控制及decision未转发时取消对照均已执行，不是事件未送达或只看源码推断。

NA007当前应标**FAIL／待原提出tester R20复查**，不是“缺一个提示未执行”的PARTIAL，也不因其他存储/恢复PASS抵销。R20 ordinary binary561A6024...AAFBE3、manifest73727BCF...B368A4为用户刚交接、原tester/reviewer正在复查，本次不提前签关闭。

## component判据

- tasks3.2：原NA024/025/031、不可导出存储/身份丢失反馈与真实完整profile退出已满足，**可完成**，不冒称检测全部私密模式。
- tasks4.1：NA001/002/015/032以及正式同包资源/实际集成检查已满足，**可完成**；CLI独立R20修复不否定未改Web/后端的正式包集成链。
- tasks2.4/2.5此前已满足，保持。
- 独立全例/归档门仍被NA007 FAIL及NA012跨SID剩余阻断，不能因component完成宣布全change通过。

只新增此合并记录及更新本tester统一表，不改产品/旧报告/规格/清单/tasks/plan，不提交、推送或归档。
