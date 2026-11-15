# 公共ACP runtime candidate-v1正式同源静态审查

2026-10-31。**v1不通过交付：新增PROMPT-001、SEEN-001、DEBUG-001三个具体合同遗漏待修复。** 四个过程问题的修复已在固定源确认静态闭合，但仍待独立同源动态证据，不提前关闭。root尚未合入，本报告不签根组合或整体WP3。

## 源/范围核验

- source-manifest SHA256 `d2c29065de9b8b45c75d167c25a687825cc419bd069b20caa6a573a710b6a416`；build-inputs `3fe26f29438d580a5867ac9580ceb00c2eb6f9b00573d2955279e446a68eb8f5`均匹配。190文件逐hash零差异，三份实际Web dist亦全部匹配。
- 对原182基线b4325cd0逐条比较，恰22路径变化：14改+8增，全在ACP目录。raw_json三源、Cargo.toml/lock、SQL/Node/管理授权/Web和原独立资产均不变。锁仍87f27b61，没有新增依赖。
- 绑定主64cc/限制8bb5/empty补充7f682592及独立f27b/077a。empty已按Result<Self,SetupError>落实；未改冻结原文。开发自测78及lib Clippy不是独立结果，本报告不据其签功能通过。

## 新具体问题

### ACP-IMPL-PROMPT-001（Open）

固定strategy.rs `ec65f0a9d67d01019fb4d9bbcbac6e051085b455ef5a7f6f3e191d5d7f2d1e1d`，prompt的resource分支仅核resource为object。合法embeddedContext广告下，resource:{}、缺uri、uri非string、无text/blob或二者均非string均可构造Request。

固定SDK两版EmbeddedResourceResource分别位于schema v1:888、v2:1430，均为anyOf(TextResourceContents,BlobResourceContents)；前者要求uri/text string，后者uri/blob string，没有该内层Other。64cc第1节要求known必需字段验证，不能以外层v2 Other绕过。

最小修复：浅查uri string及至少text或blob一个string分支，保原完整raw。anyOf不是互斥；一个分支合法时另一字段错型可作为未知额外成员保留，不强制恰一、不删字段、不加URI抓取/授权或长度规则。作者已确认在新v2修，不覆盖v1。

已亲读tester资源资产f4b78314a0deb26cb69c92e1586a01c36429fa8a9e60904a07121a8bb3020359及首红日志439789729ac1e3e82ac51f877bef2b1386a9f812438de6097ebe66eb6908b829：双版共10非法形状被接受，8个合法anyOf/unknown词法正例通过，最后断言0/1失败。但tester查明此首轮probe cwd实际用1.99.0，而固定toolchain为1.98.1；这些原记录仅补充诊断，不作为指定工具链正式验收。1.98.1重新执行另记，不回填旧日志。

### ACP-IMPL-SEEN-001（Open）

固定reader.rs `4f4be66ecdaf75ae74db0ebb1a1b7c1764252f3a8891aeae30202192010719ca`第170–177行，seen.contains始终核，但seen.insert只在!stopped时执行。ClosingDrain中Stop前已取得的两个相同全新server ID，均可被emit成Permission/Unsupported数据；虽不能回复，连接寿命防重规则仍被跳过。64cc的Closed禁新Waiting/grant没有豁免seen。

最小修复：关闭排空仍记录seen元数据，仅live建server pending/检查该grant容量；重复仍保rejected/首错误且继续后面完整帧排空，不新增授权。作者已确认此遗漏。可确定公开回归：同一真实read批首unmatched response先Stop，后两个新server同ID只应一份Received，尾通知仍drain；同receipt_order证明取得。终止首因仍UnmatchedResponse，单rejected槽不应被要求覆盖首原文；不造fakecut或生产yield。

### ACP-IMPL-DEBUG-001（Open）

固定transport.rs `c47bad6a7cb2132969c916a480aa0c8f4a02605cd71ee6b0d47b431f6e1a63a6`的Response派生Debug，RemoteError{code:i64}输出任意对端code原数值，Incoming派生Debug递归输出。64cc对外来数据Debug只允许类型/长度/静态类别，公开投影可读不代表允许Debug原值。作者已确认。

最小修复是Response自定义脱敏Debug，公开variants/code读取及原raw保持；追加真实RemoteError数字canary，同时核Response/Incoming格式和公开值/原文。字符串canary不能覆盖该数字分支。其余已读Request、RawMessage、配置/MCP/env/header、Session/Tool/Message/Option/Rpc ID、SessionEvent/PromptResult、OrderedRawObject/RawMetadata、ToolNamePatch及IO句柄均有相应脱敏边界；不因这一处声称其余未测分支已动态通过。

## 四个过程ID的固定源复查

| ID | 固定源确认 | 当前状态 |
| --- | --- | --- |
| ACP-IMPL-WRITER-001 | writer56011ac0在实际W.poll同State锁内将Err记spent/Unknown+Stop、Flushed记Written；不能再被锁外Stop覆盖为假零/Unknown。每poll代理最多一次实际调用，后置非法fence封连接，已知传输事实与协议错误分开。 | 静态闭合，待同源独立动态 |
| ACP-IMPL-READER-001 | statec877d5e0共用Core Arc<Notify>，stop/finish均wake，expire直接停止退出亦能唤醒writer/collector，不靠下一次外部shutdown。 | 同上 |
| ACP-IMPL-READER-002 | reader4f4be66e在实际R Err/EOF原State锁内先Stop，再外层通知/结束，与W.poll共享封口顺序。 | 同上 |
| ACP-IMPL-LIFECYCLE-001 | lifecycle70a77f37的collector正常及shutdown fallback均drain_io逐槽借用await真实join，collectorErr/None也不跳过；取消保句柄、同deadline覆盖，之后才terminal。 | 同上 |

启动屏障在两IO/collector slot登记后打开；普通shutdown串行、borrowed await不take后等，无ambient spawn的Drop路径正确。State/Cell锁序未见新增倒置。Stop保held LF，单reader顺序排空，不等consumer；首terminal经ended只交一次。上述均为静态观察，需真实并发/取消/跨runtime析构门闩、receipt与期限矩阵验证。

## 其余关键边界和交付限制

已读完整配置/策略和raw编码：Unknown不补空、Known(empty)明确、版本/能力按固定schema分路径验证，ordered env/header及完整MCP raw保留；decoded保留字段冲突、raw compact单LF、大数/深未知片段不回Value DOM。原32MiB/96/token/列表条数、Message/Tool长度与空array拒绝均在新链移除；既有Session/Option/Rpc及原list/replay/listing保护保原位置，未扩new/resume/setup。

P1公开先核真实Received.scope，私有observe_inner复用生产数据算法；P2公开走该入口，cfgtest apply_inner只薄组合私有生产函数，不是fake品牌。P2 decode_anchor及ViewWindows/staging旧4096和预算仍存在、明确非公共IO必经；不签完整无预算视图或全部长键旧投影。

独立36函数迁移、6负例先正对照及实际child/IO/期限/关闭证据尚在执行/整理；当前没有将未编译的旧独立API文件作为产品bug。最终必须以修复后新冻结源与指定1.98.1工具链、独立候选/原日志同源再签；三个新问题和四过程ID不因开发绿自行关闭。原v1、首轮非指定工具链日志、旧资产及历史报告永久保留。
