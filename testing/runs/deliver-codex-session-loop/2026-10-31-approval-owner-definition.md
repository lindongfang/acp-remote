# 审批 owner 独立子用例准备

2026-10-31。本文件为方案/验收定义，全部新增子断言 **not-run**。依据 permission-approvals spec、implementation-contract §8/8A、approval-owner-contract SHA256 `4b9c4f81103ab59f2a8de690d3761d157cc8a88ba51083c4f76d3efa21c6b542`、现有 schema3 及固定 SDK2.1.0 v1/v2 schema。先前读取0882a1bc...候选后，已纳入本次§3A的精确DDL/迁移；不将新定义写成已实现。已知 AP-PLAN001/002 的定义修正（planned/actual分离、唯一ReplyCell、无需数据库的协议安全取消）保留为必测依据，不因设计关闭而填写产品通过。

**当前整体不能编码。** schema4精确DDL/迁移已追加为§3A候选，尚待独立方案闭环；来源API、v1/v2取消触发矩阵和完整内存账本仍未冻结；具体阻断见 approval-owner-gaps.md。本定义不创造DTO、替代API或测试专属授权开口。不修改统一tests的ID或标题，也不替代已冻结§8A纯ACP投影用例。没有必须另问用户的产品决策。

## 执行与证据边界

后续按冻结公共API分层：L3验证来源/状态/租约/单次消费模型；L4使用自有SQLite临时副本、两个独立pool和可观察wire协议对端验证事务/并发/消费/故障；实际节点授权/WSS/生命周期为L5，真实Harness与Windows/Web闭环仍L6。每个阶段只签具体子范围，不将完整CL降级或改为Passed。本文件不运行任何审批、Harness、浏览器或Web审计，不访问用户空间/真实数据库/凭据；管理rev2的独立测试资产不修改。

正式执行先固定源码/锁/DDL/测试资产SHA，独占CARGO_TARGET_DIR及自有测试数据/端口，确定性屏障代替概率sleep。捕获真实writer帧和调用数、明确COMMIT结果、只读管理事实及受计量usage；单纯Mock返回success不证明真实持久化或原生交付。先完成故障/资源清理，日志不输出完整请求/选项/路径/凭据。真实Harness证据单列，不把协议替身当真实能力。

## 逐 Requirement / Scenario 映射

以下保持现有CL ID/标题，字母仅为本文断言组。

| Requirement / Scenario | 本阶段定义 | 稳定ID |
| --- | --- | --- |
| 保留审批语义 / 不支持的请求 | A/B/C：保留实际raw和广告，合法无subject不误拒；未知subject/kind/重复optionId不得selected，明确限制并依版安全结束 | CL-013/068/078；§8A既有纯协议范围另验 |
| 审批事实独立于聊天缓存 / 展示缓存失败时已有审批 | B/J：来源仍有效且已通过基础门禁，展示/重放失败不默认允许、不复活旧请求、不依赖历史强刷 | CL-025/026/065/068/078 |
| 唯一裁决与交付分离 / 两客户端同时审批 | D/E：两actor/两pool相反选择，最多一个决定/命令及一次实际响应，另一方得原事实或明确冲突 | CL-004/014/018 |
| 同上 / 决定交付失败 | F/H：保留决定，传输未知不记送达、不重发、不宣称peer执行 | CL-015/017/076 |
| 决定持久化失败不授予权限 / 批准保存失败或提交未知 | F/G：必要COMMIT未确认则零selected，保留未知责任，可独立safe cancelled | CL-075/077/078 |
| 同上 / 已有允许决定随后协议要求取消 | E/G：原决定allow/planned selected/actual cancelled可区分，只有同一RPC一次消费，不是第二人工决定 | CL-068/078 |
| 同上 / 已发决定但交付记录失败 | H：已发/可能已发事实保留，过期/撤销不阻止保存已有事实，失败后零重发 | CL-076/078/084 |
| 审批关联真实原生操作上下文 / 加载审批无输入执行 | A/B：加载/恢复/后台标准请求无Run仍可决定，不造Run/Turn/Item | CL-065/066/068 |
| 同上 / 上下文失效后交付 | E/I：请求/专属工作/资源/连接失效先到则零selected，不向新连接挪用 | CL-016/068 |
| 同上 / 无法证明审批归属 | A：裸DTO、字段拼装、最近Run、旧DB行不能铸造Observation；验证私有Connection当前pending+raw | CL-068 |
| 请求有效性受原执行约束 / 恢复后批准旧请求 | A/I：旧连接/旧scope拒绝；新连接同短RPC为独立来源、重新裁决 | CL-016/068 |
| 同上 / 前台结束后后台请求审批 | B：Run已结束后新发后台请求真实来源足够即可，不伪Run且保护实际资源责任 | CL-066/067 |
| 同上 / 前台结束时后台审批仍待处理 | B/I：Run终态/idle/load完成均不批量失效既有有效pending请求 | CL-067 |
| 同上 / 协议不区分请求的前后台归属 | B/I：真实session/connection/RPC足够，按固定该版取消范围逐一响应，不猜最近Run或按tool区分 | CL-033/037/041/067/068 |

## A. 实际来源与不可伪造观察

步骤：从真实公开Connection双工读取Incoming::Permission建立实际请求；逐项替换平台target、source/nativeID、runtime/connection/load、instance、版本、JSON标量RPC、原始raw和广告。构造相同可见字段的裸对象/网络DTO/旧管理行；保留旧scope对象向新连接提交；输入整数7和字符串"7"、同义不同空白raw、改一个广告字段的raw。同一(load_epoch,RPC摘要)再次观察相同/不同request_digest及资源引用。

预期：只有节点当前连接owner可发行Observation；各伪造来源零selected。typed RPC摘要区分7与"7"；raw摘要对实际读取UTF-8字节计算，不重序列化冒充原始字节。相同请求不得重复登记/取得第二ReplyCell；冲突作为连接错误，不覆盖原来源。摘要不是授权证明，原始RPC/广告由私有owner在实际writer前再次验证。当前持久行不能恢复旧原生资格。确切公开构造/观察API及冲突顺序待冻结。

## B. 无Run、真实生命周期与缓存失败

步骤：合法加载/恢复、原生自发、后台四类实际请求分别无Run；可靠WorkToken关联时附可选Run，对照仅有“最近Run”。登记后令Run结束、前台idle、load/replay返回；再对照request取消/已答复、连接换代、可靠专属工作结束且使请求失效。对已准入来源注入展示快照/重放失败；对未满足基础重放门禁的来源尝试绕过。

预期：无Run和无精确前后台分类不误拒真实标准请求；不造Run/Turn/Item，不把工具关联作为批准前提。前四类平台/前台状态不单独失效实际请求；真实协议失效事实才撤其资格，仍保护所属资源责任。显示失败不默认允许、不复活旧请求；未准入来源不能借审批绕过门禁。浏览器断开不结束节点owner；actor撤销只撤该actor资格，不将有效后台请求全局过期。

## C. 原广告与完整签名绑定

步骤：在§8A合法v2无subject/null、command无toolCallId分支使用原广告；v1必需tool字段保留。遍历未知subject/kind、重复optionId、选择不在广告中、同label不同id/kind。对固定数组payload逐一改变approvalId/targetId/sourceId/nativeSessionId/各epoch/instanceId/RPC摘要/raw摘要/option摘要；改变签名typ/action/typed target/node/actor/generation，请求jti时间范围及原广告。

预期：只有实际广告中唯一已理解选项可人工selected；展示文本不是授权。optionDigest绑定原[optionId,kind]；payloadDigest绑定合同数组，字符串/数字/顺序/大小写按冻结规范向量严格验证。acpr产品请求独立用途，签发不批准、不创建command/Run或发送。不能用任意Value/虚构action代替正式DTO与摘要向量。非法主题的协议安全cancelled仍须同实际pending RPC一次消费。

## D. 唯一决定、重复与命令容量

步骤：两个actor/两个pool在同pending approval作相反选择，交错事务与最后writer屏障；同actor/request相同payload重试、异payload重试、另一actor新request；同一命名空间已有其他产品action。查询原决定、delivery及attempt，重启后再次查询/提交。

预期：pending→decided最多一次，原actor/选项/决定不被后请求覆盖；一个决定command恰好4个审计stage，审批不造Run。合法重试只返回原事实，无ticket/permit/新attempt/新writer；异绑定拒绝。10000现存保护命令边界及审计4slot原子预留沿管理合同，不能为审批另设重复命名空间或靠清理腾位置。本阶段不清理；100000槽自然不可达的既定限制保留，不用非法fixture冒充容量证据。容量不足零危险交付，safe cancel另走安全边界。

## E. ticket、claim与唯一ReplyCell

步骤：成功决定返回本运行期ticket；并发claim/重复claim/查询再claim/重启读行再claim；在claim前后、ReplyCell消费前后交错资源/actor/request失效。交错selected与safe cancelled争同一RPC；准备planned selected后在尚未可能传输前抑制，改走安全消费；再对照进入首次可能传输后失败/取消。

预期：必要claim COMMIT明确成功才产生非Clone一次permit；query/retry/restart不重造资格。ReplyCell只Pending→Consumed(actual-kind)→Written/Unknown，不能回Pending。所有selected/safe cancelled共用同一边界，实际wire最多一份响应。planned只代表持久claim意图；actual在真实唯一writer消费边界由节点回执证明，可actual cancelled而原决定allow/planned selected；未证实actual可为空，不能由planned猜填。一旦可能selected传输，禁止补cancelled或改actual，未知保持未知。无真实pending时零writer/suppressed，不向新连接重放。

## F. 必要提交故障与真实COMMIT未知

步骤：来源观察、决定受理、claim各事务逐写入失败、DDL/FK实际COMMIT拒绝、COMMIT确已提交但故意丢失成功回执分别执行。COMMIT等待/取消注入使用冻结存储边界；确认实际DB行与返回证明分开。故障后重试、只读核查或进程重启。

预期：观察不确认不能报告可靠登记；任一危险必要COMMIT失败/未知不产生允许ticket/permit、不可靠接受、零selected。真实提交未知可能已有决定/claimed行，原事实不被抹掉，但不能靠查询行补发/重新授权。关闭危险入口、保留资源/管理未知；未确认SQL连接不得回池。没有DB提交不阻挡仍有效RPC的协议安全cancelled，见G。恢复危险ready和最终reconciliation属后续阶段，保守unknown不是恢复完成。

## G. 无数据库依赖的安全cancelled

步骤：观察事务失败导致没有approval行、决定COMMIT未知、claimCOMMIT失败三分支让真实pending RPC仍有效；持续让DB不可写，调用安全结束；对照已取消/已回复/无待答复RPC、错误连接、原raw不匹配、已经Consumed selected未知。actor已撤销时分开测“读取敏感内容”和“节点安全结束”。

预期：仅私有连接scope+原标量RPC/raw+协议资格足以安全消费，不要求危险permit/可信原actor/DB提交。zero selected、最多一次cancelled；无pending可零writer，跨scope与已Consumed不再答复。不能因为DB故障无限保留等待，也不把安全响应当第二个用户决定。未持久化则节点仅保留有界元数据回执/资源责任并明确管理记录未确认；不冒称已持久记录，不泄漏撤销actor受保护内容。safe未知也不重发。如何为无先前attempt的safe响应补记metadata/投影须先冻结，测试不自造claim。

## H. 已发送事实、结果记录和回执

步骤：wire已收到selected/cancelled、write/flush明确完成、write中断/timeout/future Drop可能部分发送分别令结果事务失败/COMMIT未知；在结果保存前让请求exp/原actor撤销；DB恢复后仅报告同原来源可证明事实；进程退出丢失内存回执分支。

预期：transport_written只证明传输，无peer执行ACK。原决定/实际种类/传输未知分别保存；记录失败不清成not_sent、不复活审批、不重复writer、不改实际执行事实。过期或actor撤销不阻止记录已发生结果；危险入口关闭并保留责任。回执丢失不能伪造actual/完成；后续补记只幂等协调、不能重发。没有本阶段恢复合同不签恢复ready。

## I. 时钟、原生期限、取消与所有者围栏

步骤：签发/受理/claim/最后writer分别处在expiresAt前、等于和之后；原生取消早于24h、前台终态但请求仍有效；旧request_epoch/connection/load/runtime；等待actor gate/target owner/SQL/队列/writer分别超时和取消。撤销COMMIT屏障与消费屏障前后交错；排队资格取消后晚消费；同实例其他目标仍工作。逐版取消矩阵冻结后列具体触发/范围/对应wire。

预期：`now>=expiresAt`拒绝危险资格，期限/epoch先于去重；签发不授权，重连不续签；原生有效期独立，可先于产品请求截止。已发生结果可晚记录不重发。floor/单调钟回退规则沿管理合同，必要floor提交未知零selected。按client_gate→目标owner→短SQL→释放SQL→最后验证/writer顺序，不能持写事务等外部；最长10s操作、gate/owner各5s、SQL5s、writer2s和剩余期限取最早，内部重试不重新计时。取消/Drop在释放gate前失效潜伏首次写资格，迟到队列项不发；已可能交字节仅未知，不声称收回。安全结束受有界owner期限约束，不借actor过期无限等待。不能因一目标故障强杀共享实例。

## J. 正文隔离、完整预算与任务停止

步骤：原请求中放入唯一敏感标记（仅合成），在来源/raw/parsed字段/广告选项/冻结显示/writer暂借/旧lease全路径持有并逐个Drop；恰好/超过单请求1MiB、连接8MiB、节点32MiB、目标64当前引用；继续接收32MiB帧边界；超预算/展示失败/撤销/停止/传输未知分别清理。检查SQL所有新旧正文表、审计、错误、Debug及日志，不输出正文。停止时检查监督任务和队列全部有owner并join。

预期：计费先于拥有/复制，各完整结构和暂借/旧lease未释放时不能退账；无无计费clone/泄漏。超限不自动selected，按协议安全cancelled或未知；显示引用不能复活失效请求，浏览器断开不取消节点owner。持久只含允许的ID/摘要/状态/操作者/代数，无title/path/tool正文/原optionId/原完整请求；旧正文不读写，不建Turn/Item/强刷pin。清理只释放自有内存不删决定/去重/资源责任。停止先禁新危险资格、失效发送权、依协议安全结束、关闭writer/连接、join，独占树必要回收/共享实例隔离另验。确切固定费用/计费转移/Drop时机/统计观察API未冻结前，本组不能编码，更不能靠raw.len充当全账本。

## K. schema4副本升级与元数据约束

按§3A定义以下真实SQL子分支；最终代码入口/故障枚举冻结后适配，不开放生产任意SQL或版本覆盖：

- 真实原始schema1/2副本、完整schema3非空管理副本及全新库→4，4再重开；1/2在同事务建立原schema3再增4，3只追加新索引/表，更新node_meta和PRAGMA一致。原schema3副本须实际有命令（含终态/未知）、Run、同target/load无Run审批、有关联Run审批、delivery、4stage槽及原始身份/撤销/旧正文，不能空表代签。
- 对所有既有表逐行/字段/原DDL及索引保留比较，特别是request_epoch/time_floor、原决定actor/option摘要、claimed/unknown/transport_written既有delivery及未终结责任。新增attempt表可空；迁移不得从旧delivery推断actual、凭旧行重建ticket/permit。新库只生成一次epoch，3→4及4重开完全保留原epoch/floor。
- 未知/未来version、双version冲突、残缺原布局/控制行、缺身份但存在CORE/SESSION/product任类事实、旧version夹带新表/新索引的部分schema4、schema4缺任一新对象分别只读拒绝。复制已提交未checkpoint WAL中的未知version，独立只读确认该值确在WAL而非主文件；读取完整WAL视图后拒绝，不能用immutable漏读换绿。拒绝前后比较主DB/原有WAL字节与已提交事实，沿R003允许owner私有非权威SHM维护及原不存在0字节WAL，核对目录锁/私有ACL。
- 在旧布局建成（1/2适用）、新索引、新表、metadata、PRAGMA、最终验证、COMMIT各边界注入故障；真实延迟FK导致COMMIT拒绝单独验证完整回滚。不把PRAGMA前hook当真实PRAGMA失败，不用trigger拦PRAGMA。另将真实COMMIT成功后丢失回执与回滚失败分列：实际DB可能已为4，返回未知不得冒称完整回滚或删除重建；重开仅按实际完整布局判断，不产生危险资格。
- 实际SQL正负矩阵：planned selected+合法digest与cancelled+NULL可通过；selected+NULL、cancelled+digest、非lowerhex64/错长度/未知kind拒绝；actual(NULL,NULL)、(selected,digest)、(cancelled,NULL)分别按合法transport状态通过；actual=NULL/digest非NULL、selected+NULL、cancelled+digest拒绝，尤其覆盖SQLite CHECK(NULL)不可漏过的NULL分支。claimed必须actualNULL，transport_written必须actual非NULL；unknown可actualNULL或可证明selected/cancelled。时间0/安全整数最大值可通过，负数/越界/小数/非数值类型及未知reason/state拒绝，记录SQLite STRICT对可无损整数转换的实际语义，不擅自要求拒绝合法整数转换。
- attempt approval PK、claim唯一及(approval_id,claim_id)复合FK：两个真实approval/delivery各自claim，正确组合通过，跨配组合/不存在claim/重复approval/重复claim拒绝；尚无claim的not_sent不能凭NULL绕过attempt NOT NULL。原有无Run与同target/load合法、cross-target/load/run拒绝的FK保证继续成立。attempt/投影/result的同事务更新失败不得留下半边状态；actual不可从已可能响应的selected改为cancelled由应用单次消费/更新CAS验证，不能假设DDL跨行历史会自动保证。

预期：增量事务原子、旧字节/事实保留；拒绝不重建/不清库，标准read-only读完整WAL视图，遵守已关闭R003的侧文件边界。新attempt表不要求把旧历史补造成新actual。安全响应补记与delivery约束不得强迫伪造人工决定/危险claim/actual，相关运行路径仍属缺口，不能以纯DDL通过签整体owner通过。

## 本阶段门禁

以上为产品必须覆盖的用例准备，不是schema4或owner开工许可。approval-owner-gaps.md所列工程缺口关闭并由独立reviewer核对最终合同、测试入口/故障点及A–K映射后，才能针对明确子阶段编码。§8A投影独立阶段及管理rev2可按其各自冻结边界推进，不因本定义扩大通过范围。实际执行报告必须逐组给命令、数量、实际wire、DB前后、失败/未执行与资源清理；不得用计划/开发自测代签。
