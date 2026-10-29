# 独立协议预算验收（2026-10-30）

当前结论：**本预算侧报告列明的补验缺口已完成：IPC缺陷在R14关闭、真实300秒pair流通过、R17慢writer真实队列至WSS关闭的有界调度回归通过**。证据分属明确记录的固定版本与适配器边界，不将本侧定向执行冒称整产品或主方完整check，也不代签Web变更。首轮未发现缺陷，追加R9真实IPC容量测试确认第32连接导致节点退出，R14修复复测证据见末节。仅新增专属探针和本报告；未改产品、预期、任务勾选或归档，未提交/推送。

## 版本、环境与边界

- Windows，PowerShell，Node v22.20.0，Rust 1.98.1；Node 内建 HTTPS/TLS 和自行编码 RFC6455 客户端帧。该适配器仅替代客户端传输，不替代真实节点、TLS、时间、数据库和签名。WSS 始终携带正确 Origin、Host、subprotocol，rejectUnauthorized=true，SNI 为证书原域名。
- 输入 r5a 路径在首次复制前已被构建更新。首次 SHA 校验失败即停止，未把该事件视为产品失败。依据用户追加授权，实际复制并测试 r6：`135BD9BDDABFDD530A5251AECB1DFF0DC640E81F80241EC53C8703C2336D1509`，独占 `.tmp/budget-acceptance-20261030/bin/acp-remote.exe`。随后不再依赖主方 target 的二进制。
- 预算负责源码与 r5a manifest 逐字相同：pairing_http.rs `29466EACFF9F65F06826E1909EAAFAC58A032F2070552EE8B60EA50794977917`；management_ws.rs `EBC7EB0D839DE2450FAD4BCFB8DE6990A53772FF9DAB8858E9DD09E3AA05E00B`；strict_json.rs `A20B389A58975834069CBE4BE152122EE69A49187964465C5898A6E0F723803F`。
- tests.md 摘要 `6421ECD2763F0438B9296C0BC6DC577C7143E2614ED2899B7AF1A6FAE18F54BF`；access-contract.md `012F5004B5D0611928A39A42E8D93409EA763EC662B5ECC819D76F4BE22C986D`。NA-006 Steps/Expected 位于 tests.md 79–83 行，NA-023 位于 181–185 行；合同§5为判据。
- 两个真实节点只监听回环 30457/30458。客户端绑定 127.0.0.1、.2、.3、.4 及有限其他回环来源；不同来源是实际 TCP bind，不是转发头模拟，不访问其他目标。初始 wildcard 配置被产品明确拒绝，随后改为显式回环接口；不是预算失败。
- owner-only 运行根包含所有 data/key/config/log/JSON/temp/target。证书和私钥只由产品读取原路径，未复制或打印私钥。客户端私钥仅在 Node 内存。

## 真实入口结果

| 当前用例对应断言 | 实际输入与观察 | 结果 |
|---|---|---|
| NA-006 HTTP 16KiB | 16,383 / 16,384 字节均得到 400 unsupported_version；16,385 字节得到 400 invalid_request。有效 JSON v=2 后加空白，区别解析接受与前置预算拒绝，耗时分别 21/2/1ms | PASS |
| HTTP 深度、压缩、未知路径版本 | 根对象计深度，8/9 层分别 27/29 字节：unsupported_version / invalid_request；Content-Encoding:gzip 拒绝；未列路径404 invalid_request；v=2报unsupported_version | PASS |
| 四端点每来源限流、畸形请求、伪造头 | lookup前5次400、第6次429；claim/challenge/status各前30次400、第31次429。每次正文1字节畸形JSON，X-Forwarded-For逐次变化、X-Forwarded-Host伪造；均未换桶。各429 Retry-After=60，JSON retryAfterSeconds=60 | PASS，仅畸形请求计数子场景 |
| 四端点全局限流 | lookup跨真实来源前30次400、第31次429；其他三端点各前120次400、第121次429，所有来源自身均未超过来源额度 | PASS |
| 固定窗口真实恢复 | 全局lookup超限429，retry=60；同来源实等60,121ms，随后返回400 invalid_request，证明重新受理而非继续限流 | PASS，真实墙钟等待 |
| WSS 32KiB | 32,767 / 32,768 字节合法hello+空白均返回challenge；32,769字节关闭4429 limit exceeded；TLS authorized=true | PASS |
| 认证后深度8/9 | 8层command进入业务校验返回result/error.invalid_request；9层同构command关闭4400。数组payload故意不是node.info业务要求的空对象，业务拒绝证明解析层已接受 | PASS |
| 每来源握手 | 正确Origin与协议的前30次成功101，第31次HTTP429 | PASS |
| 全局握手 | 多真实回环来源前120次成功101，第121次HTTP429 | PASS |
| 未认证连接32 | .2来源24条+.3来源8条，共32条保持；第33条升级后关闭4429；该瞬间既有32条均无关闭记录 | PASS；未等同已认证正常客户端 |
| 每客户端8及正常连接存活 | 真实owner终端明确批准、P-256/JWS认证；第9个认证关闭4429，原8条随后各发node.info均成功。两轮独立新身份均复现 | PASS |
| 单连接120命令 | 新认证连接前120个node.info成功，第121个关闭4429，实测85ms | PASS |
| 附带认证回归 | 错私钥与跨连接重放各4401；同连接重复proof与认证前管理帧各4400 | PASS |

第一轮 wire.json 中 `ws-depth-8` 的 FAIL 是探针把认证前command误期望4401；产品实际4400符合格式语义。原始记录保留，未将其上报产品bug，也不以此证明深度8接受。随后 auth.mjs 的真实认证后独立场景补齐深度8/9断言。旧探针未修改。

## 已实际运行的源码测试与适配器范围

独立 `CARGO_TARGET_DIR=.tmp/budget-acceptance-20261030/target`，TEMP/TMP指向该运行根temp。使用 `cargo test -j 2 --manifest-path app/Cargo.toml --lib FILTER -- --nocapture`，按下列 FILTER 定向执行，无完整suite声明。

| FILTER | 实际数量 | 可以证明什么／不能证明什么 |
|---|---:|---|
| management_ws::tests | 4通过 | registry适配器实际构造128条（16身份×8）并拒绝129、未认证32/33、单身份8/9；握手来源30和全局120；CommandWindow 120/121；Outbound真实mpsc队列256/257。不是真实网络128或慢客户端验收；没有1MiB字节耗尽断言 |
| pairing_http::tests | 7通过 | 包含实际RateLimiter的check_at时间适配器：来源5/6、换端口不换来源、不同来源独立；同pairing状态0ms接受、500ms拒绝retry=1、1000ms恢复。其余合同端点与封闭错误测试实际通过 |
| ipc::tests::frame_reader_enforces_the_message_budget | 1通过 | Cursor适配器实际读32,767字节正文+换行、拒绝32,769字节正文+换行；没有恰等32KiB、32连接及短请求时限断言 |
| pairing::tests::pairing_lifetime_is_three_hundred_seconds_and_cannot_be_renewed | 1通过 | 注入时钟，299秒lookup成功且原expiresAt不变；300秒lookup失败。不是IPC流式会话越过30秒的验证 |

合计13个通过。首次默认并发编译 futures-intrusive 失败（exit101，无充分诊断判断根因），-j2重试编译成功，management 4项通过。后续三个过滤器日志分别显示7/1/1通过；PowerShell把cargo stderr包装成NativeCommandError的外层退出状态不能代替实际测试计数。现有日志保留，不虚报首次成功。

## 精确未覆盖项与所需最小入口

以下均来自当前 NA-006 Steps/Expected 或 NA-023 Steps/Expected，不增加硬件或无穷组合要求。

1. **1024来源桶及空闲5分钟**：HTTP和WSS均缺可执行断言。需在各自私有RateLimiter/HandshakeLimiter的cfg(test)模块，以check_at填满1024不同来源；第1025来源拒绝；现有已满来源不能因新来源获得新桶；299秒保留、300秒空闲淘汰后新来源可加入。必须适当推进60秒窗口避免全局配额先行遮蔽，不能单凭某次429判桶满。
2. **固定60秒精确边界**：已有真实等待60,121ms恢复，但check_at现有测试没有59,999ms仍拒绝、60,000ms重置和拒绝请求不延长窗口的精确断言。HTTP各端点与握手均应补；CommandWindow也没有窗口重置断言。
3. **成功与失败都计数及status每pairing1/s真实入口**：已实测畸形计数，现有HTTP成功流程已运行，时间适配器已测0/500/1000ms；尚未组合真实已签状态proof与连续status限流、成功/业务失败混合达到来源阈值。不将单独成功流程等同预算计数通过。
4. **真实全局128连接**：registry适配器128/129已运行；网络入口尚未持有128条。真实前置至少16个owner批准身份×8条，或96条已认证+32未认证，再以第129条验证4429且既有客户端命令仍成功；须跨来源或等待握手窗口，避免120握手和32未认证遮蔽。本轮只有两测试身份，不能宣称真实128通过。
5. **慢连接1MiB或256帧先到**：256/257队列适配器通过，1MiB字节先到及网络慢读未执行。最小cfg(test)入口应持有receiver不消费，分别构造未满256帧而字节许可耗尽、未满1MiB而帧数达到256；确认permit释放后可再入队；再用可控writer阻塞验证4429关闭路径。不能因暂停读取普通socket就推断已填满服务端队列，OS缓冲可能吸收数据。
6. **IPC连接32与30秒/300秒流式区别**：已有IPC帧适配器不覆盖。需named-pipe或明确调度入口持有32合法owner连接后第33拒绝；短请求29,999ms未超时、30,000ms超时；pair流式会话越过30秒继续可认领/取消，至300秒终结。配对领域299/300秒测试不能代签IPC流式时限。未更改这些产品断言。

这些缺口已向主执行者即时提供入口位置；本轮不修改生产源码或预期，不以源码存在填PASS。NA-006/023整体保留未完成。

## 可执行资产、证据与清理

专属探针目录：`testing/probes/establish-node-access-budget-20261030/`。setup.ps1为可复用初始化（运行根已存在则拒绝，避免覆盖）；run.mjs为第一轮入口矩阵；global.mjs为全局预算和真实窗口等待；auth.mjs读取owner-only pair-input.json，在真实owner device pair提示后明确批准；wire.mjs来源于既有protocol-wire、保留严格TLS与Origin；cleanup.ps1执行owner stop与释放核验。后补setup双节点循环仅为复现便利，本轮实际启动等价地逐节点执行。

真实命令：`node testing/probes/establish-node-access-budget-20261030/run.mjs`、`global.mjs`、两次`auth.mjs`；PowerShell以ExecutionPolicy Bypass执行专属setup/cleanup脚本。沙箱首次网络EACCES后改以真实owner授权环境执行；未关闭TLS。

原始证据仅在 `.tmp/budget-acceptance-20261030/`：evidence/wire.json（20条含上述1条探针预期错误）、global.json（6条通过）、evidence/auth-first.json与auth-matrix.json（第二轮8个场景）、evidence/unit-*.log、source-hashes.json、cleanup.json。JSON中仅隔离测试节点公钥、挑战及签名证明，均保留owner-only目录；正文和proof不复制进报告。

两节点均正常owner stop成功。PID 8636 / 44268均退出；各监听数0；本次pipe停止前存在、停止后不存在；各.node.lock可独占打开；data ACL只允许owner。探针连接全部destroy/正常结束，两次device pair终端都exit0，无遗留pair进程。独立target、数据和证据保留在自有根，不删除用户资料。未触碰30447或30451–30456。

## 追加独立补验：R9真实入口、R10组件

本节更新前述缺口状态，保留上轮原始结论。真实节点改用独占复制的R9二进制：`AB97418E98D67A71E25BBF62B1E780F2F767530CA4290FB9C78E220A222A6024`。R10组件测试依据83项manifest `C2B1B9B6C021991952F7C2DE27696D8557874B31493C10E6584C679E58B967A1`，实际独立编译运行，不以主方结果代签；不把R10源码测试说成R9二进制带测试hook。

### 已完成补验

| 子断言 | 实际结果与证据边界 |
|---|---|
| lookup成功/失败/畸形均计数 | 同一真实TCP来源序列200、401、400、200、200、429；第6次超限，实际HTTPS入口。只代表lookup混合计数，不冒称其他三端点均已混合验证 |
| 有效status proof每配对1秒 | 实际HTTP 200→429→等待1141ms后用新鲜challenge/proof得到200；429 Retry-After=1，JSON retryAfterSeconds=1。第一次错误尝试重复已消费proof得到401，属于探针前置错误，原始r9-first-attempt.json保留，不报产品缺陷 |
| IPC短请求与pair专用流 | 显式Windows Impersonation级别的.NET NamedPipeClientStream，真实owner SID校验；短请求在29007ms仍未结束、29994ms结束无响应；同轮pair在31245ms后仍可经HTTPS认领、IPC收到pending_confirmation，expiresAt保持原值。随后本机reject，无新信任。首轮另有31249ms/29999ms记录 |
| IPC32KiB实际消息 | LF作为1字节分隔符，正文32767和32768字节（线上总32768/32769字节）status响应ok；正文32769字节（线上32770）无响应关闭；耗时17/18/19ms。适配器初版CRLF多算CR、GUID格式requestId错误均已纠正，不能作为产品失败 |
| HTTP 1024来源桶/300秒 | 独立执行R10 `source_bucket_capacity_cannot_evict_active_sources_and_prunes_at_five_minutes`，真实limiter.check_at构造1024桶；60秒新窗口global计数1时第1025源仍拒绝；旧源不被驱逐重置，299秒仍满，300秒仅近期旧源+新源2桶。不是网络创建1024来源 |
| WSS 1024来源桶/300秒 | 独立执行R10 `handshake_buckets_reject_eviction_and_expire_at_five_idle_minutes`，同类容量/淘汰/不驱逐断言通过。299秒与300秒为当前测试实际取点，不冒称299999ms已测 |
| 固定窗口精确边界 | R10 HTTP `all_endpoint_windows_reset_at_sixty_seconds_without_sliding_on_rejection` 与WSS/command `handshake_and_command_rejections_do_not_slide_fixed_windows`实际通过；59999ms拒绝、60000ms接受，拒绝不续窗。时间参数适配器，不是现实等待精度声明 |
| 出站1MiB与256帧独立额度 | R10 `outbound_byte_and_frame_limits_are_independent_and_release_permits`实际通过：32×32768=1048576字节先满，后续入队拒绝；出队drop后许可释放并可复用；256个小帧先满，257拒绝且失败入队不漏permit，释放后可复用；drop receiver后字节许可全回收。组件receiver暂停，不代表实际网络slowreader已通过 |

R10定向测试 `management_ws::tests` 7项通过，`pairing_http::tests` 9项通过，合计16项；日志evidence/r10-management.log、r10-http.log，源码/manifest摘要evidence/r10-hashes.json。独立target和TEMP/TMP同上轮，未与main争用。先前领域clock 299/300秒测试仍有效地证明不可续期，但不替代IPC流在原deadline300秒终结的调度测试。

新增 `owner-bridge.ps1` 仅为本机owner管道客户端；Node原生pipe未带所需模拟能力，其EPIPE/未收到启动响应不计产品失败。后续所有IPC结果使用显式模拟级别.NET客户端。`r9-completion.mjs`保留实际TLS Origin，子进程标准输出只在owner-only父进程读取，不打印配对码/证明。第一版准备身份达到临时pair保留名额后停止，未声称已建立128连接；`--capacity-only`分批本机批准并正常重启清理临时配对，保留已提交信任，所有客户端私钥仍只在Node内存。

### 真实128连接已完成

`r9-completion.mjs --capacity-only`真实通过，evidence/r9-capacity.json：16个身份全部由真实owner named pipe显式approve；每4个批准后正常owner stop/start清理临时pair保留项，已提交信任仍保留。不是TTY16次人工操作，也没有网络批准路径。

每身份8条、共128条真实TLS/WSS已认证连接；各来源实际绑定回环地址，始终携带正确Origin、Host和subprotocol。第120条后实等61秒避开全局握手窗口，再建立其余8条。第129条升级后关闭4429 limit exceeded；随后原128条各自node.info成功，survivors=128。连接均在finally销毁；父Node进程最终exit0。此前的registry适配器128/129不再是该真实入口断言的唯一证据。

### NA-BUDGET-IPC-001：第32个owner管道连接导致整个节点退出

- 严重性：高，阻断NA-006连接预算Expected及NA-023资源耗尽后的正常客户端可用性Expected。R9发现时OPEN；现已由原独立提出者在R14复测关闭，见末节，以下保留原始失败证据。
- 版本：上述R9固定binary。目标仅自有30457，故障PID49040；没有主动stop/kill触发该观察。
- 稳定复现：真实owner运行 `powershell -NoProfile -ExecutionPolicy Bypass -File testing/probes/establish-node-access-budget-20261030/ipc-capacity.ps1`。按自有pipe.json地址，用显式Impersonation级别.NET NamedPipeClientStream依次连接，每条写1字节`{`保持短请求未完成，并启动异步读。只持有最多32条，远小于30秒超时。
- 实测：30连接时154ms、0条读结束；31连接时264ms、0条读结束；32连接时374ms、全部32条读结束。2924ms时32条均结束，PID49040已退出，30457监听数0。原始evidence/ipc-capacity.json保留数量/时间/进程观察。
- 同次产品stderr（自有restart-*.err）明确为 `creating the next owner control instance failed`、`os error 231`，并报告node shutdown failed。源码对应 `platform/windows.rs::create_private_pipe` 的max_instances(32)，`ipc.rs::serve` 在已accept后先bind_next、后try_acquire permit。该关联由动态错误支持，不是仅凭源码推断。Windows实例总数包含监听实例，不能把31活动+1监听当合同允许32活动。
- 预期：第32条允许作为活动连接而不关闭既有31条；超过活动连接预算应有界拒绝，不终止整个节点；释放一个活动连接后入口应恢复。最小修复位置由主方决定，tester不改产品或削弱Expected。

主方已收到即时问题ID、版本、步骤和实测数据。故障后不伪称30457正常ownerstop：它已自行退出，无需强杀。`cleanup-after-fault.ps1`核验其pipe不存在、锁可独占打开、监听0；30458 PID34580仍存活，正常ownerstop成功。两节点最终均退出，data ACL只允许owner、pipe均消失、锁均可独占打开、两个监听均0；evidence/cleanup-r9.json。无需触碰用户进程，无残留本次owner bridge/Node进程。

### 当前剩余精确范围

- IPC32容量：R9实际FAIL，R14已重测32/33、原连接存活及释放恢复全部通过，NA-BUDGET-IPC-001关闭；不记“缺环境”。
- IPC pair原始300秒流式deadline：此前领域299/300秒与真实流越过30秒分别通过，追加真实完整300秒流式会话现已通过，见末节；无需为本断言增加双虚拟时钟seam。不声明纳秒精度。
- 出站writer积压关闭：此前组件1MiB/256先到及回收已通过；R17已独立执行真实TLS/WSS+writer暂停适配器，实际超限入队并收到4429，两种先到条件均通过，见末节。属于有界调度验收，不是普通客户端慢读或真实网络拥塞性能测试。
- HTTP混合计数缺口已关闭：追加`http-mixed.mjs`在正常重启后的同一R9自有节点上实测claim前30次交替200/401、第31次429；challenge前30次交替200/400、第31次429；status首次200、随后29次401、第31次429。三端点的Retry-After均60。status业务失败使用格式无效proof，在每pairing1/s检查之前拒绝，避免将配对频率限制误当来源阈值。evidence/r9-http-mixed.json保留完整31次状态序列；此前lookup混合计数及status1秒恢复证据共同覆盖当前混合计数断言。

不重复此前正常用例；新补入口或修复版本到位后只重跑受影响断言。所有追加源码只读，新增写入仍限自有probe与本报告。

混合计数补验后的节点再次正常ownerstop，故障清理记录另保存在evidence/cleanup-r9-fault.json，最后清理记录为cleanup-r9.json；自有30457/30458均无监听、pipe消失、数据锁可独占打开，未杀任何用户进程。新增脚本语法和范围差异检查通过，未提交或推送。

### R14复测准备（未执行）

主方通知拟修复Windows实例数与IPC accept/permit/bind次序；正式R14二进制尚未交接，本记录不视为修复通过。新增独立 `ipc-capacity-r14.ps1`，保留原R9复现脚本与原始证据。新脚本要求显式ExpectedBinarySha256，校验独占bin及运行进程路径，明确拒绝已知R9哈希，避免误重测旧FAIL。

已准备的断言：31和32活动连接均保持；第33条必须先真实OS连接成功，再在2秒内无响应关闭（OS连接失败不计应用限额通过）；既有32条不结束；完成原第1条status以释放名额，新连接可执行status；原其余31条status全部成功；节点原PID仍活且30457仍由它监听。所有持有管道均在finally释放，成败证据写自有evidence/ipc-capacity-r14.json。这里只完成脚本语法检查，不声明这些行为已经运行。此前128条管理连接测试已结束并清理，不存在仍运行的连接测试。

### R14正式独立复测：NA-BUDGET-IPC-001 CLOSED

主方正式交接后，独立核验源binary并复制到自有bin，复制后再次核验：`CD6C2843899541AFB9AB6FD479960AD17E5003B6CC4F0E325618DE5A6DBA56A5`。对应manifest SHA256=`AB503036ACA9D970C9CF3D8D8EE99F80572E6DCF292252DA119DDD3E3F422A37`。本次源码记录：ipc.rs `1F388E3EBE46DB7F64FA6F52E5C6352E6B4562AA6247E91C6E275E650ED2C408`；platform/windows.rs `5221E4E5184183CB7464ED9CCFF36080A16D35767F6529B0D38CC4EE7107FF5A`。证据evidence/r14-hashes.json。

实际运行命令：`powershell -NoProfile -ExecutionPolicy Bypass -File testing/probes/establish-node-access-budget-20261030/ipc-capacity-r14.ps1 -ExpectedBinarySha256 CD6C2843899541AFB9AB6FD479960AD17E5003B6CC4F0E325618DE5A6DBA56A5`，退出0，7条断言全部通过。真实owner管道，没有模拟服务端或执行主方回归代签。

| 断言 | 实测 |
|---|---|
| 31活动连接 | 160ms时31条均未结束 |
| 32活动连接 | 270ms时32条均未结束 |
| 第33条拒绝 | OS连接成功，275ms时已无响应关闭；原32条结束数0 |
| 原连接释放名额 | 310ms时原第1条status成功，随后关闭该客户端handle |
| 释放后恢复 | 保持另外31条活动，411ms时新连接status成功 |
| 原32条存活 | 428ms时原32条全部各返回成功status，匹配各自requestId |
| 节点保持可用 | 2045ms时原PID46428仍活，30457监听恰1个且归该PID |

总探针耗时2046ms，原始evidence/ipc-capacity-r14.json，passed=true。自有节点本次stderr为空。由原问题提出者据此关闭NA-BUDGET-IPC-001，范围是实际32/33边界、既有客户端存活、释放后恢复和节点存活；不扩大为本轮已独立运行原生254实例耗尽/stop取消测试。该254原生实例验证为主方提供结果，本报告不代签。修复最终使用有限254原生实例、应用32活动permit，不沿用已放弃的33原生实例方案。

随后两个自有R14节点均正常ownerstop成功：PID46428（30457）和30832（30458）均退出；两个监听数0、pipe停止前存在而后消失、两个.node.lock均可独占打开，data ACL仍仅owner。证据evidence/cleanup-r14.json。未触碰其他端口或进程。R14本机IPC复测不依赖Web资源，不对后续Web资源冻结问题给出结论；主方完整check结果亦未作为本次独立执行结果。

### 真实300秒pair流补验（执行记录）

依据用户追加授权，新建独占`pair-lifetime.ps1`（真实Windows owner IPC适配器）与`pair-lifetime.mjs`（实时时间观测、TLS lookup与信任快照比较）。继续使用已固定R14 binary，不依赖当前源目录后续构建，不引入虚拟时钟或test-util。客户端全部由本探针创建并在finally清理；每次等待最多45秒，父任务轮询也不超过60秒。原始owner返回expiresAt记入owner-only evidence/pair-lifetime-real.json，不输出配对码或秘密。

实测启动返回expiresAt=1793299681，收到启动响应时UTC Unix毫秒1793299381957，IPC请求发送至启动响应耗时24ms。墙钟expiresAt为整秒，不用它推断纳秒级生存时间；超时事件另用真实Stopwatch记录从请求发送的毫秒数。脚本准备检查299秒仍保持、lookup回显原始expiresAt、约300秒expired、管道EOF、旧码401和device list输出摘要不变。结果在下面补记，单凭本段启动信息不代表通过。

实际执行 `node testing/probes/establish-node-access-budget-20261030/pair-lifetime.mjs`，退出0，7条断言全部通过。evidence/pair-lifetime-real.json最终passed=true、clientExited=true。真实系统时间和owner IPC，无虚拟时间、无加速、无服务端模拟。

| 真实观测 | 实测结果 |
|---|---|
| 299秒仍保持 | 从发送device.pair请求起299003ms，未收到事件，owner客户端未退出 |
| 到期前旧码仍可查询 | 299035ms，同源HTTPS lookup=200，返回expiresAt=1793299681，逐字等于owner最初值 |
| 到期事件 | 300048ms，IPC收到event=expired，clientId=null，requestId匹配原请求，expiresAt仍1793299681 |
| 管道关闭 | 300049ms读到EOF，比观测事件晚1ms |
| 到期后旧码 | HTTPS lookup=401 pairing_invalid，无新增生命期 |
| 无信任变化 | device list前后均4340字节，SHA256均39456f78c870f6e4123828f8dafb805d69c798a57d17dbf1bcbcba3b09e0868b；本轮未认领、未批准 |
| 客户端收尾 | owner适配器exit0，stderr为空，Node父进程exit0，finally已清理 |

300048ms包含真实系统调度、IPC传输和客户端100ms轮询采样，不能反推服务器内部精确超时点；可支持约300秒有界终结及299秒仍有效的当前验收要求，不宣称精确纳秒。事件收到时墙钟Unix毫秒1793299681979，与整秒expiresAt相差979ms；这包含整秒expiresAt截断与观测延迟，不能用作额外续期证据。299秒lookup并未改变原expiresAt，随后原会话按原期限终结。

最终两个自有节点均正常ownerstop并通过进程退出、监听0、pipe消失、锁独占打开及owner-only ACL检查；证据evidence/cleanup-lifetime.json。仅使用原R14独占binary，不等待或复用R16的新Web资源包。至此本报告剩余功能缺口为真实writer队列超限到关闭的有界调度验证，等待主方提供相应测试入口后独立执行。

### R17慢writer队列关闭独立验收：PASS

版本固定：普通R17 binary SHA256=`38AA9D02F334E3C81FDC900ED4F8943BB78339650981BC0046A609381D03E6C2`，先从主方target复制至自有`bin/acp-remote-r17.exe`，复制前后核验一致；未直接运行共享target。manifest SHA256=`5074890C209A702335F13042A26DBA64278EFD7A7B0A70A4E42E2F698BA3E26F`。独立逐项核对`.tmp/frozen-implementation-r17-20261030`中83项与manifest一致（83/83），另外核对本次复制Webdist3个文件与冻结dist完全一致，不自行重建Web资源。

待验入口是冻结源码`node::tests::paused_real_writers_close_at_the_byte_or_frame_queue_budget`。它启动真实NodeRuntime、数据库、TLS监听及WSS客户端，Origin=`https://node.example.test`、正确控制subprotocol，测试证书作为客户端信任根验证TLS。身份预置通过测试pair_browser完成，密钥存储使用FakeProtector适配器，不冒称普通二进制DPAPI路径本轮被验。

为遵守仅30457/30458的资源约束，`slow-writer-r17.ps1`在自有owner-only `.tmp/budget-acceptance-20261030/r17-slow-writer-source`创建冻结app与Webdist副本，**仅修改上述cfg(test)函数**：把该用例的随机free_address改为127.0.0.1:30457，以及增加数值/时间stderr观测。其他app文件逐字相同，冻结原件未改；该两场景串行复用30457，未监听其他端口。

冻结node.rs SHA256=`C398D8C3FB95692A940D9A36383E8BA5F4E52AC1A6288DB374A8E58101AC856C`；自有测试适配node.rs=`21188EC059704352DA61D414FD251B1E64B0AE0FD07CBE8DC792EB615C75A4FE`；逐字未改的management_ws.rs=`654245F22950222B97EBAEE2EB196973B1F5C442BBDB39724BF9456F85DED554`。evidence/r17-slow-writer-fixture.json记录摘要、适配范围和核对数量。

实际命令：`powershell -NoProfile -ExecutionPolicy Bypass -File testing/probes/establish-node-access-budget-20261030/slow-writer-r17.ps1`；内部使用独立target和TEMP/TMP，执行`cargo test -j 2 --locked --manifest-path 自有副本/app/Cargo.toml --lib node::tests::paused_real_writers_close_at_the_byte_or_frame_queue_budget -- --exact --nocapture`。结果exit0，**1个测试、2个场景通过**，142项filtered out，测试合计0.48s；不把主方143库+4向量、Web50等汇报为本轮独立全量结果。

| 场景 | 实测服务端队列观测 | 真实socket关闭观测 |
|---|---|---|
| 字节先到 | 暂停writer，保持默认命令窗口；1条node.info后发送119条有界device.list请求，页数据由31个测试记录填充；292ms时占用1046252字节、63帧，拒绝下一帧16602字节。1046252+16602=1062854，大于1048576；帧数小于256；队列失败记录关闭码4429 | 释放writer后，真实WSS收到Close(4429)，从该场景开始293ms |
| 帧数先到 | 暂停writer；cfg(test)令每条命令重新建立CommandWindow以避开120命令窗口遮蔽，生产额度未改；1条node.info后发送257条node.info。158ms时占用83782字节、256帧，拒绝帧326字节；字节数远小于1048576；队列失败记录关闭码4429 | 释放writer后，真实WSS收到Close(4429)，从该场景开始159ms |

观测数字来自本轮可执行探针stderr，不是由源码常量推算。真实路径包括解析、认证、命令执行、真实Outbound入队与失败、关闭信号及TLS/WSS Close帧；writer暂停是明确的服务端调度适配器，帧数场景的命令窗口重置也是适配器。数据库预置的大页与FakeProtector属于有限测试前置。未以外部暂停socket读取或OS缓冲猜测来宣称达限，也不声称普通R17 binary包含这些cfg(test)入口。

两场景均执行`runtime.shutdown(None)`并成功；测试进程exit0。独立清理核验：测试进程数0、30457/30458监听数0、自有TEMP中遗留节点测试目录0；临时节点data/key/cert/lock目录已随TestArea销毁，进程退出释放其管道handle，无需额外kill或用户手验。冻结原node.rs摘要复核不变；自有副本仅node.rs不同，Webdist3/3复制一致。evidence/r17-slow-writer-cleanup.json和r17-slow-writer.log保留清理与数值记录。

因此关闭本报告此前最后一个慢writer调度验证缺口。此前IPC32/33和真实300秒寿命仍按R14证据归属，1024桶、固定窗口与额度独立性按R10组件证据归属；本轮不凭R17执行结果重新宣称那些入口已重跑，也不对R16→R17 Web Vault身份并发改动签署预算外验收。

## 追加平台子项：本机回环SMB正控制与产品远程pipe拒绝

用户明确追加授权本有限子项，只写本自有probe与报告。先读取`2026-10-30-platform-acceptance.md`的NA-012既有范围和证据复用边界，及当前tests.md NA-012操作步骤/预期（owner/另一Windows用户五命令、远程pipe、token检查失败、独占）。本次仅补“远程命名管道”中的**本机回环SMB、同owner**有限子断言，不代签另一机器、另一SID、另一SID五命令或token API故障注入，也不把本结果直接提升为NA-012整例PASS。是否满足当前NA-012 finite验收范围由reviewer判断。

### 只读环境核验

`smb-environment.ps1`真实owner运行，2026-10-29T18:54:28Z（本机时区对应2026-10-30）记录：LanmanServer=Running、StartType=Automatic；445监听在`::`，PID4；单次127.0.0.1:445 TCP连接成功，17ms；现成IPC$存在且special=true。本机名FARADAY。普通沙箱最初查询CIM失败，未把权限失败当服务不可用；随后真实owner只读检查上述各项成功。证据evidence/smb-environment.json。

没有创建账户、启动/停止系统服务、修改防火墙/SMB策略/凭据/安全设置；没有访问其他机器、扫描IP或连接用户管道。只在前置通过后创建自有正控制fixture与自有普通产品节点。

### 同一UNC主机路径的实际对照

实际执行`powershell -NoProfile -ExecutionPolicy Bypass -File testing/probes/establish-node-access-budget-20261030/smb-pipe-control.ps1`，exit0。普通产品为自有`bin/acp-remote-r17.exe`，SHA256仍`38AA9D02F334E3C81FDC900ED4F8943BB78339650981BC0046A609381D03E6C2`；只启动自有30457配置与数据。对照前产品本地owner `daemon status`成功且state=ready。

正控制是本次随机唯一自有pipe，直接Win32 CreateNamedPipe设置字节模式0，明确不设置PIPE_REJECT_REMOTE_CLIENTS(0x8)；DACL=`D:P(A;;GA;;;当前owner SID)`，实际GetAccessControl确认唯一允许SID为owner，没有Everyone或其他SID。服务端handle由.NET NamedPipeServerStream拥有，异步连接有取消期限；客户端显式Impersonation级别，同一现有owner登录态，不提供新凭据。

| 路径及对象 | 实际结果 |
|---|---|
| 正控制 `\\127.0.0.1\pipe\acp-remote.budget-smb-positive.<本次随机ID>`，reject_remote=false | UNC连接成功；客户端→服务端字节81、服务端→客户端字节82均实际读回；总34ms |
| 普通产品 `\\127.0.0.1\pipe\acp-remote.<自有data对应摘要>`，生产create_private_pipe设置reject_remote_clients(true) | 同一127.0.0.1主机路径开管道失败，12ms；UnauthorizedAccessException，HRESULT=-2147024891，即0x80070005 AccessDenied |

只有正控制成功之后才执行产品对照，因此不是“两者均因SMB不可用而失败”假阳性。正控制与产品具有同owner-only DACL边界，本地owner产品查询成功，而同一UNC主机路径正控制可双向通信、产品拒绝，支持当前生产remote-reject行为在本机SMB路径实际生效。**这是本机回环SMB；不是另一机器发起，也不是另一SID**。没有以网络拒绝证明异SID检查或SMB所有部署组合。原始evidence/smb-pipe-control.json，passed=true。

### 所有权与清理

正控制服务端、两个客户端、取消源和安全描述符均在finally释放；正控制pipe停止后不再列出。普通产品节点正常ownerstop，未杀用户进程；仅自有30457监听使用，未碰30447或其他tester端口。收尾再次核验自有PID已退出、产品pipe不存在、.node.lock可独占打开、30457/30458监听数0，evidence/smb-cleanup.json。没有留存账户、服务配置变更或共享目录，不要求用户手动复测。
