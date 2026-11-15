# 后续WP3派发草稿：具体前置接口盘点

2026-10-31，只读建议；**不是方案门禁，不放行编码或schema6执行**。SQL v3关闭修复优先。亲读派发草稿 `4428e3f7bf9f462a86102f149231662c07d4b7d54b502931c60f4c53685142aa`、DDL `63e0b7ca4e2782b12c1917e6e87e06f6fc9ee77feacd514621eaedfc0ab81558`，并核当前公共ACP实现及固定SDK2.1.0双版schema。

结论：按“公共ACP精确迁移与IO先行→必要派发账本及actual R3 owner接真实IO→Node/WSS”串行更可执行。第一段应交可被第二段直接调用的真实协议适配器，不另交一个只有抽象trait的模型；第二段仍必须同时完成真实SQL、实际owner和writer，不把整体任务缩成CAS。以下是进入相应编码门禁前必须由负责作者冻结的具体接口。

## 第一段必须固定的四处边界

| 当前具体缺口 | 最小收敛要求 |
| --- | --- |
| `RawMessage::fields()->&Map<String,Value>`、`Request.params:Value`、Strategy.prompt(Value)、prompt_result/update/permission/session_listing及P1/P2的Value接口仍在；只新增raw_json尚未迁移。 | 冻结RawMessage持有RawDocument后的原字节/只读字段接口，以及Request/Response/Incoming/Strategy各参数和返回值。固定未知结构、来源品牌、typed RPC身份、错误及replay/live语义；不留隐式全DOM兼容层。类型错误不能静默降文本；整数raw helper不自动成为RPC ID许可。按测试机械适配清单保原P0/P1/P2断言，入口/出站32MiB与旧96/131072门槛需在这一整条实际路径移除，而非再称基础层已完成。 |
| new/replay/resume目前只有cwd，load/resume还硬写mcpServers=[]；草稿没有可信配置载体和Ready证明。 | 冻结完整SessionSetup输入及可信配置来源/版本，明确KnownEmpty与Unknown，禁止以Unknown默许清空。准确区分v1/v2的MCP、additionalDirectories字段及固定schema能力来源；resume/replay/load实际编码必须保配置。可引用已有配置盘点1f1db024，不另发明厂商能力或把协商版本当所有能力支持。 |
| `Connection::request/next`都需&mut self，write_all+flush与协议表仍同对象。 | 冻结真正可分开的Reader/单Writer/ProtocolState构造与关闭句柄，以及reserve/cancel/response所有权。reader须在SQL等待和writer背压时处理嵌套审批/控制；writer取消确认、部分帧poison范围、控制优先但不插入半帧、每次poll/flush的同围栏和offset校验须有准确状态/结果类型。禁止用外层Mutex包旧Connection代替拆分。 |
| 草稿NativeRequestReservation、NativeRequestHandle、EncodedPromptFrame与DispatchPayloadPort::Frame尚是多处名称/约束，并无一个可由外部独立测试合法取得的完整正向路径。 | 固定实际协议工厂如何预留typed RPC/session/connection绑定，如何由真实codec生成frame，如何返回可取消/可join的write cell。明确关联类型相等约束、合法构造/getter、编码错误/从未poll/partial/Written结果；不用为测试开放bool授权工厂。同步创建write future不能在应用最后围栏之外偷偷写。 |

公共协议gate还需固定一个返回帧先于written补记时的处理序：RPC应在首次写前预登记，但“仅预留、从未poll”的请求不能被伪响应当成已发；已实际完整写帧而flush/SQL补记尚未完成时，reader也不能丢失合法响应。取消后迟到typed ID、旧connection/load和重复帧如何保原文并拒绝/消费一次，应在这里完成，不由schema6自行推断。

## 第二段不能由SQL形状补出的事实

固定primary明确：v1 PromptResponse要求stopReason；v2要求messageId，其响应证明插入而非执行结束。v2 StateUpdate是会话聚合前台状态，可以由agent主动工作产生，running可先于用户消息插入；idle/stopReason重复或resume快照不能计作新完成。因此草稿保持会话事实不伪填attempt是正确方向，但完成/释放映射仍应由原始帧合同作者冻结，reviewer不在此替其定义协议因果。

发现一处明确API缺口：草稿42行 `record_native(&WriteProbe,&NativeFact,...)` 强制需要attempt的WriteProbe，DDL却允许foreground_active/idle/terminal/resources_converged/connection_lost无attempt。会话在没有任何prompt时也可有前台工作，这条正向路径不能靠构造假dispatch解决。最小选择是分开会话观察与attempt事实端口，或提供经过真实session/resource来源核验的另一种probe；由作者给出可构造路径与精确参数，不新增原生完成关联。

此外，reserve/arm等待与R3管理Pending不得混成一维状态：reserve期间准备项仍在等待位，Edit/Cancel必须可先使token失效；select_reserved之后才进入独立active。冻结accept_reservation/select_reserved对两类pending的具体影响，才能写出“准备期取消零字节”真实屏障，而非只测返回码。

schema6仓储35行的detach/drop不能照搬为充分关闭保证；应在SQL v3修复通过后明确复用或提供同等真实ack关闭所有权。现在不为未来账本扩泛v3模块。ReleaseProof/NativeFact/DispatchRecoverySummary等仍须在协议/资源事实已固定后列准确字段/变体，不能由可构造SQL记录mint可写lease或把新握手当旧writer收敛。

## 本次建议的实施边界

先冻结上述公共ACP表示、配置和IO合同，并用真实可观测AsyncRead/AsyncWrite/自有协议child验证双版帧、并发reader、写取消和半帧边界；明确替身与真实Harness证据级别。然后把已固定的protocol port接入实际R3 owner和schema6 reserve/arm/record链，验证最后每次poll、未知记录恢复和slot释放。Node/WSS最后接同一真实端口并完成身份/资源及真实Harness证据。

这只是将未决的具体类型和时序前移，并未改变产品规则。新纯队列rebind_unsent_active仍须独立冻结/回归，当前不授权修改已验R3模型；schema6也未获执行许可。
