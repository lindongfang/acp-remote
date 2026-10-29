# NA-031 浏览器身份异常独立补验（r5a）

结论：高收益动态子项已执行；初始化写失败、认领前写失败、已有坏key不静默替换、新HTTPS origin隔离均通过。清本站数据库后缺少“先在本机撤销旧设备”提示，提出 **NA-STORAGE-001 / P2 / Open**，该失败断言使本报告范围内NA-031不能判PASS。私密模式未执行，新增自动重连/认证/取消/未知结果不重发等矩阵不在本次补验范围，不代签其他tester。用户告知r5a整体10PASS/23PARTIAL、NA-033通过作为交接背景保留，本报告不自行修改主方统计或NA-033结论。

## 版本、隔离与实际环境

- 独立tester，只新增本报告及 `testing/probes/establish-node-access-browser-storage-20261030/`，未修改产品、spec、现有报告或平台gate；未创建账户、提交、推送、归档。
- 实际先逐项验证 `2026-10-30-implementation-r5a.sha256`：79项，0不符；清单SHA256 **96F7B25DA1B7ABF3C5F65D069723CA87B095800F08967720AADBD4FC621975EC**。
- 原始普通binary `.tmp/acceptance-20261030-r5-target/debug/acp-remote.exe` SHA256 **2DC0AFA04B55376797587136048F5EFBE7D665FF7EE69525DA248FA4D73305F2**。先校验，再复制到自有 `.tmp/browser-storage-acceptance-20261030/bin/acp-remote.exe`，副本校验一致，运行及清理后摘要仍一致。
- 用户随后告知r6开始修改identity.rs的test-only崩溃钩子。本轮始终执行已复制r5a binary，没有编译r6或把后加源码钩子当本报告运行结果；开始时79/79与结束时binary不变是不同检查，未声称r6解冻后的工作树仍79/79匹配r5a。
- 新运行根 `.tmp/browser-storage-acceptance-20261030/`；配置/data/bin/target目录和JSON证据均在该根。没有新构建。证书直接引用旧受信Tailscale证书路径，没有复制或打印私钥。
- 两个自有节点：`https://faraday.tail74842e.ts.net:30453`（PID44052，nodeId0195885d251b212e2efa5fd60dcde8ba096ab9611d21041fadee9c787c8cb259），及同主机30454（PID28760，nodeId74ea4df6d2a10e6896da31f282a029244c2b5d3668521298a32c559e61baf740），各自全新data目录。未占用30447/30451/30452。
- 按ego-browser skill及完整v2.1.0 guide操作，清存储前已读取 `references/clearing-state.md`。真实ego Chromium UA为Chrome/156.0.0.0，Windows x64；受信HTTPS正常加载、`isSecureContext=true`，未忽略TLS证书错误。
- 仅一个agent-owned TaskSpace8，页面p1/p2；没有清profile/global cookies/cache。ego自身浏览器profile由应用管理，本轮未把它复制到.tmp，也不声称它是本轮新建的.tmp独立磁盘profile；隔离边界为两个本轮专用origin/应用DB。所有本轮生成的浏览器CryptoKey在收尾已通过仅删除两个origin各自的应用DB移除；没有导出私钥。Node DPAPI身份仍保留在隔离data中。

## 用例依据及真实/注入边界

当前NA-031要求真实Chromium IndexedDB不可导出CryptoKey读回签验，写失败不认领、不假成功，已有坏key不新建，丢失提示本机撤销再配对，新origin不迁移私钥/自动信任；完整退出持久性、私密模式和后加管理重连矩阵仍须分别判断。

故障适配器只在测试页面的JS环境覆盖原生 `IDBObjectStore.prototype.put`，匹配 `this.name==='identity' && this.transaction.db.name==='acp-remote-access-v1'` 后抛 `DOMException(...,'QuotaExceededError')`。其他DB/存储保持原API。页面生命周期前置钩子仅用于初始化写失败与生成计数；认领前写失败在健康初始化后安装，完成后恢复原put。真实WebCrypto、IDB读取、TLS、lookup、节点、批准和认证不模拟。该注入代表明确的写入异常，不声称测试过磁盘满、浏览器内部序列化失败或事务COMMIT失败的全部原因。

清数据只执行当前origin的 `indexedDB.deleteDatabase('acp-remote-access-v1')`，前置校验location.origin；没有 `Network.clearBrowserCookies/clearBrowserCache`、无scope Storage.clearCookies，也没有清理正常profile的其他origin或cookie。删除后刷新是真实页面重新启动，不替换产品boot逻辑。

## 实际动态结果

| 子项 | 结果 | 执行证据与精确断言 |
| --- | --- | --- |
| 健康初始化/即时存储 | PASS（即时子项） | 真实P-256 private CryptoKey为extractable=false，status=unbound；本地读回后可完成真实claim/认证。即时读回不当作完整退出持久性 |
| 已有坏key记录 | PASS | 保留已有record/publicKey，其privateKey改为真实不可导出HMAC CryptoKey（不合法的身份key）；刷新后显示“浏览器保存的密钥无法读回或签验失败”“本页没有生成替代密钥；请先在节点撤销旧设备，再清除本站点数据后重新配对”；crypto.generateKey计数0、identity put计数0，publicKey未变，坏HMAC仍在记录中。不是新造空DB正常路径 |
| 首次身份写失败 | PASS | 仅删除本轮DB后，在产品boot前注入精确put失败；generateKey=1、匹配put=1、claim=0；记录不存在，页面STOPPED/“浏览器身份无法写入 IndexedDB，认领未继续。”没有假成功 |
| 真实lookup成功后的认领前写失败 | PASS | 健康boot后才注入put故障，真实owner TTY有有效配对会话；UI填码/名称认领，lookup计数1、put失败1、claim计数0；持久记录仍unbound、clientId=null、pendingClaim=null，页面明确“认领未继续” |
| 解除故障后的真实可信身份前置 | PASS（后续丢失测试前置） | 恢复put后用有效码重新认领，本机TTY明确y批准，clientId=8ITdaUgOU4-98pVlOiAQjw；页面NODE CONTROL / AUTHENTICATED、显示可信设备，IDB status=consumed，私钥不可导出 |
| HTTPS新origin隔离 | PASS | 同ego TaskSpace/profile新页面进入30454，origin变化但主机/证书相同；得到不同publicKey的不可导出CryptoKey，clientId=null、status=unbound、普通配对页；未迁移30453私钥/自动信任。仅覆盖端口改变这一真实HTTPS origin变化，无需穷举浏览器 |
| 清本站DB后丢失与提示 | **FAIL：NA-STORAGE-001** | 从上述真实consumed身份仅删除本轮30453 DB，再刷新；新publicKey、unbound/clientId=null/nodeId=null，未继承权限，但只显示普通配对页，全文没有“撤销”。owner device list随后确认旧clientId仍Trusted。缺少原Expected的本机撤销指引 |
| localStorage/sessionStorage秘密 | PASS（本轮页面子项） | healthy、write-failure、lost-state记录中键名均为空；未将私钥复制到节点或证据JSON。网络观测仅统计路径不记录proof/code正文，不声称做过全部HTTP历史秘密扫描 |
| 私密模式 | NOT-RUN（ego路径缺入口） | guide没有文档化InPrivate/incognito创建API；本轮未另启自有Edge InPrivate。机器确有Edge，不能说机器绝对不能测；此项仍未执行，普通profile结果不能替代 |

## NA-STORAGE-001：清站点身份后缺少本机撤销指引

- 严重性：P2；状态Open；受测版本为上述固定r5a普通binary。未修改预期来迁就页面，未修改产品。
- 复现：真实本机批准的浏览器身份已consumed且能管理 → 只删除该origin的应用DB → 刷新 → 页面回普通配对表单；节点侧旧clientId仍Trusted。
- 预期：按当前NA-031 Expected，“丢失提示本机撤销再新配对”。应有安全可理解的旧设备撤销指引；本报告不要求浏览器在全部本地资料丢失后准确识别过去clientId。
- 实际：页面仅说明运行device pair并终端批准，没有撤销提示。新key未获信任，故不是认证绕过或私钥泄漏，失败范围仅为丢失恢复提示。
- 原始证据：`evidence/cleared-site.json`中differentPublicKey=true、unbound=true、revocationGuidance=false；`evidence/trusted-before-clear.json`为清理前真实身份；owner CLI `device list`输出旧ID `status=Trusted`、`revokedAt=-`。
- 本轮未替用户撤销/删除旧节点记录，隔离数据库保留用于复查。改动建议及修复由主方另包处理，本tester未实现或签署关闭。

## 跨完整退出持久性的旧证据适用性

旧 `2026-10-29-independent-tester-regression.md` 明确记录终止A profile全部14个Edge进程、匹配进程为0、同profile重开，原不可导出key/clientId仍可认证；原证据定位 `evidence/l6-final-state.json`、`evidence/close-code-read.json`及报告完整浏览器退出节。这是旧版实际完整退出证据，本轮没有关闭用户ego整个profile或把两页面关闭充作完整退出。

源码摘要比较：session.ts在旧71项清单和r5a均 `C41845FBE7EA9D655E6D893E558B1D1D79767C864616C45C24DB1A62DB1151B1`，jws.ts均 `5BF15B561A0AF4DFC39CDD18FC4ACA4C5F0615041A28D4522F2CD641A3DAB5B5`；identity.ts旧为 `0AF2F087CD85AD5B33F4E8C137DA9322934978BEF77107FA42C0E243A99CF7E3`，r5a为 `55D76C2FAE80DF6FB00E4C57FD79801A06C2812A8202E9BAC535876D2D5BA41C`，main.ts及management/client.ts也变化。因此不能以“文件整体没变”直接签r5a完整退出后的自动认证。

可保留旧实际事实为“同profile IDB持久化能跨全部浏览器进程退出”的既有平台证据，本轮真实持久化/坏key/正常认证也无相反观察；但若要将原r5a整条重开并自动认证判PASS，须由负责方提供变化前后的存储读写/自检负责逻辑比较及管理变化的证据映射，或在自有隔离profile重新做完整退出。本报告不凭未核验的函数级不变推断签署；此限制也不把旧证据一概删除。主方可与其现有同版本证据合并，本轮不要求用户重复技术测试。

## 证据与复现资产

运行命令入口：`setup.ps1`校验并复制binary、生成独立配置及启动两节点；`ego-browser nodejs -e "await import('file:///D:/Project/acp-remote/testing/probes/establish-node-access-browser-storage-20261030/round-NN.mjs')"`按轮次操作同一TaskSpace8；交互pair由自有bin在真实TTY运行，非脚本伪造批准；`cleanup.ps1`停止自有节点。

有效浏览器轮次：01/02建立空间与正常页面，03坏key，06初始化写失败，07认领前写失败，08真实正常claim，11可信身份与新origin，12清数据库，14保存最终DOM并仅清专属DB/关闭空间。源码中的临时配对码仅为本次短期公开测试会话，不能在后续复现重用；需在新运行生成相应输入，不把脚本重新运行当已完成测试。

工具失败不算产品FAIL：04/05在新CDP调用中试图移除前一调用的初始化脚本，返回Script not found，未进行相应故障测试，改为06同调用安装/取证/移除后成功；09误等“客户端列表”超时，10实际snapshot证实页面已显示“节点管理/已登记客户端”；13截图因窗口最小化失败，没有生成可用截图，也没有为此接管用户窗口。有效原始证据采用JSON与真实DOM snapshot，不声称截图已完成。

JSON均在 `.tmp/browser-storage-acceptance-20261030/evidence/`：bad-key.json、initial-write-failure.json、preclaim-write-failure.json、trusted-before-clear.json、new-origin.json、cleared-site.json、browser-final.json、browser-cleanup.json、node-cleanup.json。只记录公开身份元数据/页面/计数，没有序列化CryptoKey、私钥、pair proof或完整配对码到JSON；证书私钥未读取。

## 清理及交接

- TaskSpace8确认agent ownership，`finish({keep:[]})`实际成功，仅关闭本轮页面；没有停止正常用户ego/Edge进程。最终两个专属origin应用DB均删除成功、remainingApplicationDatabases=0。任务空间结束不等于完整profile退出或私密模式。
- 两个owner daemon stop均退出0；PID44052/28760消失；两个自有pipe由存在变消失；30453/30454监听数0；两个data/.node.lock均以FileShare.None独占重开成功。node-cleanup.json记录上述事实及最终binary hash。
- 未碰用户30447及主tester30451/30452，未删除节点数据库、DPAPI身份、原证书或用户正常profile；隔离node数据与脱敏证据保留。
- 主方摘要：r5a79/79初验匹配、自有binary前后2DC0…305F2一致；坏key、两种精确put失败、新origin隔离已动态通过；NA-STORAGE-001/P2为真实清DB后缺撤销提示，Open；私密模式NOT-RUN；完整退出持久性仅保留旧真实证据及负责逻辑映射限制，不冒充本轮执行；不把r6test-only钩子变更当本报告重编译版本。

## 追加：真实Edge InPrivate补验及修复版本边界

用户告知NA-STORAGE-001的帮助文字已按最小范围修复，r6待Web重建和新binary摘要。本tester未将该通知当成实测关闭：上述r5a失败及原始证据保留，问题仍待新binary定向复查。没有新增检测清空/检测私密模式的预期，没有要求额外按钮或确认。

### 私密入口确实可用

ego guide没有私密上下文入口，但这不代表机器缺浏览器。按用户已授权的备用路径，实际启动已有Edge `156.0.4314.50`，参数包括 `--inprivate --user-data-dir=D:/Project/acp-remote/.tmp/browser-storage-acceptance-20261030/profiles/edge-inprivate --remote-debugging-port=9338`。该目录原先不存在，未复用或触碰正常Edge profile；未忽略证书。

第一轮自有Edge根PID42716，Windows实际MainWindowTitle为 `ACP Remote 节点接入 - [InPrivate] - Microsoft Edge`（产品字样中有系统不可见字符，不影响标识）。自动化使用本机9338专属CDP入口，目标是自有30453页面；没有通过正常Edge的CDP端口操作。`Target.getBrowserContexts`为空并未当作普通模式判断，实际窗口InPrivate标识与启动参数共同确认模式。第二轮PID41440窗口同样有[InPrivate]，进入本例同origin无应用boot的未知路径以先检查数据库是否仍存在。

重启自有r5a节点PID36132，仍为最初复制的binary，摘要 `2DC0AFA04B55376797587136048F5EFBE7D665FF7EE69525DA248FA4D73305F2`；没有获取、复制或执行新r6binary。

### 私密子项实际断言

| 子项 | 实际结果 | 判定 |
| --- | --- | --- |
| 私密上下文安全来源、IDB健康路径 | secure=true，读回privateKey为不可导出ECDSA/P-256；用读回key签名并用记录公钥验证成功，初始unbound、真实配对表单可用 | PASS；Edge InPrivate实际支持会话内IDB/CryptoKey，不虚构“所有私密模式必定写失败” |
| 私密IDB写失败 | 同一精确DB/store的put故障适配器，hookInstalled=true、writes=1、claims=0、writeFailureShown=true、noClaimForm=true、无primary身份记录 | PASS；只在写失败后停止，未假认领成功；没有要求产品识别私密模式 |
| 私密会话结束后不持久化 | 第一轮Browser.close之后，按本例专属profile匹配msedge进程数为0；同一个user-data-dir以InPrivate重开，同origin无应用boot页面中applicationDbAbsent=true | PASS（私密会话临时存储子项）；先确认应用尚未boot，再查indexedDB.databases，避免把新页面重建后的DB误当旧存储残留 |
| 私密真实配对/全链管理 | 本追加未执行本机批准及长期认证，没有导出或保留私密身份 | 不声明通过；本轮私密覆盖限于身份存储及失败闭合，不代替完整NA-031全部Steps |

第一轮私密写失败脚本在未启用CDP Page域时钩子未生效，writes=0且未显示写失败；这是适配器未命中，**不计为产品失败或有效故障覆盖**。关闭全部自有进程、确认会话结束后，第二轮先Page.enable再安装钩子并导航，命中计数和错误页面均实际验证成功。保留两轮结果区分，不把首次未命中当通过。

私密key、publicKey、clientId、record正文和页面内容均未导出；CryptoKey读写、签验与比对全部在私密页面内部执行，JSON仅存布尔断言、计数及隐私说明，不截取或导出私密截图。`inprivate-inspect.mjs`仅检查本例固定URL、窗口/上下文技术元数据，不采集浏览历史或身份材料。测试临时私密profile仍在.tmp，InPrivate会话数据由浏览器关闭清除，没有复制至报告或普通profile。

证据路径仍为本例 `.tmp/.../evidence/`：`inprivate-assertions.json`（第一轮健康及未命中）、`inprivate-retest.json`（第二轮实际命中及退出丢库）、`inprivate-cleanup.json`。复现源码为本probe目录的inprivate-start/reopen/cleanup.ps1、private-cdp.mjs及inprivate-test/retest.mjs。没有使用ego执行器另启浏览器；备用Edge由单独命令和专属CDP控制，符合用户明确授权的隔离Edge InPrivate路径。

### 追加清理和当前交接结论

两轮Edge均通过其专属CDP Browser.close退出；最终匹配本例private profile的进程数0。节点owner stop退出0，PID36132不存在，自有pipe由存在变消失，30453/30454/9338监听数0，data-30453/.node.lock以FileShare.None独占重开成功，自有binary摘要仍2DC0…305F2。未停止正常Edge、清全profile cookies/cache、删除用户数据库/身份或修改产品。

本节取代上文私密模式NOT-RUN的现状：已实际完成上述私密存储子项，私密完整配对/管理链未执行；不能因此将NA-031整例或NA-STORAGE-001提升通过。r5a提示缺口仍作为历史FAIL保留，r6文字修复尚无本tester新binary复查证据，待主方提供完整摘要后按原case定向复查。

## 追加：r6定向复查完成，NA-STORAGE-001 Closed

**由原问题提出tester独立关闭NA-STORAGE-001的修复范围。** r5a原FAIL及证据完整保留；本节是新r6普通产品binary的真实浏览器复测，不是源码提示或开发自测代签，不宣称NA-031整例全部通过。

### 固定版本与运行隔离

- 实际校验 `testing/runs/establish-node-access/2026-10-30-implementation-r6.sha256`：清单SHA256 **1EB583A929AAB7769691FC05EF9A3A86D876E7502D94AC9CBC9F24D5EBCB10F7**，83项逐文件验证，0不符。
- 原普通binary `.tmp/acceptance-20261030-r5-target/debug/acp-remote.exe`实测SHA256 **135BD9BDDABFDD530A5251AECB1DFF0DC640E81F80241EC53C8703C2336D1509**；先核对，再复制到 `.tmp/browser-storage-acceptance-20261030/bin-r6/acp-remote.exe`，副本校验匹配；结束时仍为同一摘要。没有覆盖或使用旧bin副本混测，没有编译r6测试钩子。
- 新隔离data为 `data-r6-30453`、新Edge私密profile为 `profiles/edge-inprivate-r6`，均在本例.tmp根内；新节点PID37904，nodeId30830601c314e2d948a6790b49d794f64b810ca03d245989a0784b6f66995bad。仍使用30453和自有CDP9338，不触及30447/30451/30452或正常Edge profile。
- 第一轮Edge PID40708，第二轮39180；Windows窗口标题均明确[InPrivate]。真实Edge、匹配名称的受信HTTPS，未忽略证书。只输出布尔断言/按钮计数与公共帮助文案，不导出私密key、身份记录或页面截图。

### 原问题复测及私密子项

| 场景 | 本tester实际观察 | 结论 |
| --- | --- | --- |
| 新空store进入 | 先进入同origin不boot应用的检查路径，appDatabaseAbsent=true；进入应用后secure=true、pairForm=true、条件性撤销指引=true | PASS；不是拿已有正常record假定空store |
| 真实私密身份认领 | 页面提交本例配对，owner真实TTY核对测试名称/指纹后输入y；浏览器记录consumed且clientId非空、NODE CONTROL / AUTHENTICATED、privateKey不可导出 | PASS（本次新增真实私密配对/认证前置）；不导出其record值 |
| 清空真实可信store后重开 | 只deleteDatabase本例30453 origin的acp-remote-access-v1，再刷新；回到直接配对表单，条件性指引=true，新身份unbound/clientId=null/nodeId=null、key不可导出 | PASS，命中NA-STORAGE-001原复现路径 |
| 提示内容与交互范围 | 实际帮助文字为“输入节点终端显示的 8 位配对码。若清除了浏览器数据或丢失身份，请先在节点终端撤销旧设备，再重新配对。”；form buttonCount=1，无额外确认步骤，hasDetectionPromise=false | PASS；条件性提醒，不要求或声称检测清空/私密模式 |
| InPrivate完整结束再开 | 第一轮Browser.close后，匹配本例profile的所有Edge进程数0；同目录InPrivate重开，应用boot前DB仍不存在，再进入应用也显示同一指引 | PASS（私密会话临时存储行为），不代替普通profile跨退出保持原key/ID |

首次空store探针将“8位”作为无空格字面量，guidance=false；随后直接读取实际公共帮助文案确认产品是“ 8 位”，条件性撤销句本身完全匹配。这是探针过严的空格匹配偏差，不是新的产品失败；保留 `r6-empty-store-initial-literal.json`，调整为检查原case要求的条件性撤销完整句后，在第二轮真实空store重新执行并得到guidance=true。没有放宽“先撤销旧设备再配对”的预期。

有效新证据：本例 `evidence/r6-empty-store.json`、`r6-cleared-trusted.json`、`r6-cleanup.json`；均仅含布尔/计数/运行资源信息。r6-clear-trusted源码强制要求清理前trustedIdentity=true且authenticated=true，否则中止，避免以空白未绑定记录代替原复现。

### r5a高收益子项复用到r6的负责字节依据

比较两份固定清单，以下文件SHA256完全相同：

| 文件 | r5a与r6共同SHA256 |
| --- | --- |
| clients/web/src/pairing/identity.ts | 55D76C2FAE80DF6FB00E4C57FD79801A06C2812A8202E9BAC535876D2D5BA41C |
| clients/web/src/pairing/session.ts | C41845FBE7EA9D655E6D893E558B1D1D79767C864616C45C24DB1A62DB1151B1 |
| clients/web/src/pairing/api.ts | 8C2F5D25274592C2890235F07B87FC42CE154641F98E38E179EAED586556FB09 |
| clients/web/src/protocol/jws.ts | 5BF15B561A0AF4DFC39CDD18FC4ACA4C5F0615041A28D4522F2CD641A3DAB5B5 |
| clients/web/src/management/client.ts | 9F06BDF7D97599C1205050431A0CF967465FDB810CD8D41DA7015E5489F44246 |
| clients/web/src/management/reconnect.ts | 33644C4EA9DD9160B709D41C67BD8BC6274B27F9AA7F57C0C372356E8FB6D230 |
| clients/web/package-lock.json | 69D1E791BD5A41E585626BD234BC071677900D9A451DB8A4DABD9961038B415B |
| app/src/ipc.rs | E65826C1B56289D45E4832FEB5F2592A28050A702ED52D1FC989E05E1446162A |
| app/src/platform/windows.rs | DF461BE339E8B18AA43D9A44A3E1A6E82B81397BD92C6200174E99B439A3A5A4 |

据此可复用本报告r5a实际执行的已有坏key不替换、初始化put失败、健康lookup后put失败不claim、新origin不迁移私钥/信任，以及私密IDB读回签验和写失败闭合子项。它们由未改变的vault/repository/session/API及浏览器机制负责；r6帮助文案变化已用新binary补测，不把旧“缺撤销提示”结果继续套到新文案。当前main.ts不是字节未变，复用不覆盖其所有页面/生命周期逻辑，以上新动态结果仅签署具体展示及原复现范围。

r6 identity.rs test-only prefix-write/pre-flush钩子和真实崩溃矩阵属于另一工作包；本报告没有运行它们，没有把旧identity组件测试说成r6新测试，也不替代其独立tester结果。旧2026-10-29普通profile完整退出证据仍按上节限制保留，不能因本次InPrivate关闭后DB消失就声称普通profile原key持久性通过。NA-031新增重连/4401/4410/主动取消/未知结果不重发等未在本包全面执行，不签整例PASS，不动NA-033已有结论。

### 清理与关闭结论

第二轮InPrivate由专属CDP Browser.close结束；所有匹配edge-inprivate-r6进程数0。owner stop退出0、节点PID37904不存在，自有pipe由存在变消失，30453/30454/9338监听数0，data-r6-30453/.node.lock以FileShare.None独占重开成功，bin-r6摘要仍135B…D1509。保留隔离数据库/DPAPI身份和布尔证据，未清全局cookie/cache、导出私密身份或触正常profile。

**NA-STORAGE-001：r5a Fail → r6真实原路径复测PASS → Closed（原tester关闭）。** 主方可关闭该问题并复用上述有摘要依据的存储子断言；不据此勾选NA-031全部完成或把本报告扩为r6全量验收。
