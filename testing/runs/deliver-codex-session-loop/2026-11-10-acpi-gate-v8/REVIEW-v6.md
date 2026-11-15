# ACPI v6 独立编码门审查

**结论：BLOCK。当前材料不足以放行 ACPI v6 的 Rust 认证 WSS 上传路由编码。**

v6 已在候选层补上 `design.md` 增量，并把服务原子安装范围扩展为队列读取、队列管理和上传三项，方向合理。但认证错误来源在现有接口中被合并、验收覆盖仍有未决定事项，且缺少执行 reviewer 职责所需的活动规划和底层证据，不能给出 PASS。

本次仅依据消息内提供的文件正文进行静态审查，未调用工具、修改文件、委派、运行构建或测试。**产品测试未运行；本文不证明编译通过、真实 WSS 可用或产品验收完成。** 下列摘要均为输入标注的 SHA-256，未重新计算，也未核实其与磁盘、Git 工作区或实际文件字节的一致性。“v6”指候选包版本；候选 ACPI 帧的版本字段仍为 `1`。

## 审查范围与已确认事项

此次审查对象是编码前方案及用例，不是已经实现的 ACPI 路由交付。当前源码中上传命令仍返回 `platform_unimplemented`、Binary 仍进入共享视图控制路径，属于本次拟实现的基线，**不单独作为候选缺陷**。

静态核对确认：

- `lifecycle.rs` 先安装会话服务，再构造路由并启动 HTTPS 网络任务。因此，在该位置一次发布完整服务集合，有实际接线位置依据。
- 当前 `ManagementState` 确有独立的 `queue_reads`、`queue_management` 两个 `OnceLock`。v6 用一个私有字段、由原 `OwnedNodeSessionServices` 构造的集合取代它们，能够避免新增上传服务后继续分步发布。
- `NodeInputUploads` 只保存 owner 的弱引用；由 attachment 持有 canonical `UploadOwner` 的强引用，可使四命令、Binary 和队列消费查找到同一连接所有权。不能仅在每条命令中临时 `open`。
- `PendingReads` 是由原认证循环轮询的 futures 集合；四条命令进入该集合，不需要新增脱离连接的任务。
- 现有出站路径具有单一队列、字节许可及唯一 writer。新增 ACK Binary 类型复用该路径，在结构上可行。
- `SocketLifetime::seal` 会通知注册的上传 owner 清理条目；writer 与 protocol 的实际任务结果由 `connection_owner` 监督。连接封存、准备工作结束、任务实际 join 必须继续分别取证。

以上均为源码结构判断，不是动态验证结果。

## 阻断问题

### ACPI-V6-001 — P1：编码门所需的规划、版本绑定和关键实现证据不完整

**状态：OPEN；目标版本：本次 ACPI v6 快照。**

**位置与依据：**

- `openspec/role/reviewer.md` 的“独立性与依据”“方案与用例检视”。
- `openspec/config.yaml` 的 plan/tests/tasks 规则及 apply 前置要求。
- v6 `CONTRACT.md` 的“完整输入与优先关系”“准确写集与串行顺序”。

本次未提供以下必要输入，不能推测其内容：

| 缺少的材料 | 无法完成的核对 |
| --- | --- |
| 候选目录的 `SOURCE.json`、`MANIFEST.json` | 候选包成员是否完整、实际源码与候选绑定关系，以及历史文件复制关系 |
| 活动 change 的 `proposal.md` | 原始成功判据、Exploration 决策依据、术语与 ADR 适用性，以及 capability 登记是否与现有内容冲突 |
| 活动 change 的 `plan.md` | WP4 实际写范围、串行依赖、独立 tester/reviewer 安排、Main E2E 和同版验证安排 |
| 活动 change 的完整 `tests.md` | ACPI 增量与 CL-001—123 是否冲突、重复或遗漏，既有验收预期是否受到影响 |
| `docs/HARNESS_CONTROL_PLANE_REDESIGN.md`，以及本次确需引用的 `harness-management`、`structured-history` 规格正文 | 新路由与既有产品方向、错误边界、共享视图行为的直接一致性 |
| `app/src/node/queue_management/development_tests.rs` | 测试候选依赖的 `Case`、`body/body_for`、认证连接、目标和清理安排的真实定义 |
| `app/src/node/prompt_preparation.rs` 及其实际 preparation、test hook、shutdown port 实现 | `CpuEntered`、Registered、原任务身份及真实 join 观察是否确实可用，能否在允许写集内接入 |
| `app/src/application/shared_views/socket_transfer.rs`、`wire.rs` 的相关实现 | R05 合法 ACPV 控制的构造、状态观察及“不改变 Shared 语义”的核对 |
| `app/src/node/request_authority.rs`、`app/src/access_policy.rs` 的相关实现 | 上传内部认证错误的完整来源及取消边界；见 ACPI-V6-002 |

此外，没有基线差异或工作区修改清单，无法核对已有未提交修改和删除项。本文因此不能确认所列写集不会覆盖其他工作。

历史追溯还有一个明确缺口：tester 参考报告引用的 reviewer 报告摘要为：

`B72C7B8EA6E91218D9D620C690E8A457AFFAF6312D4AB01C6CB92AD540C019F9`

而本次提供的 `REVIEW-v4-reference.md` 摘要为：

`C7AD87A6AF88A9E168D11E7F4623065FCA0D2AF1C77C63ACEB212D458A8D2037`

这不证明任一报告有误，但不能将二者视为同一报告。需要提供前者原文或说明两份报告各自的版本和责任。

**关闭条件：**补齐上述必要材料，提供可审查的活动文档集成增量和版本绑定；复核实际工作包、测试安排及原始实现证据。不能仅以文件路径、tasks 勾选或合同中的作者陈述替代。

### ACPI-V6-002 — P1：现有上传接口丢失认证错误来源，不能直接兑现候选关闭规则

**状态：OPEN；目标版本：本次 ACPI v6 快照。**

**位置：**

- `app/src/node/input_upload.rs`：`Owner::current`、`Owner::target_guard`、`registry_error`、`view_error`。
- `app/src/application/shared_views/socket_authority.rs`：`current_actor_guard`、`policy_error`。
- v6 `CONTRACT.md`：“文本错误直接使用 `UploadError::code`”。
- `spec-candidate.md`：“文本错误映射稳定”“二进制错误和关闭优先级明确”。

候选要求区分：

1. 上传业务必要存储错误：文本 `necessary_storage_unavailable`，Binary 4400。
2. 管理认证复核的存储/内部错误：沿既有 4500 关闭。
3. 撤销：4410。

但当前调用链会合并这些来源：

- `SocketViewAuthority::current_actor_guard` 中，认证策略的其他错误经 `policy_error` 成为 `ViewError::ReadFailure`。
- `Owner::target_guard` 再经 `view_error` 将该错误映射为 `UploadError::InvalidOwner`。
- `UploadError::InvalidOwner.code()` 返回 `invalid_request`。
- 另一条 `Owner::current` 路径也会将多类 `RegistryError` 合并为 `UploadError::StorageUnavailable`。

因此，路由只拿到 `UploadError` 后，无法可靠判断它来自上传状态还是上传调用内部的认证复核。

**可复现的故障时序：**

外层认证检查成功；owner 内部随后进行的认证存储读取暂时失败；该错误被压缩成 `InvalidOwner`；结果入队前认证复核又恢复成功。按照“直接使用 `UploadError::code`”，客户端会收到 `invalid_request`，而不是候选规定的认证故障关闭。仅在 owner 调用前再做一次认证不能消除这个窗口。

也不能在外层持有同一个 actor gate 后调用现有 owner：owner 内部还会取得该 gate，会引入自等待风险。

**影响：**这是错误来源和安全边界的接口缺口，不能靠给上传路由增加 `match` 完整解决。当前写集又禁止修改 upload 算法，尚未冻结能够满足规则的适配方式。

**关闭条件：**明确并审查保留认证错误来源的接口或调用合同；如需调整 owner 的错误传播，将最小变更明确纳入写集。新增确定性验证，分别证明上传业务失败、认证存储失败、撤销不会串码。不能用故障后再次查询成功来改写原失败来源。

### ACPI-V6-003 — P1：测试覆盖和适用层级尚未冻结，不能声明方案/用例检视通过

**状态：OPEN；目标版本：本次 ACPI v6 快照。**

**位置：**`tests-candidate.md` 的共用安排、R01、R04、R06；v6 `CONTRACT.md` 的“证据与待审事项”；历史 tester 报告的限制第 1、2 项。

存在以下具体缺口：

- **错误映射覆盖未作决定。** v6 明确保留 `allocation_failed`、`necessary_storage_unavailable` 没有确定性 WSS 注入的缺口，但没有确定由哪一层证明、哪些原始证据可复用，或采用何种最小注入。历史 tester 给出的是可选处理方式，不是已完成的覆盖决定。ACPI-V6-002 也使这里不能只验证枚举到字符串的静态映射。
- **首次安装失败的启动行为未明确验证。** R01 测试重复/foreign 安装拒绝和原服务继续可用，主要证明已有集合不被替换；没有明确操作步骤证明正常产品的首次完整集合安装失败时，启动失败且不开放 HTTPS 产品入口。
- **control-only 覆盖不足。** 规格要求四条上传命令均返回 `product_not_ready`，R01 的 control-only 操作只发送 `begin`。
- **断线后旧身份的拒绝缺少明确操作。** 用例预期写了新连接不能复用旧 uploadId，但应明确关闭 A、重新认证新连接、对旧 ID 执行查询及数据尝试的步骤，不能只用同时存活的 A/B 隔离代替。
- **v6 三服务集合的回归范围未落实。** R01 应证明集合安装后既有队列读取、队列管理仍取得原服务，失败安装也不破坏这两条路径，不能只检查上传和一个读取结果。

**关闭条件：**由独立 tester 明确适用层级、补齐步骤或提供可核验的同范围证据引用，形成固定版本，再由 reviewer 复核。无需在编码前提前运行尚未实现的产品测试；阻断原因是**验证方案尚未闭合**，不是本轮没有运行产品测试。

## 需同步处理的具体问题

### ACPI-V6-004 — P2：R08 的目标错误预期需要消歧

**位置：**`tests-candidate.md`，R08 操作步骤。

“以不存在/不可解析 target(99) begin 验证 invalid_target”混合了两种情况：

- 格式合法但不存在、无法解析到真实目标：`invalid_target`。
- 标识字符串本身无法通过语法解析：`invalid_request`。

`session_protocol.rs` 已明确先进行 ID 语法校验。历史 tester 报告也明确作了这个区分，但当前候选正文仍保持原措辞，且 v6 只允许机械调整引用和导语。

**修复建议：**由 tester 将两种输入和预期分别写明，并同步允许这项有依据的验收定义修订，避免 coder 自行选择解释。

### ACPI-V6-005 — P2：现有开发回归包含与新行为直接相反的断言

**位置：**`app/src/node/session_services/queue_read_development_tests.rs`，`queue_read_valid_unimplemented_and_invalid_payload_are_distinct`。

该测试对合法 `input.upload.begin` 明确断言 `platform_unimplemented`。路由实现后，该断言必须调整；仅迁移 bundle 安装测试不够。

**修复建议：**保留“合法但未实现”与“非法 payload”的区分意图，使用本阶段仍未实现的合法 action 测试前者，并为上传 begin 建立成功断言。该文件在允许写集内，这是可预见的回归同步工作，不要求扩大产品范围。**本次没有执行该测试，未声称观察到测试失败。**

## Requirement / Scenario 覆盖核对

候选规格共八项 Requirement、十三个 Scenario。下表仅表示定义关联，不表示执行通过。

| Requirement | 对应候选用例 | 静态审查结果 |
| --- | --- | --- |
| 上传服务原子可用 | R01 | 重复/foreign 安装有安排；首次安装失败、三服务回归及四命令 control-only 覆盖需补齐 |
| 连接持有唯一上传所有权 | R04、R06 | 双连接隔离和结束清理有安排；断线重连使用旧 ID 的步骤需明确 |
| 四条上传命令使用稳定 JSON | R01、R03、R04 | 阶段投影、finish、释放及重复释放有定义；完整 parser 测试正文未提供 |
| 文本错误映射稳定 | R01、R03、R04、R06、R08 | 正文失败等有定义；错误来源和两类故障覆盖未闭合 |
| ACPI 数据帧与 ACK 使用唯一出站顺序 | R01、R02、R07 | 接收、末块幂等、FIFO、queued/in-flight 区分及字节预算模型合理 |
| 二进制错误和关闭优先级明确 | R02、R05、R06 | stop-before-dispatch 时序明确；实际 ACPV 与新增观察接口尚缺直接证据 |
| 撤销先阻断出站再完成生命周期封存 | R08 | gate 前暂停、提交撤销、再放行的顺序合理；原连接 seal 和实际 join 仍待实现取证 |
| 异步命令取消不产生无主任务 | R06 | before/after-call 与 finish 交接区别明确；原 preparation hook、身份及 join 接口未能直接核验 |

R06 中记录 `unknown` 可以如实表达未确认事实，**不能把正常清理场景中的 unknown 当成实际 join 已通过**。预期故障场景与正常退出成功场景应分别判定。

## 历史问题与本版结论的关系

- **v4 的 design 写集缺口：候选层已补齐。** v6 明确包含 `DESIGN-DELTA.md`、插入位置和合同链接维护。这项技术修订可以确认，但不证明活动 `design.md` 已更新。
- **v4 的 R06/R07/R08 时序修订：本版保留，静态上仍合理。** 尤其是 stop 先观察再 dispatch、queued 与 in-flight 分计、writer gate 前撤销，避免了原有测试自锁或调度猜测。
- **历史 tester 的有条件接收：仅按其原范围引用。** 不升级为 v6 无条件接收，更不代表动态验收。
- **本文不代替原 reviewer 关闭其问题，也不修改历史报告。** tasks 中既有完成项及历史结果路径不作为本次 ACPI 路由的通过证据。

## 已审阅输入摘要

以下覆盖本次提供并审阅的全部 29 份文件；未提供文件不列作“已审阅”。

### 仓库规则与活动文档

| 文件 | 输入标注 SHA-256 |
| --- | --- |
| `AGENTS.md` | `1019D8854ACA68D2519E599D0F3A1484E49667273708A49AC03818834AA38BE3` |
| `openspec/config.yaml` | `2838C774A014E389BE5EC702BD2F293F1686012D7D8F6C01FB500A8C21980A2F` |
| `openspec/role/reviewer.md` | `D36CD77FB71906FA3A6052C1FEBEA0F62535FEC450D9343B5648369D68320E31` |
| `openspec/changes/deliver-codex-session-loop/design.md` | `5B603C0C5BE394BEF2BB98DD79014C9D62F7EE108B8DE8542CE3C86C8AA0F335` |
| `openspec/changes/deliver-codex-session-loop/implementation-contract.md` | `CAC9593CE63B3B3841DD571813492CA1E730F4E3881EF5E80A407B0FF172D28E` |
| `openspec/changes/deliver-codex-session-loop/tasks.md` | `6333015C998DAAA1098BF00F0C9FFD5CE1571F80086427CDE2861976373BC142` |
| `openspec/changes/deliver-codex-session-loop/specs/session-execution/spec.md` | `45CB72683BFDCCD91444190DA27D076028969043B0A74FEA499CC9C927BC4287` |
| `openspec/changes/deliver-codex-session-loop/specs/acp-platform-constraints/spec.md` | `250238DC3A0E938F24FB999366F7E8AC218E41869F207E71671573B827A2D123` |

### 产品源码与开发测试

| 文件 | 输入标注 SHA-256 |
| --- | --- |
| `app/src/management_ws.rs` | `C8E83CE1C69F0AB45BDA28218A823C15225755E76CEC428B4F347E0B3EB80789` |
| `app/src/management_ws/session_commands.rs` | `E7BD2395823F1CC77ED67453C0822737BC1C5C1B5359064168641887FC2C3328` |
| `app/src/management_ws/session_protocol.rs` | `D3F6EA0847C305267911D176972E2A47EAB31B251FC1EA3AB8C68637026DF699` |
| `app/src/management_ws/input_upload.rs` | `B62A78C1BFB5BF8EB035017C49BF735A42400B9137A0D016730EFBAE6E12B5F0` |
| `app/src/management_ws/connection_owner.rs` | `C9F84DCFD0B809E23673278A4D4D24D8A6C0FA1D6F137F531905E4347F4D6913` |
| `app/src/node.rs` | `03B135C78615B840DCE409D4F505ED842D51E8E6E187ABB7665AFBEA0A3EDBF3` |
| `app/src/node/session_services.rs` | `AD6B27D320CDB034C82D2910DC8883486DB229AFA6487E4C0593B3EE7C1837F8` |
| `app/src/node/lifecycle.rs` | `69D5E790F2BDFB0446B7FE0F80F48701B2829D91F972D2A172B2DC29E398E67A` |
| `app/src/node/input_upload.rs` | `E7E93F69DEFE8B35A1568153FA0674B60E61F130BBDE663829FD0D604C1AF06A` |
| `app/src/node/queue_management.rs` | `956C83263FC0D95C6BD39C1FA8A3F0C00828619BFCC5FF3BC4B36583B253A79C` |
| `app/src/node/session_services/queue_read_development_tests.rs` | `FB445EF247900B8E41AEB24156DFFBFD111C1E11485FBCCE95E757277692E7C5` |
| `app/src/node/session_services/queue_reads.rs` | `3A451F3F6C96836BBD3E4AF4E5A5059CF41B868BB73B32665EC6514CFCFCBE53` |
| `app/src/application/shared_views/socket_authority.rs` | `787EBB8C47059D8CCEF58D7C11E587F8E24484B28BAB9AF64AE0F3B8008BACBB` |

### ACPI v6 候选包

本表路径均相对于：

`D:\Project\acp-remote\testing\runs\deliver-codex-session-loop\2026-11-10-acpi-gate-v6\`

| 文件 | 输入标注 SHA-256 |
| --- | --- |
| `CONTRACT-v4-reference.md` | `6BD506F56EC4B5AE7BA318E75C004C96463D9B692CF42F8998ECE6BA6B502C69` |
| `CONTRACT.md` | `B5F19D96990F0274C7D01515913F57A1F34F3FA314B76C7A13302B3988ACD952` |
| `DESIGN-DELTA.md` | `7A9685B242220214E6CF7082F22F9F524B0004B7C98A4F7BEA989274307118C1` |
| `REVIEW-TASK.md` | `C9ECC2B75108AEF518D94AA8CF59AE725C1E94AAFA58A925FFADE60478F42D35` |
| `REVIEW-v4-reference.md` | `C7AD87A6AF88A9E168D11E7F4623065FCA0D2AF1C77C63ACEB212D458A8D2037` |
| `spec-candidate.md` | `E450C05ED0FE24D87A31264137441A3EF597CD0FB9E40234B13A162EAC39976A` |
| `TEST-DEFINITION-REVIEW-reference.md` | `87C76AF705D25DB41CB8A659A51A93B082DEC34B5D20238FDCB76E43E69E2E7A` |
| `tests-candidate.md` | `88C97333952D738F60D1FC44626D4755184E0FE61A2A7B08E962EAFC8AB6C773` |

**编码门状态保持 BLOCK：ACPI-V6-001—003 未关闭。** 后续应先补齐审查输入、冻结认证错误传播方案和测试覆盖决定，再对新摘要复查。Web UI、最终独立验收、真实 Harness 验证及归档均不在本次放行范围内。