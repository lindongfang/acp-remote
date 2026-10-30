# WP2 task2.2 历史保存候选独立代码检视

日期：2026-10-30。**结论：BLOCKED，存在高严重性调度竞态 HIST-R001；修复及原提出reviewer复核前不得合入此冻结候选。** 本报告是静态/代码检视，没有运行本候选测试，不代签完整Web/L5/CL-030、真实Harness停止或Windows Job Object，不勾task2.2。仅写本报告，没有修改产品、开发测试或规划。

## 固定范围与独立性

工作区 `C:/Users/jack/.codex/worktrees/codex-session-wp2/acp-remote`，基线HEAD按manifest为65b46c01d6b95fc1f1a8223b1c92a729a885c185。先实际读取manifest并重算SHA256 **C1BB66052A63846ECC67F8595AA5126A8081D6BE269C9F08BDCBF2B306E39A72**，逐项读取13文件并实际重算 **13/13匹配**；其中lib.rs与session_storage/tests.rs沿用2.1，其余11项含开发README变动。审查未参与编码、不继承coder实现对话，与独立tester分开。

行为依据采用D主目录当前AGENTS/角色、spec/design/session-contract/tests，不用隔离工作区旧规划作权威。主执行正在单独纠正pageSize合同；本次只判断2.2保存/缓存/位置/任务，不把未冻结分页修订签成通过，也不要求2.2提前实现03完整分页。

## HIST-R001 — 高 — OPEN：后台worker检查状态后才登记通知，可能丢唤醒并延迟保存60秒

**位置：** app/src/application/history.rs:1153–1181，特别1164 `worker.schedule(&active)`、1180 `worker.core.changed.notified()`；生产者在387/410以及其他路径调用`notify_waiters()`。对应保存合同§6的500ms或累计65536字节先到、达到高水位立即刷新要求。

**确定交错：**

1. HistoryTask在空闲、没有dirty内容时计算schedule，958处默认deadline=now+60000，得到60秒sleep。
2. 另一runtime线程在schedule释放state锁之后、select内notified创建/登记之前完成begin_item或update_item；生产者写入dirty及first_dirty，并调用notify_waiters。
3. 此时worker还没有该轮Notified等待者。notify_waiters不会给未来等待者保留一个permit，通知丢失。
4. worker进入select，挂在刚创建的notified及旧60秒sleep。若没有额外输出/其他通知，即使500ms已过（或单次已达65536字节），也可能等到60秒才第一次提交。

这不是仅“调度延迟若干毫秒”，而是协议规定的保存触发被旧空闲时限遮蔽。tokio多线程下该交错无须await边界也可发生。现有owned_worker测试在start前已经enqueue，schedule直接看到dirty，未覆盖检查/登记间隙；全部已有测试通过不能排除这个竞态。flush自身660起已先创建、pin并enable Notified再读状态，说明项目已有可用的无丢通知模式，worker没有采用。

**最小修复：** 在每轮worker循环先创建并启用Notified等待者，再进行check_deadlines/schedule/派发，select等待这同一个已注册future；或者采用有持久permit且语义明确的notify_one/版本化watch方案，不能只把60秒缩短来掩盖。审查取消、任务完成和通知合并分支确保不会漏掉重新计算deadline。

**重测要求：** 加测试专用屏障固定“空schedule已计算、worker尚未等待”的窗口，再enqueue一条后续不再更新的尾部，放行worker；证明首脏500ms触发，另65536字节立即触发，均不等60秒。保留原保存阈值、inflight/故障watchdog/Drop及shutdown测试；形成新冻结摘要交原reviewer复核关闭。本reviewer未改代码或靠真实Harness重跑来替代这个可由确定性并发用例验证的问题。

## 其余静态核对（不表示执行通过）

- **身份、源顺序与版本：** OutputScope区分session/run/turn，Item/Source/Position/ContentVersion有独立类型；attach/耗尽区间时调用真实存储事务预留位置，64位置区间持久前移，不在重启后复用余量。begin_item取得位置后才返回可发布handle，update复用identity/position，非同源新item不按正文合并。owner_tag防跨协调器handle，raw bytes以Arc保留，Debug只输出长度。
- **SQLite latest-value/CAS：** commit_items按scope校验、既有identity所有字段匹配，同版异摘要拒绝，低版本只返回现有提交版本、高版本更新；body/item/revision/可选完整性事实在同BEGIN IMMEDIATE事务提交。旧条目首次位置不被更新。用户input与output的content purpose隔离，output GC检查item及pin引用，不会回收尚未受理input。
- **原子容量计量：** 同一个NodeState短锁统一计算并变更；每不同(item,version)包含payload、完整UTF-8类型/协议元数据及保守开销，另计scope成本；共享同版Arc不重复计payload，inflight旧版与新dirty分别计量。会话聚合16MiB、节点128MiB，不是每scope伪造节点预算；高水位ceil(4limit/5)，硬上限进入停止、丢弃正文仅记录缺失位置/字节。
- **触发量与旧ack：** dirty_bytes按每次受理的原结构快照payload累计，包含被同源新快照替换的累计输入量，不按管理信封/base64或metadata触发。领取flight时转走旧批dirty与first_dirty，flight期间新输入建立独立累计/时钟；ack仅移除版本不高于已确认值的pending，不清新版本累计。抽查65535+1、499/500和旧ack期间更新测试具有直接断言。
- **故障和资源责任：** 实际失败3次或首失败后5秒先到、重试最短250ms、单scope单flight、成功复位有明确状态；独立watchdog可在存储future阻塞时检查失败截止。forced失败/取消Future的FlightGuard会保留错误/停止责任，OutputControl返回Requested或ResponsibilityRetained，不虚称底层已停。HistoryTask拥有JoinHandle、取消和JoinSet；Drop请求停止并abort，shutdown共享至多15秒预算，超时返回失败。接口尚未接真实Harness owner，fixture的finish_stop不是进程已退出证据。
- **pin/GC与Run完整性：** pin_version在同一BEGIN IMMEDIATE中读取引用并登记pin，和替换/回收写事务串行；release_pin同样受写事务保护。startup recover清旧pin/无引用output并记可能丢尾，必须仍由node owner在接收新工作前调用。history_integrity按(session,run)主键持久化，Keep不清旧事实；新Run成功不删除旧Run不完整记录。
- **开发测试覆盖检查：** 抽查typed身份/版本冲突、元数据/空条目预算、精确容量、inflight旧新版本、强制刷新失败、worker Drop/超时、真实SQLite失败与回滚、子进程直接退出/位置间隙、input GC保护、不同Run完整性、独立连接pin/write竞争。原187库+4向量/fmt/clippy自测仅主执行入口信息，本报告未读取全部执行日志或重跑，不借此签独立运行通过。HIST-R001是这些测试未覆盖的调度窗口。

## 限制与后续

本候选仍没有真实ACP逐版本接线、前台生命周期/审批发行者、Web保存错误/正常隐藏状态、逐块撤销writer、网络租约时限、Job Object及完整进程树验收。源码README准确区分内部端口与真实产品调用顺序，不把OutputControl fixture当真实清理。2.3尚未开始，不能将审批表或BeforeApproval枚举当审批唯一裁决已实现。

schema2新增protocol列及history表，代码按完整schema2定义严格校验；已集成2.1的中间开发schema2测试库不被自动升级为本候选完整schema2，可能明确拒绝。测试应从合同支持的schema1副本/新库重建自有fixture，不能为通过而删除用户实际库或伪称支持所有中间开发schema。真实节点升级仍遵循备份、owner锁与最终版本兼容审查；本报告未运行迁移。

主执行已收到HIST-R001并确认冻结源不边查边改、主目录不集成；下一版应统一修复/冻结后由独立tester重测、原reviewer复查，不让coder代关。当前不能给该候选“无阻断”结论。

## 冻结源码SHA256

| 文件 | SHA256 |
| --- | --- |
| `app/src/lib.rs` | `4645DFF1BCB1FE5940C818E938AF987F64DC4283857130FB3C2516C4E75CC6AC` |
| `app/src/storage.rs` | `99D84BE3D66D40F88D3796B046C7011AA574782C3E93ABDA7E21C9EE0752C6D9` |
| `app/src/domain/mod.rs` | `FB901CE4EB737ABAD538646414FF1B9C0C3FCB394FBCE2B2CB2DDC6C1B42B7DC` |
| `app/src/domain/history.rs` | `371E1C88D5BE2038EC7F0BABC8A4569A7E6B640342AD0F8078EDF70C5A99DE2A` |
| `app/src/application/mod.rs` | `D6EACDDC11AB283BBE952E8DD79C4A82468C6E1F5C4AC0A19E449B67F17C63E8` |
| `app/src/application/history.rs` | `D0B6DCF9D03A2A5A2B8E69DFDA1919C36BD46C1D8658AA6DAF510E31CCF382FD` |
| `app/src/application/history/tests.rs` | `38C2B870151580BBBBC4CE66898F2E11EF1D3BF60855A4DD564028BC1B17D3C5` |
| `app/src/application/history/README.md` | `C9DD6364C80A9BDA8F900F1D03AFDF33BF30332EC1DD035440795243793DA1F4` |
| `app/src/session_storage/mod.rs` | `284E8448BC91C48ED16E5E7C50467BA0C32AA22D905D0C72AA64648AE13818E7` |
| `app/src/session_storage/schema.rs` | `E48B83DDA7DFBDCFDD02A5D3F9AF722B1ED2CB4B0FD9AEEF3EC2EF1DA0ECA0DB` |
| `app/src/session_storage/tests.rs` | `FA2FE59429979D1551EDBCCF16C83801A6E7B43813487703C967E2CF5EFA60FB` |
| `app/src/session_storage/history.rs` | `CBAC80278D4A44F6528586D694D9F47E173240DFF14687BF918EF30C78A9D588` |
| `app/src/session_storage/history/tests.rs` | `8FAB55E7974805E53A1ECA756BE5006DF6DDF992B8C6AA9F1FC1319A156DD952` |

## 本次读取的D主目录权威摘要

此表固定本次阅读事实，分页修订另行冻结，不把该表当全部规划新PASS。

| 文件 | SHA256 |
| --- | --- |
| `D:/Project/acp-remote/openspec/changes/deliver-codex-session-loop/session-contract.md` | `65D9623D704344BFB2FCABB049EE8203EBAAB1DA6385BB96B792CAC959358D45` |
| `D:/Project/acp-remote/openspec/changes/deliver-codex-session-loop/specs/structured-history/spec.md` | `A90A8CDBCC285EC8CDAB2AEFB1D7AF589DFAEF8CA5009354FBCDB1AF43D99FBB` |
| `D:/Project/acp-remote/openspec/changes/deliver-codex-session-loop/design.md` | `D42BEBFB4E93DEF9E8B20CCB5FB710E12EE531CA86F85A2DE7AFD96FB6CD566C` |
| `D:/Project/acp-remote/openspec/changes/deliver-codex-session-loop/tests.md` | `6E6DAD8DE4B68ADE19021BF5518A4115173A1E1329C0F799FC7B26956E0A3AF6` |

## 2026-10-30 HIST-R001 修复版独立复查

**结论：HIST-R001 CLOSED，由原提出reviewer对新固定版本复查关闭。此修复候选的已审2.2内层代码不再存在本轮已识别阻断；仍不是task2.2完成或真实产品验收通过。** 前节原缺陷/原冻结摘要保持历史原文，本节取代该问题OPEN状态。主目录仍2.1，未由本reviewer合入代码。

### 范围与实际验证

实际重算新manifest SHA256 **EF2FE20EEC101129B4342870106BC81CC61F8F9E4E891A347C20BAD0298C94FF**；逐文件13/13匹配。与旧C1BB...清单比较，只有application/history.rs及其tests.rs两项变化，其余11项摘要相同，原SQLite/预算/GC/Run完整性代码不被此修复暗改。

worker每轮先创建changed=Notify.notified、pin、enable，再检查deadline/schedule，select等待同一个已经注册的future；生产者仍使用notify_waiters。原“空闲60秒旧schedule已算出、首条输出广播时无等待者”的窗口消失：在状态检查之前已注册，因此广播会令该future ready；新一轮重新计算dirty截止。未通过缩短60秒掩盖竞态，也没有把广播换为单消费者通知而遗失forced flush其他等待者。

测试hook仅cfg(test)，在schedule计算后阻塞该轮，未改变生产调度逻辑。两个回归先断言旧空闲计划确为60秒/0任务，再在该固定窗口加入首次payload。第二个屏障必须在未额外发通知/未tick前达到，从而直接验证首次广播已被消费；小payload在499ms重新计算1ms剩余截止，500ms提交；65536字节在时钟0立即调度。第三例手工poll三个forced flush future使其都已注册，旧flight完成广播后都能结束，且实际单在途/共两次保存，不靠幸运调度注册。

本reviewer实际有界运行已有测试binary（没有重新编译）：

- `acp_remote-266e97892ace37dd.exe idle_registration_window --nocapture`：exit0，2/2通过，188过滤。
- 同binary `hist_r001_broadcast_completion --nocapture`：exit0，1/1通过，189过滤。

这三项使用内存repository/clock/output端口fixture，不启动Harness或操作用户数据库。执行来自本reviewer工具实际输出，不是只采信coder“已通过”。binary SHA256及源摘要在下表固定；本次没有独立重新编译该binary，不把有界执行称为独立全工程可复现构建。

实际核读verification.json逐项指向的原始log和.exit：fmt、全目标Clippy、owner前置、2个回归、history34、all-targets190库+4JWS和diff均为exit0，日志计数与摘要记录相容。该全量结果仍为coder实际执行、reviewer核读，不能代签独立tester正在进行的全量或外部验收。没有为权限问题绕过自动安全拒绝，本次既有binary有界运行默认权限成功。

### 关闭范围及剩余门禁

当前确定代码缺陷已修复并重验，允许主执行在独立tester同版结果完成后按原计划审查集成。schema/输入/历史其他已审部分保持原有限结论；不因此签2.3审批实现、WP3真实OutputControl/进程树、WP4 Web保存错误、分页/租约、正常隐藏保存状态、逐块撤销或CL-030。所有原scope owner、启动recover、真实库迁移备份及最终L5/L6责任继续保留，task2.2最终Web断言仍不勾。

D页大小合同修订与本C源修复无关，本节不签其定义通过；待tester的CL-027/042及最终合同冻结后可另作有限复核，不重跑旧产品替代定义审查。

### 修复版源码固定SHA256

| 文件 | SHA256 |
| --- | --- |
| `app/src/lib.rs` | `4645DFF1BCB1FE5940C818E938AF987F64DC4283857130FB3C2516C4E75CC6AC` |
| `app/src/storage.rs` | `99D84BE3D66D40F88D3796B046C7011AA574782C3E93ABDA7E21C9EE0752C6D9` |
| `app/src/domain/mod.rs` | `FB901CE4EB737ABAD538646414FF1B9C0C3FCB394FBCE2B2CB2DDC6C1B42B7DC` |
| `app/src/domain/history.rs` | `371E1C88D5BE2038EC7F0BABC8A4569A7E6B640342AD0F8078EDF70C5A99DE2A` |
| `app/src/application/mod.rs` | `D6EACDDC11AB283BBE952E8DD79C4A82468C6E1F5C4AC0A19E449B67F17C63E8` |
| `app/src/application/history.rs` | `686D12998D1826E77CA3183C3CE0FC69981C0FDADE601722EC85645BDF66ED58` |
| `app/src/application/history/tests.rs` | `7031D8BFE132F87CAD1F6BF24AFD433DF2E82D1C35DF14C32ACFADCE7A14F52B` |
| `app/src/application/history/README.md` | `C9DD6364C80A9BDA8F900F1D03AFDF33BF30332EC1DD035440795243793DA1F4` |
| `app/src/session_storage/mod.rs` | `284E8448BC91C48ED16E5E7C50467BA0C32AA22D905D0C72AA64648AE13818E7` |
| `app/src/session_storage/schema.rs` | `E48B83DDA7DFBDCFDD02A5D3F9AF722B1ED2CB4B0FD9AEEF3EC2EF1DA0ECA0DB` |
| `app/src/session_storage/tests.rs` | `FA2FE59429979D1551EDBCCF16C83801A6E7B43813487703C967E2CF5EFA60FB` |
| `app/src/session_storage/history.rs` | `CBAC80278D4A44F6528586D694D9F47E173240DFF14687BF918EF30C78A9D588` |
| `app/src/session_storage/history/tests.rs` | `8FAB55E7974805E53A1ECA756BE5006DF6DDF992B8C6AA9F1FC1319A156DD952` |

### 修复版验证资产SHA256（相对C隔离root）

| 文件 | SHA256 |
| --- | --- |
| `.tmp/wp2-2.2-hist-r001-logs/frozen-files.json` | `EF2FE20EEC101129B4342870106BC81CC61F8F9E4E891A347C20BAD0298C94FF` |
| `.tmp/wp2-2.2-hist-r001-logs/verification.json` | `23F0896ACDC92FF4ACE1B999D7B2771EF955D47BBDD09955201F951DEB5E2B8E` |
| `.tmp/wp2-2.2-hist-r001-logs/regression-02.log` | `F5B1B1D8ED90A5CAA7B1F22FCC4BC0848EC9AE5D342EDA1DA8E6D165C8FD44E7` |
| `.tmp/wp2-2.2-hist-r001-logs/history.log` | `32E57E79266D9817D64A1816CD5B8BB2ADC054BF940819F0929EEEC84C194B91` |
| `.tmp/wp2-2.2-hist-r001-logs/test.log` | `E39C823ADEA3B6C68EEF31C78A17FD1063D6B7D0723E3869873B962EA34CA575` |
| `.tmp/wp2-target/debug/deps/acp_remote-266e97892ace37dd.exe` | `6037F6F63F8803BE1E920007EA7E9A49C544558B4653FE322FC37672531C6EF5` |
