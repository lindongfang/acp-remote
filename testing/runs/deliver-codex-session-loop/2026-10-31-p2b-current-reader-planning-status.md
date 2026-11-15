# P2B current-reader 独立规划状态

2026-10-31。**全部not-run，仅文档初审；未形成完整编码门槛。** 已全文读取revision7目录的候选569cc9ef2c1772813f8fb4c2c09a54d308a6e4c8b1f4f1bcb463c7a057c342c6、HANDOFF607d07ae及manifest93d85c60，独立读实际Domain/P1/P2A与固定SDK2.1.0相关形状/说明。24项输入中23项仍同hash，ACP mod因已授权P2A声明从b517变934d，非候选修改；详见 `p2b-current-reader-inputs.json`。原r4 API不作为执行依据。

主执行要求优先转root P2A+新Domain正式组合测试，本规划在此留明状态。没有编写/执行P2B代码、修改root、旧候选或独立冻结证据，没有网络/audit/用户数据库操作。

## 稳定编码前缺口（全部open-definition）

- **P2B-TG01：current输出与原观察关联。** EntityIterator/CurrentBlock须给固定有界的可观察关联：covered raw作为哪个实体的来源子项、uncovered/raw-only如何独立可见、错误/事实atom的确定映射。当前只有“来源子项”文字，不能据此猜WSS DTO或把未提交raw隐藏。需要最小纯API描述符及逐condition oracle，不要求先完成UI。
- **P2B-TG02：非工具逐槽和verified转移。** 明确terminal.output:null等值是KnownEmpty或KnownAbsent、raw录入R/D/provenance/loss、compaction初始chunk小status/局部默认、适用槽合法组合及各未确认v1语义。固定v1 CompactionUpdate总description已有summary/error/_meta省略保持、null清空、concrete replace的证据；具体_meta子描述又称patch，须明确采用的范围，不能笼统说没有null清空证据，更不能直接套v2逐key merge。工具P2A原表不改。
- **P2B-TG03：独立stored-raw scanner的确定拒绝政策。** Envelope/params/sessionId/update/tag/原生ID/已知字段和metadata、转义等价重复键、非法JSON/数值/代理对、深度/token边界须有确定错误/保守结果。**已自行读取并确认RawMessage::parse调用strict_json::parse_object；所有层decoded key以HashSet拒重复**（strict_json SHA2256cd608db0b60a3a9a83306781dca6258dcd4544955bd8651edb7bf91653d3）。早期“DOM last”推断已撤回，不是现协议漏洞：真实重复wire应Connection先拒，手工Domain存储字节是scanner独立L3负例。只有metadata重复政策不足以替代全部结构键规则。
- **P2B-TG04：Extension核对raw的确定证据。** 既有kind7合法anchor要求实际raw核对compaction tag/ID，但应查哪个seq/可接受证据、raw已淘汰时失败类型及恢复路径尚未固定。损坏/未知Extension不可覆盖或另位置fallback；不能猜有头即已通过raw验证。
- **P2B-TG05：可测构造与资源返回路径。** §2构造器crate内部与§4ProjectionRead::begin的实际纯验收入口需统一；需要公开受限owner门面或明确同模块私有测试策略，不能靠测试专属生产开口。所有普通open/next/consumer拒绝/取消必须返回原预留Reader并可再open；progress/MAX/sealed只能私有cfg(test)窄fixture如有必要，禁止公开任意setter。HRTB/≤512具体布局可在实现前确定最小Rust形状、实现后真实编译证明，示意签名不是已编译证据。
- **P2B-TG06：v1 compaction实际广告来源。** 真实strategy::initialize当前v1 clientCapabilities:{}、v2 capabilities:{}，Negotiated无实际client compaction广告事实；ProjectionRead仅凭Strategy不能证明已广告。候选要求v1 compaction已广告才准入，但本阶段广告/可核来源尚未提供。必须由合同明确所属包/能力来源；若依赖P3，本包不能以peer擅发通知签合法L4支持。**编码前阻断，无需用户重新裁决产品方向。**

上述六编号已与原reviewer同步，不并造其他问题号。当前未签任一缺口关闭，不能将有限Tool小值已验替代完整current/reader/scanner门槛。

## 后续最多12组执行定义范围（仍待上述合同精化）

| 组 | 计划可观测操作及主要预期 |
| --- | --- |
| A Reader资源与返还 | 接收内容前reserve16896；满4096/节点32768项无后续输出仍with_window读取；open失败/consumer拒绝/取消均归还同一预留，费用不断档，不snapshot/lease |
| B 双身份/双版本 | 容器/source/resource/read及私有producer身份核对；两块间Domain revision改变或Domain不变但producer progress推进均Stale；ReadChanged不交付候选，MAX封闭不回绕 |
| C 七kind及raw关系 | 全kind/namespace/4096ID与v1无ID临时片段；covered来源子项、uncovered原观察完整可见；Extension冲突不覆盖；旧P2A decoder仍仅kind4 |
| D current证据与缺失恢复 | raw已录anchor失败、Dropped尾、字段raw自淘汰、无法分类洞分别不冒旧Known最新；整值replacement/clear恢复局部base，append/单keypatch不洗掉缺前缀 |
| E 消息/思考 | 双版ID/无ID；v2replace→chunk→replace、clear、角色namespace隔离；每ContentItem/ChunkMetadata实际seq-origin；保留未知结构而非拼文本 |
| F Tool完整current | P2A有限表保持；content整替换/单个ToolCallContent append、rawInput/output整值、晚完成257/长raw原位；v1/v2 diff与terminal引用均按实际结构 |
| G Terminal | snapshot替换和每chunk独立base64追加，UTF8/ANSI可断字节但不执行；null/省略/exit{}、int64与size边界；chunk/snapshot_meta非实体patch |
| H Plan/Compaction | v2嵌套planId/items/future type及v1无ID快照；两版compaction status/summary/error/meta、chunk与真实广告前提；不推断Run/可信历史起点 |
| I Metadata | omit/null/{}、删除、嵌套整值替换、无base PartialMap、长/转义/重复key、chunk与entity隔离；有界重扫而非免费Map，逐atom来源可核 |
| J Scanner/嵌套 | 100×64实际frame、完整JSON grammar/已知必需字段/未知Opaque、Media/Resource/Plan/ToolDiff/Locations/Annotations结构；真实入口拒绝与存储字节负例分开 |
| K 全预算和分块 | ≤512 Reader/Cursor按值、不双份常驻；producer额外512含Arc峰值；16384 scratch固定分区；8192/8193输出，134217728跨块累计含重扫/比较/复制/空状态步骤，失败作废候选而不放宽预算 |
| L 生命周期/控制隔离/回归 | 输入DOM/Connection Drop后只借retained raw；读取不cancel/close/执行内容，故障继续有界pump；封装/HRTB负编译与正控制、固定源/锁、适用旧资产回归；实际Node公平/授权/WSS属后续不能代签 |

每组后续需补具体输入向量、条件/返回值oracle及未执行界限再冻结；本表只是本轮暂停点的覆盖摘要，不能作为已检视通过或已执行证据。完整CL、P2B/P3、owner交付、真实Harness及完整L5/L6仍未验。
