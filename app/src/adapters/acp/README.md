# 公共 ACP 客户端：3.4 第一候选

本包只负责标准ACP编码、一次版本协商和调用者拥有的异步stdio状态机。无进程启动、SQLite、安全策略或Web接线，不表示真实Harness产品闭环通过。未知字段和未知更新种类按完整原始消息保留，不降成文本。

## 固定依据

主仓库 testing/runs/deliver-codex-session-loop/2026-10-30-network-retry.md 与 session-contract.md §7–9；官方规范观测引用 abc8a31b16fc4eb6e3578ced562088e8e26d959a。v2为Draft，程序/SDK发布号不等于协议号。

读取主仓库 .tmp/cl-wp1-probe/node_modules/@agentclientprotocol/sdk 2.1.0 固定标准schema：

- v1 schema.json SHA256：1c57095acf9c0844054dd1fdc3624c121b3fdc24209ecc987fd6748c705fdef5
- v2 schema.json SHA256：28c7251e820511da257cdb55d30e2c30bbbe59226305b04e3b751dc655e0778d

规范入口：[v1初始化](https://agentclientprotocol.com/protocol/v1/initialization)、[v2初始化](https://agentclientprotocol.com/protocol/v2/initialization)、[v2 prompt生命周期](https://agentclientprotocol.com/protocol/v2/prompt-lifecycle)。本候选手写核心必需字段及能力解释，不宣称提供完整JSON Schema引擎或全部可选方法；未广告的fs/terminal/auth客户端方法返回method-not-found。

## 版本策略

initialize使用offer形状且每连接仅一次；先关联响应ID、检查selected受支持且不高于offer，再按selected解析info/capabilities。v2 offer→v1 selected使用agentInfo/agentCapabilities。错误、未知版本或不匹配结构关闭连接，禁止再次初始化或重发业务输入回退，不依据程序名分流。

v1 prompt的stopReason响应是该调用完成；v2 messageId仅表示插入，完成依据state_update，取消通知不等于终态。resume引用原native session并省replayFrom，不替代为new/load；v2须在响应前有同session的current-state snapshot。恢复期间所有更新标为DuringResume，快照不能产生新完成或正文哈希去重。可靠Run及平台前台互斥由后续session owner实现。

工具name的省略/null在v1均保持，在v2 null清除；v2工具_meta按顶层键patch，null删除/清空，嵌套对象直接替换。其他结构保留原文，由后续版本化事件映射解释，不能套通用深合并。native request/tool/message/session/option ID分类型，数字请求ID与字符串不同。

权限tool_call及带toolCallId的command subject可提供引用。无法关联的command、缺失/未知subject、未知选项不能选择允许；原文保留，应用层仍须同session/Run/代际关联已观察工具。缺rawInput本身不拒合法引用。运输层绑定私有连接来源，响应时重新解析原广告，调用方篡改、跨连接同ID借用及重复交付不能扩大选项。cancelled可明确关闭未支持请求。这里的来源不是应用授权issuer。

## 所有权和资源预算

Connection独占读半部、唯一writer、关联表及部分读缓冲，无后台任务或无界发送队列。调用者持续pump next，等待prompt时须处理权限/更新。request注册后直接有界write_all+flush；写future取消即留下closed状态，不能重试。未知/重复响应、超时、EOF、非法消息使连接失效，无自动重发。shutdown用绝对Instant预算清理关联并关闭stdin；平台OwnedProcess负责完整Job回收，stdout EOF不是树回收证明。

ACP stdio独立预算：消息32MiB（不含CRLF），容纳16MiB输入加RPC信封；JSON深度96、结构token计数131072、native字符串ID4096 UTF-8字节、RPC字符串ID1024字节、双方各最多64未决请求、对端已见ID4096个。满则明确失败/关闭，不驱逐旧ID复活，不裁原文。事件返回后保留内容由调用者预算负责。管理WSS的32KiB/深度8/限流/队列不受此预算影响。

最终接线在同一client_gate→session owner围栏内核实真实issuer、actor、Run及代际；强刷和版本检查后持久化claim并直接调用实际writer。write/flush只是transport事实，无peer应用ACK。公共内部传输API不是远程授权入口。

## 开发验证范围

参数化duplex fixture覆盖双版/回退、字段、关联乱序、未知/重复响应、取消/插入、恢复边界、深层原文、权限广告防篡改/来源隔离、patch差异、异步字节边界及读写future取消。fixture不是实际Codex/DeepSeek或Windows Job。真实兼容、恢复、审批围栏、树清理由后续固定集成版独立验收。

## ACP-R001 / ACP-R002 修复候选

v1仅固定官方五个stopReason（end_turn/max_tokens/max_turn_requests/refusal/cancelled）产生Completed。未知、空或错类型控制值使连接失败关闭，不能释放上层Run互斥；通过take_rejected_message一次取回唯一被拒解析帧的完整原文，最多保留一帧，不进入普通日志。v2原始state事件仍由后续owner判定可靠Run事实，不由此包把任意字符串升级为完成。

Request带私有Strategy来源tag；Connection在登记ID及任何写入前核对其当前冻结Strategy来源。其他连接（即使同版本同能力）或已关闭连接的旧Strategy生成的Request均零写拒绝，未改变正常目标连接/关联ID，可继续用目标自身Strategy构造请求。此约束不依赖调用方保证不拿错，也不是应用授权issuer。原C381快照及原独立问题报告保留，新候选须原tester/reviewer复查，coder不关闭问题。
