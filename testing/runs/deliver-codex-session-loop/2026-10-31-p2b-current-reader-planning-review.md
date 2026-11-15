# P2B current reader：候选只读预审

2026-10-31。结论：方向可继续精化，当前整体不放行编码。独立≤12组定义尚未交付，六项工程定义缺口需闭环；无需用户重新裁决既定产品规则。优先完成root Domain+P2A实际组合验证，本候选不代替该组合的来源快照。

## 固定输入

外部deliver-codex-session-loop-view-adapter-revision7：
- 合同569cc9ef2c1772813f8fb4c2c09a54d308a6e4c8b1f4f1bcb463c7a057c342c6
- HANDOFF607d07aec57a5aa717868bb7d7076cf0b483286c561b0fb121ab92fdd84f7919
- source-manifest93d85c6088d0de6da81c9f663313fc15f92177c9512fafa12e8c9df5d7fe9fab

全文读取合同，实际重算24输入。当前23匹配，唯一root acp/mod.rs因已授权窄合P2A声明由旧b517改934d，属明确后续来源变化，不能称旧24清单是当前root全快照。已读实际01e8/ad3 Domain API、P2A4cf2实现、P1与model/strategy的来源/JSON边界，核固定SDK2.1.0 v1 CompactionUpdate正文。没有编码/编译/测试本候选；没有网络、真实Harness、用户数据或生产写入。

## 可保留方向

按值Reader/Cursor所有权返回、先预留16896后接收、StageWorkspace承接旧cell费用、完整scope/原handle和私有producer身份双绑定、Domain revision与producer checked progress分开，是可实现的方向。HRTB同步借用后postcheck再accept可沿原Domain端口构建，不要求自引用cursor、免费snapshot或新touch setter。

P2A旧format1/decoder不放宽、新kind分别严格解码、工具仅调用既有P2A一次、非工具至多一次额外anchor尝试，能够保留原位与raw一次录入边界。Extension遇非Compaction既有内容不覆盖的原则正确；是否有足够原始依据须按TG04进一步精确。统一付费scanner、逐块来源、非永久DrainOnly与有界失败继续pump符合现阶段方向。实际layout、HRTB/API编译、所有权和scanner oracle仍未证明，不据示意代码作通过结论。

## 编码前缺口（与原tester统一编号）

以下均为P2工程门槛，目标版本为本569候选；不是现有已通过实现的新代码漏洞。

| ID | 合同位置与实际影响 | 最小闭环依据/必要用例 |
| --- | --- | --- |
| P2B-TG01 | §2/§6/§9中来源观察子项、covered/uncovered与CurrentBlock/field fact/error的可观察关系未冻结。仅kind/position等示意getter不足以让独立oracle判断一条raw是否可见、何时属来源而非新聊天，以及缺失用事实atom还是Err。 | 明确有界DTO/借用访问入口、关联坐标与分类规则；raw成功但anchor失败、后续仅部分字段恢复、unknown/无ID独立片段、旧raw被覆盖仍作为来源、未覆盖观察不可隐藏；atom/错误与知识状态逐结果固定。输出不需提前接WSS。 |
| P2B-TG02 | §4/§7只概括clear集合/标量，非工具逐槽Knowledge、小码、四项seq/origin/loss和Verified结果尚不全。例如terminal.output:null究竟KnownEmpty还是KnownAbsent需唯一答案。v1 compaction的未证泛化也需按实际schema细分。 | 明确全部新kind逐字段omitted/null/concrete/append表及R/D两列，含不适用槽/flags/Unknown code；分清登记事实与verified current、错误/恢复。v1 CompactionUpdate总description已明确summary/error/_meta省略保持、null清空、具体值替换；子字段“metadata patch”不能抹去此明确整字段证据，亦不能未经证明套v2逐key合并。 |
| P2B-TG03 | §7只明确metadata重复解码键，§8通用scanner对Envelope params/sessionId/update、tag/native ID、已知字段等重复键（含转义等价）未固定。不能默认任取第一份或跳过歧义。 | 通用存储scanner重复键的拒绝/Uninterpreted边界及原观察保留规则、工作预算必须固定；真实Connection重复wire先拒，与手工Domain坏存储字节分层用例。 |
| P2B-TG04 | §4“Extension既有kind7实际按raw核对compaction tag/ID”没有定义核first/last/哪个field seq、原raw已淘汰时读/替换怎样失败。可能导致实现猜MissingRaw/InvalidAnchor或不当新位置fallback。 | 精确选取依据/身份校验与缺raw结果；旧anchor仍在但原raw已FIFO淘汰、新update raw存在/也Dropped、未知Extension占同ID、错误wire/损坏头均测试；禁止覆盖未知或因缺raw另建位置。 |
| P2B-TG05 | §2构造器crate内部与示意公开begin/API的独立可测入口不一致；所有普通错误/取消归还reader与producer极值的可审入口尚需冻结。 | 选定正常公开依赖可测边界或明确隔离cfg子范围；closed private progress极值入口，不导出任意setter。reserve/open/next的身份错误、ConsumerRejected、ScanBudget、ReadChanged、ProducerExhausted、取消归还同reader账本均可观察，生产owner义务另留。 |
| P2B-TG06 | §7要求v1 compaction按实际广告准入，但当前strategy.rs:547初始化V1 clientCapabilities:{}、V2 capabilities:{}，Negotiated不携本地client compaction事实。仅begin(strategy)无法证明已广告，peer擅发不是合法准入happy path。 | 明确能力广告及保存其实际来源的最小依赖包/输入；未广告真实Connection负例、确实广告后双compaction tag正例必须可执行。不能用任意bool/对端广告替代本地已发送事实；若先拆包，整体compaction/完整P2B仍待其实际前置。 |

TG编号与独立tester协同，tester随后定义的TG01–06文字为同一批缺口的用例映射，不另造并行问题号。定义层关闭需复读新冻结合同和独立oracle；未执行的编译/layout/边界用例仍按not-run处理。

## TG03的事实更正

预审沟通中我曾按普通serde行为推断现入口DOM可能last-wins；继续读取实际源码后已立即向主执行和tester撤回。RawMessage::parse实际调用strict_json::parse_object；StrictValueVisitor对所有层解码后的String键用HashSet检查，重复就拒绝。因此这里不是现Connection重复键解析漏洞。TG03保留的是“scanner不能假设任意保留字节天然合法”的独立负面合同，与真实入口先拒的分层测试；不得把前述错误推断写成产品反证。

## 范围与后续门槛

本候选仍覆盖大量完整current与nested schema语义，不能以P2A小值通过、Domain借用通过或本方向预审替代其算法/协议验收。full4096/32768首开读取、跨块累计134217728预算、8192输出与100×64实际布局/容量、重复键/Unicode/base64/scanner状态、覆盖洞恢复/失基础、返回R/accept所有权与原始来源都须可执行oracle。真实单一producer/预留slot、公平与背压/权限撤销的P3依赖继续保留；纯层通过也不签页面或有权交付完成。VPROJ-PLAN-002完整义务和任务3.2仍开放。

