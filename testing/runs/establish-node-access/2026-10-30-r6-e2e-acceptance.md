# Windows A/B E2E、持久性与关闭独立验收

> 最新有效交付见文末“R16/R17正式嵌入包最终补验”和“R18最小晚到分支”。早期r6/r9段落按执行历史保留，受NA-SC003资源固定性限制；不应把早期阶段结论当作最新结论。本包最终完成NA-013、NA-032、NA-015及NA-026指定矩阵；NA-E2E-001已由原tester在正式嵌入包关闭，R18晚到真实4410遇到已consumed replacement的分支也已通过。NA-031其余未在本包执行的分支不代签。

当前最终动态版本为 **r9普通binary**，文件名按原授权保留r6。NA-032的真实单机A/B闭环已完成；NA-017的重启/三类损坏副本已完成并按既有平台证据复用跨SID DPAPI子项；NA-031普通profile完整退出后原key/ID认证子项通过。**NA-E2E-001 / P2 / Open**：两个同profile的B页面收到4410后，其中一个未回配对表单而进入key签验错误页，r6观察、r9复现，阻止将NA-031整例写通过。整体change是否待归档由主方结合reviewer及其他必需断言判断，本报告不代签。

## 版本与隔离

- 本tester只新增本报告及 `testing/probes/establish-node-access-r6-e2e-20261030/`。没有修改产品、规格、既有报告、索引，没有提交/推送/归档或创建账户。
- r6普通binary SHA256 `135BD9BDDABFDD530A5251AECB1DFF0DC640E81F80241EC53C8703C2336D1509`，复制至 `.tmp/r6-e2e-acceptance-20261030/bin/`。接到等待r9指示前已做的旧版A/B观察和A普通持久性保留，不当作最终r9链路。等待期间仅做独立身份副本保护；没有拿r6线上观察关闭NA-RV001/002。
- r9清单 `testing/runs/establish-node-access/2026-10-30-implementation-r9.sha256` 自身SHA256 **7876C7803106193C476EE0C23DAC27AA25036154CDF293D6A7EB7ECD2FEA6DB7**，83条。当前工作树逐项校验为82匹配、1不符：`app/src/pairing_http.rs`预期 `29466EACFF9F65F06826E1909EAAFAC58A032F2070552EE8B60EA50794977917`，实际 `03A9E54A70B3DB0CB52AF9417F6383C4E16BBE2FAAEBFDF5E4C95A473FE68776`。未自行断定差异仅为test；起始差异另存证据。
- r9受测原普通binary `.tmp/acceptance-20261030-r5-target/debug/acp-remote.exe` 与复制后的 `.tmp/r6-e2e-acceptance-20261030/r9/bin/acp-remote.exe` 前后均为SHA256 **AB97418E98D67A71E25BBF62B1E780F2F767530CA4290FB9C78E220A222A6024**。本次签署对象为该精确binary，不把源码工作树误写83/83，也没有从改动中的源码重编译冒充固定binary。主方全check及组件栅栏结果是交接信息，本tester未重跑或代签。
- r9所有运行资源独立在 `.tmp/r6-e2e-acceptance-20261030/r9/`，包括data、bin、profiles/A|B、target、证据。普通Edge分别用9338/9339自有CDP端口；产品30453，损坏副本30454。没有InPrivate参数；未触正常用户Edge/profile、30447、30451/30452。
- 全新A/B各自独立profile、真实Windows x64、SQLite/DPAPI/named pipe/WebCrypto/IndexedDB，真实受信HTTPS，无忽略TLS错误。证书直接引用既有受信证书路径，未导出私钥。ego技能的隔离/清理约束沿用；完整profile退出使用用户授权的自有Edge普通profile及其专属CDP，未关闭用户ego/Edge。

## r9真实网络及A/B链路

真实 `tailscale ip -4` 为100.119.123.57，系统DNS将faraday.tail74842e.ts.net解析到该地址，节点实际listen=100.119.123.57:30453。浏览器origin为 `https://faraday.tail74842e.ts.net:30453`，secure=true且正常载入受信页面。该轮是本机到真实Tailscale地址的路径，无代理/mock；没有声称跨另一台机器或证明VPN远端隧道流量。loopback与Tailscale连续身份的补充路径见后续有界NA-013包。

节点nodeId **5524a3cae75fffa3ccfb494dbd0bac2a99510c46078ccf0d4a0dda5cdc207f1f**，各次同data启动保持不变。r9节点PID依次5736、44476、44600、38652，均由本tester启动和关闭。

| 操作 | 已执行事实 |
| --- | --- |
| A配对 | 真实owner TTY运行device pair，Web表单认领，终端显示名称/公钥指纹/到期后明确输入y；A clientId=puhFbgzxIORzttg5YX6xQg，status=consumed、私钥不可导出 |
| B配对 | 独立普通profile同样真实本机批准；B clientId=Es1VJg0WgzHhigdzHprz1Q；通过Web新WSS完成身份认证和node.info/list/subscribe |
| list/订阅 | A首次revision1；B加入后A原连接收到devices.changed revision2并自动list看到B。B另开第二同profile页面，两条连接均有authenticated及subscribe成功记录 |
| A撤销B | 真实Web按钮“撤销此客户端”及“确认撤销”；A收到成功result和revision3，继续有效。两条B连接均于浏览器观察时间1793297846384收到4410/revoked/clean=true |
| B原key重连 | 原B CryptoKey只保留在一个B页面内存，不导出；额外真实WSS协议探针用该key签新鲜挑战，proofSent=true，4401/authentication failed/clean=true；不是仅用已被UI重置的新key试连接 |
| 同目录重启 | owner stop退出0、同data重新启动nodeId不变；A页面未手动点击认证而自动恢复原clientId并读取列表，B原key再签新挑战仍4401 |
| 最后组合stop再重启 | 见下节；旧pair401，A再次自动认证、原B4401，最后owner正常关闭所有节点 |

WebSocket观察器只记录帧类型、requestId、允许的公开管理result、revision和close；hello/proof正文及hostProof不写证据，私钥不传Node进程/节点。B拒绝探针是明确额外浏览器协议客户端，不把它说成被撤销后UI会继续使用原key。产品Web自己核验hostProof的成功链由真实配对/认证/管理完成；额外命令探针也在页面内验证节点签名与绑定后签名，不模拟认证。

## NA-031普通profile完整退出持久性

r9 `persist-cycle.ps1`确认A专属普通profile进程12个，通过该profile的Browser.close全部退出，随后CIM匹配同完整路径进程数0；同一个profile目录重开、未清storage、未重配对。`A-persistence-comparison.json`实际为samePublicKey=true、sameClientId=true、sameNodeId=true、nonExtractable=true、authenticated=true、proofObserved=true、subscribeObserved=true。重新打开页面自动完成认证，无需手动认证按钮。

这是本版真实跨完整退出证据，非只刷新、关闭单tab或套用旧报告。未声称同一浏览器所有用户profile都退出，仅本tester拥有的完整profile进程树为0；用户正常Edge未触及。r6同类12→0成功仅作为历史记录，r9本轮已重跑。

## NA-E2E-001：同profile多页面撤销后的重新配对失败

- P2，Open；r6已有一次观察，r9用全新data/profile重复同一动作后再现。实际复现条件：B的同一个profile两个页面均已认证并订阅 → A撤销B → 两连接均4410 → 第一个页面显示直接配对表单，第二个页面STOPPED/“无法准备重新配对”，并提示“浏览器保存的密钥无法读回或签验失败”。
- 当前NA-031 Expected要求4401/4410停止旧身份重试并直接展示配对页、使用新key重新配对仍需本机批准。第二页面状态不符合这一要求；这不是撤销未生效、认证绕过或原key复活。
- 负责原始证据：`B-all-revoked-final.json`两个target的close/events/body；`final-summary.json`分别pairForm=true/recoveryFatal=false与pairForm=false/recoveryFatal=true。r6相同证据在上一层运行根保留。
- 目前仅定位到共享同origin身份的多页面恢复路径；“并发重建key竞争”是待核实解释，不作为已证实根因。没有修改产品/测试预期或代签关闭。此问题应由主方安排修复后由本tester复查；原key4401及服务端两连接4410结果不受该UI失败否定。

## NA-015组合stop：实际时序与限制

第一轮真实外部SQLite BEGIN IMMEDIATE写锁尝试：额外认证协议连接已subscribe，发送重复撤销B命令；stop前检查发现它已返回存储错误，不再在途，因此 **不将第一轮计作在途命中**。这一轮未完成TTY pair被取消，重启后旧码lookup为401/pairing_invalid。

第二轮在隔离真实数据库临时加入明确的测试工作负载触发器 `na015_test_workload`：在audit insert前实际计算4000000项递归sum，以有界SQL工作延长真实命令执行。没有改产品、注入内部模拟栅栏或模拟存储；额外SQL只存在自有数据库，结束后DROP并核验不存在。正常A/B配对/撤销及重启均在无该适配器时执行。

此时两个产品A页面已真实认证订阅，另有一个已认证并subscribe的协议探针，以及新建但未完成的真实owner TTY pair。实际发送重复撤销B命令（不改变A资格）：

| 事件 | 实际观察时间/结果 |
| --- | --- |
| 命令发送 | 1793298232881 ms |
| stop前检查 | 1793298232945 ms，outstanding=true、该requestId无result（发送后64ms） |
| 启动owner CLI stop | 1793298232945 ms |
| owner CLI完成 | 1793298234066 ms，退出0，state=stopped |
| 真实事务结果 | stop后SQLite management_commands有该请求的已提交记录；B仍revoked，A仍trusted，审计计数3→4；没有声称撤回已提交动作 |
| 未完成pair | TTY退出1，“pairing was cancelled”；重启后旧码lookup401/pairing_invalid，不能恢复旧配对 |
| 连接与新操作 | 两产品A连接观察到1001/node stopping；额外承载命令的协议socket最终4500/storage unavailable，未收到该命令result；停止后新WSS尝试未open，1006（网络不可连接），不是应用node_stopping响应 |
| 重启 | 同nodeId，A原key/clientId自动认证读取列表；原B key仍4401；最终owner再次正常stop |

时间为本机Date.now及浏览器JS事件处理的观察时间，不是内核收发或服务端栅栏。后台Edge可延迟dispatch，不能拿晚到的close回调证明stop提前报成功；也不能把所有连接统写为1001。额外probe的4500与结果丢失作为本例实际异常保留，不能扩成“所有在途请求均成功回包”；本用例预期要求真实结果收敛与资源释放，不承诺已关闭socket还能收到成功。节点stderr为空、stdout有正常stopped，事务已提交且四处锁/pipe/进程均释放；没有据此签NA-016的15秒期限/失败保存报告。

结论：原Steps的未完成pair、多订阅、真实在途操作与owner stop组合已实际运行；持久状态可恢复、配对无恢复、无孤儿资源已验证。**NA-015按当前原Steps/Expected通过这一组合范围**；协议probe异常关闭的进一步语义可交reviewer检查，不以额外自造“每个连接必须1001”门禁改写原用例，也不掩盖4500事实。

## NA-017损坏副本与平台证据复用

最终停止r9节点后，仅复制本例data到三个独立damage目录，逐例用同r9普通binary、30454启动：

| 注入 | 实际结果 |
| --- | --- |
| key-corrupt | 副本密文替换为无效测试字节，退出1，CryptUnprotectData os error87，不报ready |
| key-unreadable | 测试进程实际以FileShare.None持有副本identity.key，退出1，“reading the protected node identity”/os error32，不报ready |
| meta-mismatch | 仅副本node_meta.node_id改为不匹配值，退出1，key/meta不描述同一身份，不报ready |

每例启动前后副本key哈希相同、没有新key替换；node_meta、两条client记录和审计count=4逻辑状态均保持注入后值，未清库或悄悄修复/替换。原data状态也未变；每个副本.node.lock均独占重开成功，无残留pipe。SQLite探针使用Node已有内置node:sqlite（输出实验性提示），未加产品依赖。

r6准备阶段首次试图仅加key拒读ACL，产品启动secure_managed_files将其按设计重新收紧为owner可读，节点正常ready，不能算不可读失败保护通过；脚本随后只终止该自有副本进程并恢复测试ACL，不归为产品FAIL。有效最终r9注入改为真实独占文件句柄；`.NET Process`采集到三例实际退出码1，不用初次Start-Process返回的null ExitCode冒充1。

DPAPI跨另一真实SID未新执行，沿用独立平台报告中已有真实1004 SID可读密文解密失败的子断言，依据platform/windows.rs负责字节及固定entropy/CurrentUser作用域未改变，见 `2026-10-30-platform-acceptance.md` 的SHA256表与原始证据有效性节。不是客户端ACL拒读替代解密，也没有创建账户。结合本轮真实A有效/B撤销重启及三类副本失败保护，NA-017原断言可按“本轮动态+已核实平台子项复用”判通过；不声称本轮五条异SID CLI矩阵已跑。

## 当前用例状态与交接

| ID/范围 | 本包结论 |
| --- | --- |
| NA-032真实Windows A/B受信TLS/Tailscale闭环 | PASS（精确r9binary）；真实依赖无mock，后加SQL负载仅用于NA-015补验 |
| NA-017持久信任/损坏保护 | PASS（动态重启/副本＋既有跨SID DPAPI子项复用） |
| NA-015组合停止 | PASS（上述真实在途/未完成pair/多订阅组合与资源结果）；4500及事件时序限制保留 |
| NA-031普通profile完整退出原key/ID认证 | 子项PASS；整例因NA-E2E-001为FAIL，其他未在本包执行矩阵也不代签 |
| NA-RV001/002 reviewer问题 | 本tester未运行组件内部栅栏；最终线上r9结果提供集成证据，不代替原reviewer关闭其问题 |
| NA-013 loopback/Tailscale完整有限矩阵 | 尚待下一有界路径补验，不能只凭本轮URL包含Tailscale就签整例 |

## 清理与证据

最终 `final-cleanup.json`：自有A/B profile匹配Edge进程0，30453/30454/9338/9339监听0；主data及三个damage目录的派生pipe均不存在、各.node.lock独占重开成功；自有binary仍AB974…A6024。只关闭本tester profile，保留隔离节点数据库/身份/profile和脱敏证据，不删用户数据。未清全局cookie/cache，未操作其他agent文件或端口。

有效最终证据根 `.tmp/r6-e2e-acceptance-20261030/r9/evidence/`：A-before-B/after-B/restarted/reopened/final-restored.json、B-original/second/all-revoked-final.json、B-old-auth-before-restart/after-restart/final-restart.json、A-full-exit.json、A-persistence-comparison.json、combo-stop.json（未命中在途的第一轮）、combo-stop-workload.json、after-combo-stop.json、old-pair-first-stop/workload-stop.json、damage-prepare/after/results-v2.json、final-summary.json、final-cleanup.json、source-mismatches-at-start.json。未导出私钥、CryptoKey或proof正文，短期测试pair输入只在.tmp，报告不记录码。

复现入口为本probe目录setup-r9.ps1及CDP/browser/persist/damage/combo脚本；环境变量ACP_E2E_ROOT显式选择r9根，避免r6/r9证据混用。源码/工具错误（UTF8未指定导致PowerShell读旧JSON失败、第一次过早采B close、旧退出码采集null）均未当产品失败，最终以UTF8 Node读取、实际命中关闭、独立Process退出码重新核对。

## 重要追加：NA-SC003使旧Web整包固定性结论失效，等待R15

主方独立供应链review确认rust-embed默认debug程序运行时读取构建dist。**本报告此前“精确r9 binary”的固定对象仅成立于exe本身，不能证明当时Web资源已嵌入或与该源码清单唯一绑定。** 上文NA-032 PASS不得再用作最终冻结单包验收签署；此前浏览器侧结论仅保留当时真实页面/协议观察及可分离的机制子项，待R15固定Web资源的正式binary重验最终受信链。不能把后来开启debug-embed倒称为旧exe已有能力。

本轮未保存每次加载的完整Web bundle字节清单，不能事后把旧运行中Web内容精确归因于r6/r9版本。NA-E2E-001保留为真实运行中多页面恢复异常及待固定整包重现，暂不把“r6/r9均复现”扩成两个冻结Web版本的独立回归结论。普通profile完整退出原key/ID、真实DPAPI副本保护等可分离观察仍保留，按负责路径复用，不要求因该嵌入问题把所有机制测试重新运行。

NA-013新链尝试在任何运行资源创建前核对当前构建目录binary，发现已不等于R12的92D9…DD44（实际CD6C…56A5），setup按设计退出，**没有启动该次节点或浏览器，也没有执行loopback测试**。不能将失败的启动检查写成NA-013路径通过。现已停止新增关键完整链步骤，等待主方提供R15正式单包binary与清单；不会继续旧不固定Web资源的重复全链。

## R16/R17正式嵌入包最终补验

以下为NA-SC003之后的新实际运行，优先于上文等待状态及旧Web版本归因。只使用tester自己的bin副本，没有直接运行会被主方后续构建替换的target产物。

### 固定来源与资源字节

| 版本 | manifest SHA256（83项） | 普通binary SHA256 |
| --- | --- | --- |
| R16 | FCA2669CFC561CC159C498F19262D9B893A0E590F4FEA8D5EF3169059F74D8E3 | 3EBF8E44380DF74082A588E5506CB2F3DA91A3B2DA8E94265F5837E1DD96709B |
| R17 | 5074890C209A702335F13042A26DBA64278EFD7A7B0A70A4E42E2F698BA3E26F | 38AA9D02F334E3C81FDC900ED4F8943BB78339650981BC0046A609381D03E6C2 |

R16开始时当前源码83/83匹配；另独立校验 `.tmp/frozen-implementation-r16-20261030` 83/83匹配。R17直接独立校验其冻结源码83/83。复制后和清理后自有bin摘要均未变。R17最初取得正式产物时已复制到自有bin，主方随后提供永久入口 `.tmp/frozen-implementation-r17-20261030/bin/acp-remote.exe`，本tester额外核对其摘要与自有副本完全一致；后续复现脚本已改用这个固定路径，不再按旧hash追逐main target。

R16/R17实际返回的index、JS、CSS完整UTF8字节均可在对应自有exe原始字节中找到；不是只比入口URL或exe摘要。R17返回三资源还分别与冻结副本Webdist文件摘要一致：

| R17资源 | 字节数 | 实际响应及冻结Webdist共同SHA256 |
| --- | --- | --- |
| index.html | 529 | DE3C5092BD012A99B1F7BF9F5FBA3E8542699565D69FD307DD6E7E120E1C3C83 |
| assets/index-BlW8fYdm.js | 60710 | 81E6A4A199706A51C221EC365FE6A6D7EA0E0319E2E9C7970D510C9DDEA6D53D |
| assets/index-0luCBpbO.css | 5767 | 59F460F235F69FC261AB0D97C4518EE9FB511717330AFA09C4DA71276C5B3880 |

冻结Cargo配置已启用rust-embed/debug-embed；R16运行开始、重启后返回三资源摘要也一致。这里只签署本次实际包/字节检查，不倒称旧debug exe已内嵌，不把本检查代作原供应链reviewer的全部迁移/构建树不可见验收或问题签署。

运行根为 `.tmp/r6-e2e-acceptance-20261030/r16/`、`r17/`；R17沿用完全属于本tester且已正常关闭的R16 data/profile以验证身份连续，其新bin/配置/日志/证据另存r17根。真实节点nodeId始终为 **b7962018bb159230a2718e805843753b1573b45dde2ec8e5e841d3f0a73c9009**。证书仍仅引用既有受信路径，没有读取/导出私钥或忽略证书错误。

### NA-013：单listen切换的真实有限矩阵

先R16 listen=127.0.0.1:30453，自有普通Edge仅用进程参数将canonical域映射127.0.0.1；URL、SNI、Host、Origin始终是 `https://faraday.tail74842e.ts.net:30453`。节点停后只改listen=100.119.123.57:30453，同data/key及canonicalOrigin不变；A普通profile的8个匹配进程全部退出为0后，同目录重开且不再带host-resolver-rules，使用正常DNS。

发现用户系统代理启用后，最初只加域映射的尝试出现ERR_CONNECTION_CLOSED，没有进入应用，未当产品拒绝或功能通过。只对本例Edge进程加入 `--no-proxy-server` 后路径成功；没有改变用户全局代理配置。curl直连诊断因系统证书吊销查询离线失败，也没有加关闭吊销/忽略证书的选项绕过。有效浏览器运行均由实际securityState=secure确认。旧r9没有同样的socket/无代理记录，所以旧段落“无代理”不能仅凭域名/DNS认定；最终以下直接证据取代该归因。

| 断言 | loopback实际结果 | Tailscale地址实际结果 |
| --- | --- | --- |
| 浏览器主文档及JS/CSS remoteIPAddress | 127.0.0.1，remotePort30453 | 100.119.123.57，remotePort30453 |
| HTTPS安全状态 | secure，证书issuer YE1 | secure，同域/证书 |
| 未认证node.info | 4400 invalid frame，无数据交付 | 4400 invalid frame，无数据交付 |
| 未认证device.list | 同上 | 同上 |
| 未认证subscribe | 同上，无devices.changed交付 | 同上 |
| 原A真实持有证明后的node.info/list/subscribe | 三项实际成功result | 三项实际成功result，自动认证无需点击 |
| 身份连续 | A clientId=u8wWc2lfU5RBDVM77rj2ow | publicKey/clientId/nodeId与前次完全相同，privateKey仍不可导出 |

未认证探针按操作各建立真实WSS，在hello/认证之前发送对应帧；4400是协议前置拒绝，不冒称应用成功错误result或4401。有效身份路径使用真实产品Web产生证明，不mock应用。该轮是同主机真实Tailscale地址直连，不声称跨另一台机器或VPN远端隧道流量。`NA013-path-comparison.json`及两份 `loopback-browser.json`/`tailscale-browser.json`含完整路径元数据与三项结果关联。**NA-013原有限矩阵PASS。**

### R16完整A/B、普通profile持久性及NA-E2E-001关闭

A在loopback阶段真实Web认领、owner TTY显示名称/指纹/到期后输入y；B在Tailscale阶段独立普通profile真实认领/TTY y，clientId=cLAnJcMu-hOchGftugP6Kw。A原连接由revision1观察B加入后的revision2并自动list；B同profile第二页也真实认证和subscribe。A真实Web确认撤销B后，两个本例B连接均4410，A仍有效并看到revision3。B原key只保留在页面内存作新鲜auth证明，重连4401；节点同目录重启后A原key/ID恢复、B原key仍4401。

R16两个B页面本次均直接回配对表单、没有key签验错误。透明计数器仅包装原生generateKey和本应用identity store.delete计数，不模拟行为；合计删除1次、生成1次，两页读取同一个新不可导出key、均实际签验通过，且新key不同于被撤销key。**NA-E2E-001由原tester在正式嵌入包原路径复查Closed。**

R17保存路径变化后只补受影响回归：A自动复用原key/clientId；B用其未绑定replacement真实配对成为TwVLu0a5A9mcCFitBBWc_A，两个页面真实认证/订阅后A撤销；再次两页4410/直接表单，删除1、生成1、同key、两页真实签验通过、原key重连4401。两页navigator.locks可用。Web Locks是浏览器提供的同源跨页面互斥锁，测试未用假的锁实现替代。原reviewer的stale-save问题由其自己Closed（用户交接），本tester不以本次E2E代关该reviewer问题，也不宣称已执行其所有确定性交错。

R16普通profile完整退出的直接证据同时覆盖NA-031指定持久性：A的8个专属进程→0，同一profile/key/ID在Tailscale阶段重开自动认证并执行三操作；R17再次在所有自有profile已关的条件下重开，仍是原key/ID。不是InPrivate，也不是只刷新页面。R16组合stop后原页面在探针10秒窗口未观察到热重连，脚本随后关闭了页面，**该次热重连结果保持未验证**，不以重新打开替代；之后同profile实际重开已自动认证，证明重启后资格/持久状态仍有效。此前普通正常重启后的原页自动认证也有独立记录。本包不把NA-031全部重连/未知结果不重发分支一并签过。

### R16组合stop补足NA-015

同R16单包、两个已认证订阅A产品页面、另一个已认证并subscribe的真实协议探针、一个未完成的真实TTY pair同时存在。在隔离数据库内短暂加入前述有界SQL工作负载触发器，只用于使真实命令保持在途，正常A/B链没有该适配器。

- 命令发送1793299563956；stop前1793299564008，outstanding=true且无result（发送后52ms）；同刻启动owner CLI stop，1793299564976完成，退出0。
- stop后management_commands确有该请求已提交记录；没有成功命令响应交付保证。两个产品订阅连接收到1001/node stopping，额外承载命令探针为4500/storage unavailable；本轮wasClean=false如实保留，不写成全连接clean close。
- 未完成pair退出1且明确cancelled；同data重启后旧码401/pairing_invalid。新WSS停止后未open、1006；这是网络不可达，不冒称应用503或node_stopping响应。
- nodeId稳定，原B新鲜认证仍4401；A重开普通profile后原key/ID实际认证、list成功。测试工作负载trigger已删除，资源检查通过。没有把外部SQL负载当作内部actor/enqueue模拟栅栏，也不据此签15秒关闭超时用例。

按NA-015现有Steps/Expected，真实在途、未完成pair、多订阅组合及实际状态收敛/资源释放已经执行，范围PASS；异常4500、非clean关闭及未观察的那次热重连保留供reviewer核查，不隐去或转写为理想时序。

### 后端副本保护复用的准确依据

R9实际普通产品binary上的三类副本失败（key损坏87、真实独占文件使key不可读32、meta不符，均退出1、无ready、不替换/清库）仍可按负责代码复用，**不依赖旧Web是否嵌入**。R9与R16/R17/R18以下文件完整摘要相同：identity.rs `8EE7C63F149B689F70324C6C0962C5F8DD303E2E76A86BA6DB9E341F841BF600`，storage.rs `B6532B45DE877CD2D8F15FA79F36AE18A29CB611C06BA31B5A8865EA7723515B`，data_dir.rs `DBD57605D59201DEF4F95AD5F41C9A1F85B6881211644DE539DA15B570F613C5`，Cargo.lock `28639ACDBA0A6C445FCA88F9191E3E1484DEECA21E63F0421BD2E2DFD9AB6B5A`。platform/windows.rs整文件后来有变，不能声称全文件与早期一致；CurrentUser DPAPI跨SID子项按已接受的独立平台有效性分析复用，不新造另一SID执行或把客户端ACL错误当解密失败。

R16→R17冻结source diff：node.rs差异位于tests模块；management_ws.rs增加cfg(test)统计/fixture，此外将相同send_json失败的map_err表达式改写为等价if/return（仍4429）。不把整个文件说成字节不变；生产预算/错误条件未改，依据该具体语义范围复用正常组合停止，未把新增慢writer测试当成我已执行。R16/R17/R18的management/client.ts、management/protocol.ts、pairing/api.ts、pairing/session.ts摘要均保持相同；R17/R18的identity.ts同为 `B1312C39BE307DA7E1D9C8D0AE500F8F79BE193A1F0AFDE765CAACACCC165F92`。R17新保存路径已实测，R18仅追加下面明确变更分支。

## R17 NA-026真实浏览器与批准事务失败联动

该包在上述R17其他实测正常关闭后才启动，bin仍38AA…3E6C2；只使用自有R16 data及B普通profile，node输出/evidence分开保存。没有改产品或复跑NA-007所有SQL/COMMIT故障矩阵。

适配器只有自有SQLite的 `BEFORE INSERT ON security_audit WHEN NEW.action='device.pair.approve'` trigger，执行RAISE(ABORT)。不拦截其他action、不假返回网络成功、不改变UI预期。真实Web lookup/claim后owner TTY核对测试名称/指纹并输入y，实际得到退出1、`local IPC failed: storage_unavailable`。

| 必要断言 | 实际结果 |
| --- | --- |
| 实际事务回滚、无半信任 | 客户端列表3条、总audit6、批准audit3、revision5均与故障前完全一致；owner list只有已有A trusted及两个已有revoked，无失败申请新身份 |
| 浏览器失败状态 | status=cancelled，clientId=null，无AUTHENTICATED UI；页面明确本机取消、可新配对，未假显示已授权 |
| 真实管理拒绝 | 失败申请key仍能正确签新auth proof，但真实WSS4401/authentication failed、未认证成功 |
| 仅删除自有trigger后恢复 | 新配对码/同浏览器key，真实TTY再次y，得到新clientId=1LScnT6ISYXYC8s0zw316w；页面实际AUTHENTICATED并node.info/list/subscribe成功 |
| 正常提交可持久 | clients3→4、approve audit3→4、总audit6→7、revision5→6；刷新浏览器及同data节点重启后原key/clientId不变且再次真实认证 |
| 配对凭据不能作长期认证 | 见下段真实新鲜proof，未导出秘密 |

捕获的原claim proof只存在页面内存，签名验证有效，但到跨用途测试时已过期；它的4401仅作为观察，不用它单独排除“只是期限拒绝”。随后真实 `/pair/challenge` 200，浏览器内存生成新鲜 `acpr-pair-status+jwt`，先送真实pair/status得到200且hostProof签验/clientId绑定成功，再送长期auth入口；当前client确已trusted、proof尚新鲜，仍4401，未得到authenticated。这是明确的配对用途凭据拒绝，不声称单独隔离typ/挑战绑定的每种原因；也不冒称对新鲜claim和status所有变异做了穷举。

移除trigger后才进行恢复，最终triggerExists=false。没有导出claim/status proof、私钥或CryptoKey；JSON只含状态、公开身份元数据及布尔/计数。终止浏览器清除了内存proof。证据在r17/evidence：NA026-install/failure/drop/recovered/final-sql.json、B-na026-failure/recovered/refresh/restart.json、B-old-auth-na026-failed-approval.json、NA026-cross-purpose-original.json（过期观察）、NA026-cross-purpose-fresh-status.json、NA026-summary.json。**当前NA-026所列正常持久化、实际批准事务失败后的浏览器/管理结果、跨用途拒绝均已补齐，PASS；不以NA-033人工理解性判断替代这些客观检查。**

## R18最小晚到分支：真实旧4410遇到已consumed replacement

R17全部case完成并正常清理后另开本包。固定入口 `.tmp/frozen-implementation-r18-20261030/bin/acp-remote.exe`，SHA256 **328D7CEBCA708A300D48AA673204E4E52F1131E6E05951F85DA4601DA7AA2800**；复制自有r18/bin后才运行，前后摘要一致。manifest SHA256 **614413C0243680E6E7E0DF3C98141F0D568DDCAA042209EB9C996A99881E5B8A**，冻结source独立83/83匹配。只复查R18新增main分支，没有重跑无关R9全套。

沿用完全自有data/profile：B此前NA-026真实可信身份1LScnT6ISYXYC8s0zw316w在R18两页面均先经真实产品hello/challenge/proof/authenticated并subscribe。原页在创建真实WebSocket后，只把产品的close事件listener用透明转发器包装；收到原生4410时保留**该实际CloseEvent对象和原listener**，暂不调用，其他产品验证不改。独立原生close日志仍记录事件，native event.isTrusted=true。没有设置或伪造hostVerified、没有构造假的challenge/close、没有修改身份库或跳过产品hostProof验证。

真实owner revoke旧client后：原页真实socket已4410，但仍显示原AUTHENTICATED UI，队列恰有1个真实回调；另一普通页面正常处理4410并旋转到新key/直接表单。接着在另一页真实输入新码、本机TTY显示名称/指纹后明确y；新身份 **Neuzp5nIf2ao8L6xnINGNQ** 实际consumed且页面已认证后，才放行原页那1个原生回调。

`late-summary.json`最终断言全部为true：原页随后自动认证并显示replacement clientId、没有配对form；另一页仍认证；两页DB记录均consumed且clientId/key一致，两页实际签验成功。**总delete=1、generateKey=1**，即旧回调没有再次删除不同新key或生成第三key。这实际覆盖“晚到旧4410＋另一页replacement已消费”的分支，不把pending/approved所有替代状态都说成已动态穷举。fresh unbound原路径沿用R16/R17独立实测；R18此分支PASS，NA-E2E-001保持Closed。

证据根r18/evidence：version.json、late-original-authenticated.json、B-other.json、late-held.json、late-released.json、B-rotation-fix.json、late-summary.json。第一次沿用unbound收集器误等form且读取不存在的B-original文件失败，未当产品FAIL；之后仅修正测试收集器为consumed状态、使用真实已有B-other基线再读取，**没有重放或伪造旧回调**。原回调只实际释放一次。

R18三资源也实际返回200，完整字节均在自有exe中：index SHA256 `02830F2F514A7023B3026BE7AAD0375EE3BE0E65180F23DDA100FFB00942727D`，JS `assets/index-DpQyfqZ1.js` 60755字节、SHA256 `E7D2D3679EB07C5D5033E265A7FC8A2F308B41F5E00EC1A9749BDD3B820F355B`，CSS仍59F460…B3880。没有将旧非冻结Web版本自动关闭结果套给R18。

## 最终交接状态（按断言/版本复用，不扩为全change代签）

| 范围 | 最终结果及边界 |
| --- | --- |
| NA-013 | R16实际loopback/Tailscale同origin三项正/负矩阵PASS；R17/R18网络/协议负责路径未变，受影响保存/恢复路径分别新测 |
| NA-032 | R16正式嵌入包实际A/B、受信TLS、真实Tailscale地址、批准/认证/list/订阅/两B4410/原key4401/同data重启/关闭PASS；R17保存/旋转及R18晚到分支分别补测，其他未变路径有据复用 |
| NA-015 | 正式R16真实在途SQL工作＋未完成pair＋多订阅组合及资源结果PASS；4500/非clean关闭及那次10秒热重连未观察如实保留，不假称模拟内部栅栏或每请求都回成功 |
| NA-017 | 新正式链保留nodeId/A资格/B撤销；身份/存储负责字节相同的R9三类副本实测＋已接受DPAPI跨SID平台子项复用；没有新执行另一SID |
| NA-026 | R17本包真实批准失败→浏览器未获权/真实WSS拒绝→移除trigger新配对→刷新/重启原ID→新鲜配对status proof跨用途4401，PASS；R18对应Vault/API字节未变，主视图改动分支另测 |
| NA-031本包指定子项 | 普通profile全退出原key/ID、双页旋转与consumed replacement晚到旧事件恢复PASS；其他case分支仍按主方总矩阵，不自动从一个问题Closed升级整例PASS |
| NA-E2E-001 | Closed，原tester在R16嵌入包关闭，R17复查保持，R18新增晚到分支也真实通过 |
| stale-save/NA-SC003及其他reviewer问题 | 本包提供对应集成/包字节证据，问题关闭仍由各原reviewer负责；不代签 |

最终自有Edge进程匹配数0，30453/30454/9338/9339监听0；主data、R15 data、实际供R16/R17/R18共用的R16 data、R9 data及三损坏副本派生pipe均不存在，所有检查锁FileShare.None独占重开成功。R18 owner stop退出0，PID25636已退出。运行SQL测试trigger均已删除；自有数据库/身份/profile和脱敏证据保留，没删用户身份/数据库、没改普通用户ACL/全局代理、没创建账户、没碰30447/30451/30452，未启动Harness。

最终资源原始汇总仍在r17/evidence/latest-cleanup.json（最后执行已纳入R18 binary摘要），R16/R17汇总为latest-summary.json，R18最小包为late-summary.json，NA026为NA026-summary.json。只写本报告和自有probe源码，所有运行bin/profile/SQL/JSON/log在.tmp；未提交、推送或归档。主方可按上述版本/断言聚合，不把本报告作为其余必需用例或独立reviewer的代签。
