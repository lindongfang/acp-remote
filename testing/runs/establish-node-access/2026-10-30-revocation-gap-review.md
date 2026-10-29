# 撤销、分页与订阅有限缺口检视

日期：2026-10-30。范围：当前 NA008/027/028/029 Steps/Expected、access-contract §4、r6生产代码、现有单测和持久存储探针，另纳入r7 SQLITE_FULL定向独立执行。结论：**BLOCKED（静态确认的两处协调窗口需定向修复/验证）；整例未代签**。不要求无穷调度组合、真实硬件断电或填满宿主盘。

## 版本与实际执行

r6 83项已在前轮独立匹配；本轮关键 management_ws.rs SHA256 `EBC7EB0D839DE2450FAD4BCFB8DE6990A53772FF9DAB8858E9DD09E3AA05E00B` 与r6一致。初读trust.rs为 `6E73220DEBEB61400C6B84C483234D8D0F5D548BB11CF51E930FDCB7FCE6F6BF`；后续用户明确交付r7，只新增cfg(test)测试，生产路径继续按r6审查。

本轮独立使用reviewer专用target/测试自有资源，真实用户执行：r6 trust::tests **7/7通过**；`node::tests::revoke_commit_discards_an_already_queued_management_result` **1/1通过**；r7真实SQLITE_FULL **1/1通过**。未重跑全套旧测试，未新增probe或修改产品。存储58断言复用已读的2026-10-29-storage-acceptance及持久probe实际源码/README边界，不冒充本轮重跑。

## 具体代码问题

### NA-RV-001 / Medium / Open：受理和入队未与撤销串行化

位置：`app/src/management_ws.rs:695–713,728–771,790–795,869–875,1206–1228`；对比 `trust.rs:315–316,417–418`、`management_ws.rs:1286–1320`。

合同§4第70行要求受理与实际出站核对处于同身份协调器，提交后拒绝新入队。实际仅writer和目标revoke持client_gate；execute_command的数据库资格检查及返回后的send_json没有同一gate，subscribe分支也没有独立资格检查。revision分支检查后enqueue同样未串行。可达时序：检查旧trusted完成→其他执行线程提交撤销→本线程接受处理/调用try_send；writer后续仍会拒绝发送，因此**未据此认定已发生撤销后数据泄漏**，但受理/入队合同没有成立。关闭watch最终清空队列不等于提交临界区内清空并禁止重新入队。

这是源码同步关系的静态发现，未做真实WSS受控时序动态复现；现有write_barrier仅在出队后、取writer gate前，不能排除此窗口。最小闭环：在真实资格检查后/入队前置测试栅栏，先撤销提交再释放，断言不新增入队、无新受理；缓存结果/首次订阅/变更通知各按同边界检查。实现可在短临界区内重新核对并入队，注意不要在actor gate内直接await自撤销而递归取得同gate；保留已受理动作按实际结果收敛规则。不得仅补断言称writer已阻止泄漏就关闭合同差异。

### NA-RV-002 / Medium / Open：首次订阅可能吞掉新revision

位置：`app/src/management_ws.rs:752–771`。

首次subscribe先await读SQL revision=r，再enqueue初始r，最后 `revisions.borrow_and_update()`标记当时watch的全部更新已读。若其他连接在SQL读r后提交并publish r+1，最后一步会吞掉r+1，last_sent_revision却仍设r；没有下一次变更就一直不通知最新值。允许合并到最新revision不允许永久漏掉唯一更新。真实初次读取/提交之间存在await与多线程执行窗口；这是静态推导的具体竞态，未声称现有真实浏览器已复现。

最小闭环：首次订阅SQL读后加栅栏，提交另一项信任/撤销并publish后释放，不再触发任何新变更；要求收到r+1（可先r再r+1）且随后分页读到最新列表。可避免在旧SQL结果之后无条件标记新watch值已读，或比较并发送当前最新revision；维持首次通知与后续通知同一有序边界。需要加入资格检查并与NA-RV-001一起审查，不能仅删一行后忽略并发。

## 可复用覆盖与有限执行清单

以下G项为证据缺口ID，**不是已复现产品缺陷**。每项只对应当前用例明确要求；完成后按子断言收缩，不自动整例PASS。

| ID / 用例 | 已有真实覆盖 | 最小剩余动作与闭环 |
|---|---|---|
| RV-G01 / 008 | auth resolve后撤销测试；本轮重新通过queued-result writer栅栏；writer持gate跨sink.send，target revoke同gate | 固定三种边界：资格读取完成→受理/入队；原缓存命中→入队；writer gate/send先赢与撤销先赢。断言先受理按真实结果收敛，提交后新受理/入队/发送计数为0。无需排列无限跨积。001为实现差异，先闭环它。 |
| RV-G02 / 027、008 | 真实存储probe的语句失败及延迟FK COMMIT失败，状态/generation/revokedAt/revision/审计/结果原子回滚；不是单纯结构门禁 | WSS A撤B的失败响应及B仍有效可并入下一条真实连接场景，不能将FK错误写成磁盘满。r7 pager SQLITE_FULL已独立通过，见独立r7报告，无需另造宿主磁盘耗尽。 |
| RV-G03 / 027 | 本轮真实TrustStore可控时钟86399秒请求冲突、86400秒同ID变更载荷新受理、再重放、重开后仍revoked；已不是“24h完全未测” | 将这组纳入证据。补一组时钟满额联动：10000条未过期仍保留旧响应；精确到期后回收一个槽位、同ID可新受理且旧身份不复活。旧probe直接改expires_at只能覆盖夹具过期，不能当时钟推进。 |
| RV-G04 / 027 | 58断言probe真实9999→10000、满额拒绝/快照不变、旧结果重放/冲突、封闭字符串载荷规范化digest匹配 | 不要求10000次浏览器点击。复用容量夹具；仅补WSS node.info/list/订阅与多帧后SQL结果/审计计数不增长，并确认成功/失败返回词表。固定封闭载荷无需无穷RFC8785排列。 |
| RV-G05 / 028 | 既有真实WSS A撤B、B close4410/重连4401；单个B队列丢弃 | B用两个真实连接均订阅devices，同时发命令/读通知；A撤B后两连接关闭，无提交后的受保护帧/新入队。另一隔离场景A自撤销，允许无success；owner list确认并从owner创建/管理恢复入口。对既有未变证据按断言复用，不因Web改字重跑基础TLS。 |
| RV-G06 / 029 | TrustStore70条分页32/32/6、different actor拒绝；SQLite同快照栅栏；真实WSS旧cursor在撤销revision后stale测试 | Web有效身份下准备65或70条，遍历验证ID集合精确相等、无重复、末页/nextCursor=null；SQL单测只有页长，不能代替Web完整性。cursor交换到另一节点/另一认证客户端、篡改拒绝；受控59/60秒（现有仅61秒）到期边界；分别真实新信任和撤销后旧cursor拒绝并首页恢复。固定两节点两客户端足够，不需无穷组合。 |
| RV-G07 / 029 | 单次首次及撤销后的revision通知已在节点集成测试断言 | 先闭RV002，再固定连续3次变更，允许省略中间值但最终revision必须送达，帧仅v/type/revision；通知后主动分页读最新状态，取回时撤销身份必须失败。比对无全量列表推送。 |
| RV-G08 / 029 | DTO字段白名单、静态安全头等已有测试可按未变源复用；完整公钥只允许node.info用途，不能误报为列表泄漏 | 对本次成功/失败/revoke审计、列表序列化、Web页面和静态页做有限敏感字段/材料断言；验证no-store/no-referrer/CSP及配对码不进URL/storage/第三方请求。不得用Gitleaks替代运行时证据，也不因node.info合法公钥判失败。 |

## 缓存与执行适配说明

管理WSS无单独响应LRU；“缓存命中”对应TrustStore.revoke读取management_commands原结果（trust.rs:322–341）。目前该函数持的是被撤销目标gate，不是调用actor gate；actor资格在WSS外层查DB。可用B重放撤销C的原requestId，同时A撤B，检查actor失效的结果入队边界；不需要虚构新的缓存模块。

客户端gate是Weak索引，持有者取得Arc并lock保证writer/revoke共享；外部probe不能直接访问私有gate、队列长度、精确SQL后栅栏。要复现G01/RV002，需授权的cfg(test)定向seam或在自有独立源码副本插入测量，明确区分插桩版本；当前只读约束下未擅改产品。多连接WSS、计数和分页可直接用自有独立probe实现。用户已单独交付r7 pager测试，本reviewer不重复开发该项。

本报告没有用tester旧统计作当前事实；引用2026-10-30-independent-tester.md旧PARTIAL只作证据索引，其中“全部调度组合”“24h未测”等宽泛描述应按以上明确必要断言收敛。新增稳定问题RV001/002不重开原CON/LIFE/SC/I01。未提交、推送或归档，未操作用户30447或日常浏览器。
