# WP3 3.4 公共ACP客户端：独立验收

日期：2026-10-30。结论：**BLOCKED，ACP-R001/ACP-R002已独立动态复现。** 现有17项及外部正向4项通过，不能覆盖这两个未测分支；新增保持正确预期的回归2项均失败。不得签该候选完整3.4或最终CL通过。原reviewer问题ID沿用，不重复发明编号，也不代关。

范围仅冻结公共版本策略/泛型AsyncIO合同，无真实进程、Harness、安全issuer、actor/source围栏、SQLite/Web或最终运行状态机。本tester未改产品、用例或配置，独占外部probe与target。

## 固定版本和环境

- worktree：`C:/Users/jack/.codex/worktrees/codex-session-wp2/acp-remote`。8文件manifest `.tmp/wp3-acp-logs/frozen-files.json` SHA-256 `C38150F250C0BC2DFBC73717B7397F89F801BE9D43FDEA9C0E886425A70C1A09`。
- 全部8文件运行前、正向测试后及失败复现后均匹配；独立清单`.tmp/acp-client-independent/frozen-before.sha256`、`frozen-after.sha256`，SHA-256 `A84C969678E9E3094E090AA197DC77F3EE5EAE9AC2C32CCEE6B37EA1D6588BC1`。不把HEAD或主D产品代替候选。
- 主D合同§7–10及最新tests作为权威；同日CL-004另补四生命周期动作崩溃窗口，最终tests SHA-256 `5FD9D9975EB1CB0C29694339489705C1ECA474AE6E3D5FC9DAE8C9759B3275F7`，不改变本次ACP预期。
- 读取本机固定SDK2.1.0的官方v1/v2 schema：SHA-256分别为`1C57095ACF9C0844054DD1FDC3624C121B3FDC24209ECC987FD6748C705FDEF5`与`28C7251E820511DA257CDB55D30E2C30BBBE59226305B04E3B751DC655E0778D`。v2仍Draft；可选capabilities省略默认空，session={}广告baseline能力，未将合法省略误报成必填错误。
- Windows本机rustc/cargo1.98.1，真实owner CI前置成功；仅进程CARGO_TARGET_DIR=`D:/Project/acp-remote/.tmp/acp-client-independent/target`，TEMP/TMP同自有根。独立构建真实执行，外部依赖离线解析；不读取凭据、不启动真实Harness或改变用户代理。

## 实际命令和结果

| 命令 | 实际结果 |
| --- | --- |
| 冻结worktree：`cargo test --manifest-path app/Cargo.toml --locked --lib adapters::acp::tests -- --nocapture` | 退出0，17 passed/0 failed/0 ignored，200 filtered，0.88秒。 |
| 自有probe：`cargo test --manifest-path D:/Project/acp-remote/.tmp/acp-client-independent/Cargo.toml --offline`（正向4项） | 退出0，4 passed/0 failed/0 ignored，0.02秒；0项doc-test不计功能覆盖。 |
| 同一外部工程：`cargo test --manifest-path D:/Project/acp-remote/.tmp/acp-client-independent/Cargo.toml --offline --lib regression_ -- --nocapture` | 退出101，0 passed/2 failed、4 filtered，0.01秒；两项对应下述原reviewer问题，未改预期掩盖失败。 |

开发217库+4向量、fmt/clippy/all日志仅其开发证据，本tester未重跑完整217/4，也不将自评代签。每轮构建/测试串行使用独占target，没有产品源码写入。

## 阻断复现

### ACP-R001 — 未知v1 stopReason成为可靠完成

目标位置：`app/src/adapters/acp/strategy.rs::prompt_result`。先协商真实泛型IO fixture的v1，五个已定义合法值end_turn/max_tokens/max_turn_requests/refusal/cancelled均可得到Completed；再传`{"stopReason":"future_or_invalid"}`，独立实际输出`unknown_stop_reason_completed=true`。预期“未知不得成为Completed”断言失败。

未知控制语义不能因字符串类型就解除上层Run前台互斥。固定schema StopReason只有上述五种，raw保真不等于可靠终态。此处为公共客户端直接控制语义，不依赖真实Harness即可确认；原reviewer提出者修复后复查关闭。

### ACP-R002 — Request跨连接绕过目标能力

目标位置：Strategy生成Request与`Connection::request/submit`。建立A/B两条独立、已协商的v1连接：A广告image，B agentCapabilities={}明确未广告image。从A策略生成image prompt，将Request交给B.request，返回成功并实际从B对端读到`method="session/prompt"`。预期目标拒绝且零写入断言失败；不是仅从代码推断可达。

私有Request字段未阻止合法公开构造对象被交给别的连接。目标发送边界必须核实其selected策略/能力，不能用未来调用者保证不拿错替代端口合同。本次未用厂商名分流、未伪造Request私有字段。不同版本跨送等相关回归由修复版继续覆盖。

## 已执行的其他有限证据

17原测试已实际覆盖：v1/v2及v2 offer→selected v1单次引导；错误ID/版本/响应形状；prompt取消通知与v1终态/v2插入区别；resume省略重放但v2必须先有同session快照且标DuringResume；typed原生/RPC ID；原权限广告防篡改、未知选项/subject失败关闭、同wire ID跨连接来源拒绝；原始深层结构、工具null/省略patch差异；读future取消保留部分帧、写取消poison；乱序/重复响应、deadline/EOF与容量；32MiB帧恰好/超限、深度96、token/ID/64未决边界、有界出站序列化。它们是duplex fixtures，不是对真实对端的兼容证明。

外部4组另独立核验：

- v1、v2、v2offer→v1selected的实际能力解码及原响应未知字段；image支持/audio未支持按selected解释，未知notification保留带空白/转义的原始字节，Debug不泄露其字段。
- 同一连接并行保留RPC数字1/字符串1两项权限请求、不同session；反序各回复一次，重复UnmatchedResponse，另连接即使同wire ID也不能借用来源。此为连接来源，不等于应用Run/actor授权。
- 已完成new响应重复则poison且不重发；131073 null结构集合超预算Capacity；有未决请求时断线DeliveryUnknown、清关联。
- 自定义AsyncWrite真实future分别接受prefix后BrokenPipe、write_all成功但flush失败、prefix后Pending再取消；返回Unknown或保留Closed状态，不能再initialize，shutdown清关联。这里的写入仅内存替身，不是网络/子进程传输确认。

## 限制、清理与证据

没有真实进程/Job、真实ACP程序、审批issuer/Run/代际/client_gate/source fence、writer最终锁区、Web或SQLite参与。v2 idle快照/终态仍需后续应用owner解释，公共库不能当现成Run状态机。没有签完整JSON Schema引擎、全部可选方法、真实取消/原生恢复或安全交付。平台PLAT-T001另行阻断，不能借本报告消除。

泛型IO任务均由测试JoinHandle/timeout拥有，测试进程已退出，无自有子进程或用户进程清理操作。原始日志和外部源码保留自有目录。正向4项源码当时摘要`294E0303B9A6199CA4BA9BD007CD425B56B315E6B6E2ED10E492332EB89A749B`；追加两失败回归后当前源码摘要`C5B399EEC378BC321998CBC7C62FAA3264DFB46BF02F9B8F752EA618FB401CD1`。产品8文件始终不变。

| 自有日志 | SHA-256 |
| --- | --- |
| contract-tests.log | C7660C2815B769E86BD39AF9B2275BF2F6CD4969ACAF2DFAC307004E1A9DFE4A |
| external-tests.log（正向4项） | 8F81533711830199C2B92C3BD08FC40BF8BFB52FB05F36C6CAE81D372898300C |
| reviewer-findings-repro.log（保留失败2项） | BD6A488943004049020CDFCFB4553E0A3E9DD2F13BE9E9C625AA7F869450D66D |

旧候选已完成首尾固定与独立证据采集，可交原coder修复；新候选冻结后必须重新判断/复测，不沿用旧版21项正向结果宣布通过。

## ACP-R001/002 修复版独立复测（追加新版本）

2026-10-30：本tester原动态失败回归现已真实通过，且既有正向4项与修复版19项合同重新执行通过。未改原两失败测试、未改产品或原预期，旧BLOCKED/失败日志保留。此处记录原复现已消失的独立证据，ACP-R001/002由原提出reviewer独立复查关闭；不自签最终产品/安全通过。

新冻结manifest为C工作区`.tmp/wp3-acp-r001-r002/frozen-files.json`，SHA-256 `55FD2951F95A3B420554970FA27F905008860477520F9474F6B86F176B4D875D`。8文件开始/结束逐项匹配；独立清单`.tmp/acp-client-independent/fixed-before.sha256`/`fixed-after.sha256`的SHA-256均为`0D0BB3A4F0C86E51AC4176BCE15A62D480AC0B6C3A9A5C8AE145BBE289D20591`。旧manifest C38150F2…仍保留历史值；WP2原2.3清单14文件另逐项核对全部不变。没有覆盖存储、平台、Cargo或主D其他产品。

真实owner CI前置成功，沿用独占D盘target/TEMP及离线外部依赖，独立重新编译实际执行：

| 命令 | 结果 |
| --- | --- |
| 自有工程`cargo test --manifest-path D:/Project/acp-remote/.tmp/acp-client-independent/Cargo.toml --offline --lib -- --nocapture` | 退出0；6 passed/0 failed/0 ignored，0.02秒，包含原2失败回归及原正向4项。 |
| 冻结worktree `cargo test --manifest-path app/Cargo.toml --locked --lib adapters::acp::tests -- --nocapture` | 退出0；19 passed/0 failed/0 ignored，200 filtered，0.84秒。未重复完整无变基础200及向量测试。 |

ACP-R001：原外部同样先检查5合法v1值，再送future_or_invalid，本次输出`unknown_stop_reason_completed=false`且拒绝断言通过。新增开发合同经独立实跑还覆盖未知/空/非字符串、连接关闭及保留单个原始拒绝帧；v2未知state/stopReason仍作为raw控制事实，不产生Completed，后续Run owner不得据任意字符串解除前台互斥。

ACP-R002：原外部A广告image、B未广告image的跨Connection Request现在被拒，目标仍可用；没有旧版实际session/prompt输出。实现以私有协商Strategy scope判来源，在注册request ID及写入前拒绝。19项中的新增参数化合同实际覆盖v1/v2四种source/target组合、旧source shutdown后的stale Strategy、target pending_count=0、无对端字节、拒绝不关闭目标及合法目标自身request仍使用ID2并成功完成。相同连接Strategy clone保持合法来源，不按厂商分支。

外部源码仍为`C5B399EEC378BC321998CBC7C62FAA3264DFB46BF02F9B8F752EA618FB401CD1`，与旧版失败2项复现后的源码完全相同，故不是换预期得到绿色。未发现本次范围的新阻断；完整schema/全部可选方法、真实Harness/Run/审批issuer/actor gate/writer/资源树及Web仍未验收。

| 新日志 | SHA-256 |
| --- | --- |
| `.tmp/acp-client-independent/external-fixed-tests.log` | 86A9C12979F504745BA895B9AA386E15C300A6E7CA08A9556A4BEC7EA0A73BBE |
| `.tmp/acp-client-independent/contract-fixed-tests.log` | 601E9C8C1AD63261AEC819A972D2CED4566B91E31754C02E76E5F886B65E7599 |

相应.exit均0。泛型IO测试进程已退出、无实际子进程或凭据参与，日志/独占工程保留用于审查。修复版8文件首尾不变，报告不签219基础全测独立执行、不解除后续集成/安全/真实兼容门禁。