# NA-010/030 最小运行时能力门：独立设计检视

日期：2026-10-30。结论：**PASS（设计范围，按下述边界实施）**。该方案可以满足当前 spec 及 NA-010/030 的负向授权断言，不需要新增公网 action、页面、真实 Harness 或审批 issuer。仅新增一个测试专用“始终拒绝”函数则不满足：必须有普通产品构建中的应用能力调用入口、真实管理资格校验和受门控的派发边界。

尚未实现或执行新场景，本报告不关闭 NA-010/030 的验收缺口，不改变整个 change 的 BLOCKED 状态，也不授权实际归档。

## 基线与已读依据

- closeout 清单 `testing/runs/establish-node-access/2026-10-30-closeout.sha256` 自身 SHA-256 独立重算为 `12AA4143865D984E49807CBAFA97542A57F7368DB8B4FB19C0517E5B9D9D1919`，**77/77 匹配**，0 mismatch、无重复路径。产品同 r3；plan 的追加已由 closeout 固定。
- 读取当前 AGENTS、OpenSpec config/reviewer 职责、client-trust 增量/长期规范、tests、design、合同及当前 TrustStore、Storage、NodeRuntime、管理认证/命令源码。保护已有未提交修改，仅写本报告，不改产品、规格、计划或用例。
- 规格 `specs/client-trust/spec.md:106-112` 要求管理身份与 Harness 操作审批分开；已认证身份不能充当操作批准证据，也不能复活失效审批。
- `tests.md:103-107` 的 NA-010 允许明确 mock 后续 Harness，在真实应用策略边界测试有效管理身份配合缺失/过期审批。`:223-227` 的 NA-030 要求验收可调用的策略入口；没有要求为此扩展现有网络协议或建立真实审批颁发能力。
- design 的领域/入口分离及本 change 非目标仍适用：只建立身份与访问边界，不运行 Harness 任务。重设计中的运行连接、会话/Run、请求绑定与裁决保存/交付是后续能力的依据；本次不能用一个到期时间戳冒充未来完整授权模型。

## 当前代码给出的实际集成位置

- `app/src/management_ws.rs:610-627` 已在真实客户端 proof 验证后解析 TrustStore 记录；`:895-935` 的私有 AuthenticatedClient 保存 client ID/public key/auth generation，并在 still_valid 中重新核对当前信任。
- `app/src/trust.rs:529` 可查当前记录；`:189` 的 client_gate 由撤销路径 `:315-316` 使用。不要把 TrustedClientRecord 的公开字段或客户端自报 clientId 直接视为已认证凭证。
- `app/src/node.rs:129-135` 已把同一 TrustStore 的 clone 注入管理服务，适合作为新应用策略服务的装配位置。`Storage::trust_store()` 当前为 crate 内 API；每次重新调用会构造新的协调器，不应为每次请求重建它而丢失共享的 client_gate。
- 现有管理命令只有 node.info/device.list/device.revoke，未知 action 拒绝；它们属于当前管理能力，不能被新运行时审批门误拦。无需把假能力塞进这个公网 action 集合。

## 最小 API 与模块建议

以下为接口建议，不是本次写入的实现；具体命名可调整，安全职责不变。

| 接口/对象 | 最小职责与可见性 |
| --- | --- |
| ApplicationPolicy / 应用访问策略服务 | 普通产品模块，例如 app/src/access_policy.rs；NodeRuntime 用现有同一 TrustStore clone 装配并持有，管理服务复用其身份资格校验，不能只有 cfg(test) 实现 |
| AuthenticatedActor | 不可由外部裸字段、反序列化或 clientId 构造的上下文；从现有已验客户端 proof + 当前 TrustStore 结果产生，保存节点作用域、ID、公钥及认证代数；crate 内构造且限制在认证/可信装配边界 |
| require_current_actor(actor) | 查询真实 TrustStore，比对 trusted/public key/generation/节点作用域；未知、撤销、不匹配或存储失败关闭入口；该校验不能仅返回从测试传入的 authenticated=true |
| request_runtime_capability(actor, request, dispatcher) | 真正的应用调用边界：先验证当前操作者，再检查独立审批依据，只有授权成功才可能进入 dispatcher。普通产品构建中可调用；不映射 HTTP/WSS/CLI 新命令 |
| RuntimeCapabilityRequest | 仅内部类型，明确该能力需要运行时审批。假能力名称是适配器夹具，不登记为已支持的真实工具；调用者不能靠 requiresApproval=false 绕过门 |
| ApprovalPresentation | 可选的内部“提交的审批资料”，不叫可信 Grant。若需要区分过期，包含可供产品代码计算的 expires_at；该字段本身无授权效力。不要提供 caller 可构造的 Approved/Valid 布尔值 |
| CapabilityDispatcher | 副作用边界，测试实现记录 dispatch 次数；门控服务持有或接收它，禁止测试在服务返回后自行决定“若允许则调用 spy” |
| 语义错误 | 区分身份无效、存储不可用、审批缺失、审批过期、审批不可验证/issuer 未提供；均无派发。对外无新 wire 错误协议，不泄漏 proof/密钥 |

### 没有 issuer 时如何拒绝

1. 有效管理身份 + 没有审批资料：返回 ApprovalRequired，dispatch=0。
2. 有效管理身份 + 已过期资料：由真实门控代码使用服务端/可注入时钟判断，expires_at <= now 即失效，返回 ApprovalExpired，dispatch=0。测试不能先自行判定 Expired 再把结论传给空函数。
3. 有效管理身份 + 未过期但没有可验证来源的资料：返回 ApprovalUnavailable/Unverified，dispatch=0。`expires_at > now` 只表示声称的时间未过，不表示有效授权。
4. 时钟错误、字段越界、无法解析或缺少必要依据：关闭入口；不回退成允许。

当前没有任何可颁发的可信批准，也不需要增加成功批准测试。禁止通过默认 Allow、总是返回 Valid 的测试 verifier 或公开 Permit 构造器制造成功流程。门内检查顺序与未来派发接口可以存在，但本 change 的运行时能力始终没有可用的批准来源，这应明确返回拒绝，而不是伪造已实现完整审批。

审批资料只用于这些负向边界，不建立生产 bearer token 格式、签发密钥、审批数据库或恢复规则。未来 issuer 接入时另做设计/验收，至少绑定节点、操作者、会话/Run/运行连接、请求与操作作用域，并处理撤销、消费和到期；不能沿用本次简单时间资料就放开。

## 防止成为死代码或测试自证

**NA-APR-D01（设计落实条件，Medium）：不得只新增无人调用的 deny helper。**

最小落实方式：将现有 AuthenticatedClient 的资格模型/校验提取或委托给应用策略服务，使真实 WSS 认证和命令访问继续使用同一服务；运行时请求是该服务的独立方法，产品 NodeRuntime 装配它，验收适配器可实际调用。调用链应为：真实 proof/信任解析 → 不可伪造的 actor 上下文 → 应用能力入口 → 独立审批检查 → 派发接口。传输层继续只暴露当前三个管理 action。

允许通过 crate 内独立集成测试或验收测试模块调用普通产品 API，而不增加公网入口。若采用内部测试模块，需把可执行目标和精确测试名登记，tester 独立实现/审阅适配器；不能因为方法不是 pub 就声称“没有入口”，也不能只引用源文件存在。

调用当前私有认证功能时，最小做法是让测试适配器走真实 PairingService 挑战与 AuthProof 校验，随后复用同一 actor 建立逻辑。有效身份用真实 SQLite/TrustStore 登记；测试不能仅构造 TrustedClientRecord 或按任意 ID 获取 actor。测试可明确 mock 后续能力 dispatcher，但不能 mock 掉所验策略、身份资格和审批判定。

**NA-APR-D02（设计落实条件，High）：审批资料不得由调用者自报为有效。**

即使真实 TrustStore 返回 trusted，也只能证明资格。ApprovalPresentation 不是授权令牌；未来到期时间、同名 ID 或管理会话都不能越过当前“无 issuer”拒绝。禁止将身份校验成功值直接复用为运行时 permit。

**NA-APR-D03（设计落实条件，Medium）：资格校验不能形成过期缓存权限。**

能力入口必须重新核对状态/公钥/认证代数；与现有撤销使用相同 client_gate，把资格校验与门控决定放在明确串行边界。不要先独立读取 trusted、等待任意异步工作后再把该快照当现行资格。当前所有审批都拒绝，因此无运行时派发竞态；以后放开成功路径时仍必须定义派发受理点，不在本轮假造该成功语义。避免门内外重复获取同一个不可重入锁而死锁。

这三条是方案实施所需的具体边界，不是对尚未写出的代码认定缺陷。按上述最小集成落实后，没有需要用户再次决策的设计阻断。

## 必要断言与证据

### NA-010：策略/时钟适配器

- 真实 TrustStore 中登记有效客户端，通过真实证明建立 actor，先证明它可通过管理资格校验。
- 同一 actor 请求需要审批的假能力：无资料明确拒绝；过期资料明确拒绝；未过期但无法验证来源的资料也拒绝；dispatcher 总调用数 0。
- 时钟覆盖 expires_at 前一刻、相等、后一刻。无 issuer 时三者都不能派发；相等及之后必须是过期，之前仍不可被当成已批准。
- 过期资料重复请求、管理重新认证、节点/存储重新打开后仍不能变成有效。不要用操作系统改钟；对门注入受控时钟，清楚区分进程内重建与真实重启证据。
- 未知/撤销 actor、ID/key/generation 不匹配，以及真实存储关闭/读取失败都拒绝，无派发；错误不得含密钥/proof。

### NA-030：可见应用调用入口

- 独立可执行测试确实调用 NodeRuntime 装配所用的应用服务和 runtime capability 方法，不是只直接测底层 reject 判断。
- 从真实认证路径获取 actor，调用无/过期审批两个必要场景；断言得到相应错误且没有回调、任务创建、工具调用或派发副作用。
- spy dispatcher 的直接独立正对照可证明计数器工作（例如直接调用测试 spy 后计数恰+1，再重置）；该对照不是“产品允许能力”证据。禁止造一个通行审批来满足对照。
- 正常管理 node.info/device.list/device.revoke 与订阅行为不被误纳入运行时审批；未知 action 仍拒绝，没有新增公网 action/页面。适用管理回归真实运行。
- 测试适配器的假能力与 fixture 明确标注，不宣称启动了 Harness 或已存在批准、裁决交付及恢复功能。

### 最小交付与关闭条件

1. 应用服务、身份上下文、实际 gate/dispatcher 边界在普通产品构建可达，测试入口可由 tester 独立执行；不为测试开放裸 SQL、原始私钥或伪造 actor 的生产接口。
2. Coder 附策略单元回归；tester 对固定新版本运行 NA-010/030 入口并保留真实/模拟边界、错误及 dispatch=0 证据。仅开发自测不能关闭旧 NOT-RUN。
3. 更新负责实现细节的 design/合同及实际测试入口记录，保留 specs 当前可观察语义；无需为这些内部实现细节扩展 wire 协议。用户已授权实现细节，主方可依本报告落实，不再请求重复确认。
4. 按变更范围运行 Rust fmt/Clippy/测试和受影响管理回归，建立新摘要后独立复查。不要将此前网络扫描阻断或其他 PARTIAL 随此能力门一并关闭。

## 最终设计结论

方案在“真实资格校验 + 独立 fail-closed 审批门 + 普通产品应用入口 + 受控假能力派发观察”范围内符合当前 spec/NA-010/030。没有 issuer 不妨碍验证缺失/过期必须拒绝，亦不应成为制造成功审批的理由。不能接受只测一份测试代码自己返回 false 的替代方案。报告仅完成设计检视；源码与验收状态保持原样，暂不归档。

## 实施中只读观察（非最终交付检视）

本节应用户授权并行读取正在实现的文件，不运行编译/测试，不据此宣布 NA-010/030 通过。closeout 是修改前基线，不能用于证明以下新增/变更文件；本节按读取时字节记录，允许后续开发继续变化，不将已授权编辑当作意外变动。

### 关键集成观察

- `app/src/access_policy.rs` 是普通产品模块。NodeRuntime 在 `node.rs:131-142` 使用原 TrustStore 的 clone 装配并注入管理服务，在 `:214` 暴露 in-process policy 句柄。WSS `management_ws.rs:617-629` 实际调用 policy.authenticate，`:911-923` 的 still_valid 委托 require_current_actor；不是仅供测试的孤立 deny helper。
- authenticate 复用 PairingService 的真实单次客户端证明消费，核对 proof 的 nodeId，之后查真实 TrustStore，核对 trusted 和公钥；actor 字段对外不可构造，包含节点/ID/key/generation。旧 resolve_client 的资格检查未被替换成 caller 自报布尔值。ID 非空路径按该 ID 查库，null 路径按 proof 的公钥查库，且最终比较公钥。
- runtime request 取得同一 TrustStore.client_gate，再读取当前 actor 资格，之后检查审批；现 require_current_actor 内不获取 client_gate，因此没有发现“外层锁 + 内层同锁”的递归等待。WSS writer 本来已持该 gate 时调用 still_valid，也保持此性质。今后如果给 require_current_actor 自行加锁，须同步拆分 locked/unlocked 校验入口，避免引入死锁。
- Missing/Expired/Unverified 三路都返回错误，dispatcher 位于 require_runtime_approval 成功之后；当前没有返回 Ok 的审批路径。future timestamp 和可注入 clock 都不能产生批准。只有这一负向含义可接受，不能由代码里的 dispatcher 接口声称已支持实际 Harness 执行。
- AccessPolicyError 的 Proof/InvalidActor/其他错误在 WSS 映射仍分别为原证明错误处理、4401 和内部失败；新增模块没有增加公网 action 或改变 node.info/device.list/device.revoke 的审批要求。错误的 source 链是否被日志展开、完整管理回归仍需最终测试。

### 资源引用检查

policy 保存 Weak<PairingService>，不是 Arc<NodeIdentity>/Arc<PairingService>；actor 只保存公开身份元数据，没有节点或客户端私钥。issue_authentication_challenge 的 Weak upgrade 在同步发挑战调用后结束；authenticate 读取 proof 的同步调用语句结束后才开始 TrustStore 的异步 DB 访问，源码未将升级的 Arc 跨该 DB await 持有。仅保留外部 policy 句柄不会通过该引用链长期持有节点私钥。

policy 仍持有 TrustStore/SqlitePool clone。NodeRuntime 关闭时显式调用 Storage.close → pool.close，而不是仅 drop 一个池句柄，所以“外部 clone 存在”本身不等于连接泄漏；必须用实际停机测试确认 pool 关闭、锁释放及错误返回。NodeRuntime 当前顺序是先停网络/连接，再 drop policy/pairing、关 storage、drop identity，最后收敛 IPC；IPC 在收敛期间还可能强持有 pairing。故仅验证 Weak 的结构不能代替关闭结果证据。

### NA-APR-I01：Medium / 实施收敛项 — in-process challenge 的取消所有者尚未暴露

- 读取时位置：`access_policy.rs:109-121` 的公开 issue_authentication_challenge；`management_ws.rs:900-907` 的原 AuthChallengeCleanup；`pairing.rs:699-703` 的 crate 内 discard_auth_challenge。
- 实际差异：WSS 已为连接作用域创建析构清理守卫；新 policy 只返回 IssuedChallenge，外部适配器没有相应的取消/释放接口。若调用者生成挑战后取消、尚未调用 authenticate 就退出，普通外部 API 无法立即清理自己拥有的挑战。
- 这是静态可见的资源归属缺口，尚未执行耗尽/取消复现；并非无限内存泄漏。现注册表上限 4096，签发时会 prune 过期记录，proof 解析/绑定失败已有 discard 路径。但不能把这些机制等同于“调用取消立即清理”。
- 最小建议：返回拥有作用域的 challenge lease（Drop 清理），或提供受限且明确由调用方负责的取消接口；不要让外部适配器借此任意清除其他 WSS 连接挑战。lease 应 Weak 引用 PairingService，或取消方法仅暂时升级 Weak，不能为修复清理而重新引入强持有节点密钥的链。WSS 可以保留现有 guard，无需扩展网络协议。
- 最终复查条件：生成后取消/丢弃使该挑战立即不可验证；错误 proof 只清自己作用域；成功证明只能消费一次；丢弃 in-process 挑战不影响其他连接。若主方采用“内部调用者必须显式清理”约定，需把可调用入口与 cleanup 实际接上，而非仅写注释。

### 待附资源与安全回归（不计已执行）

1. 外部持有 policy、with_clock 派生句柄和 actor 时执行真实 node stop，确认端口/pipe/目录锁可复用，关闭报告不被句柄存活拖成超时；停机后 challenge/authenticate 不能恢复节点服务。
2. 保留 policy 并关闭真实 DB 后 require_current_actor/runtime request 返回明确失败，dispatcher=0；不要重开数据库作为失败回退。停机开始后到最终关闭之间是否允许 in-process 新挑战，需明确生命周期策略，并避免在 pairing.clear 之后不断重新制造临时挑战；最终测试应覆盖这一窗口，不能只测停机完成后 Weak 已失效。
3. 让 authenticate 在 DB 读取/撤销边界暂停：真实撤销提交后，用先前 actor 请求能力须因资格失效拒绝；runtime gate 的锁已释放，不影响后续撤销或正常管理。当前所有审批拒绝，因此不宣称已覆盖未来授权成功时的派发竞态。
4. spy dispatcher 正对照及真实 gate 的 Missing/Expired/Unverified/clock failure/revoked/cross-node actor 断言；其中 fake 能力与可控 clock 是适配器，真实 proof/Store/gate 不替换。

除 I01 的待收敛接口外，本次未发现已验证的权限放行、循环引用或嵌套 client_gate 死锁；这是实现中静态观察，不构成无缺陷或交付通过结论。测试与入口说明仍由主方补齐后在新固定版本提交独立验收。

读取时摘要（实施中版本，不替代后续快照）：

```text
84171A64922D9C346503B45B86323F36735231E8886C76B5A9D8919F3157F181  app/src/access_policy.rs
3AB7D42F105712CC0707B45414D981F4EC979F6995ABE6D7365ABBCF03A50BB5  app/src/node.rs
EBC7EB0D839DE2450FAD4BCFB8DE6990A53772FF9DAB8858E9DD09E3AA05E00B  app/src/management_ws.rs
64B898B58B057F76146795A0A47E468A80ABA4BB5561A3AC7FECFEE269FACE58  app/src/lib.rs
```

本次只追加此报告，没有改源码、规格、任务、计划、清单或其他报告，没有运行影响用户 30447 节点及其身份的操作。
