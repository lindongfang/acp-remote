# Windows + Web 接入合同

状态：编码前设计合同，待独立检视；没有实现或互操作通过声明。只覆盖本 change，移动端扫码不在范围。本文负责字节格式、端点、预算和平台安全规则；行为需求引用 specs，design不复制参数。

## 1. 部署与信任根

首期Windows x64节点、受支持的Chromium桌面浏览器。Web与API同源，由节点提供同包静态入口。只接受用户配置的一个canonicalOrigin：标准HTTPS origin（无userinfo、路径、query、fragment；默认443省略，主机小写）。启动时解析并冻结配置，所有客户端逐字使用规范化结果。

首期节点直接终止TLS，加载用户提供且浏览器信任、名称匹配的证书与私钥；不自动安装根证书、不关闭校验、不提供公网明文模式。Tailscale提供可达路径，证书配置由部署者完成；本change不实现自动反代/Tailscale Serve控制。TLS缺失或不可用则启动失败。浏览器外部静态资源、分析脚本与第三方配对服务不允许。

初次Web通过用户从本机终端打开的HTTPS地址进入，TLS与本机终端构成引导信任；短配对码自身不是独立于TLS的主机身份锚。终端显示完整节点SHA-256指纹，配对页展示同一指纹以供核对。配对成功后浏览器保存节点公钥绑定；节点公钥发生变化必须停止，不能自动覆盖旧绑定。不能宣称本方案能防御控制本机终端、受信站点代码或同用户进程的攻击者。

所有配对POST及WSS升级要求Origin恰等于canonicalOrigin，缺失、null、多值或不同来源拒绝；Host解析后也须匹配配置。首期不开放无Origin的原生客户端路径。禁止信任任意X-Forwarded-For/Host，限流来源取实际TCP对端。浏览器响应带Cache-Control:no-store、Referrer-Policy:no-referrer及同源CSP；配对码只放POST正文，不放URL、storage、遥测或日志。公开静态页不包含用户或节点管理资料。

## 2. 身份编码与签名

使用ECDSA P-256 + SHA-256，公钥SEC1未压缩65字节，必须校验曲线点；签名使用64字节r||s（P1363），不接受DER、长度变化或越界/零r/s；接受合法high-S与low-S。所有二进制字段为严格无填充base64url，拒绝替代编码。节点ID为小写hex(SHA-256(节点65字节公钥))，clientId为服务端分配随机128bit标识，不等于指纹。

使用标准JWS Compact Serialization（RFC7515），alg=ES256（RFC7518）；三个段均无填充base64url。protected header固定字段alg、typ，禁止未保护header、crit、jku、x5u以及未知字段。payload为本节规定的封闭JSON对象，签名覆盖原始JWS两段ASCII，不在验签前重序列化。拒绝重复JSON键、非UTF-8、非法Unicode、非对象payload、未知字段及超预算值。时间为UTC Unix秒安全整数，id为规定长度的base64url，不接收任意大数。

typ按用途严格分开：acpr-pair-claim+jwt、acpr-pair-status+jwt、acpr-host-claim+jwt、acpr-host-status+jwt、acpr-auth-client+jwt、acpr-auth-host+jwt。它们是当前协议的应用类型，不能将别的用途有效签名拿来替代。

typ只位于JWS protected header，不属于payload。claim客户端使用acpr-pair-claim+jwt，status客户端使用acpr-pair-status+jwt；对应hostProof分别使用acpr-host-claim+jwt和acpr-host-status+jwt。

配对claim/status挑战payload共同字段（不用于WSS）：v=1、nodeId、origin、requestId、challengeId（随机128bit）、nonce（随机256bit）、iat、exp。exp由服务端生成，最长30秒；单调时钟判断进程内超时，墙钟变化不延长有效期。claim证明还含clientPublicKey、displayName（1–80 Unicode标量，拒绝控制字符）；status不含displayName；验签必须使用该公钥，不能只登记一个没有持有证明的key。服务端只接受自己发出的、与当前request、用途和连接/配对对应的共同字段，iat不是客户端自由选的有效期。

私钥生成和签名调用成熟平台/密码库，不实现曲线运算或随机数算法。随机材料均来自系统密码学随机源；失败即关闭入口，不退回时间戳随机。

## 3. 配对状态与端点

本机命令：acp-remote daemon start（前台）、daemon status、daemon stop、device pair、device list、device revoke --device-id ID。只通过第6节本机IPC操作，不开放网络本机批准端点。

device pair生成pairingId（128bit）、8位Crockford配对码（40bit），字母表0123456789ABCDEFGHJKMNPQRSTVWXYZ；显示XXXX-XXXX。输入仅允许大小写折叠和一个中间连字符，不做O/I等模糊替换。码在同一次配对内固定，最长300秒且不可续期。最多同时8个配对，碰撞重新生成；码不入数据库。CLI只在当前交互终端专门显示，不进入普通日志；非交互执行拒绝创建待确认配对。

网络统一前缀/api/access/v1。所有POST Content-Type:application/json，解码前限制16KiB，最大嵌套8，Content-Encoding压缩拒绝。响应封闭结构，未知版本400 unsupported_version。除静态资源外未列路径拒绝；拒绝JSON重复键，避免签名解析差异。

| POST路径 | 请求 | 成功响应及约束 |
|---|---|---|
| /pair/lookup | {v:1,code,requestId} | {v:1,pairingId,nodeId,nodePublicKey,origin,requestId,expiresAt,challengeId,nonce,iat,exp}；取回不建立信任，每次最多一个有效claim挑战，重取使旧挑战失效 |
| /pair/claim | {v:1,proof:JWS} | 验证claim payload，原子绑定公钥；返回{v:1,hostProof:JWS} |
| /pair/challenge | {v:1,pairingId,clientPublicKey,requestId} | 仅返回状态查询挑战共同字段，不返回是否批准/客户端身份；同一pairing每次一个状态挑战，重取失效旧挑战 |
| /pair/status | {v:1,proof:JWS} | 校验被绑定公钥的状态proof，返回{v:1,hostProof:JWS} |

claim payload除共同字段外含pairingId、clientPublicKey、displayName。requestId由客户端在lookup或status挑战请求中生成随机128bit，服务端回显并绑定挑战；服务端缓存已接受proof摘要与结果直到配对终结，逐字重复重试可取相同hostProof，不再次认领；相同requestId不同proof报request_conflict。其他有效挑战下的不同key不能覆盖已绑定key。已在信任库有效或撤销的公钥均拒绝再次建立新身份，尤其不能通过upsert复活撤销身份；需要重新配对时客户端生成新key。

claim hostProof payload固定{v,purpose:"claim-result",nodeId,origin,pairingId,requestId,clientPublicKey,status:"pending_confirmation",expiresAt}，由节点签名。客户端验证key指纹与绑定节点一致后才显示待确认。status客户端payload为共同字段+pairingId+clientPublicKey，不含displayName；status hostProof payload固定{v,purpose:"status-result",nodeId,origin,pairingId,requestId,clientPublicKey,status,clientId,expiresAt}，未批准clientId为null，批准后为稳定ID。签名typ区分claim/status。

状态：pending_claim -> pending_confirmation -> approved -> consumed；pending_claim/pending_confirmation可进入rejected/cancelled/expired。成功提交批准的线性化点之后CLI取消不能回滚信任；此时CLI提示“已批准，请用device revoke撤销”。否则CLI关闭、IPC断开或Ctrl-C立即取消。300秒到期使尚未批准请求失效。批准结果保留到原expiresAt或批准后60秒，两者取较晚者，随后只清理临时材料，不撤销已提交信任。首次管理认证成功标记consumed，不再允许第二个key认领；同一绑定key的批准结果在保留期内仍可重取。节点重启清除所有临时配对/挑战；已提交信任保留，浏览器以已保存key重新配对时如服务器已批准，则用恢复流程查询其clientId。

恢复流程：WSS hello允许clientId=null且携带clientPublicKey，服务器生成通用挑战；验证私钥证明后才查询有效公钥对应clientId并返回。有效公钥唯一；未知/撤销公钥对外统一拒绝。这样批准响应丢失或客户端ID写入失败不要求平台删除或恢复撤销记录。

## 4. 管理连接与撤销

WSS /api/access/v1/control；subprotocol acpr.access.v1。公开HTTP只有配对端点与静态资源；管理查询/命令均在已认证连接中，不采用长期cookie/bearer token。

Web 已保存有效身份时自动恢复管理连接，不要求用户点击“重新尝试认证”。临时网络中断、节点重启或暂时不可用时复用同源 IndexedDB 中的原私钥及客户端标识，按有界退避自动重连；限流至少等待一个固定窗口。每条新连接仍须使用新鲜的一次性挑战完成双方证明，不重放旧 proof，也不自动重发结果未知的管理命令。身份拒绝/撤销时停止旧身份重试，清理失效的本地配对并直接进入配对页，以新密钥重新配对，仍需本机明确批准；节点绑定或协议校验失败、本地密钥损坏时保留错误，不自动替换身份。用户主动断开或页面离开时取消待执行重连。

客户端5秒内发hello {v:1,type:"hello",clientId:ID|null,clientPublicKey,clientNonce:32bytes}。服务端返回challenge {v:1,type:"challenge",nodeId,nodePublicKey,origin,connectionId:128bit,challengeId,nonce,iat,exp,hostProof}。hostProof payload为上述全部身份及挑战字段加clientId、clientPublicKey、clientNonce，typ=acpr-auth-host+jwt。客户端先核对已钉住的节点key/来源再验签，不给替代key签客户端证明。

客户端发{v:1,type:"proof",proof:JWS}，payload同hostProof payload，typ=acpr-auth-client+jwt。服务端检查当前连接、新鲜挑战、clientId/key绑定及有效信任，先原子消费挑战再发布authenticated {v:1,type:"authenticated",clientId,connectionId}。失败、超时与断开均清除挑战；同一签名跨连接、跨用途、过期、重复提交均失败。认证总超时15秒（exp仍不得超过30秒），认证前任何管理帧关闭。

WSS两向签名payload的完整封闭字段集合为：{v,nodeId,nodePublicKey,origin,connectionId,challengeId,nonce,iat,exp,clientId,clientPublicKey,clientNonce}。不含type、hostProof、proof或requestId，不递归包含签名自身；两向逐字段相同，仅protected header的typ不同。挑战共同字段规则只适用于配对，不能向WSS自动添加字段。

认证后请求{v:1,type:"command",requestId,action,payload}。首期action=node.info、device.list、device.revoke；revoke payload={clientId}，node.info payload={}，device.list载荷按下方分页合同。响应{v:1,type:"result",requestId,ok,result,error}，成功error=null，失败result=null；device.list不返回私钥。配对批准只能走本机IPC。管理状态订阅采用{v:1,type:"subscribe",requestId,resource:"devices"}，只通知当前列表revision，后续按下方失效通知合同推送；它不复用未来会话历史协议。

device.list改用唯一载荷{cursor:null|字符串}，每页最多32个客户端，结果{items,nextCursor,revision}；游标绑定节点、认证客户端及列表revision，最长60秒有效。发生新信任或撤销使revision递增，旧游标报409 stale_cursor并从第一页重取，不串用历史分页规则。items仅含clientId、displayName、status、公钥指纹和createdAt/revokedAt，公钥完整值不在列表中返回。devices订阅只发送{v:1,type:"devices.changed",revision}，首次订阅也仅通知revision，由客户端分页查询；不发送无上限全量快照。失效通知可合并到最新revision，不包含敏感正文。

node.info结果为{nodeId,publicKey,origin,status:"ready"}；device.revoke结果为{clientId,status:"revoked"}。管理错误为{code}，词表为invalid_request、not_found、request_conflict、stale_cursor、rate_limited、storage_unavailable、node_stopping；帧解析与认证错误直接按关闭码处理。list游标取回与revoke均受每次身份有效性检查。

每个身份有认证代数，撤销事务的SQLite COMMIT、更新代数和队列失效全部在目标身份同一串行协调器临界区内；受理和实际出站提交都在该身份串行协调器内核对代数。撤销提交后清空该身份所有尚未提交socket发送的数据队列，拒绝新入队和新命令，关闭所有连接。提交前已交给网络栈的数据不能收回；规格“停止继续交付”针对提交后的新发送，不承诺擦除在途/客户端副本。自撤销不等待成功响应送达；list或重连可确认已撤销结果。

相同clientId可有多个连接，首期不沿用旧系统“新连接踢旧连接”。每客户端最多8连接；既有连接不会因新连接出现而丢失身份。配对与身份不替代后续Harness权限审批。

## 5. 固定预算与失败语义

| 项目 | 上限 |
|---|---|
| 配对lookup | 每TCP来源5次/分钟，全节点30次/分钟，成功失败均计 |
| claim/challenge/status各端点 | 每来源30次/分钟，全节点120次/分钟；status每配对最多1次/秒 |
| 管理握手 | 每来源30次/分钟，全节点120次/分钟 |
| 连接 | 全节点128，未认证32，每客户端已认证8 |
| 管理帧 | 32KiB，深度8；单连接120命令/分钟 |
| 出站队列 | 每连接1MiB或256帧，先达到者关闭慢连接 |
| 本机IPC | 每消息32KiB，连接32，短请求30秒；device pair为最长300秒的专用流式会话，非短请求 |
| 关闭 | 总15秒；超时返回失败并尽力关闭已拥有资源，不报告完全成功 |
| 限流桶 | 最多1024个来源条目，空闲5分钟淘汰；满时新来源拒绝，不驱逐现有桶绕过限制 |

限流按固定60秒窗口计数，单调时钟推进；超限返回429 rate_limited，不延长配对生命。畸形请求也计数。来源字段不取客户端自报值。计数与认领全在节点协调边界内原子更新。预算适用于部署反向代理之前的首期直接TLS模式。

HTTP错误封闭{v:1,error:{code,retryAfterSeconds:null|整数}}；400 invalid_request/unsupported_version，401 pairing_invalid（未知、过期、占用及拒绝统一），409 request_conflict，429 rate_limited，503 storage_unavailable/node_stopping。外部不回显code、公钥、proof或内部路径。已绑定公钥通过有效状态proof可得到rejected/cancelled/expired，未认证猜测者只得到统一失败。

WSS关闭：4400格式/版本，4401认证无效（未知与撤销相同），4408超时，4429超限，4410撤销，4500内部存储错误；已认证连接收到撤销关闭理由只给固定词，不含敏感字段。认证前不得通过错误区分某clientId是否存在。控制失败不能改变已提交事实。

## 6. Windows、本机所有权与存储

单应用Rust二进制，identity/policy/gateway/storage/platform分模块，领域不依赖网络及Win32 API。元数据采用SQLite事务存储，节点身份私钥采用Windows当前用户范围DPAPI保护独立密钥文件；不使用CRYPTPROTECT_LOCAL_MACHINE、不把私钥放SQLite。数据目录、密钥文件及本机管道DACL仅授予当前owner SID必要权限，不依赖Windows默认DACL。DPAPI不是防御同用户恶意进程的隔离机制。

本机IPC命名包含owner SID与数据目录规范化摘要，Windows named pipe拒绝远程客户端；服务端模拟并检查对端token SID与owner精确相同，校验后立即revert。显式拒绝其他用户及模拟失败，不凭客户端传入SID授权。创建首实例管道与数据目录独占锁，避免同用户第二节点使用同一数据目录；进程退出释放OS锁，不靠删除未知PID文件抢锁。IPC保留至网络入口关闭后，使stop仍可收敛。CLI控制不依赖Web已配对身份。

元数据：node_meta(schemaVersion,nodeId,publicKey,keyFileRef)、clients(clientId,publicKey UNIQUE,displayName,status,authGeneration,createdAt,revokedAt)、management_commands(actor,requestId,action,payloadDigest,result,expiresAt UNIQUE(actor,requestId))、security_audit(actorId,action,targetId,outcome,time)。当前已建立对应 SQLite 实现；不迁移旧产品数据库。revoke重复请求返回原结果，同一requestId不同载荷拒绝；审计和状态同事务提交。

只有device.revoke持久化幂等结果；payloadDigest=SHA-256(UTF8 RFC8785规范化JSON的{action,payload})，协议v1不允许在重试中改变版本。结果保留24小时，全节点最多10000条，满且无过期项时拒绝新revoke（503 storage_unavailable），不驱逐未过期项。过期后相同ID作为新请求处理，但撤销操作本身单调幂等，不会复活身份。device.list/node.info是只读，不持久化每次请求。clients撤销记录不自动清理（防止公钥复活）；磁盘满明确失败，不声称数据库无限容量。security_audit每个管理受理事实写一次，不逐帧/逐轮询写记录，保留策略由后续数据生命周期change承接。

身份首初始化：独占数据目录后以临时文件写DPAPI密文、刷新并原子更名，再事务写node_meta。若崩溃留下密钥而无meta，解密校验同一公钥后完成meta；有meta但密钥缺失/不可解密则拒绝启动。数据库和密钥均存在且身份不符也失败，不自动替换。首次生成只在新目录或确认无任何历史身份痕迹时发生。未知schema拒绝，不清空或降级。

Web密钥为不可导出WebCrypto P-256 CryptoKey，连同非秘密nodeId/publicKey/clientId在同源IndexedDB保存，不使用localStorage存私钥。开始认领前完成密钥持久化并读回签验自检，失败不认领；写读失败时明确拒绝配对；正常浏览器配置必须真实验证跨关闭保留，不承诺检测所有私密模式或浏览器自动清理。刷新/重启加载已有key；已有记录但key读取失败不静默新建。清除站点数据意味着丢失本地身份，提示本机撤销旧设备并新配对，不能冒充恢复。HTTPS origin变化需要显式重新配对，不迁移私钥或自动信任新来源。

## 7. 实施与验证分界

本文定义算法和应用编码，独立reviewer仍需检视。实现已引入依赖并纳入锁文件，选择依据见 `app/DEPENDENCIES.md`；构建或 npm audit 通过不能替代 Rust 公告库扫描及独立供应链验收。

必须建立Windows真实TLS + 浏览器WebCrypto/IndexedDB + named pipe/DPAPI + SQLite的端到端验收。故障测试可在可控存储端口注入失败，但正式正常配对、撤销和重启不可用mock替代。固定测试向量包含JWS header/payload原字节、SEC1公钥、正反签名、跨用途与跨连接重放；使用专门公开测试key，不放生产秘密。向量和运行证据由实现及独立tester实际建立，文档存在不代表密码学互操作实测通过。

Rust 编译、测试、格式、Clippy 以及 Web 构建、类型检查和测试入口已建立，实际命令以 README 和工程配置为准。逐例通过范围以对应固定版本的运行报告为准，不将入口存在或结构校验视为产品验收。

## 8. 标准依据

- [RFC7515 JWS](https://www.rfc-editor.org/rfc/rfc7515.html)：标准签名载体。
- [RFC7518 JWA](https://www.rfc-editor.org/rfc/rfc7518.html)：ES256及ECDSA签名编码。
- [W3C WebCrypto](https://www.w3.org/TR/webcrypto/)：浏览器密钥与签名接口；平台支持需实测。
- [Microsoft named pipe security](https://learn.microsoft.com/en-us/windows/win32/ipc/named-pipe-security-and-access-rights)：管道访问检查与DACL。
- [Microsoft CryptProtectData](https://learn.microsoft.com/en-us/windows/win32/api/dpapi/nf-dpapi-cryptprotectdata)：当前用户密钥保护及其平台边界。

历史本地配对与身份合同仅作安全审查输入；不继承旧Node Link或旧会话同步协议。

