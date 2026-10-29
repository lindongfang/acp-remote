# R17 管理有限验收：NA027 / NA028 / NA029

**结论：NA027、NA028、NA029均PASS。** 本轮R17真实WSS/SQLite/Chromium补齐G02/G04–G08；G03采用本tester已独立完成的R12满10000/精确24小时联动，SQLITE_FULL采用R7独立结果；受理/入队/writer撤销门采用原reviewer独立关闭RV001/002的定向证据及当前未变生产门路径，不将其写成本轮新栅栏执行。不是只凭源码或开发自评签整例，不扩大成真实Harness/移动端验收。

当前33例统一统计另见 `2026-10-30-current-independent-status.md`，旧各轮累计数字只作为历史记录。本报告不代签reviewer，不修改tasks/plan，不宣布整体可归档。

## 版本与实际资源

- R16准备：冻结83项、0 mismatch，清单FCA2669CFC561CC159C498F19262D9B893A0E590F4FEA8D5EF3169059F74D8E3；准备用binary3EBF8E...6709B。仅用于自有A/B、本机真实Web配对、69→70记录准备，不把该页准备记录签为R17。
- R17最终目标：`.tmp/frozen-implementation-r17-20261030`，manifest自身 **5074890C209A702335F13042A26DBA64278EFD7A7B0A70A4E42E2F698BA3E26F**。独立前后均 **83/83、0 missing/mismatch**；Web dist是单列生成资产，不冒称83原项。
- R17普通嵌入binary **38AA9D02F334E3C81FDC900ED4F8943BB78339650981BC0046A609381D03E6C2**，复制到自有bin后才运行，执行后hash相同。两个R16准备节点先owner stop退出，再切换自有exe，同data启动R17；生产页面重新加载R17嵌入资源。没有运行或锁住主target的exe。
- 自有runtime `.tmp/independent-management-r16-20261030`（名字保留R16准备历史，最终实际R17）；节点分别30451/30452，真实SQLite、DPAPI、owner named pipe、直接TLS/WSS。证书/私钥只读引用先前受信同域材料，没有复制或打印key，没有关闭证书校验。
- Windows真实jack，节点/SQL/owner操作显式require_escalated。自有根在生成运行身份前设置owner-only继承DACL。Node22.20内置node:sqlite为测试观察/夹具，无产品新增依赖。
- 单一ego空间13、p1、Chrome156、secureContext=true；R17真实生产页面，真实IndexedDB不可导出key。没有把Node签名说成Chrome签名，没有接管其他空间。先清理仅本tester30451 origin旧测试IDB，不清profile/cookie或用户30447。
- 原生owner适配器从冻结R16公开IPC API编译，只作本机客户端；普通节点明确为上列R17 binary。主方R17全check143库+4/Web50为可复用工程证据，不说本tester重跑全套。

## NA027：PASS，实际WSS只读/失败/重放关联

| 原有限断言 | 证据及实际结果 |
|---|---|
| 真实list/revoke、B认证拒绝 | A真实WSS撤销B，成功结果；两个已订阅B连接4410，原B私钥签署新挑战重连4401。不是用UI换新key后的拒绝代替原key拒绝。 |
| 同request重放、改载荷、同ID新request | 对D首次撤销成功；原request原载荷结果相同；原request换B返回request_conflict；新request撤销已撤销D仍成功、不复活。 |
| 失败明确且原子 | 自有SQLite临时BEFORE audit INSERT trigger真实RAISE(ABORT)；真实WSS返回storage_unavailable；B status/generation/revokedAt不变，revision/audit/result计数不变；两条B连接仍可info/list。finally移除trigger。不是模拟“失败但已撤销”。 |
| COMMIT失败与磁盘满 | 原独立storage58断言包含延迟外键实际COMMIT失败及语句失败原子回滚；R7由本tester独立运行真实SQLITE_FULL(13)，恢复pager容量同request重试成功。适配器不耗尽宿主磁盘。 |
| 满10000与24小时 | 本testerR12独立执行full_revoke_budget_preserves_results_until_the_exact_clock_deadline：86399秒保留旧结果/新request满额拒绝；86400秒回收一槽，同request可新受理、结果可重放、旧身份不复活。原R12报告记录1 passed/139 filtered；不是直接改原结果expiresAt替代时钟。 |
| 规范化摘要 | 采用封闭v1 clientId字符串载荷；原storage独立probe与reviewer审核确认RFC8785摘要和原子状态/审计/结果，不虚构无穷JSON排列。 |
| 只读不增长 | 本轮A/B真实订阅后10轮info+list共20命令及无效list请求，前后**audit=3、management_commands=0、revision=70**。跨actor/node/篡改/到期游标所有请求后仍相同。认证/订阅和多帧读取不是逐帧写审计/幂等结果。 |

本轮后续产生的9条审计是5次批准＋4次真实撤销事实；幂等结果4条。与20次读取阶段明确分开，不能拿后续写操作增加误判成只读膨胀。查询输出仅固定事件元数据，无code/proof/私钥；节点日志与审计不含本次真实配对码。

## NA028：PASS，两个B订阅、并发命令、自撤销与owner恢复

- A、B1、B2均真实WSS认证并subscribe devices。先执行存储故障撤销，B1/B2仍成功管理，证明后续关闭不是本来不可用。
- 故障移除后，B1并发node.info、B2并发device.list，并读取推送；A成功撤销B。两连接均实际Close **4410/revoked**。这次竞争的两项命令均以连接关闭收敛，**没有要求每连接先成功返回命令结果**。
- 用原B私钥签新鲜挑战，重连Close4401。现存订阅关闭、新身份操作拒绝均实际观察。
- 本轮普通网络时间戳不当作内核COMMIT/入队精确栅栏。提交后不新受理/入队/发送及已排队结果丢弃，复用原reviewer R9 `revoked_actor_cannot_be_admitted_or_enqueue_a_completed_result` 三场景（撤销先于受理、普通完成后撤销先于入队、真实缓存命中后撤销先于入队）和 `revoke_commit_discards_an_already_queued_management_result`。当前生产actor gate/enqueue/writer检查继续相同；R17慢writer仅cfg(test)增加，不改这些正常门。
- 独立A自撤销：**未观察到success result**，但实际Close4410，DB A已revoked、原A proof新连接4401。按原预期允许无成功响应，不据此报假失败。
- owner CLI device list仍exit0；owner再明确批准新独立key，真实WSS认证及list成功，恢复管理入口。没有借助已撤销Web身份恢复。

物理资源关闭和正常清理见末节；不承诺撤回提交前已交网络栈字节，也不把晚到的客户端回调当作新服务端发送。

## NA029：PASS，R17真实Web分页与游标/通知/输出

1. 夹具中A/B真实配对；仅为有限分页容量，在节点停机时SQLite植入67条合成合法公钥记录并更新revision。随后真实生产Web产生自己的不可导出key、实际认领及owner批准，形成**70条**。不是声称70次真实用户点击/批准。
2. R17生产页自动认证，实际点“刷新列表”及两次“载入下一页”。浏览器原生WebSocket实际响应为 **32/32/6**，最后nextCursor=null；ID集合与自有数据库70条**精确相等、无遗漏/重复**。列表项仅clientId/createdAt/displayName/publicKeyFingerprint/revokedAt/status，无完整公钥。
3. 真实WSS同cursor跨另一认证actor、另一独立节点、单字篡改均stale_cursor。原actor实际等待 **59105.95ms成功、60211.54ms stale_cursor**。这是59秒/60秒后真实单调经过时间，包含调度/传输，不冒称恰60000ms或纳秒服务端时钟。
4. 分别真实建立新信任和真实撤销D，旧cursor均stale_cursor；各自从cursor:null首屏重取成功。不是只直接UPDATE revision。
5. 连续三个状态变更：新信任→D撤销→B撤销，revision **70→71→72→73**。A订阅无需第四次唤醒即收到最终73，主动list返回73。允许合并中间值，本轮恰观察到全部；每帧**仅v/type/revision**，没有全量列表推送。
6. 后续自撤销与owner恢复到75；生产Chromium实际通知序列70/71/72/73/74/75，最终75，页面仍认证。不是仅Node客户端看到最新revision。取回时失效身份检查由两条B关闭及原key4401、原reviewer内部资格门证据共同覆盖。
7. 本轮成功/错误/撤销输出有限检查：审计仅固定事件字段，实际只包含device.pair.approve/device.revoke事实；成功幂等结果仅clientId/status，digest为64位hex。错误是封闭invalid_request/request_conflict/stale_cursor/storage_unavailable。
8. 实际生产index headers **Cache-Control:no-store、Referrer-Policy:no-referrer、CSP default-src 'self'等**；公开静态HTML没有本轮客户端ID。资源性能记录第三方请求0。测试码不在URL/localStorage/sessionStorage/最终身份记录；真实IndexedDB key extractable=false、pendingClaim=null，不残留pending proof。

首次订阅可能丢revision的RV002，采用其原reviewer真实SQL读r=1栅栏→另一真实批准publish2→释放后收到1和2、主动list2的独立闭环；本轮补普通R17真实连续三变更及Web主动取回，不声称本轮再次插桩SQL窗口。

## 实际入口、退出与夹具错误

专属源码 `testing/probes/establish-node-access-management-independent-20261030` 及相邻 `establish-node-access-management-native-20261030`，材料/target全部.tmp。成功入口：setup native版、upgrade-r17、browser-page-through、readonly-cursors、revocations、audit-output-final、browser-final-observe、browser-cleanup、stop/锁核验，全部实际exit0；语法检查及范围git diff --check exit0。另 `npx.cmd --no-install openspec validate establish-node-access --strict` exit0。

已保留失败事实，均未作为产品缺陷或通过证据：

- icacls初次原SID漏星号，exit1332；加原生SID格式后保护成功，才生成运行身份。
- 自写Node owner pipe的requestId/命令前置和连续第二会话读取超时，失败setup进程经命令行核对仅终止自己，节点owner stop。随后改用产品公开Rust IPC适配器，成功完成全部批准。没有用该自写传输失败冒充产品拒绝。
- CDP新文档WS观察未持久，页内观察undefined；改在同一页安装脱敏观察器并以明确pagehide/pageshow事件触发生产重连。不是实际BFcache验收，不为超时新建空间。
- 原夹具误以为Web自动读全部分页，等待20秒超时；检查实际按钮后按真实“载入下一页”完成，不改产品或断言。
- 审计白名单最初误写device.approve，exit1；核对真实schema固定动作device.pair.approve后通过。该错误没有修改审计数据。

## 任务2.5限定判断

独立读取当前tasks2.5原判据NA011/015–017：**证据足够，可由主方勾完成**，不等待全33例。NA011本tester失败修复重试；NA016本tester默认15秒预算（清理14秒＋IPC预留1秒）真实Storage.close阻塞诚实失败；NA015独立E2E未完成pair/多订阅/实际在途事务stop；NA017独立E2E同data A有效B撤销保持、三类损坏副本不替换/清库，DPAPI历史仅按未变平台子路径复用。R14 reviewer真实254 native实例背压等待可取消和32/33应用限额、预算tester最终binary正常stop资源恢复补足新增停止路径。NA015曾有在途事务提交但socket4500/结果未交付，原报告保留，不额外发明所有连接必须1001/success的判据。

## 清理及证据

两R17节点均正常owner stop，精确PID已退出。自有pipe最初Node fs.exists路径检查不能当存在正控制，因此未采纳该字段；末轮改用产品LocalIpcAddress API精确推导实际名称，PowerShell限定两确切名称检查消失，.node.lock逐一独占打开成功。规范化绝对路径限本轮root后删除两个自有data/DPAPI身份/SQLite/配置；临时配对码及批准flag删除。借用TLS原key/cert未修改或删除。

30451/30452无监听；没有对30447发任何命令。最终只读30447也无listener，本轮没有据此声称用户节点一直运行或归因其停止。空间13只清本tester30451 origin IDB后finish，回执closedSpace=true、closedManagedLabels=[p1]、preservedUnmanagedCount=0；没有清日常profile。

| 原始证据（runtime根中） | SHA256 |
|---|---|
| browser-paging.json | 755C9C4008800821FAAD1EA16DBB97195F5E06A9665FACA3A9B863FA61B3F02A |
| readonly-cursors.json | 16DB011023374656D2FB40F57D9F3B6F785D2E23AEF64D6E526A5D8D5157DCDC |
| revocations-final.json | 3D244E79A8F9A4E71BD32F3AC03EDFACD902795CA2EAD373784D9571B80DE35A |
| browser-final.json | 3FC515C6FF29B687E999AC1AF44ADD0D1B10E91534284D406FD363D2DB330F3C |
| audit-output.json | ABDE31FFC1316B294AC30C393CA4574650F268557F517E10F970DE9996AA73E2 |
| cleanup-native.json | 609FBF8DE299BE1D717ADDC116B1EF72DA201B8234B81A7A00C11183E96B0CE8 |

无产品代码/规格/清单/plan/tasks修改，无提交、推送、归档。
