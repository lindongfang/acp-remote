# WP3 §6A 公共ACP发现与重放独立用例子定义

2026-10-31。依据当前七份spec、implementation-contract.md §6A、实际公共ACP代码及本地固定SDK2.1.0 v1/v2 schema。只细化 CL-032–041、047–053、058–059 的纯L3/L4协议子范围，不新增产品用例、不降低这些稳定用例的完整L5/L6及真实组合义务。本轮仅规划，全部新增执行断言not-run，不改产品、不接Node/SQL/Web/进程，也未运行npm audit或外发依赖元数据。

## 固定来源与阶段判断

- 本地SDK：`.tmp/cl-wp1-probe/node_modules/@agentclientprotocol/sdk`，package.json确为2.1.0；稳定schema/v1/schema.json SHA256 `1c57095acf9c0844054dd1fdc3624c121b3fdc24209ecc987fd6748c705fdef5`，稳定v2 SHA256 `28c7251e820511da257cdb55d30e2c30bbbe59226305b04e3b751dc655e0778d`，与现有ACP README固定摘要一致。不使用unstable schema代替，不把SDK号当ACP版本号。
- 已逐项读取InitializeRequest/Response、AgentCapabilities、SessionCapabilities、ListSessionsRequest/Response、SessionInfo、v1 LoadSessionRequest/Response、两版ResumeSessionRequest/Response及v2 ReplayFrom/StateUpdate相关定义。v2的capabilities.session={}提供基线list/resume；v1 list与顶层loadSession独立。v2 resume在重放后、响应前有当前state_update；idle快照不是新的完成事实，未知state不能当idle。
- 旧代码目前只有显式initialize、resume、DuringResume；已有连接来源tag、一次initialize、v2恢复snapshot前提、原始载荷、严格RPC关联与预算可增量复用。旧ACP README仍引用停用session-contract及历史强刷/必Run措辞，这些不作为新应用安全权威；本阶段只复用当前代码事实、固定schema与§6A。
- §6A与固定schema的此子阶段方向相符，未发现需要再问用户的产品分支。SDK的cwd绝对路径语义须保留，纯客户端不做本机目录存在性/权限校验；平台登记与安全根检查留在所属应用层，不由客户端猜厂商或扫描私有文件。

## 执行边界与观察方法

未来用独立协议对端/duplex真实异步字节流调用公共API，固定收发帧、RPC ID、pending_count、连接状态、raw字节和时序屏障；L3检查编码/投影/能力解释，L4检查完整Connection初始化/写入/路由/超时/关闭。只用合成内容，不启动Harness、认证网关、模型或用户配置。测试应以公开API独立实现，不能仅引用coder测试通过。

每个拒绝分支除检查错误，还读对端确认零新增字节、pending关联数量不变或按故障关闭；不能只看返回值。每分支独立连接或明确恢复到合法状态，测试任务/reader/writer由测试拥有且有关闭/超时，不能留后台无限任务。现有传输预算沿代码和冻结合同，不为方便测试放宽。后续固定实现/独立测试摘要再执行，缺入口按未执行处理。

纯客户端的session互斥仅指同一连接内的标准原生sessionId；跨入口/多连接的sourceId+nativeId唯一资源归属由应用层协调。协议层能构造某标准请求不代表产品已经准入；特别是CL-047，当前子范围仅验证能力呈现及不支持replay的零写拒绝，不能冒称已验证产品创建/输入/恢复/接管入口关闭。

## 稳定用例与子范围映射

| 稳定ID | §6A纯协议子范围 | 继续保留的完整义务 |
| --- | --- | --- |
| CL-032、048 | 相同协商结果/能力的不同对端信息走同一路径，list/replay能力解释及编码一致 | 真实跨款/节点准入、白名单与安全条件由L5/L6另验。 |
| CL-033、052 | 两版重放完成信号、v2插入/状态/快照分离，无重放resume与replay互斥及区分 | 前台互斥、Run/审批有效性、恢复授权在应用层真实验证。 |
| CL-034 | list/raw与重放更新的结构/未知字段/顺序/标量RPC ID保真 | 视图投影、安全显示与资源读取不由纯协议结果代签。 |
| CL-035、036、058 | preferred一次v2初始化、合法selected v1按v1结构、无共同/非法选择/网络错误零业务重发 | 实际产品版本呈现、未知程序实测证据与真实协商另验。 |
| CL-037、041 | 固定schema/实际协议号与合同夹具证据分列，不冒称真实v2或Harness验收 | 缺真实对端/规范依据仍按原例保留未验证状态，不静默降级目标。 |
| CL-038、039、040、059 | 新连接重新协商、能力改变后的纯请求门禁、不自动私有桥接/降版/重复副作用 | 程序升级复验状态、用户决策、管理事实/资源责任和旧审批失效另验。 |
| CL-047 | v1只有resume、无loadSession时standardReplay=false，replay构造/发送被拒绝 | 产品所有入口的基础重放准入仍完整保留，不能凭此层验收通过。 |
| CL-049 | 标准全量重放可逐帧流式消费，客户端不累计整段历史；list项数/原有帧解析/关联预算 | 会话/节点视图、候选/冻结响应/分页等全部保留缓冲预算及真实全量重放另验。 |
| CL-050 | 响应前中断、非法结构、超帧/超关联、deadline与无副作用自动重试 | 页面完整性、保留资源及有效审批的实际管理由L5另验。 |
| CL-051 | DuringReplay只表示窗口，重复正文不合并，窗口/连接来源不混用 | 读取代际重建、旧响应不盖视图、无稳定ID衔接在视图层另验。 |
| CL-053 | 新连接失去标准重放能力时独立能力重算且旧连接Request不能拿来发送 | 已受理命令、期限、资源所有权与安全关闭不在客户端层虚构。 |

## A. preferred初始化与独立能力

操作步骤：每次创建新Connection，调用默认preferred入口并观察唯一initialize帧；分别返回v2合法capabilities.session={}、v1合法agentCapabilities（loadSession/list/resume分别组合）、v2 session省略/null、未知/高于offer版本、错RPC ID、selected形状不符及初始化error/EOF/deadline。旧报告/上一连接的版本不作为输入；另使用保留的显式版本入口完成受控v1合同。改变对端info.name/version但不改变协议能力。

预期结果：preferred始终先发v2，合法selected v1使用v1字段而不先套v2成功schema；发起/实际版本能从本次关联帧核验。每连接一次initialize，成功后、失败后及结果未知后均不再次initialize；没有未知未来版支持、厂商分支或自动低版业务试发。v1 list仅对象广告启用，省略/null或非对象不启用；standardReplay仅loadSession=true启用，与resume独立。v2 session对象即启用基线list和标准重放，不需多余可选标志；缺session不启用。缺能力明确拒绝对应调用，不偷偷new/load/私有替代。协议发现不创建产品支持证据。

## B. session/list请求、响应及保真

操作步骤：两版各在允许能力上分别调用无筛选/游标、省略cwd但有cursor、有cwd无cursor及两者都有；用多字节字符串检查4096 UTF-8字节边界及超出一字节的拒绝。返回空sessions、多个乱标题顺序的合法项、nextCursor省略/null/字符串；条目含未知结构_meta、title/updatedAt/额外根字段，并保留精确输入raw。另返回缺sessions、非数组、项缺sessionId/cwd或错类型、非法nextCursor类型、4096及4097项。缺能力对照调用须零请求字节。

预期结果：标准session/list，未给可选参数实际省略而非补默认筛选/游标；不改原始cursor/cwd。至少投影sessionId与cwd并保持数组顺序、nextCursor语义；4096项内按预算可解析，4097明确容量失败，不静默裁项。required字段/类型错误明确失败，不把不完整列表当完整；raw完整保留未知字段，optional显示字段不由本层丢弃或执行。不写平台库/扫描私有目录/自动追完所有游标；分页显示与原生列表游标不混同。

## C. 标准replay与无重放resume

操作步骤：v1 loadSession=true时请求replay，检查session/load及原sessionId/cwd/mcpServers=[]；v1只resume无load时尝试replay。v2 session={}时请求replay，检查session/resume与replayFrom:{type:start}，对照普通resume实际不带replayFrom。保持相同原ID，不发送新输入。两版交付各自合法空result及带可选字段/未知扩展result。

预期结果：严格按实际版本/能力选择标准方法，不把resume成功当历史重放成功；unsupported零写，不代换new或厂商接口。v1以load成功响应结束读取，v2须满足下节snapshot前提。result的原始结构保留；重放完成只是传输/读取事实，不产生平台Run、聊天轮次、原执行完成或工具批准。既有不重放resume合同保持。

## D. 同session窗口互斥与连接scope

操作步骤：同连接对session A分别先resume再replay、先replay再resume、重复replay、重复resume，在首个在途时提交第二个；同时向session B发起独立操作并乱序完成。结束A响应/error后由调用者显式再次请求A。取消通知到达/发出但原在途未结束时也尝试第二个。另把其他连接Strategy生成的list/replay/resume Request交给当前连接，包括相同协商版本/能力和旧已关闭连接的Strategy。

预期结果：A第二个冲突在登记RPC/分配有效关联及写入字节前拒绝，首个请求/pending保持可正常完成；B不因A冲突被阻挡。取消通知本身不提前释放在途窗口。合法响应/error结束相应窗口；调用者显式下一次操作可进入，但没有自动重试。跨连接scope拒绝且零写，不消耗当前请求的关联身份；错误连接恢复不复用旧Request。超时/协议错误导致整连接失效时所有窗口随连接结束，不承诺B继续可用。

## E. DuringReplay仅窗口来源，权限保持独立

操作步骤：A重放在途前、中、后注入A/B的消息、工具更新、未知结构和state_update；A重放窗口内交错实时活动通知及真实permission请求，并返回内容完全相同的不同条目。用普通resume对照DuringResume，逐个响应/error结束对应操作；保留原raw字节顺序。

预期结果：仅同目标重放窗口内更新标DuringReplay，B及窗口外不误标；普通resume保持DuringResume。该origin不证明每个通知都是历史、不能按文本/哈希去重或凭窗口把工具/权限当旧记录。permission仍独立Incoming，不自动回应/复用旧允许；真实请求响应按原scope/广告及一次性规则。不得把重放期间的idle快照变为新的Run完成事件。消费层仍须按来源/代际判断，协议客户端不伪造可靠历史条目/轮次ID。

## F. v2当前状态快照与完成前提

操作步骤：v2 replay与普通resume分别先发同目标当前state_update(running/idle/requires_action)再成功响应；正向重放序列为内容更新、当前state_update、response。对照无snapshot、只有其他session snapshot、请求前同session snapshot、只有工具/消息更新及响应之后才到snapshot。加入同目标未知state字符串和重复idle/带旧stopReason的snapshot；v1 load无v2 snapshot对照成功。

预期结果：v2必须在该目标当前在途窗口内、响应前观察当前state_update；缺失或错目标返回MissingSnapshot并关闭连接，不先返回读取成功再补状态。未知state原样保留供上层维持未知，不能当idle准入；重复snapshot幂等，不产生新完成/取消/用量。v1不错误套v2 snapshot前提。快照存在与完整原生历史/可靠前台终态不同；DuringReplay不提供比协议更多的“每条是否历史”判定，不能假设禁止当前快照后的所有实时更新来伪造边界。

## G. 失败、预算及零自动重发

操作步骤：list/replay/resume各自注入远端业务error、EOF、非法JSON/required字段、未知/重复响应、超deadline；标准重放逐帧输出大量内容并由调用者及时消费。新增操作回归32MiB帧/深度96/131072结构token、64未决关联及连接scope预算的代表性边界；检查写future取消及部分读取取消的既有安全语义。记录所有业务帧及initialize次数。

预期结果：错误不变成完成/完整历史，未知不重发；不为业务失败切低协议或再initialize，不自动发prompt/批准/delete/new作为兼容探针。远端error按原连接合同结束对应窗口；传输/解析/超时按原规则关闭、清关联，不复用旧请求。纯流式消费不在客户端累计整段历史；事件返回后的视图预算归消费层，不据此声称全平台内存有界。写中取消保持连接不再可安全发送，部分读取取消不丢已保留字节或多发业务请求，按现有运输合同重测而不重定义。敏感raw不进入Debug/普通错误。

## H. 新连接与证据分层

操作步骤：先一连接合法selected v1，保持存量连接，再创建preferred新连接选择v2；交换能力组合并检查旧Strategy/Request隔离。高版已选后使list/replay业务error或能力缺失，对照更高未知版本。记录规范/本地schema、夹具与当前协商分别属于何种证据。

预期结果：存量不热切，新连接重新优先v2，各自能力独立，不沿用旧低版缓存/报告；高版失败不降版重发，缺标准重放只给能力缺口，产品资源/旧命令/审批保护仍待应用接线验证。CL-037/041的规范依据核实不冒称真实Harness执行；不因某程序版本无旧报告建立白名单。私有桥接与降低准入未获决定时不自动实现；纯协议层没有UI/管理状态，不能以此签CL-038–040完整用户可观察行为。

## 交付与独立复核

此阶段不运行真实Harness、模型、网络依赖审计或Web检查，不涉及Node/SQL装配。未来实际资产/报告分别保留测试数量、逐稳定ID子分支、收发计数/零写证据、实现与固定schema摘要以及任务关闭结果。独立reviewer先复核本定义与§6A后编码；编码后的纯协议验收及交付diff检视各自独立。完整L5/L6仍保持未执行，不以本文件存在或原ACP开发测试代签。
