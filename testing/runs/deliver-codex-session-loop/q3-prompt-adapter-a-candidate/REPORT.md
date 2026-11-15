# WP4-A 正文数据适配候选

已实现获准三条生产路径及一开发叶，等待原tester同版PA01–08后验及reviewer检视。未合Root，不签Node/WSS/实际上传来源、审批或完整WP4。

基线为设计ccf6e531绑定的393f19c8固定448文件，含既有dist，逐hash核同复制到只读baseline。最终450文件，只新增prompt_input.rs和其tests.rs；原mod.rs增私有声明及PromptDocument数据导出，Strategy仅在原逐block位置提取原语。无RegistryError/RawDocument/model/queue/native捕获/Node/Storage/DDL/Cargo/依赖或锁改动。

PromptDocument唯一parse(Vec)保留传入完整字节；两个只读借用分别表示全上传与根片段。它不能证明调用者上游从未自行裁剪/生成Vec，也不授身份、能力、受理或派发资格。Arc<AcpNativeAdapter>的真实ValidatedBodyPort计算完整字节SHA（含外围空白），Body clone同RawDocument；原adapter本体RetainPrompt只root_owned，供原NativeProtocolPort消费同Inner根引用，没有全文复制/hash/parse进入原捕获短锁。

Strategy保sessions→extra→array→逐block tag→cap/version→fields→下一block及原request编码顺序。未知V1拒绝/V2保留、媒体先cap后字段与早block先报错均保持；新parse只做无协商shape校验。错误沿原RawJsonError→AcpError，不新增RegistryError，也不将parse Allocation推入StorageUnavailable兜底。公共类型和trait实现有真实数据/协议职责，无新增allow或测试fake-use。

## 实际检查与运行

所有Cargo命令offline locked，原CargoHome，独占target `D:/Project/acp-remote/.tmp/q3-wp4-prompt-author-target`、TMP `D:/Project/acp-remote/.tmp/q3-wp4-prompt-author-tmp`。runs保存每次真实命令、cwd、完整前后源SHA和exit；外部执行均有180秒整例watchdog、准确exeSHA/PID/输出/剩余目录记录，不以0 tests通过。

|命令/范围|实际结果|
|---|---|
|fmt --all -- --check|0|
|check --all-targets 默认 / --all-features|各0|
|clippy --all-targets [--all-features] -- -D warnings|两个配置均0|
|默认lib单测、原RawJSON/capabilities wire二进制编译|各0|
|新prompt_input开发4函数|4/4|
|原adapters::acp::tests::|37/37|
|原adapters::acp::runtime_tests::|13/13|
|原application::queue_owner::tests::|23/23（实际list为23，不沿旧21计数）|
|原raw_json_acceptance公开资产|33/33|
|原acp_runtime_capabilities_config_wire公开资产|5/5|

新4函数为：full_upload_binding_and_root_retention_share_storage；complete_structure_rejects_late_damage_and_preserves_extensions；negotiated_prompt_preserves_early_errors_and_exact_wire；missing_sessions_precedes_reserved_extra_and_bad_root。覆盖完整字节/根地址与Drop后保留、服务器digest跨度、未知数值/转义及晚畸形、真实offer2选1/选2握手与早错误、实际wire根片段、原IO owner shutdown及peer EOF。

编码精度：原encoder的compact规则可移除字符串外四种JSON空白，原始storage另保；本A不改encoder，不把wire称为全部上传字节的原样副本。开发wire向量的根原本没有内部空白，故逐byte相等只证该向量；更一般wire oracle须沿旧compact允许变换，完整上传摘要和retained根原件仍不compact。

六个运行组均同最终450源，正常退出/原测试尾通过、无watchdog/新TMP残余，runner没有清目录或做晚重试。原R3 23项是原回归；**新adapter贯穿真实R3去重及两capture的PA06/07仍等独立tester**，不以旧合成validator代签。本轮新数据wire是标准可控双工，不是Windows进程/真实Harness。

## 首红与实际装配证据

首次default check0后，首strict红：新提取导致text/resource_link两个match arm相同，以及新开发byte literal多余raw hashes。首完整源、日志及两文件前像保留在runs/和development-history/first-lint/。机械合并相同pattern（同tag仍传原字段原语）及等价literal后，最终默认/allfeatures strict真实0；没有新增allow/公开接口或改变错误顺序。

原RawJSON33两次require_escalated调用在CreateProcess前自动审核超时，**进程从未启动**，分别保prelaunch记录；按工具允许一次重试后不再重复escalation。随后同命令在普通sandbox/原Windows owner环境实际33/33。不是测试红转绿、不覆盖先前执行结果，也没有改变目录权限、降断言、sleep或清残余。

portable/candidate.patch从四路径精确raw前后像生成，独占pristine before真实git apply --check0/apply0，所有after逐SHA一致。未向Root应用。普通lint合并模式是相对门槛示例的唯一语义等价结构简化，详实际diff。

## 未验证及交接

- 独立PA01–08全部由原tester另行执行；外部负编译、新完整R3/capture以及33MiB实际新PromptDocument wire未由本报告代签。纯Allocation转换不证明真实OOM恢复。
- 未运行npm check/build、全Rust运行集、真实WSS/浏览器/Harness；不称完整工程绿。393448原Shared/Native runtime红、关闭/审批/审计未签分支不因本A回归消失。
- 仅传入Vec保真；尚无上传owner、认证/撤销、正文二阶段网络交接或Node唯一工厂。B/C/D仍另审；Root新增Node cfgtest1dfb不混，审批新read WIP未用。
- 无网络/Web audit、新依赖、用户真实DB/config/密钥/目录、Git提交/推送。实际Root/旧A2/旧设计/其它候选未写。

candidate/source为完整可验450源，source-manifest固定摘要；candidate.patch仅4路径，changes/roundtrip及原始runs/exe一并冻结。候选冻结后保持不可变，交原baseline_tester与reviewer；Root后续合入必须按实际前像局部check/apply，不整份覆盖其它候选。
