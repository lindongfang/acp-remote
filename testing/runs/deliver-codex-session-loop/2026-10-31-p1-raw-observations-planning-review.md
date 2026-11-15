# P1原始观察记录编码前独立复核

2026-10-31，reviewer `/root/baseline_reviewer`。只读根合同、独立最终用例及既有公开Domain/ACP类型，没有产品修改或新执行。

**P1 raw观察记录子包编码门槛通过，未见剩余具体定义阻断。** 仅放行新`adapters/acp/view_observations`模块、其开发测试和ACP模块声明；不改Domain、既有Strategy/transport、namespace、依赖或数据库，不接node/WSS/实际owner。不是完整投影或任务3.2通过。

已实际完整读取并核SHA256：

- 根`p1-raw-observations-contract.md`：`a071335b22ed7b028323f1e629d9df3460e858b1434b0adf43aa76d3659f1f3a`。
- 原request-tester最终`p1-raw-observations-definition.md`：`8f8e7d49b518e31e55fe3ef8b9bfec323c43fd1950ec49b677189a061840dd2e`。
- 最终输入清单：`ee348f112c52829fba15fbec96e916646804c4aba649d36d491b587e52d3b6de`。

原外部8e稿和初定义20f4只保留历史归属，不拿未精化稿覆盖本轮。原tester提出的P1-TG01–03在最终定义层均可关闭，实际八组仍全部not-run。

## 方案与独立用例一致性

begin只借Strategy提取实际协商Version，不克隆其initialize raw；SessionId复用已计费ViewHandle.native_id，不藏另一动态字符串。当前Incoming尚无私有连接proof，来源/操作者仍由实际Connection owner负责，本模块不自称授权边界。真实frame仅借用，持久条目仅Domain不可变字节，没有RawMessage/Value/Arc免费保活。

分流先处理NotUpdate/OtherSession及仅协商v2的StateFirst，Ignored零序号/费用/丢失/状态作用；v1同名未知tag仍按未知观察保留。匹配显示输入先查当前handle与raw通知形状一致性，再checked发seq；Stale/InvalidObservation不伪装Capacity/成功来源证明。每次合格显示Update即使不能保留也消耗序号，Dropped不占成功position，CounterExhausted不回绕且只停止该read显示登记，控制/权限仍可分流。首因/计数/first-last固定O(1)，不声称边界内连续全丢。

每条目32字节ACPO格式及完整原JSON-RPC通知字节，key=None，不当聊天实体或可靠原生轮次。parse_header只校验固定magic/envelope1/wire1或2/origin1–3/kind1、seq正数、raw非空、保留位0和精确长度；origin0/尾随字节拒绝，转换checked。借用输出不产生拥有型复制；头格式正确而raw非JSON不构成来源或内容验证。

read先收1536（512+1024）、每帧工作区768（512+256）、候选512+32+raw实际容量及1对象。全部同原target/node账本，先预留再复制；候选只有一次Insert，无索引/scanner/current、无重试队列，不把一次失败变全局DrainOnly。原Domain FIFO/真实水位/旧租约峰值可复用同版已验证部分，本模块新增固定字段/实际capacity与Drop峰值仍须独立编译证据，超出预定上界先补合同，不事后补费。

R1–R8有可执行的真实双工Connection输入及逐byte oracle、origin/seq/分流/换代、生产尺度多次FIFO与超限后恢复、权限/状态/别目标/响应持续pump零cancel/close、费用/容量/旧引用、header负面与零分配、误用/陈旧/计数极值/负编译、同版原视图/预算/ACP发现权限回归。原raw比较不能用Value重序列化替代；parser不能同时当encoder唯一oracle。全局OOM边界仍继承d6，不把选定null注入当全部heap可恢复。

P0实际null入口已单独通过，P1可按真实Connection加入相应用例；不为本包重新修改Strategy。真正协议错误仍走Connection原边界，不能吞成显示Dropped。测试实际数量/源前后摘要/退出码与未动态分支须按冻结实现报告。

## VPROJ-PLAN-003限定关闭与002保留

**003在原始观察记录层的定义缺口可关闭。** 新格式逐条同时保存原raw、seq、origin，不再用单个实体标签覆盖Replay→Live两种观察；摘要仅ObservedOrigins/Mixed且以记录为准，DuringReplay不升级为“每条都是历史”的证明。R2及R6可独立验证该定义。当前没有运行证明；若P2再引入聚合，必须继承每条原观察标签/精确边界，不能只携一个起始或末次origin然后冒称本次关闭覆盖新设计。

**002仍Open。** 本包只保留raw观察，不提供实体current/锚点/原位工具更新；所有观察各有位置不能被当成已满足“工具晚完成更新原工具位置”。256索引遗忘及软轮转/RawOnly旧锚仍在的原位义务继续由P2设计解决，不能以本子包“不聚合”永久排除。P1门槛不放行新lookup/namespace/scanner或完整P2。

无新增用户产品规则，无必要等待完整node/WSS才能实施此纯数据子包。实现冻结后仍须独立交付检视与八组实际验收，不签完整CL/L5/L6、真实Harness、页面安全呈现、来源授权/撤销、公平调度或分页。
