# 固定ACP候选四项发现：依据、正式红与修复边界

2026-10-31。**PROMPT-001、SEEN-001、DEBUG-001、REJECTED-001均成立，仍Open；最终修复/独立验收等待新固定候选。** 不改变已确认产品规则、不扩public API、无需新用户确认。原candidate-v1/d2c290、已冻结v2、旧报告和原红记录保持原字节；不把过程修复或开发自测当最终关闭。

此报告追加前静态审查658fed5e576757bd90ebc5d9c8e474d6c9b7c65d51ade6cad1daaf6fb8866029，不覆盖其当时状态。首轮probe外cwd实际1.99.0的结果只作补充；本次已亲读指定1.98.1实际版本记录和重新取得的三个红。

## 固定源与合同

v1源码manifest `d2c29065de9b8b45c75d167c25a687825cc419bd069b20caa6a573a710b6a416`、build-inputs `3fe26f29438d580a5867ac9580ceb00c2eb6f9b00573d2955279e446a68eb8f5`，reviewer已逐核190源码及3实际dist。相对182为ACP14改8增，raw基础/锁/SQL/Node/独立oracle未变。

主合同64cc19beaeb9fd96e66500aeb9b0048519b2038705b2717fc82a5ccfb924a0cb、限制8bb5d345f34952d47f08f50571d1ebc85cfcbefc7b05abc0ac3aaa4d4bd3e3cf、empty补充7f682592及独立定义f27b/077a继续有效。固定SDK双版schema v1 1c57095a、v2 28c7251e原字节，不依赖网络或厂商猜测。

## ACP-IMPL-PROMPT-001

v1 strategy.rs ec65f0a9的known resource分支只object(resource)，放行{}、无/错uri、无text/blob或两者均错型。固定双版EmbeddedResourceResource.anyOf分别引用TextResourceContents(required uri/text:string)、BlobResourceContents(required uri/blob:string)，没有内层Other；对应64cc第1节known必需字段校验。

最小修复浅查uri string与至少text/blob一个string分支；anyOf非互斥，允许双支及单支有效+另一额外字段错型，原raw/unknown不丢；不加URL抓取、授权、非空、长度或独占分支规则。

已亲读正式1.98.1红日志 `prompt-resource-fixed-toolchain-red.txt` SHA256 `9caaa04255924f0ae08062c8aece0537e6ab467ac2532bade4fb50ab24bd3d36`：双版各5非法结构全部误接受，10对0断言失败；8个合法anyOf/unknown大数片段正例及零新增写断言先执行通过。资产 `resource_shapes.rs` SHA `f4b78314a0deb26cb69c92e1586a01c36429fa8a9e60904a07121a8bb3020359`。这是实际0/1失败，不是未编译或预期描述。

## ACP-IMPL-SEEN-001

v1 reader.rs 4f4be66e的seen.insert仅在!stopped内；ClosingDrain对先前未见的同server ID重复完整帧各自emit。64cc第3/7节连接寿命seen唯一性没有Closed豁免；Closed不建Waiting/reply grant不等于不记录seen。

最小修复在关闭排空仍记seen元数据，仅live建grant/核grant容量；重复拒绝但继续后续完整帧，不恢复grant、不删seen、不增加新额度。

正式同源公开红资产 `closing_seen.rs` SHA `3f42efd206eb7c0ecef710d619846b6174f42ab8f3d2304063f5e473b31997c0`。同一实际Ready read批首unmatched response触发Stop，后两相同新ID Permission或Unsupported均重复emit，尾notification仍交付且receipt_order相同；server pending=0、终错仍首UnmatchedResponse。两种分支均红，说明Closed防重遗漏，不是制造新grant。

## ACP-IMPL-DEBUG-001

v1 transport.rs c47bad6a的Response派生Debug，RemoteError{code}任意原数值暴露，Incoming Debug递归同样暴露。64cc第1节外来数据Debug只类型/长度/静态类别；公开读取code不等于允许Debug原值。其余字符串canary或Received整体掩码不能代验此数字分支。

最小修复Response自定义脱敏Debug，公开variant/code及完整raw保持，不扩错误语义。

正式红资产 `remote_error_debug.rs` SHA `0dd7885d4e68d5cafb46f4265081fb8ace01246b68f0b72d3e30a388547e45f6`：双版真实RemoteError的Response/Incoming共4处numeric canary泄露；Received掩码通过，公开code和原raw断言通过后最后隐私断言失败。与SEEN共用正式1.98.1日志 `seen-debug-fixed-toolchain-red.txt` SHA `c7431efe6e684db633f12e00d9c3b9b0b20d50306f9af142bda6a7d1a925d072`，两个target实际均0/1失败。

以上两个正式日志及版本文件位于tester/acp-runtime/accepted-candidate-v1；实际rustc/cargo记录均1.98.1（host x86_64-pc-windows-msvc），与source固定toolchain一致。首轮1.99.0日志保留，不回写成指定工具链通过。

## ACP-IMPL-REJECTED-001

v1 State.rejected为Option<RawMessage>，reader.route错误用get_or_insert，take_rejected_message只take。同一已取得批次第1坏route占槽后，第2坏route的完整原Raw直接丢失。

独立判定依据是64cc第7.3节原文：“排空中后续路由错误保原 rejected raw，不覆盖第一个 terminal 原因，也不截掉后面完整帧；每帧独立按原关联/形状核验。”这里“第一个”限定terminal原因，不限定拒绝原文数量；未发现只承诺首份raw的取代条款。不能从旧Option实现反推新承诺，也不能用正文日志替代保留。

最小修复内部VecDeque<RawMessage>按route拒绝顺序保存，现API同签名pop_front、重复调用取全；首terminal原因不覆盖、后合法帧继续drain。仅保已经成功得到RawMessage而route拒绝的原文，invalid JSON/非object parse失败不伪造RawMessage。不设新正文条数/字节quota。

最小独立回归：同一真实read批两个不同坏route object和尾合法notification；两完整原raw按FIFO分别取出后None，尾通知仍Received，首terminal保持且只Err一次。当前此项为源/合同确认，tester正准备原v1/v2单点红；尚未将它写成实际已执行。

## 少量公开责任Rustdoc须同步修正

v1 EventReceiver.next复用“enqueue errors retain owned inputs”等文字，与它无消费发送参数的真实行为不符。新版本应精准说明Received先交、terminal Err一次后None、取消借用等待不丢队列；RawMessage.parse仅语法+根object，不检路由/来源/身份控制；encode借用失败保Reservation，enqueue错误以EnqueueError退还所有权，两者不能混写；shutdown的joined/error/unresolved/截止后保句柄分开。不扩API或创造新归属语义，最终源码后验这些少量责任说明。

## 未提前关闭的事项

四个过程ID WRITER-001、READER-001/002、LIFECYCLE-001在固定v1已静态闭合，但仍需最终修复候选同源独立IO/期限/跨runtime析构与关闭证据。36旧函数迁移和6负例先正对照、实际child/根组合也未由本报告预签。新v2若只修前三项不得据此绕过REJECTED-001，需保v2原文另新候选。

本次仍不签必要派发账本、actual R3接IO、持久审批、原生Run因果、真实Harness/Job树、Node/WSS/Web或完整无预算共享视图。原固定源与所有失败/工具链纠偏历史保留。
