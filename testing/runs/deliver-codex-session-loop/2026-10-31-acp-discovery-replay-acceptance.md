# WP3 §6A 独立公共ACP客户端验收

2026-10-31。结论：**当前纯公共客户端L3/L4子范围17/17独立测试通过**，0失败、0忽略；未发现该子阶段阻断。它们是合成协议对端与公开API证据，不签CL-032–041/047–053/058–059整例通过，也不证明Node/SQL/Web、共享视图或真实Harness能力。

## 版本、资产与独立性

tester只新增 `app/tests/acceptance/acp_discovery_replay.rs` 并在 `app/tests/session_loop_acceptance.rs` 声明模块；生产源码只读。最终独立测试SHA256 `48ac5cb99352caafa01d1294304e67a0c46d81cb98a71daa349ff739d123cbca`，入口 `3c0b5ffb7d90bbbc4ce99523997f1dc5819bd521db3661bb3ddaaf01fc1def0d`。

生产ACP/锁执行前后逐项摘要相同，见 `acp-discovery-replay-implementation-before.json`、`acp-discovery-replay-acceptance-source.json`。关键：model `a14f33cf708aa1a93b8291fbf0e5d55f18627190d54f086505f4f1c883bb2b60`，strategy `61c917f443b967d5132c1fdbe71a92c47fe5569c73cddbf413230a2f3821bac4`，transport `93bd37e419387eefbb5f5a8819bbbf9c678f44a0baccec89b9edfc61d34f535b`；mod/tests/discovery/README完整摘要在清单。没有因父执行准备其他阶段而把其他模块变化当成本阶段验证范围。

本地SDK2.1.0 v1/v2稳定schema已实读并核对SHA（1c57095...、28c7251...，见清单/阶段定义）；没有运行npm audit、上传Web依赖元数据或联网。旧ACP README的停用合同/强刷/必Run措辞在当前产品版本已清理，本轮不再沿用。

测试使用独立的64KiB双工流，通过公开Connection/Strategy API及真实异步字节收发核验；没有调用私有测试入口或复用coder测试正文。大帧与大列表采用同owner的join读写，读写future取消有明确所有者；没有脱离测试的后台任务、Harness进程、模型、用户配置、SQLite或认证网关。CARGO_TARGET_DIR为tester独占activity-target。

## 实际检查

从仓库根运行：

- `cargo test --manifest-path app/Cargo.toml --locked --offline --test session_loop_acceptance acp_discovery_replay -- --nocapture`：最终17 passed、0 failed、0 ignored、21 filtered out，退出0；日志 `acp-discovery-replay-test-final.log`。
- `cargo clippy --manifest-path app/Cargo.toml --test session_loop_acceptance --locked --offline -- -D warnings`：退出0；日志 `acp-discovery-replay-clippy-final.log`。
- 两个授权测试文件已单独rustfmt；单文件格式及git差异检查无错误。没有运行或外推完整工程/真实产品检查。

首批15个独立测试也实际通过（acp-discovery-replay-attempt1.log）；随后补初始化传输错误和对端info差异，并修正测试循环以明确覆盖null/字符串cursor。测试自身clippy的4项引用/断言风格告警已修，初始输出保留在acp-discovery-replay-clippy.log；没有产品失败或通过放宽预期换绿。PowerShell对cargo stderr的NativeCommandError装饰不等于失败，上述退出码与Rust测试摘要均实际核对。

## 逐稳定用例子范围证据

| 稳定ID | 本轮实际验证/证据边界 |
| --- | --- |
| CL-032、048 | v1八种list/loadSession/resume独立能力组合、v2省略/null/{}基线；不同合成程序info/name/version不改变相同标准replay wire。缺能力构造拒绝且pending=0、对端零字节。没有程序版本白名单或厂商实测结论。 |
| CL-033、052 | 两版标准load/replay与无重放resume方法/参数/完成响应分别核验；v2当前snapshot窗口前提、重复/未知状态原样保留；权限取消与读取窗口结束分开。不证明实际Run互斥/重新准入。 |
| CL-034 | 列表可选参数实际省略，顺序不按title重排，空/省略/null/字符串cursor、unknown字段及精确raw字节保留；unknown update重复正文不合并。数字/字符串权限RPC ID独立，Debug不泄露合成敏感载荷。 |
| CL-035、036、058 | preferred实发v2，合法selected v1按v1结构并保留offered/selected；未知/超offer版本、缺info、初始化业务错误、EOF、deadline、错RPC均不二次initialize或低版重发。显式v1合同入口保持。 |
| CL-037、041 | 固定规范schema/SDK身份经本地核验，wire显示的是ACP版本；所有结果明确是夹具，不将JSON-RPC/SDK版本或本轮通过写成真实v2/Harness验收。此两例的真实矩阵/产品状态部分未执行。 |
| CL-038、039、040、053、059 | 存量低版不热切，新连接优先高版、独立重算能力；高版null工具名语义未压成v1；旧/其他连接Strategy的list/replay/resume Request零写拒绝，关闭后的旧Strategy也拒绝。高版业务失败仍保留实际版本/结果、不再initialize或自动副作用/私有回退。实际升级证据管理、用户决策及旧资源/审批责任不在本层。 |
| CL-047 | 只有无重放resume时standardReplay不虚报支持，replay请求拒绝且零写；纯协议客户端不能替代产品创建/输入/恢复/接管的基础准入。 |
| CL-049 | UTF-8 4096参数边界/超一字节拒绝、4096列表项完整返回/4097容量错误；64未决请求边界、96/97 JSON深度、结构token代表性上下界、32MiB帧精确边界/超一字节。大帧与大列表实际经双工流，不以直接解析私有函数代替。视图与全平台预算尚未验证。 |
| CL-050 | required列表字段/类型错误无部分发布；非法JSON、EOF、deadline、重复/错类型RPC关闭并清关联；远端error不自动重试。部分读future取消后剩余字节能完成原响应，写future取消使连接不可再用，不重发已部分写出的prompt。 |
| CL-051 | 同session四种resume/replay互斥组合、另session独立完成、cancel不提前释放窗口、error后仅显式新请求可入；DuringReplay仅同目标窗口，其他目标及响应后为Live，权限仍独立且只能响应一次。未声称窗口每条必是历史或建立可靠跨加载ID。 |

v2缺当前snapshot测试分别覆盖请求前同目标state、仅其他目标state及完全缺失，均MissingSnapshot关闭；正向覆盖running/idle/requires_action/future_state及重复snapshot。客户端只返回State/Resumed/Replayed事实，没有把快照冒充Prompt完成事件。

## 限制与后续

本轮未覆盖全部既有传输内部预算的穷举（例如4096个已见对端RPC ID持续累计），只对§6A新增方法及指定代表性边界独立回归；原开发测试不能代签未执行分支。参数的本机绝对路径授权/目录可用性、跨连接source原生目标协调、可信来源/唯一裁决/实际writer授权围栏、能力证据UI、共享缓存/重放代际/分页/内容与资源安全、Node/SQL/WSS、真实Harness/Windows进程树均不在本次通过范围。没有将17/17写成19个完整CL用例或产品交付通过。

独立reviewer仍须对同版实现/diff及本证据形成交付检视结论；后续父执行可复制本报告、源码清单与日志至仓库testing/runs，tester不并行编辑其副本。
