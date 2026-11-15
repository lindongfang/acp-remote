# 公共ACP runtime revision2独立合同复核

2026-10-31。**revision2工程合同限定PASS；关闭ACP-PLAN001、002、003。** 当前没有新增合同阻断或待用户决定的产品分支。编码门禁还须以独立tester最终同revision2定义绑定相同SHA，不能把尚指向v1的59行定义自动冒配本版；本报告不宣称代码已写、测试已过或产品已支持。

## 固定输入

- 主合同：`64cc19beaeb9fd96e66500aeb9b0048519b2038705b2717fc82a5ccfb924a0cb`。
- 限制表：`8bb5d345f34952d47f08f50571d1ebc85cfcbefc7b05abc0ac3aaa4d4bd3e3cf`。
- freeze-v2：`3dd8a31b225e00127380d7634432890e60a8c1c34def0d7ec8d91005060f8940`。
- 已核v1主文49f5c9a1、限制4b5ae041、freeze5d2cd253原字节仍在；原v1预审3b6d4ca7不覆盖。行为来源及22固定源与固定SDK双版schema沿v1核验，不读取私人配置或网络。

## 原问题逐项关闭

| 问题 | 同版明确规则及结论 |
| --- | --- |
| ACP-PLAN001 | 第3节Reservation唯一未enqueue责任；encode借用不转交，Frame无槽副作用；EnqueueError退还原Reservation/Frame，直接Drop会走Reservation RAII；成功enqueue同锁消耗Armed并转给owned cell/Ticket，无无人持责缝隙。Drop/cancel首终态不覆盖；普通pending/setup归还、server原generation仍live才回Waiting、seen不删；initialize成功reserve后不可复活。跨scope/cell参数错误不改另一cell。Closed。 |
| ACP-PLAN002 | 第5节真实R.poll_read与writer真实poll/FrameAccepted共享短锁，read receipt取终止LF所在批次cut，完整多帧即使尚未拆出也属取得。route需cut资格和当前原generation，parse后才打开/发送不能补资格；同批次先response后update会关闭窗口。第7节Stop先seal新IO、保body-free关联和held cuts，不abort持帧reader；按原序parse/route全部已取LF帧，无新读/新grant，后续帧不因前错被静默丢弃。任务真实终止后terminal一次排尾，Err一次后永久None；shutdown借句柄可取消/并发/再次join，超时不提前terminal、不丢Core责任。Closed。 |
| ACP-PLAN003 | 第1/2/3/5节补齐RawMetadata/OrderedRawObject、Listing/Directories、Permission/Choice/Unsupported/Received、EnqueueError等精确getter/迭代/错误所有权。第6节At从reserve起由真实owner推进，不依赖next/wait；Reserved/queued/Pending0期限为Zero，partial/full-beforeflush为Unknown+Stop，Written不回滚，response消费后解除其At。pre-At合格held cut暂缓该记录expiry、暂停新writer poll，仅处理已取得集合，有效匹配才解除期限；无匹配则按原表到期，恰等At不能补资格。不加长模型统一两秒。Closed。 |

这是可实现的本地读取顺序与传输证据模型；ReadReceiptCut不证明原生发送时间、Run因果或原生ACK。每批最多64未决的浅关联快照不含正文；有限scratch只是处理块，不是新总帧配额。锁外大parse、锁内真实poll和标记的顺序自洽。实现仍须防止监督路径在两个IO handle登记之前先报终止，不能把首个task退出或Vec暂空当joined。

## 保留的额度分类及真实实现范围

revision2限制表准确落实v1分类：公共链删32MiB/96/token、列表4096条、Message/Tool纯长度过滤及额外非空prompt拒绝；保两方向64同时pending、防重复seen而无累计4096；Session/Option/RPC和原list/replay/listing参数保护只保既有位置，不能扩到新SessionSetup/new/resume。明确这些保留数字是平台差异而非SDK或用户逐数字规定。

P2 parse_patch去输出身份长度过滤，但旧decode_anchor/ViewWindows/staging长键及所有视图预算只属非必经遗留显示路径；新IO/Received不调用。NativeSessionKey/SQLite长ID、Node/WSS/必要账本和完整无预算共享视图另gate，不能以这一阶段测试签其已交付。没有更改身份、授权或原生Run产品规则。

配置Known/Unknown、完整MCP/roots/版本能力、v2 Other、浅raw及敏感Debug边界沿原审查保持；所有公开数据可构造性与私有grant来源分开。必须交真实spawn/reserve/encode/enqueue/next/shutdown路径、独立reader/单writer及自有协议child实际管道，不能只实现新trait或CAS。

## P1导入3例的迁移许可

亲读`app/tests/acceptance/p1_observation_ordering.rs`的三个完整原函数。它们由主P1文件mod ordering导入，位于原33个主函数清单之外，必须另外逐例登记，不能静默漏编或用33计数遮盖。

限定准许：保原三个函数及全部stats/usage/newview Reading断言，分别改为真实异session→Ignored、真实v2state→Ignored、真实同session旧view→Stale。原raw/event不一致通过修改Incoming公字段建立；新Received私有且仅借出&Incoming后，该攻击公开不可达，因此不是字面保留同一动态攻击路径。需单列替代证明：同源外部正对照先编译0；负例验证incoming不可改、克隆后伪Incoming不能传给observe/apply(&Received)或重建Received；静态核无public factory/From/Deserialize/可写字段。负例必须核正确错误位置/类别，不得以坏import或产品自身编译失败充数。不得增加fake品牌/生产hook。原wire坏shape、scope拒绝和其他动态断言仍保。

实际候选逐hunk和compile-fail原日志尚待交付后审查，本许可不表示已运行通过。旧三例原文件/hash与旧历史报告保留。

## 编码与最终交付门禁

合同三问题已关；最终同版独立定义需包括新增receipt_order、批内LF顺序、pre-At held-cut无匹配终结、Err一次None、Reservation.cancel/EnqueueError及上述三P1迁移。收到精确SHA并核覆盖后可在已授权隔离workspace按此窄范围编码，无需新增用户确认。

编码PASS将仅许可实施，不代替同源产品审查或真实动态验收。任何实现偏离、API修订或新产品分支仍另revision；v1/v2冻结字节均保留。未签真实Harness、Job树、SQL派发及actual R3 owner接线、持久审批、Node/WSS或完整WP3。
