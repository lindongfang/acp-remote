# A9 重放失败后 current/candidate 与分段读取：独立窄解释

结论：**已有语义足以要求失败candidate有终结出口、旧current保Stale且可按当前权限读取、后续真读取可继续；不需新增用户产品决定。** 但“失败后的同SID live段如何公开可读、如何与下一read隔离”尚缺精确工程路由/metadata ABI，记 **SV-REPLAY-FAILURE-ABI001 Open-design**。允许在原授权范围落实明确的私有失败marker/状态责任，不批准未见的新分段路由设计、用失败candidate替掉旧current或长期只内部保raw的方案。Shared83/任何WIP均未签通过。

本轮只读现合同、原83定义及WIP5三个相关源，不改Root/作者/tester/旧包，不运行矩阵。作者其它已获授权工作无需暂停。

## 依据与输入

- structured-history：`bfcd46d1735773b920f7400524dacb1fedf218452ecfdc1fa00842d8525ab13a`，29–56/77–89：刷新失败明确旧视图；当前连接内容须可展示；来源不明分开展示/明示限制，不猜合并；读取未完整不得伪Complete。
- revision2 engineering：`e54ee9755a4bbf62b2999f9846a41d69a7db6a2ca400faad6e3ebc3382a77aeb`，R2-2保留v1无预算/raw/FIFO/H/tail等非接口语义；R2-3完成与传输/新故障范围分轴。
- 继承v1 engineering：`eee353d81eb018c32a694ff3e9c59f3dd60222602ca2c1e70002ef649f5868ad` §4–7，特别85–97：失败保旧Stale；一个在途read；候选按原RPC/read绑定；无稳定ID重建；完成后flush Unknown不撤完成；无法衔接不阻展示实际内容。
- 当前read/source：`8422edfd377f4171b30985c0a513297b02978a69f63fbb427eb7593ac90954f5` A2（151–159）、A3、A5/A9；send：`ea8a3ab7f9da7642b23c9760d6bc15814600adc58883aa1ba94e53ddd20ea26e`；assembly：`e0ea8a3b753bb23b54376dc0abf30832296937b28691316147036144b0e4b6d4`。
- same-read补充3594608d/c6a106bd及既有gate6b5b原字节：只对原operation真实完成且current正其ReadEpoch的成功发布续接；失败/Incomplete不是此续接许可。
- 独立83定义 `d2eaac1ff350dcc88c4c7bbeadf19f5fb2b674babf724a502437ef23c73bc717`：SV032取消只本consumer；033共享已有内容；034失效与仍保留旧视图分支；038失败stale+独立错误；039单在途/desired_refresh；029–031/041–049的固定版本、无丢唤醒、真实完成及来源限制均保留。

WIP5 manifest实际为 `a22bdc038ed689dee45cf391e068c4f249f75fc53fed9a88859ce81e86e4bc8d`（355源/253日志清单）。本轮只核所读source.rs `b9d3f61e0c309b9f82f8cb6002be1765ee4c09147c5f2ee479a5c1d40a7bc068`、reader.rs `a7e86c3666d36a3e6641427a0d2ab1e68807f4fec4b80bba6f627695aa321f11`、shared_view_read.rs `b992fc63d9b106f2266880171d049a839d8d182a25a30e901e3be12f9322086d` 与该固定manifest相符，不宣称整包代码/日志已验收。它是背景，不是本分支新设计。

## 已足够明确，可按现契约落实

1. **同原operation失败有FIFO终结。** candidate绑定原source/actualscope、target、原operation/typedRPC/ReadEpoch、route代际。来自该原读取的真实失败应以同单producer顺序中的私有failure marker传给worker，定明capture cut/route version与静态失败原因；非Received故障不能造新Received.sequence/receipt。worker结束这个candidate的占用，保其已收raw及来源/失败，不让candidate永久卡住下一bind。旧op/旧scope迟到marker只归原段，不能终结新candidate。
2. **有旧current时保旧视图。** 刷新在可靠完成前失败，不能仅为清candidate就发布失败candidate覆盖原current。原已保内容、旧不可变版本和H仍可在明确Stale/failure的合法读路径展示；不把它当最新/已同步。源真实释放/换代可按SV034失效，不能借普通刷新失败假装cache已释放以规避SV038。
3. **无旧current是另一分支。** 首次读取失败不能凭空造旧cache；实际已收的当前/部分内容按其来源及Incomplete/CurrentOnly限制可读，不能为统一“丢candidate”把唯一内容丢失。具体投影/段metadata由下述小补充固定。
4. **完成与传输分轴且有赢家。** 原真实Replayed匹配且前置raw barrier无gap已完成后，后来flush Err/Unknown、source失联不走“未完成失败撤candidate”回退。原完成段保Complete；Transport/Freshness及后续capture范围另记。真实Replayed但边界前raw有gap只能Incomplete，不能假完整。
5. **view candidate退役不等于native原物退役。** RemoteError可早于本地flush终态；metadata标Incomplete不授第二真实Replay资格。原Ticket/Reservation/operation稳定槽仍由原native supervisor负责到真实终态/必要收敛。A9后继必须满足原单在途规则，预算只在真正新op取一次；join仍零provider调用、不续旧预算。
6. **新读不能被失败candidate永久卡住。** 在原native操作可合法结束、后继需要开始时，新bind与旧failure标记应在同producer FIFO有明确顺序，worker可按序释放旧candidate并建新candidate；不能让native控制/ack或所有者长等viewworker来伪造原子性。消费者取消、分配失败及lost-wake仍沿原稳定槽/通知规则。

这些是失败退出/责任归属实现细节，不是新的重读政策；不得自动发用户输入、session cancel、假grant、假完整页或用总量quota清旧raw。

## 当前缺的不是“要不要展示”，而是如何合法分段可读

现有A2只声明`JoinState::UnassociatedSegments`这一状态；A2的RecordAddress字段、TargetInspection/AnchorMetadata等未给完整段边界/失败操作来源的读取表达。本轮在有效A2/A3/A5/B页及相关WIP源未见名为`SourceUnassociatedSegments`的已冻结类型或方法。**不能把一个enum标签或内部容器名字当已存在的可读分段接口。**

A3 source未分配区负责无可靠session/未管理target的数据隔离；它不是“已可信绑定target发生读取失败后，所有后续同SID live永远不展示”的许可。若source/scope/target绑定仍真实有效，标准session envelope能证明这些raw归该target，则应经当前权限路径可见，至少明确分段原文/限制；如果SID/来源仍不能可靠关联，才继续source-only，不给任意target泄露。

不能为了让这些live可见，复用旧generation的tool/message ID或旧AnchorId跨load更新，亦不能用文字/时间去重。它们可作为另一个明确来源段显示，但要给读取者真实可观察的段边界与provenance，保持旧历史Stale。内部retain却长期没有合法read枚举/地址路径不满足structured-history当前连接内容要求。

下一bind的“回填所有未分配同SID”还需读取代际边界：曾属于失败read的raw不得先丢assignment，再被新成功candidate吸入当作此次完整重放。旧段可保留并独立展示，不影响新op基于自己的真实完成proof声明的范围；同一source原文引用可共享，但来源段和Complete范围不能伪合并。缺跨load稳定身份时只按可靠操作/范围衔接，不用SID相同证明条目身份相同。

## 所需最小补充页与断言（不要求新用户决定）

请作者新冻结一个窄补充，**覆盖/引用A2+A3+A5/A9**，不回写旧八文；若用既有Status/ItemProjection承载段metadata，只补B2/B7的该数据schema，不改72字节wire或新建grant。至少精确：

- failure marker的原物来源、完整typed metadata、FIFO失败cut与旧Complete冲突的确定裁决；failed candidate/raw/原Ticket各归哪个稳定槽及如何清理。
- 仍有效绑定上的失败前/中/后live路由状态；段ID/来源范围/原op或CurrentObservation标记、Incomplete/failure与ReadEpoch/Complete的关系；**实际合法read入口/记录地址/只读getter或Status载荷**，而非只给内部enum。任何新平台段ID不是native身份/顺序/新来源授权。
- 后继bind精确回填资格，失败旧段永不假入新Complete；旧current保持Stale可读、旧cursor和H/tail在各自generation按原规则稳定，新段如何可见及何时明确StaleCursor。
- 真绑定失效时继续source-only与重新合法绑定的区别；当前权限/每chunk/seal/撤销/新stream credit责任原样，不加正文/段/引用总quota。

最小独立后验应覆盖：完成前RemoteError/真Zero/Unknown各真实路径及原native责任；失败后无新增也能开旧Stale内容；同SID新增live经合法读显示但不改旧anchor跨load；原op终态后新op实际只发一次/provider选一次并完成；旧失败段不混入新Complete；Replayed早到再flushErr保完成；旧scope/迟failure与并发新bind/取消/分配失败不串代，capture/通知仍前进。Module层不代WSS/ND/Harness实际证据，未提供入口则记未执行，不删除原SV038等期待。

本轮未收到此新冻结分支，因此只给上述现有语义解释与最小缺口，**不批准作者提议的具体路由/公开状态/新代码**。原83、同read续接gate与既有授权工作继续有效；后续按新固定补充和独立定义再做窄门槛，不扩大成整个Shared阶段重设计。
