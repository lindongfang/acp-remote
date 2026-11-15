> 五项设计收口已确认：新版合同须区分启动配置/原生存储/连接身份、可选 Run 的原生操作审批、节点共享视图、能力准入分层及新依赖；下方原合同仍停用，未完成独立用例及检视。

> 2026-10-30 状态：原方案实施合同，已暂停使用。当前产品权威为更新后的 specs 与总体设计；下方历史正文保留供迁移分析，不作为新版 wire、数据库迁移、缓存或审批强刷的实施依据。新版合同尚未完成独立复核，不能据此恢复编码。
> 待重建：原生列表/读取与身份、缓存代际/游标、原生附件访问、查看激活、平台管理元数据与临时输入生命周期、无历史接口分支。保留已归档节点认证/撤销/预算；取消独立聊天条目存储、提交版本、500ms/64KiB 强刷与平台历史恢复。未确定新版 schema 编号，不执行迁移或删除任何用户数据。

# 会话管理、存储与同步实施合同

状态：2026-10-30 WP1 重基准修订，待独立 tester/reviewer 核对；不是实现或互操作通过声明。本文是本 change 的 wire（线上编码）、存储升级与同步细节的单一来源。行为权威仍为本 change specs 及重设计；节点认证、撤销与固定预算继承[已归档接入合同](../archive/2026-10-30-establish-node-access/access-contract.md) §4–6，本文只扩展会话操作，不替代节点信任规则。

## 1. 现有基线与接线

上游于2026-10-30归档，README登记33/33验收；最终证据见 testing/runs/establish-node-access/2026-10-30-ready-metadata-review.md、2026-10-30-archive-review.md 与 2026-10-30-archive-record.md。历史合同中的“待检视”是归档原文，不代表当前交付状态；复用以最终固定版本及对应报告范围为限，不外推为会话验收。

实际代码为 app/src/storage.rs、management_ws.rs、access_policy.rs、trust.rs、node.rs、lib.rs，不能把拟新增 storage/ 或 gateway/ 当成现有入口。Storage 当前 SCHEMA_VERSION=1、PRAGMA user_version=1，单连接池、WAL、synchronous=FULL、foreign_keys=true；node_meta 另存 schema_version。TrustStore 的 client_gate 与认证代数是撤销协调权威；ApplicationPolicy 生成不可伪造 actor，当前 request_runtime_capability 无可信审批发行者，始终拒绝运行能力。这一默认拒绝不能因可信设备身份而删除，WP3必须接入真正有效的审批关联/裁决，不接受客户端自报时间戳作为审批。

WP2负责 storage.rs 迁移及内层事务；WP3负责 access_policy.rs/trust.rs 安全接线与 node.rs/lib.rs 资源装配；WP4负责 management_ws.rs 扩展及最终 node.rs/lib.rs 组合装配。实际写入严格按 plan Merge Order 串行，不为并行强制重构现有安全模块。

## 2. 同一认证连接上的封闭扩展

复用 WSS /api/access/v1/control、acpr.access.v1、既有新鲜挑战和 authenticated actor。访问协议 v=1 与 ACP v1/v2 是不同命名空间，不能互相推断。认证完成后新增 command action；既有 node.info/device.list/device.revoke 和 devices 订阅逐字保留。未列 action、未知字段或不匹配类型返回 invalid_request；扩展尚未实现的节点仍按现有 invalid_request 处理，Web不得视作成功。

请求沿用 `{v:1,type:"command",requestId,action,payload}`，结果沿用 `{v:1,type:"result",requestId,ok,result,error}`，成功 error=null，失败 result=null。requestId、平台对象ID采用128bit随机无填充base64url；原生ID作为不透明内容存储，不冒充平台ID。计数与版本使用非负安全整数，溢出前拒绝创建新版本并明确失败。每个 payload 为表列精确字段集合，nullable 字段必须显式为 null；仅标有`?`的pageSize可省略，省略与null不同，未知字段仍拒绝。

| action | payload | result 的主要字段 |
|---|---|---|
| harness.register | {name,executable,arguments,configRef} | {harnessId,revision} |
| harness.list | {cursor} | {items,nextCursor,revision} |
| workspace.register | {path} | {workspaceId,revision} |
| workspace.list | {cursor} | {items,nextCursor,revision} |
| session.create | {harnessId,workspaceId} | {sessionId,commandId} |
| session.list | {cursor} | {items,nextCursor,revision} |
| session.input | {sessionId,contentRef} | {commandId,runId,turnId} |
| session.cancel | {sessionId,runId} | {commandId,runId} |
| session.release | {sessionId} | {commandId,sessionId} |
| session.resume | {sessionId} | {commandId,sessionId} |
| approval.decide | {sessionId,approvalId,decisionId} | {commandId,approvalId} |
| session.history | {sessionId,cursor,pageSize?} | {snapshotId,turns,nextCursor,revision,pageSize} |
| session.snapshot | {sessionId,pageSize?} | {snapshotId,revision,state,items,approvals,pageSize} |
| command.get | {commandId} | {commandId,status,resultRef} |
| content.begin | {sessionId,purpose,totalBytes,sha256} | {uploadId,chunkBytes,expiresAt} |
| content.put | {uploadId,index,data} | {uploadId,index} |
| content.finish | {uploadId} | {contentRef} |
| content.read | {sessionId,contentRef,offset} | {contentRef,offset,data,nextOffset,totalBytes,sha256} |

name最多80 Unicode标量，路径/可执行文件最多4096 UTF-8字节、arguments最多64项且合计16KiB；路径由节点解析检查，不以客户端路径作为授权。configRef只引用节点本机非秘密配置，不接受私钥/令牌；禁止以此读取任意文件。session.create/resume结果仅表示命令受理，不表示外部原生操作成功；最终状态由已提交投影给出。approval decisionId必须是该审批广告的选项，不接受任意新能力。只读列表每页最多32条；过大轮次以元数据和 contentRef 返回，不能为帧预算裁掉结构。

父历史pageSize由前端传入，省略默认10，合法值为1–200的正安全整数；null、0、负数、小数或超限均invalid_request，在建立快照/执行查询前拒绝，不静默改为10。父轮次按稳定轮次边界倒序选择有效pageSize轮，当前正在执行轮计入；轮内按首次源位置正序，不把一轮切成相邻逻辑页。下一逻辑页游标绑定目标、方向、认证范围与原排他轮次边界，不绑定页大小；下一页可按新的合法pageSize从该边界续取，不退回最新页或重复前页轮次。逻辑页内分块句柄另冻结该页选中轮次集合和有效pageSize，块续取不能因新参数重新选轮或漏掉剩余条目。跨目标/方向/认证范围的游标明确失败。02最小Web可以始终显式传10，不要求本change提供页大小选择器；03仍负责完整长历史、父子通用分页/展开及并发衔接验收，02不提前承诺真实子身份关系或原位展开。该后续归属不允许02入口写死与已确认前端传参相反的wire语义。

新增会话错误 code：busy、request_expired、invalid_state、approval_invalid、delivery_unknown、unsupported_capability、content_incomplete、content_conflict、content_too_large；保留上游 invalid_request/not_found/request_conflict/stale_cursor/rate_limited/storage_unavailable/node_stopping。不得将 Harness 正文、路径、密钥或底层异常拼进错误。会话不存在与无目标权限统一 not_found。审批已失效、已决定或代际不符统一 approval_invalid；同请求的已提交结果仍可通过 command.get 取得。存储失败不返回已受理成功。

## 3. 帧、结构内容与撤销

所有管理帧仍最多32KiB、JSON深度8，严格UTF-8、拒绝重复键；每连接120 command/分钟、出站1MiB或256帧先到者关闭，既有连接和节点上限不变。分页、通知与分块都计入同一连接预算；不建立未认证下载URL、独立HTTP旁路或无限出站队列。

正文使用内容引用，结构JSON以原始UTF-8字节封装为 base64url 分块，因此原始结构可深于管理信封8层但不能直接嵌入信封突破限制；未知结构与原始扩展字段必须保留。每块最多12KiB原始字节，含信封后仍检查32KiB。单内容对象最多16MiB，单身份同时最多2个上传、节点最多16个，临时上传总量节点128MiB；非终结输出按内容对象分段，不把整个会话塞入一个对象。超限明确 content_too_large/繁忙并保留已接收事实，不能静默截断。上传从begin受理起单调时钟硬截止60分钟，不因块/重试续期；连续120秒未受理有效新块亦超时，逐字重复块不刷新空闲时钟。1366个12KiB块可在既有120命令/分钟下完成16MiB对象（还需begin/finish预算），不提高限流；共享该预算的其他命令可能导致超时，客户端须节流并明确重建上传，不声称无限吞吐。断连/撤销/节点停止取消并清理本次拥有的临时文件；未提交对象不用于输入。此上限是实施预算，须由tester验证限流窗口、截止瞬间、空闲计时、最大对象成功和无期限续传拒绝。

本节的会话内容范围另有§9/10明确列出的节点登记元数据只读例外，不能将该例外扩展到任意正文。begin分配绑定 actor、认证代数、sessionId、purpose、长度和SHA-256的uploadId；purpose仅input，服务器输出引用由内部建立。put index从0连续递增，逐字同块重试幂等，异内容content_conflict；finish核验完整长度/摘要、持久化对象及引用元数据后才返回contentRef。contentRef是节点随机不透明标识，不是任意路径或裸正文哈希；仅允许对应会话权限及用途读取/消费，不能用另一会话或另一操作者的input引用注入命令。读 offset为12KiB边界，末块返回nextOffset=null；客户端检查偏移、长度和最终摘要后解析，内容损坏不显示为完整。

每次命令受理、订阅、内容读写与每个出站块均在现有 client_gate 内复核当前可信身份/代数及目标权限；实际提交给socket的writer同样受该身份协调器约束。撤销事务COMMIT、代数更新、出站失效保持现有同一临界区。不得仅在begin或查询开始时检查权限。撤销COMMIT后清空未发送队列、取消上传/订阅、拒绝新入队及新发送；已交网络栈数据无法收回。审计不保存正文或每块记录。

## 4. 受理事务与存储升级

新增 session_commands 与 management_commands 分表，后者及 device.revoke 的24小时/10000条规则不变。session_commands唯一键(actor_id,request_id)，摘要是SHA-256(RFC8785规范化JSON {action,payload})；input摘要还绑定contentRef所指已校验内容摘要。相同ID不同摘要冲突。会话命令去重结果保留到会话删除（数据生命周期尚未实现时不自动删除）；最多100000条/节点，满且无合法清理项时拒绝新命令，不驱逐未过期事实。不能在24小时后自动重派未知输入。command.get只读同一actor可访问的命令；断连重连仅查询结果，不自动重发副作用。

输入受理同一个SQLite事务完成：复核actor及会话状态/前台互斥、登记Command、用户输入ConversationItem和Turn、建立Run、关联已持久化内容引用及受理审计，提交成功后才派发一次。任一步失败全部回滚，不留下孤立Run或没有输入的执行。Command状态区分accepted、dispatching、completed、failed、delivery_unknown；派发前持久化dispatching，崩溃/未知交付不自动重发，重启将未决执行标为interrupted/unknown并保持事实。审批决定与有效性比较、唯一裁决和审计同事务，交付另记，未知不自动重试。

schema目标为2：在同一owner数据目录独占锁下识别0/1/2；0仅无历史痕迹的新库初始化，1以事务新增harnesses/workspaces/sessions/session_commands/runs/turns/conversation_items/approvals/content_objects/session_state等表及约束，更新PRAGMA user_version与node_meta.schema_version为2后COMMIT；2验证所需结构。保留node_meta的nodeId/publicKey/keyFileRef、DPAPI密钥、clients全部状态/公钥/认证代数、management_commands、security_audit和node_state revision。迁移不修改信任、不清空库、不重新配对、不替换身份。未知版本、版本字段矛盾、残缺结构均停止启动而非重建。

升级前停止节点并使用SQLite一致性备份保存现有数据库及受保护身份文件；备份仍按owner DACL管理，不记录私钥。DDL/版本更新失败回滚，旧库仍为1；失败后保持管理入口关闭。提交后旧schema1程序须拒绝2，不支持就地降级。回滚先停止所有写入，保留2库/附件，只有确认无迁移后新事实需保留时才可在隔离目录验证完整备份恢复；不能自动用旧备份覆盖真实节点。专用测试根的新库与schema1副本迁移是两条测试路径，“空库验证”不是清理真实库的授权。

## 5. 顺序、版本与恢复同步

会话内容字段至少含sessionId、turnId、itemId、sourceId、firstPosition、kind、contentRef、contentVersion、committedVersion。源位置进入异步队列前分配，区间事务预留、允许崩溃间隙、不得复用；内容版本只单调递增，同一来源更新保留同一条目和首次位置。数据库以最新版本合并，提交成功才推进committedVersion及session revision。内存实时事件不能冒充数据库提交。

新增 `{v:1,type:"subscribe",requestId,resource:"session",sessionId,afterRevision,pageSize?}`，afterRevision为null或已见提交版本；取消使用§9的精确unsubscribe字段集合，不带afterRevision/pageSize。pageSize采用§2同一默认、上限及非法值规则。订阅不是先查询后监听：会话串行所有者在一个临界区登记订阅并捕获已提交revision、当前liveRevision及全部当前未提交条目的版本/内容引用，先排入 `{v:1,type:"session.baseline",sessionId,snapshotId,revision,liveRevision,baselineRef,pageSize}`，再排该基线后所有变化。baselineRef指不可变基线，包括按有效pageSize选取的最新父轮次提交投影、当前Run/审批及当前live条目（含已存在但此后不再更新的未提交尾部）；历史更早轮次继续分页。捕获后发生的提交/实时事件严格排在baseline后，客户端先安装基线再处理更高版本，不能等下一次增量才显示当前尾部。客户端afterRevision仅作一致性提示，不允许节点省略首次live基线，也不能把显式pageSize重置成10。

后续事件 `{v:1,type:"session.changed",sessionId,revision,liveRevision}` 只发失效版本，可合并；正文通过snapshot/history/content.read取回。快照绑定snapshotId、actor/代数、sessionId、revision、liveRevision及有效pageSize，并以同样原子捕获取得当前实时尾部；session.snapshot与首次baseline采用同一父轮次选择规则，不能覆盖客户端页大小。并发新提交不拼进旧快照；历史分页游标最长60秒，过期stale_cursor后按原有效pageSize重取第一页；已取得内容引用的读取租约不随分页游标失效。结构数据均放contentRef，快照/基线/历史元数据超过帧预算时返回不可变分段引用，每对象仍不超过16MiB，不返回不完整JSON。

实时输出通过 `{v:1,type:"session.live",sessionId,liveRevision,itemId,contentVersion,committedVersion,contentRef}` 给出不可变当前内容版本引用；引用可指未提交临时内容，但必须显式携带committedVersion，不能提升其提交事实。当前live条目在被替换/提交前持续可从新基线取得，不因旧引用到期消失。每次基线/快照/历史读取为其返回的不可变内容版本建立新的60分钟读取租约（单调时钟，从签发开始，不由逐块读取续期）；引用绑定actor、认证代数及会话，逐块复核，不是离线权限。内容版本在租约内保持不变，可按offset分块读完16MiB；原120命令/分钟下1366块约需12分钟，60分钟租约留出管理流量余量。过期、撤销或断连后旧租约作废，客户端丢弃不完整拼装，重新认证取原子基线和新引用，不把两个版本拼接；已提交内容仍由数据库保留，临时live版本仅在当前状态或有效租约需要时保留且受§6缓存预算，无法保留则显式完整性错误并进入既定故障处理，不无限占用内存。

实时引用/事件不作断线永久重放日志；丢事件、版本缺口、重连或节点重启后重新订阅取得原子提交/live基线，异常丢失的未提交尾部明确标错。客户端只应用更高contentVersion，迟到事件不回退，未知完整性展示错误；正常页面不显示保存状态/版本。上传/下载队列背压遵循§3，页面断开仅取消订阅、读取租约和该连接上传，不停止已受理Run。tester须验证先有未提交尾部且后续无增量时首次订阅可见、捕获/排队交界更新不丢失、慢速最大内容读取与租约到期/撤销边界。

## 6. 保存故障与停止判据

500ms或累计64KiB脏数据先满足者触发合并保存；条目结束、轮次结束、审批展示/交付前、正常释放及正常关闭强制刷新。每会话脏缓存16MiB、节点128MiB，高水位80%停止新输入并立即刷新。计时使用单调时钟，合并替换的真实占用仍计入预算。

持续失败的可执行判据：同一会话首次提交失败起5秒仍未有一次成功提交，或连续3次实际提交失败，先达到者触发；写入重试最短间隔250ms且单会话只一个在途提交。提交成功后清零失败次数/时钟。硬上限到达不等待上述阈值，立即进入故障停止。强制刷新失败立即返回失败并阻止审批交付/释放成功，不等待阈值，也不假称正常释放。

故障停止原子禁止该会话新输入，标记保存失败和可能缺失范围，取消该Run输出并由资源owner协调终止；共享实例不得为单会话强杀，无法安全隔离时保留管理责任并报告失败。不可持久化的错误保留于内存状态/安全的固定错误提示，不能伪造数据库已记录；恢复后补记事实。硬上限后不继续累积正文，缺失范围显式记录；不重发输入、不自动恢复执行。节点关闭总预算沿用上游15秒，失败仍须完成拥有资源的尽力清理并返回不完全成功。

## 7. 原生审批关联与安全交付

只有会话资源owner管理的、已完成协商且仍有效的ACP连接可以发行待审批事实。事实不可变地绑定session、Run、instance、连接generation、原生会话、原生请求ID、工具关联、完整请求结构及广告选项；Web输入、客户端时间戳、可信设备身份和内部ActorContext均不能充当发行者。授权凭据由内部私有构造入口产生，不能由数据库“已决定”字段单独重建运行权限。

原生请求ID、toolCallId和messageId分别属于不同命名空间，按对应协议允许的JSON标量类型保存，数字1与字符串"1"不相等，不把它们转成同一个字符串键。工具索引至少绑定instance、连接generation、原生会话及当前Run。权限请求可通过相同作用范围内已验证的toolCallId关联此前结构化工具事件；请求自身字段与关联事件均保留原始结构，冲突不能默默覆盖。不得跨会话、Run或旧连接借用工具参数。缺失、冲突、迟到且未建立关联或无法证明同源时明确失败关闭，维持未授权，不推测正文、不默认放行；本change不等待未来工具事件来自动批准。缺少rawInput本身不意味着必须拒绝所有请求，但无法构成可解释且可信审批事实时须报告不支持，不以探针固定命令白名单代替产品规则。

每次展示和交付前强制刷新相关历史；裁决受理前也须成功强刷。唯一裁决及审计提交后，交付仍单独认领，不把“受理”“写入传输”“对端执行”混为一谈。交付前再次核验原操作者当前身份/认证代数、原Run、instance/generation、原生请求和选项仍有效，并再次完成强刷；失败保留原决定，禁止交付。撤销使未越过交付边界的决定失去交付资格，不取消已受理Run的后台保存。

统一锁顺序为client_gate→会话owner的交付协调区→短SQLite事务；不允许在持有会话owner锁时反向等待client_gate，不在SQLite事务内等待强刷或网络。强刷在进入最终交付协调区前完成，随后在该区复核关联历史版本；如期间变化则释放锁后重新强刷/核验。取消/Run结束/连接退出通过同一会话协调区使审批失效。最终交付认领持久化和向该连接实际writer提交必须处于同一client_gate及会话交付协调区内，writer不能仅接受未经复核的异步队列。撤销先COMMIT或取消/代际失效先完成，则后续交付为零；若实际提交先完成，之后撤销不能收回已交网络栈的数据，保留真实交付事实。阻塞写受关闭预算约束，不能无限持锁。认领后发生崩溃、传输错误或确认持久化失败按unknown处理，绝不自动重发。

## 8. 初始化版本引导

initialize只发送一次对应offer版本的规范形状。公共传输先在受限响应中读取并核验响应ID和selected protocolVersion，确认是平台支持且不高于offer的版本，再使用selected版本的严格schema解析完整响应并冻结该连接策略。因此v2形状offer合法选回v1时使用v1的agentInfo/agentCapabilities响应结构，不先套v2成功结构误拒。未知选择、无共同版本、ID不匹配或响应形状不符均明确失败；不再次initialize、不按程序名选协议、不通过重发业务输入回退。真实DeepSeek单次v2 offer→selected v1证据只证明该协商路径，不宣称对端支持v2。

## 9. Web投影的具体编码

以下是Web投影类型，不是数据库记录或原生协议类型。全部nullable字段显式null，revision/liveRevision/contentVersion/committedVersion及ordinal/position为安全整数；平台ID遵循§2。list的cursor/nextCursor为不透明字符串或null，revision为捕获的节点提交版本；items依稳定平台ID升序选取最多32项，cursor绑定列表种类/认证范围/捕获版本及排他边界，60秒过期，过期明确stale_cursor。登记结果中的revision同属节点提交版本。

- HarnessSummary：`{harnessId,name,verification,detailsRef}`；verification为unverified/verified/unsupported/incompatible，只有同程序/配置固定摘要的实际证据才能标verified。detailsRef读取HarnessDetails，不直接把可执行参数塞进列表。
- HarnessDetails：`{harnessId,executable,arguments,configRef,programVersion,adapterVersion,protocolVersion,capabilities}`；arguments为字符串数组、configRef为字符串或null，不含凭据/环境秘密/原生恢复参数。programVersion/adapterVersion为已证实字符串或null，protocolVersion为正安全整数或null（ACP为1/2，未知不猜测）；capabilities为`{name,status,rawRef}`数组，status为unverified/verified/unsupported/incompatible，rawRef可null，必要原始协商字段从同范围引用保留。未启动未协商时版本null且能力未验证，不因注册成功虚报支持。未经证明的程序/适配器版本保留null，前端显示“未验证”。
- WorkspaceSummary：`{workspaceId,detailsRef}`；详情为`{workspaceId,path}`，path为节点已核验的规范绝对路径。
- SessionSummary：`{sessionId,harnessId,workspaceId,state}`，不公开原生恢复令牌。state统一采用下述SessionState。
- SessionState：`{lifecycle,run,integrityError}`。lifecycle为creating/loaded/releasing/released/resuming/interrupted/failed；creating/resuming反映原生命令未完成，不能因命令受理即显示loaded。run为null或`{runId,turnId,status}`，status保留accepted/dispatching/running/cancelling/completed/cancelled/failed/interrupted/unknown；integrityError为null或`{code,firstPosition,lastPosition}`，后两项可null，code固定为save_failed/tail_lost/integrity_unknown。不将正常保存版本放入页面状态文案。
- ItemProjection：`{sessionId,turnId,itemId,sourceId,firstPosition,kind,protocol,protocolVersion,contentRef,contentVersion,committedVersion}`。protocolVersion为正安全整数（标准ACP为1/2，平台输入为1），不与程序版本字符串混同；kind为保留的字符串，未知kind不能当纯文本丢弃结构；contentRef读取后是该条目的完整结构化JSON，原生字段不经有损字符串拼接。未提交live条目的committedVersion可为0，其余规则见§5。
- TurnProjection：`{turnId,ordinal,items}`，items为本轮ItemProjection数组、按firstPosition升序；轮次按ordinal倒序。没有真实子关系时不虚构parent/child字段。
- ApprovalProjection：`{approvalId,runId,status,restriction,requestRef,options,selectedDecisionId,delivery}`。status为pending/decided/unsupported/invalid；restriction为null或固定unknown_request/unknown_option/unknown_scope/unresolved_tool/ambiguous_options；requestRef指向保留的完整审批请求及已验证工具关联结构。options为`{decisionId,label,kind,supported}`数组；decisionId为平台随机ID，对应原广告选项，kind保留allow_once/allow_always/reject_once/reject_always或未知原字符串，未知选项supported=false且不能点击。selectedDecisionId可null，delivery为not_sent/claimed/transport_written/unknown/suppressed。transport_written仅表示写入传输，页面不得显示为工具执行成功。

session.snapshot的内联结果为`{snapshotId,revision,liveRevision,pageSize,state,items,approvals}`；items为按所选轮次顺序展平的ItemProjection。session.history内联结果为`{snapshotId,revision,liveRevision,pageSize,turns,nextCursor}`。baselineRef解码为`{sessionId,snapshotId,revision,liveRevision,pageSize,state,turns,liveItems,approvals,nextCursor}`，其中turns是按有效pageSize选出的提交投影，liveItems包含当前未提交版本；相同itemId仅应用更高版本，相同版本内容不一致是完整性错误。同一item/version可因live转持久或新读取租约取得不同contentRef，引用字符串本身不是内容身份；客户端先经content.read核验SHA-256及字节，再比较同版本内容，未校验完成不得替换已完整内容；同版本仅committedVersion推进不算正文更新。snapshot/history/baseline按同一稳定轮次边界选择，且各自原子捕获。snapshot.items包含所选提交条目与当前Run全部live条目的合并投影，同itemId取较高contentVersion，尚未提交的新item也包含；不另设liveItems字段。baseline以turns保存所选提交投影、liveItems单独保存当前Run全部live条目，安装时按同一规则合并。当前Run通常属于最新页，但snapshot/baseline不得因轮次选择或此后没有增量而丢掉当前live尾部。history.turns只返回该逻辑页选中的轮次，并将这些轮次在捕获时已有的live条目按itemId/版本合入对应items；旧页不额外插入当前Run或设置独立liveItems字段，避免改变页边界。history续取不能把后来新输出拼进已冻结的页。

超出管理帧预算的snapshot/history结果只返回`{snapshotId,revision,liveRevision,pageSize,projectionRef}`，不能同时给出部分inline数组。projectionRef或baselineRef可读取完整上述JSON，或读取`{kind:"projection_manifest",projectionType,head,parts}`：projectionType为snapshot/history/baseline；head为上述类型去掉数组字段后的完整头；parts为`{field,contentRef}`数组，field仅items/turns/liveItems/approvals。各part对象为对应字段的一段JSON数组，按parts顺序连接同名字段；逻辑页所选轮次集合不变，turns中同一轮items很大时可以在多个part中携带相同turnId/ordinal的连续items片段，客户端合并为一轮而非两页。manifest与各part均不可变、受同一快照和读取租约约束；每对象不超过16MiB，manifest不能再嵌套manifest。若索引本身或客户端/节点明确预算无法容纳，返回content_too_large/完整性错误，不能静默截断或展示半份投影为完整。节点按帧预算选择内联或引用，不由客户端猜测；客户端在数组完整接收与校验前不替换当前完整投影。

输入content对象是UTF-8 JSON `{prompt:[ContentBlock...]}`，至少一个块。ContentBlock采用ACP标准结构化内容的原始字段（最小Web发送`{type:"text",text}`）；版本策略按已协商能力验证并转换，不支持的内容显式unsupported_capability，不压为文本或猜测私有字段。原始input对象与内容摘要随受理事务保存。content.read的data为无填充base64url，sha256为原始对象字节的小写64位十六进制。

session subscribe成功沿用result信封，result=`{resource:"session",sessionId}`；该结果后先排baseline再排后续事件。unsubscribe使用`{v:1,type:"unsubscribe",requestId,resource:"session",sessionId}`，成功result相同，停止该连接该会话的后续入队；已交网络栈帧不可收回。取消订阅不取消Run，也不改变device订阅既有编码。全部通知、结果和读取仍受§3认证/撤销及队列预算约束。

列表按实际UTF-8编码帧预算选择不超过32项，不能先选32再裁掉字段；至少一项应可容纳，否则明确content_too_large。Harness/Workspace的长字段通过上述detailsRef承载，因此即使单项路径/参数转义后较大也不突破帧预算。此类节点登记元数据引用通过`content.read {sessionId:null,contentRef,offset}`读取；null仅适用于节点签发的登记详情/能力元数据及§10限定的登记Command结果元数据，不用于input或会话正文/会话Command结果。节点引用绑定actor/认证代数/快照及60分钟读取租约，逐块执行同一client_gate和撤销复核；不得把null当作跳过会话权限的通配符，也不能用任意路径创建引用。会话内容的sessionId始终必须为对应非null ID。输入上传不接受null sessionId，节点元数据仍受每对象16MiB和同连接限流/队列预算。

## 10. 登记、生命周期与通用命令存储接线

本节补齐既有§2动作的内部持久化接口，不授权新的Web动作或私有协议。登记、创建、取消、释放和恢复均由对应应用owner调用存储端口；WP3不能在适配器或进程模块直接写业务SQL。当前内层schema2尚未发布，补齐布局仍需无损schema1升级/严格校验/故障回滚，不能清理早期草案库。

所有本change变更动作共用session_commands的(actor_id,request_id)唯一空间及100000条节点上限；原device.revoke仍保留上游独立management_commands规则。登记动作以harness/workspace为目标，session_id可以null，但必须有明确目标种类和目标ID，禁止创建假会话满足外键。会话动作必须绑定真实session；审批还绑定原approval。每种action使用明确类型的参数摘要及结果，不把input专用Run关联作为所有命令的结果模型。原请求原样重试返回已提交同一结果，即使对象随后换状态；同ID异动作/异参数冲突。通用command.get只读同一当前actor可访问的Command，返回§2定义的status/resultRef；结果引用内容保留动作、目标和已提交结果，不能含凭据/提示正文或凭空推导对端执行。结果引用的会话/节点元数据作用范围遵守§3/9。

Harness/Workspace登记先由节点解析并验证路径和非秘密configRef，再在同一事务写定义、Command、元数据审计及node_state revision。保存可追溯的程序/配置固定摘要及已证实版本/能力报告；没有报告时仍unverified，不为登记执行任意版本探测命令或宣称已协商。配置变化不能沿用不匹配摘要的verified结果。列表和详情使用§9的投影，不把配置或恢复秘密作为引用正文。

session.create受理在同一事务写Command、新session（creating）、session_state初始版本及审计，提交后只派发一次。原生创建成功由持有该Command一次执行认领的资源owner提交绑定：Harness/配置作用范围、原生scope/session、非秘密恢复参数及单调bindingVersion；此时才转loaded并完成Command。原生失败或结果未知分别保存失败/未知，不能提前loaded、重发创建或把新会话当旧会话恢复。

cancel/release/resume受理均复核目标状态、原请求去重及审计后提交，交付/资源结果另记。cancel记录取消请求但保留Run前台互斥到可靠终态；release先锁定新输入并保持releasing，由资源owner协调Run/审批、强刷、原生关闭和引用回收后才确认released；强刷或资源关闭失败不返回正常释放成功。resume只针对保存的原生绑定，保持resuming直至同作用范围原ID恢复成功，失败保留旧绑定和历史，不创建替代。需要把中断Run转为可继续管理的终态时，必须由owner确认其旧资源已结束/不存在，并保留原Command的delivery_unknown事实，不能把未知运行记为成功或自动重派。

原生绑定和生命周期回报使用Command/预期状态/bindingVersion的比较交换；旧连接、迟到回调或重复回报不能覆盖新绑定/较新状态。只有当前内部资源owner持有的一次认领可以推进外部操作，客户端ActorContext和时间戳不构成此所有权。节点重启把未完成的原生操作保留为中断/未知，并让所有旧审批发行来源失效，禁止自动重试副作用。既有2.3审批来源/Run失效事务与这些生命周期操作串行协调；进入取消/释放/换代后实际writer不能再使用旧审批，安全锁序仍遵守§7。

登记Command（仅harness.register/workspace.register）的resultRef属于受限节点元数据类型catalog_command_result，可用sessionId:null读取，引用绑定当前actor/认证代数、确切Command及60分钟租约，逐块复核同一client_gate；command.get只能为该actor自己的既有登记Command签发此引用。正文仅含commandId、action、targetKind、targetId、status及该动作已提交的ID/revision结果，不含定义参数、凭据或原生恢复信息。其他会话Command结果必须使用真实sessionId和会话读取范围，null不能读取。此例外与§9登记详情/能力引用一样不是任意节点文件或所有Command的通配读取权限。

## 11. 交给独立 tester/reviewer 的断言建议

请独立维护 tests.md（本文不代写验收）：CL-003覆盖连续3次/5秒、250ms重试、成功复位、硬上限及条目结束刷新；CL-004覆盖Command/输入/Turn/Run/审计同事务和未知派发不重试；CL-011覆盖强制刷新失败及共享实例责任；补充schema1副本升级保留身份/撤销/审计、DDL/COMMIT故障和未知schema无修改；CL-031覆盖每块复核、撤销COMMIT后writer不发送、跨会话引用拒绝及帧/深度/队列原预算；CL-042核对实时/已提交快照、版本缺口/重连、正常DOM不显示保存版本。具体用例归属和覆盖充分性由独立tester/reviewer判断，不将建议当成已验证。
