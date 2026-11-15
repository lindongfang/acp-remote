# WP3 §6A公共ACP客户端独立代码检视

2026-10-31，reviewer `/root/baseline_reviewer`，只读生产文件，未参与产品修改；与coder及独立tester为不同执行者。

结论：**限定静态代码/安全检视通过，六文件当前纯客户端范围未发现阻断。** 独立tester同版执行结果另取证；开发clippy或开发测试不代签。本结论不涵盖真实Harness、审批/资源owner、SQLite、WSS或Web。

## 实际固定源码

基线为当前主工作区中先前已审ACP实现上的增量。六文件SHA-256由reviewer自行获取：

| 文件 | SHA-256 |
| --- | --- |
| app/src/adapters/acp/model.rs | a14f33cf708aa1a93b8291fbf0e5d55f18627190d54f086505f4f1c883bb2b60 |
| app/src/adapters/acp/strategy.rs | 61c917f443b967d5132c1fdbe71a92c47fe5569c73cddbf413230a2f3821bac4 |
| app/src/adapters/acp/transport.rs | 93bd37e419387eefbb5f5a8819bbbf9c678f44a0baccec89b9edfc61d34f535b |
| app/src/adapters/acp/tests.rs | a3279660e6f1a8a456da570513a5af13002874f62107c40677317c67dc65ba3c |
| app/src/adapters/acp/tests/discovery.rs | c3b0f0ef486863c796eed398e8054b8bf26999fe631f51093c7acc6a7c0c4e02 |
| app/src/adapters/acp/README.md | 3a05bb14dfa3e353fcaacc542b59437d3539d229b70ccf840787f03c7fa0859e |

依据为已通过的`acp-discovery-replay-planning-review.md`、实施合同§6A及独立定义`0d2897bd651000351bc35d21256023ae0ee0e686306129fea264267707e92129`。本次重新核对`testing/probes/codex-session-loop/node_modules/@agentclientprotocol/sdk/schema`中的稳定v1/v2摘要仍为已审`1c57095a...fdef5`及`28c7251e...e0778d`，未换成unstable。

## 核对结果

- `Version::preferred`明确当前v2；initialize_preferred完全复用每连接一次初始化路径。Negotiated.offered来自当前Pending的真实offer，version来自实际selected，未根据程序名/旧报告猜测选择原因或未来支持。
- v1 list对象、loadSession=true、resume广告独立解释；v2有session对象才提供相应基线能力。缺能力在构造前拒绝，不写RPC或改用new/私有接口。合法低版选择仍按v1字段解析，不重初始化/业务重发。
- list可选参数真实省略，字符串UTF-8字节≤4096；最多4096项。sessionId仍通过原不透明原生字符串类型，不套平台ID；必需sessionId/cwd或cursor错类型拒绝，不发布部分列表。投影保持数组顺序，完整raw经Incoming保留，optional/未知内容未丢失或执行。cwd绝对性/权限明示由owner验证。
- replay v1只用session/load及原ID，v2用session/resume加start，与普通不重放resume分开。Connection.request在submit登记ID及任何写入前检查同session的Resume/Replay联合窗口；冲突不改变首请求或下一ID，另一session可并行。
- route仅对当前窗口同session更新标DuringReplay，其他session及响应/error后回Live。该枚举与注释明确只是时间窗口，不做历史身份/轮次、文本去重或完成推断。
- permission仍走独立Incoming::Permission及原server_pending/private connection tag、原广告摘要/选项一次响应规则，没有因replay窗口伪造历史审批来源或复用旧批准。当前完整command subject适配限制在README如实保留为后续，不把工具必关联当新版产品权威。
- v2 resume/replay成功响应要求该Pending内同session State观测；窗口前/其他session不满足，MissingSnapshot由next统一poison连接并清所有关联。未知state原样保留给owner维持未知，不在此层解释idle或stopReason为新Run终态。v1不误套v2快照前提。
- 原有32MiB帧、深度96、结构token、RPC/原生ID及未决数量限制、Strategy来源tag、唯一writer、deadline、部分读与写future取消语义未被放宽。无新增后台任务、无界发送队列、SQL、日志正文或危险副作用探针。
- README已同步当前合同/独立能力与证据边界，删除停用历史强刷/必Run前提；未虚报owner/原生恢复/真实程序已交付。开发测试仅新增discovery模块，原测试正文未删除或弱化。

## 未验证范围

本reviewer实际只读源码、diff、开发测试及摘要，没有执行构建或测试。阶段独立协议夹具验收仍待读取同版原始证据；真实Codex/其他Harness、完整历史视图/分页、全平台内存、认证/撤销/审批owner、Windows资源树均不由本结果证明。

Web npm audit按用户“暂不外发”保持not-run，本次未调用网络依赖审计或传出依赖元数据，不将其写成供应链通过。后续若候选源码变化需重新判断证据，当前没有新增静态问题ID。

## 同版独立证据与最终限定结论

2026-10-31，同一reviewer已实际读取tester的`acp-discovery-replay-acceptance.md`、`acp-discovery-replay-test-final.log`和`acp-discovery-replay-acceptance-source.json`。六文件生产/开发资产摘要与本报告表逐项一致；产品锁`87f27b619281b37015c79ceae8d95471c8a4de50f202f4dd8a6d99fa0d27e324`未变。独立资产`app/tests/acceptance/acp_discovery_replay.rs`摘要`48ac5cb99352caafa01d1294304e67a0c46d81cb98a71daa349ff739d123cbca`，独立集成测试入口`3c0b5ffb7d90bbbc4ce99523997f1dc5819bd521db3661bb3ddaaf01fc1def0d`。

最终原始日志实际17项全部ok、0失败/忽略、21 filtered，exit0。17项是独立合成对端/公开API经真实异步双工字节流的约定L3/L4测试，不是19个完整稳定CL用例通过，也不把filtered项目计为执行。用例实际包含preferred/selected分层、独立能力、列表raw/顺序/cursor及精确预算、same-session窗口与scope零写、v2缺snapshot关连接/未知状态、权限独立、初始化及业务失败不重试、读取消保留/写取消poison；32MiB、深度、token及64未决的代表性边界已执行。

tester明确没有穷举4096个server_seen持续累计等旧内部预算，亦未声明所有旧客户端细节的独立全覆盖。本轮需求定义内的指定代表性回归已有证据；未执行的旧分支继续如实保留，不能由开发测试代签。

最终限定结论：**§6A纯公共客户端增量的静态/安全检视及同版独立协议子范围通过，无未关闭阶段问题。** 产品Node/SQL/WSS、source跨连接所有权、共享视图/轮次/分页/渲染、审批发行者/真实writer围栏、真实Harness及Windows树等范围不变，完整L5/L6仍未通过。Web npm audit依用户限制继续not-run，不借此次客户端验收声明供应链全部通过。
