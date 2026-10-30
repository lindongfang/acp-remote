# WP2 编码前能力与阶段门禁独立复核

日期：2026-10-30。**结论：READY（仅WP2领域/存储实现及其开发测试）；WP1整体和产品验收仍未完成，不据此开始WP3/WP4或勾task1.2/1.3。** 本reviewer只写本报告，未修改产品或规划、未重跑Harness、未读取/复制凭据。独立上下文延续原规划问题复核，原PR-001–003设计Closed继续有效。

## 实际独立核对

实际读取新result-v1、result-v2、result-approved-v2及对应retry.mjs/approved.mjs；按白名单解析协议结果、事件类型、marker和权限决定，不输出敏感日志。核读独立tester的network-retry-acceptance，不以主执行的成功自评替代原始证据。初始git状态仅既有规划修改及探测/报告/合同未跟踪项；tracked产品范围app、clients/web及根产品清单/锁差异无输出。strict和git diff --check实际exit0，未重跑无关产品suite。

- 两版普通探测各9步骤，无failure字段；initialize分别1/2，创建、marker、close、换进程resume同ID、recall、cancel、permission、finalclose均有实际结果。全部session/update维持各自创建的同一原生ID。
- 原始agent_message_chunk含ACK_63F9与WP1_NATIVE_CONTEXT_63F9；探针recall问题没有夹带答案，且确在旧适配器退出、新PID连接后恢复。v1两个连接PID23572/42268，v2为33284/33440。独立tester进一步对messageId拼接作精确字符串比较，支持本次真实上下文恢复，不是向新会话灌平台历史。
- v1 marker/recall prompt终态end_turn，主动cancel终态cancelled；v2 prompt响应仅messageId，后续idle+end_turn或cancelled才是本轮终态。resume时v2已有idle快照没有stopReason，不当新执行完成。两版权限请求均真实广告allow_once等选项，cancelled回复后本轮终态cancelled；不是仅发cancel通知就记完成。
- approved-v2证据包含一次selected/allow_once、权限轮idle+end_turn及实际tool_call_update。approved脚本限制本次专用cwd与唯一固定PowerShell Set-Content命令，拒绝危险分隔符；本reviewer独立实际读取专用workspace/approval-probe.txt，精确等于WP1_APPROVAL_PROBE，与approvedFileMatches=true一致。因此v2不再只有取消权限请求，也已有真实批准后单文件写入能力前置证据。此文件是合成测试marker，不是用户数据。
- 每份结果两适配器父进程exit0、stdio exited。没有把父退出推导为完整后代进程清理；产品Job Object仍待WP3/CL-030真实验收。主执行称CIM自有路径无残留，前轮本reviewer权限受限，不代签该OS枚举。新一轮登录副本清理仍由主执行拥有并记录，不授权清理用户原始登录/会话。

这些真实证据足以解除此前“连基本执行/原生恢复能否达到都未知”的WP2实现风险。此前无登录/直连超时报告保留历史意义，不应继续作为当前两版能力的全面BLOCKED事实。进程代理启用和HTTP405可达仅主执行环境说明，本reviewer没有独立重现其因果，也不把单个HTTP状态当API执行成功；本次执行与恢复的依据是实际协议和文件证据。

## GATE-001：分开能力前置与最终任务判据（阶段依赖澄清，非降低要求）

当前task1.2完成判据包含CL-005真实报告，而CL-005要求已认证Web中的产品能力门控；task1.3包含CL-001/002最终集成目标检查。二者依赖WP2–WP4产品实现。如果把plan的“WP2依赖WP1”机械解释为1.2/1.3的最终产品判据必须先完成，会产生先有产品才允许写产品的阶段循环。

应在plan明确两个独立状态，保留原checkbox和全部CL断言：

1. **WP1编码前可行性门禁**：最终上游/当前schema/身份撤销重基准、方案用例独立通过、固定真实程序/版本与逐版本基本链路证据，以及可运行工程基线。这部分当前足以让WP2在已审合同内开始实现。
2. **WP1最终交付任务**：task1.2的产品CL-005门控、task1.3的新增依赖审查/锁与本change最终编译/协议/前端/供应链检查仍待完成，继续不勾。不得把当前probe冒充这些产品报告，或改完成判据使其提前完成。

此澄清是已授权实施的依赖及验证顺序细化，不修改产品行为、安全或数据权威，无须用户重新批准实现细节。主执行应先在plan/任务状态说明中写明这两个阶段及对应证据，不能口头声称WP1全部完成。1.1保持完成，1.2/1.3保持待办；总计数不因本报告改为3/14。

## 当前可开始范围

仅WP2已批准写范围及串行owner：domain/application、storage.rs与新增session_storage、开发单元测试、lib.rs模块声明。实现schema1→2保留身份/信任/撤销/审计、Command/输入/Turn/Run/引用/审计原子受理、前台互斥与幂等、审批领域唯一裁决和交付状态分离、条目源顺序/版本/有界保存与故障停止；同时交付合同要求的真实SQLite、故障、并发开发测试。依照session-contract，不因探测成功重选协议语义或改保存参数。开发测试的fixture只用于其声明层级，不能冒充L5/L6。

只在专用测试库/schema1副本验证迁移；本报告不授权对用户真实节点库立即运行迁移，也不接入用户真实Harness会话。现有安全模块接线与Web仍按计划owner及合入顺序，不静默扩写。

## 仍缺项与后续门禁

- v1批准后真实工具成功执行在本节固定时尚未读取，不能把v2一次allow_once升级成双版批准路径均通过；v1待补证据。两版取消请求都不证明多客户端唯一裁决、失效审批拒绝或未知交付不重发已经由产品实现。
- WP3双版客户端/安全审批接线开始前仍须对应逐版本合同与受影响真实能力证据及新增运行依赖安全审查；本报告不授权私有桥接、协议降级或按厂商分支。需要新库时先审必要性、许可证、维护、安全和Windows支持，锁定版本。
- Web身份、逐块撤销writer、SQLite会话保存故障、原子live基线/租约、大内容原预算、共享实例隔离、Windows完整进程树、CL-031安全/DOM及CL-030真实产品闭环全部仍未验收。前置probe成功不替代最终独立tester/reviewer同版本检查。
- 不宣称持续网络可用、所有v2草案组合、所有恢复/重放时序或ZCode/DeepSeek兼容通过。原官方规范固定日期/摘要及升级复验要求保留。

## 固定目标SHA256

下表固定本次决定依据。原始result含任务合成对话，留在拥有的临时根；后续归档使用脱敏持久摘要及原始摘要映射，不把凭据或私有正文纳入Git。任何证据替换、协议程序或合同变化后重新判断影响。

| 文件 | SHA256 |
| --- | --- |
| `.tmp/cl-wp1-proxy-retry/result-v1.json` | `12857A90B89048C41F33EC5AE16BC8CFCE521C9DFF264EE7D044BE1C6BB681E0` |
| `.tmp/cl-wp1-proxy-retry/result-v2.json` | `50B67B9F01B944C07840F1C0BB12E6751429DFB1F13CA78FC3E851E3DE7A7CF9` |
| `.tmp/cl-wp1-proxy-retry/result-approved-v2.json` | `83E5E66384CDC9210B5B0CCBD647154DAC12F369EC409B0101C2901A7F74596A` |
| `.tmp/cl-wp1-proxy-retry/retry.mjs` | `9D2524AEC8D0C57CDB08B2A15F85061CFFFEF13521D0CFF1A7D258F061CF18E3` |
| `.tmp/cl-wp1-proxy-retry/approved.mjs` | `18242CDF234DCAD85D0332C5E94C849C7D963E3135211161022AF91721F1635F` |
| `openspec/changes/deliver-codex-session-loop/proposal.md` | `6D887C8C93B54BEE128CF86F2A9DC91F64102B2EFCE84E6D65B530F66A4BA720` |
| `openspec/changes/deliver-codex-session-loop/design.md` | `86FE6612EBBAEC3B490DFF50E24ECF7CABB004B61A4080357DF3D166048184C4` |
| `openspec/changes/deliver-codex-session-loop/plan.md` | `03CA368F075533F0275762903C073A8070620667CC820D344B4559B78AC45925` |
| `openspec/changes/deliver-codex-session-loop/tasks.md` | `730BEF5291870EA05A76E4E38F53FC1819B94B0F75A025148F73747D6EFF26BA` |
| `openspec/changes/deliver-codex-session-loop/session-contract.md` | `BB8F6661EB66C43EE4CFAD7B31FA30C4B07AD5FACDADD3A96D200F7FB264A7CA` |
| `openspec/changes/deliver-codex-session-loop/tests.md` | `6E6DAD8DE4B68ADE19021BF5518A4115173A1E1329C0F799FC7B26956E0A3AF6` |
| `testing/runs/deliver-codex-session-loop/2026-10-30-network-retry-acceptance.md` | `50B3DDB54020F3CA07C2C4734965BFC0F72A8BEBEF3026D66259E3E3732E6B57` |
| `testing/runs/deliver-codex-session-loop/2026-10-30-planning-review.md` | `63C8F0EDF30D26E5AE3A5FFA66EC46A9B0D4E8C4F84ACEC77E7A4501025403BA` |

## 同日补充：v1 allow_once、阶段澄清与持久证据有限复核

**WP2 READY结论保持；本节解除上文“v1批准后执行尚未读取”的有限缺项，不扩为WP3/WP4或最终产品验收批准。** 本reviewer仅追加本报告，未运行真实Harness或全工程，未改产品/规划。

实际读取result-approved-v1，独立重算SHA256为2CF52A5C0AF81775321A8CB3AB5A5B4EE5D975B07D49DD1A06306412D2987B55；v2仍83E5E66384CDC9210B5B0CCBD647154DAC12F369EC409B0101C2901A7F74596A。v1有9步骤、一次selected/allow_once、对应tool_call_update completed、permission prompt终态end_turn、recordedFileMatch=true，无failure。与独立tester新增逐项核对相容。当前合成marker文件原字节及v2判断保持前节所述；两版共用workspace，单次读取当前文件不能分别归因给两次运行，不声称每版独立新建落盘已证明。请求、指定命令、同ID工具完成轨迹和运行后匹配记录支持双版真实allow_once工具执行的有限能力前置。

实际读取network-retry-acceptance新节：tester在其有权限环境按专用ExecutablePath查询0残留；范围不涵盖系统路径PowerShell后代，不能签Job Object/全进程树。此处准确引用独立tester负责范围，不改写我此前CIM受限经历。本reviewer独立Test-Path核对4份任务auth副本均不存在，不读凭据，不删除用户登录或原生数据。

实际解析持久proxy-retry-summary，4条记录对应v1/v2 cancelled及approved原始结果，逐条rawSha256重算4/4一致。其SHA256为53621DDD880CDA73E55DA9A4302B268A2AE625E2F9E3AB57ABD23A5EC18A87DE。持久脱敏记录可以在.tmp原始文件清理后保留证据映射及阶段事实；它不会补出原记录没有的系统进程树、唯一审批竞态或产品数据库验收。后续不能因原始报告路径消失而悄改历史通过范围。

plan顶部明确当前阶段取代下方超时历史，GATE-001的能力前置与task1.2/1.3最终产品判据已分开，WP2工作区/基线、主目录只读规划交接、写范围及独立CARGO_TARGET_DIR明确。当前1.1已勾，1.2/1.3仍待办，没有提前签产品CL-005或最终CL-001/002。此依赖澄清满足上文实施前记录要求，WP2可在指定隔离范围继续；产品集成仍须实际diff和受影响验证。未读取工作区实现，不能将本报告当该实现交付检视。

持久authenticated.mjs与README明示路径参数化和更严格白名单后仅语法核对，真实结论归原型及原始摘要。本reviewer实际 `node --check` exit0，未将其计为新脚本完整链路重跑。`git diff --name-only -- app clients/web package.json package-lock.json`无输出，`git diff --check` exit0；不据此声称隔离工作区没有新产品代码。

| 文件 | SHA256 |
| --- | --- |
| `openspec/changes/deliver-codex-session-loop/plan.md` | `89FB0E60BFCC77B7A82140C9BC5AD73668CACAF99BCD8990C5E1CA68DB16698B` |
| `openspec/changes/deliver-codex-session-loop/tasks.md` | `C0DA3A4F0C6457B3FE4B948263CC25592ADB6E30558CE721135AA1343A66C3E3` |
| `.tmp/cl-wp1-proxy-retry/result-approved-v1.json` | `2CF52A5C0AF81775321A8CB3AB5A5B4EE5D975B07D49DD1A06306412D2987B55` |
| `.tmp/cl-wp1-proxy-retry/result-approved-v2.json` | `83E5E66384CDC9210B5B0CCBD647154DAC12F369EC409B0101C2901A7F74596A` |
| `testing/runs/deliver-codex-session-loop/2026-10-30-network-retry-acceptance.md` | `3CBDD377C89E9143AA23D5AB976326627081408A06FB2D18EEAC353F34F1F14B` |
| `testing/runs/deliver-codex-session-loop/2026-10-30-proxy-retry-summary.json` | `53621DDD880CDA73E55DA9A4302B268A2AE625E2F9E3AB57ABD23A5EC18A87DE` |
| `testing/probes/codex-session-loop/authenticated.mjs` | `1C7AEBAEC3C29A6029F36123FD880E038AFC0E3322F9E1F66E90B59645AD23EC` |
| `testing/probes/codex-session-loop/README.md` | `185A5DC2F57A8748E4D8AD083101FB4861B50D5FAB0DF748479C05228241F6A5` |

## 同日WP2 task2.1主目录集成及元数据有限复核

**PASS（仅七文件保真集成、已有检查证据与元数据门禁表述）；不扩大到正在隔离实现的2.2，不签task2.1最终产品完成或Web/L5/CL-030。** 本轮只读取D主目录和日志，未读取C工作区活动2.2、未改代码，未重新执行全工程检查。

实际逐文件重算D盘app/src/lib.rs、storage.rs、domain/mod.rs、application/mod.rs、session_storage/{mod,schema,tests}.rs，与原wp2-input-review冻结摘要 **7/7一致**。因此本轮集成未改变原有限静态审查的代码字节；原ActorContext不是外部授权、owner启动前显式recover、模拟派发不等真实副作用等限制保持。首次自有匹配脚本的shell反引号字符串造成false，未采为字节不同；改为按每个文件对应行比较完整SHA256，结果7/7 true。

实际读取.tmp/wp2-integrated/check.log、build.log与两个.exit文件（均0），检查日志包含npm check:web/check:rust真实命令链、Web50、Rust156库及4JWS通过，fmt/全目标Clippy/类型/构建链没有改为跳过目标；build日志实际cargo build D主目录app成功。check.log SHA256 AEBAFEB29F242A17418AD9EF870AD2DAE80EEF0A7338330C9BF1604985A896AC、build.log 88944CD6837EDC9915799FFF85172523CA6441D3E8B64DBA70E522F538940149，均与development记录相符。这是对主执行已有实际集成日志的独立核读，不冒称本reviewer重新运行命令，也不将开发检查升级为独立功能验收。

README明确输入事务/升级基础已进入实现、Harness/Web闭环尚未交付；升级链接指向已审session-contract §4，要求停止写入、备份数据库/受保护身份，旧程序不能直接打开升级库。plan汇总限定七文件与当前阶段，2.1仍不勾，2.2/2.3继续隔离；主目录不并行写活动产品包。原门禁没有因“集成检查通过”被放宽。

实际读取openspec/config.yaml相对HEAD diff，变化只在context项目现状/命令/证据解释，纠正旧“没有Rust/没有检查”的事实。独立用Node取当前及git show HEAD文本，自`# Artifact`标记起统一CRLF/LF后完整字符串比较 **相同**，rules/operations语义未改；不把context事实更新解释为豁免独立验收或修改OpenSpec流程。plan WP1写范围登记仅context事实重基准，未授权更改rules/operations。已有上游证据仍按固定范围复用，不外推为会话交付。

本reviewer实际再执行git diff --check exit0（仅已有LF/CRLF提示）；本次无需为文档事实收口重跑全产品检查。源代码本体的7项既有摘要继续引用wp2-input-review；最新元数据与已有运行证据固定如下。2.2活动代码及其测试、性能、保存失败/缓存行为不在本次范围，不能从本报告推导通过。

| 文件 | SHA256 |
| --- | --- |
| `README.md` | `F823644D89EFE928B5D447F5DDDF5407208B6CE5FB74968CCE77931446A0C33F` |
| `openspec/config.yaml` | `600C07F7D8B8F4F5CD2699226B921F0B75238FE2026BF17D53440F30F45915CE` |
| `openspec/changes/deliver-codex-session-loop/plan.md` | `ABEBA05EC4DEFF6883AE331246D22B7873165BDB0E8965F37F5283577A1275C3` |
| `openspec/changes/deliver-codex-session-loop/tasks.md` | `C0DA3A4F0C6457B3FE4B948263CC25592ADB6E30558CE721135AA1343A66C3E3` |
| `testing/runs/deliver-codex-session-loop/2026-10-30-wp2-input-development.md` | `FBC9CBD41B6432841432A8B82E0C6D1536FB597E2728BC06673D9622937B01A6` |
| `testing/runs/deliver-codex-session-loop/2026-10-30-wp2-input-acceptance.md` | `7CAE9ADD5AB2D11A75E63A38F13A724D81EA2C317D832092B0FAF7ADB4275083` |
| `.tmp/wp2-integrated/check.log` | `AEBAFEB29F242A17418AD9EF870AD2DAE80EEF0A7338330C9BF1604985A896AC` |
| `.tmp/wp2-integrated/check.exit` | `5FECEB66FFC86F38D952786C6D696C79C2DBC239DD4E91B46729D73A27FB57E9` |
| `.tmp/wp2-integrated/build.log` | `88944CD6837EDC9915799FFF85172523CA6441D3E8B64DBA70E522F538940149` |
| `.tmp/wp2-integrated/build.exit` | `5FECEB66FFC86F38D952786C6D696C79C2DBC239DD4E91B46729D73A27FB57E9` |
