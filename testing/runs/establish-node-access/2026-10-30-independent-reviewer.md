# establish-node-access 独立交付检视（2026-10-30）

当前状态（r3 独立复查）：**BLOCKED（整体证据门禁）**；本轮 R01/R02/R03/D01 均 Closed，r3 聚焦复查通过。R02/R03 已由执行真实 main.ts 的受控页面协调场景验证，不仅依据底层逻辑测试关闭。供应链和整体独立验收仍按实际未完成范围保留。以下最新 revision 优先于后文 r2/r1 历史。NA-033 用户确认保持通过，未扩展联网检视。

## r3 Revision：R02/R03 聚焦独立复查

### 版本

- 清单：`testing/runs/establish-node-access/2026-10-30-acceptance-r3.sha256`，77 项；独立计算自身 SHA-256 `351D086478979DFE2BDF4FE7B47B3B9B46FD67058BB786DCB9CE43877EA263A2`，逐文件 **77/77 匹配**，结束再核 0 mismatch。相对 r2 唯一变更文件为 `clients/web/src/main.ts`，新 SHA-256 `1284160FA218436368B2C7B02F0BA7D961EA11EFDF3CED271C01950F1AFEA6E0`。
- 未变的 R01/D01 修复保持 Closed；本次不重复扩展 Rust、依赖联网或整个 change 的验收。

### 实际页面协调回归

沿用 r2 的独立内存方法：读取实际 r3 main.ts 与 reconnect.ts，经现有 Vite transformWithOxc 转译，在 Node vm 中执行真实页面协调代码；使用受控 DOM、连接、存储 promise、时间及页面事件替身。入口仍执行原 boot、pagehide、pageshow、online 和实际恢复调度器，未直接调用一份复制的协调逻辑。所有脚本只在命令输入内存中运行，未新增或修改测试/产品文件。

共 9 个场景，**9 PASS / 0 FAIL，进程退出 0**：

| 场景 | 独立观察与断言 |
| --- | --- |
| 限流第 10 秒 pagehide/pageshow | 原 60000ms deadline 保留；10000ms 恢复后仍只有原 1 条历史连接；重复 online 仍只有 1 个 timer；59999ms 未连接，60000ms 恰新建 1 条，随后 timer 为 0 |
| 未验证 4401 后 BFcache/online | 保持 automaticReconnectAllowed=false；保留同一错误对象；推进至 120000ms 仍未新增连接 |
| node_mismatch 后 BFcache/online | 同样保持停止、原错误及零新连接 |
| host_proof_invalid 后 BFcache/online | 同样保持停止、原错误及零新连接 |
| invalid_frame 后 BFcache/online | 同样保持停止、原错误及零新连接 |
| 正常已认证连接 pagehide/pageshow | 恢复恰新建 1 条连接；重复 online 不再创建并行连接 |
| 初次 boot 初始化延迟期间 pagehide | 未新增连接或赋值 session；pageshow 后恢复原 consumed 身份并建 1 条连接 |
| reset 删除延迟期间 pagehide | 隐藏期间不继续初始化、不恢复 session；resetting 收敛为 false；pageshow 恢复 unbound 配对身份，不建管理连接 |
| reset 初始化延迟期间 pagehide | 隐藏期间不恢复 session、不新增连接；pageshow 恢复 unbound 配对身份，resetting=false |

方法限制：DOM、IndexedDB/连接为替身，这不是 Chromium 真正冻结/解冻进程和真实 IndexedDB 的端到端实测，也不证明所有浏览器存储故障组合；但它直接重跑并验证了 r2 已复现的真实 main.ts 控制流及相关延迟阶段。

### 代码与闭环

- `main.ts:105-109` 的 persisted pageshow 进入 `boot(true)`；读取完成后的 approved/consumed 分支先恢复 currentRecord，再检查恢复资格，允许时进入原 resumeManagement 调度器，不直接绕过 deadline。
- 终端错误时保留 lastManagementError，恢复页显示停止状态；initial boot 默认直接认证，正常新页面行为保持。reset 的 unbound 分支继续回配对，未触发已认证身份的自动管理连接。
- **NA-REV-030-R03：Closed**。r2 的两条已动态复现失败在 r3 均转为通过；没有发现新的确认失败。
- **NA-REV-030-R02：Closed（本次页面协调修复范围）**。原隐藏后继续工作风险与其 r2 BFcache 回归均已有相应 main.ts 动态验证；不再仅凭 IdentityVault 取消单测或静态判断保留为部分解决。
- reviewer 独立再次运行 Web typecheck、Vitest，7 文件/46 测试通过；git diff --check 通过。未改源码、清单、主报告、plan/tasks 或用户节点。

### 当前结论

本次 R02/R03 聚焦复查 **PASS**；R01、D01 保持 Closed。整个 change 仍按本报告已记录的供应链扫描及最终逐例证据门禁为 BLOCKED，不把聚焦通过扩展为可归档。网络门禁未重新尝试或扩大。

## r2 Revision：独立复查（2026-10-30）

### 版本与执行证据

- 新清单：`testing/runs/establish-node-access/2026-10-30-acceptance-r2.sha256`，自身 SHA-256 `409A743B2E725B43B05C3CF644F1506812D51294C902F9C3A2E612D6FF733506`。独立校验 77 个唯一条目，**77/77 匹配**；结束前再校验仍 0 mismatch。旧清单未改。
- 独立运行 Web typecheck 和 Vitest，均通过：7 文件、46 测试。新增 R01 测试实际覆盖早 4401/4410、challenge 验证中关闭、错误 hostProof 后 4410、合法 hostProof 后拒绝；初始化取消测试覆盖读取后取消不 save。
- 已读取主方全量原始日志 `.tmp/acceptance-r2-check.log`，SHA-256 `43D777453C257C2A69E43011444113166ED156389DCF7D941E088B0216F4E52D`，内含 Web 46、Rust 118 库+4 向量通过及静态检查。该日志是主方执行证据，未标作 reviewer 亲自重跑 Rust 全量。
- reviewer 额外动态执行实际 r2 `main.ts` 和 `reconnect.ts`：使用现有 Vite `transformWithOxc` 在内存转译、Node vm 执行，注入受控 DOM、页面事件、时间、连接/存储替身及可延迟 promise；未改源码或写测试文件。它验证页面协调代码，超出仅测试调度器/IdentityVault 的纯逻辑测试，但不等同于真实 Chromium、真实 IndexedDB 或真实 BFcache 验收。
- 第一版临时 harness 在完成两条恢复观察后因自身延迟钩子初始化顺序错误退出 1，不计整套通过；已更正为独立的取消 harness 与两场景恢复 harness，后两次均退出 0。下面输出来自后两次成功执行，未把 harness 自身错误记成产品失败。

### R01：Closed（本次固定版本修复范围）

- `management/client.ts` 的 `ManagementConnectionError.nodeVerified` 默认为 false；仅当 `verifyChallenge` 和客户端签名成功、连接仍处于 verifying_challenge 后设置 `#challenge`，关闭错误才据此标记已验证节点。已有本地 `#failure` 优先保留，后续 4401/4410 不再覆盖节点/协议证明失败。
- `main.ts:865-872`、`:901-916` 仅对 nodeVerified 的拒绝/撤销进入删除身份路径。未验证关闭停止重试并保留记录。上述真实连接层新增测试均由 reviewer 独立重跑通过；页面协调 harness 对未验证 4401 观察到不触发删除，对已验证 4410 执行 reset。
- 结论：原“未经证明关闭码可丢失 pin”风险可在代码与受控回归范围关闭。真实节点拒绝后新 key 配对/批准的集成流程仍由 tester 记录；此关闭不代签该浏览器全链路。

### R02：Partially resolved（原取消路径已通过；生命周期整体受 R03 阻断）

- `main.ts:135-157` 的 boot 与 `:927-977` 的 reset 引入 identityController、串行 identityWork 和每次 await 后的取消检查；`openManagement:480` 检查 pageActive/manual/resetting；reset 立即清 session、替换旧 UI，防止重复 reset。IdentityVault initialize 在 load/generate 后及写入前检查 signal。
- 实际 main 协调 harness 已执行：延迟初次初始化后 pagehide、延迟 reset 删除后 pagehide、延迟 reset 初始化后 pagehide。隐藏期间均未新增连接、未恢复 session、未保留 resetting；pageshow 后原 approved 身份仅建一条连接，reset 后恢复 unbound 配对页。

```text
boot_HIDDEN sockets=0 initBefore=1 initAfter=1 pageActive=false session=absent
boot_RESUMED sockets=1 session.status=consumed
reset-delete_HIDDEN sockets=1 initBefore=1 initAfter=1 resetting=false session=absent
reset-delete_RESUMED sockets=1 session.status=unbound resetting=false
reset-init_HIDDEN sockets=1 initBefore=2 initAfter=2 resetting=false session=absent
reset-init_RESUMED sockets=1 session.status=unbound resetting=false
```

- 以上 sockets=1 在 reset 两例中为离开前已存在的原连接，不是隐藏期间新建连接。存储/连接为受控替身，不能据此声称所有真实 IndexedDB 提交阶段取消均已测。
- 原 R02 的“离开后继续开连接/渲染”路径在这些时序下已修复；但新 pageshow→boot 路由引入下述 R03，因此暂不宣布页面生命周期整体关闭。

### NA-REV-030-R03：Medium / Open — BFcache 重 boot 绕过重连策略并复活终端错误（已受控动态复现）

位置：`clients/web/src/main.ts:94-109`、`:135-177`、`:476-493`；关联 `clients/web/src/management/reconnect.ts` 的 deadline 和停止分类。

根因：pagehide 无条件 abort identityController；pageshow(persisted) 因 signal.aborted 总是 boot。boot 恢复 approved/consumed 记录后直接 openManagement，没有经过 reconnect.resume，也没有遵守 automaticReconnectAllowed。openManagement 只检查 active/manual/resetting。成功订阅后又把 automaticReconnectAllowed 设为 true。

受控动态复现（执行真实 main/reconnect，其他服务为上述替身）：

1. 初始认证建立 1 条连接；关闭码 4429 安排到 60000ms 重连。在时间 10000ms 派发 pagehide，然后 pageshow({persisted:true}) 并完成 IndexedDB 读取。观察已创建第 2 条连接，timer 为 0；应仍只有 1 条历史连接并等待剩余 50000ms。
2. 初始认证建立 1 条连接；收到 nodeVerified=false 的 4401，确认 automaticReconnectAllowed=false 且身份未删。随后同样 pagehide/pageshow。观察第 2 条连接被建立，并在成功后把 allowed 改为 true；终端错误应保持停止。真正 node/protocol 错误也走同一 boot 路由，仍需回归覆盖。

```json
{"scenario":"rate-limit-bfcache","timeMs":10000,"connections":2,"requiredConnections":1,"timerCount":0,"allowedAfter":true,"reproduced":true}
{"scenario":"terminal-bfcache","timeMs":10000,"connections":2,"requiredConnections":1,"timerCount":0,"allowedAfter":true,"reproduced":true}
```

这不是仅凭代码推断，两个观察均在独立内存协调 harness 中重复执行且对应预期断言确认；未宣称在真实 Chromium 中复现。第一条破坏至少 60 秒限流窗口，第二条违反 BFcache 只恢复临时错误及终端错误停止要求；不声称重新连接已绕过节点认证。

最小修复：区分 BFcache 恢复与显式重新加载；恢复身份后仍经统一 reconnect 调度，保留原 deadline 与终端错误停止资格。不要让恢复用 boot 直接进入忽略 allowed 的 openManagement。对 reset 后新的 unbound 身份继续显示配对页，后续合法配对/认证可显式建立新恢复资格。

关闭条件：持久化页面协调回归，覆盖限流第 10 秒 pagehide/pageshow 仍等到 60 秒且仅重连一次；未验证 4401、node_mismatch、host_proof_invalid、协议错误后 pagehide/pageshow 不连接；正常连接隐藏/恢复恰一条认证连接；reset 中止后的 pageshow 仍可正常配对。补上述修复后同时复查 R02 和历史 NA-RC-001 的页面恢复保证。

### D01：Closed

- 按当前 OpenSpec config 对实际文本核对，未运行或采信旧 verify-suite.mjs 的 34 例/12 字段门禁。
- 当前 change 33 例；长期 node-access 11 例、client-trust 22 例。逐个比较 ID、标题及用例正文，与当前 tests **33/33 一致**，没有重复、缺例或正文差异。
- traceability 分配为 11+22；删除旧安全汇总当前 ID，主观理解为当前 NA-033；追踪表未保留旧 NA-034 条目。历史编号解释可保留，不要求恢复旧技术人工签署。

### r2 阶段交付结论

R01/D01 Closed，R02 原路径修复已接受但整体 Partially resolved；**R03 仍为产品行为阻断**。总状态仍 BLOCKED。供应链扫描和完整 tester 证据沿用其实际未完成边界，不因本次 46 项单测替代；NA-033 用户确认保持通过。

本 revision 仅更新本报告；未改产品、规格、suite、plan/tasks、清单或主报告，未启动/停止用户 30447 节点。后文为 r1 审查历史，旧风险状态不覆盖本节最新结论。

## r1 历史记录

## 固定范围与独立性

- 工作目录：D:/Project/acp-remote。清单 `.tmp/2026-10-30-acceptance.sha256` 自身 SHA-256 为 `27AAB89FA0092439FB9FC16754C21113921620166C179263F481F8D4C7B40932`；独立逐文件校验 **69/69 匹配**，0 mismatch。结论绑定这些字节，不覆盖后续修复。
- 读取当前 AGENTS、OpenSpec config/reviewer 职责、change proposal、design、两份增量规格、合同、tests、plan、tasks、review、两份长期规格/套件、追踪表，以及本轮用户确认、开发报告和执行中的独立 tester 报告。未继承 coder/tester 会话结论。
- 本轮只写本报告。未修改源码、规格、计划、任务、主报告或清单；未操作用户 30447 节点、身份、密钥或日常浏览器，未提交/推送/归档。
- 69 项清单不是完整工程配置清单；AGENTS、CI、Gitleaks 配置、toolchain、.nvmrc、DEPENDENCIES 及存储探针 Cargo.toml/lock 不在其中。下文单列已读资产摘要，避免把未固定的配置默认为由 69 项摘要证明。

## 源码风险与已核对的文档问题

### NA-REV-030-R01：High / 待复现风险 — 未证明节点身份的关闭码可能删除私钥和节点 pin

位置：`clients/web/src/management/client.ts:468-475`；`clients/web/src/main.ts:839-845`、`:892-927`。依据：access-contract §1/§4 及 client-trust 的目标节点核实要求；身份拒绝可重新配对，但节点身份不匹配/协议校验失败必须保留绑定和错误。

连接层对任何阶段的 4401/4410 都构造身份拒绝/撤销错误，且这两个远端关闭码优先覆盖本地失败。它没有给页面提供“已通过原节点 hostProof 验证”的事实。页面在初次 connect() 拒绝后同样进入 showManagementFailure，看到 4401 或 revoked 即执行 resetLocalIdentity。该流程删除整个 IndexedDB identity 记录，并生成 nodeId/nodePublicKey 均为空的新身份。

建议复现（未执行）：预置 consumed 身份及原节点 pin；连接开启后不发送 challenge，直接关闭 4401（4410 同理）。源码显示 connect() 拒绝可经 main catch 调用 reset，无需先成功验签。需实际观察原 pin/私钥是否被删除，不能把该观察写成已完成。另一条待测路径是在验证替代节点挑战失败、客户端已开始协议关闭时，对端返回 4401/4410；连接层的优先规则可能掩盖原节点证明失败。本 reviewer 未执行上述 fake socket/页面协调或真实浏览器攻击复现。

独立判断：清理行为本身已有用户授权；风险在于没有把该授权限定为已验证绑定节点的拒绝。如果上述路径成立，就违反“节点替换不覆盖”的边界，影响为删除本地身份和信任锚、允许后续重新绑定。本报告不声称已经观察到该结果或攻击者已取得管理权限，重新配对仍需本机批准。

最小修复范围：连接层显式记录并传递已验证绑定节点的状态（建议只在 hostProof 验证完成且未取消后建立），将可信拒绝/撤销与未验证关闭分开；main 仅对已验证绑定节点的拒绝清理。已有节点/protocol 失败不能被后续关闭码覆盖。未验证的 4401/4410 停止重试并保留原记录及错误。无需修改 Rust 正常协议或放宽身份校验。

关闭条件：fake socket 和页面/存储协调回归分别覆盖 challenge 前 4401/4410、替代 key/无效 hostProof 后关闭、挑战验证中关闭，断言 delete/save 新 key 均不发生且 pin 不变；合法原节点证明后的拒绝/撤销仍按用户要求生成新身份回配对，不自动授权；真实浏览器补一轮撤销后重新批准。

### NA-REV-030-R02：Medium / 待复现风险 — 页面离开可能未覆盖启动和自动清理的异步所有权

位置：`clients/web/src/main.ts:86-93`、`:132-158`、`:457-474`、`:900-929`。

pagehide 会取消当前 timer/controller、断开已知 management 并增加 managementGeneration。但 boot() 等待 vault.initialize() 后没有检查页面活跃状态或启动时 generation；openManagement() 入口也没有 pageActive 检查。若 IndexedDB 读取在 pagehide 后才完成，boot 仍能调用 openManagement，后者在 pagehide 之后重新增加 generation 并创建新 WSS。原先的 generation 失效逻辑对此无效，因为新连接捕获的是离开后的 generation。

自动清理也跨越 `await vault.reset()` 和 `await vault.initialize()`，没有页面/操作所有权检查。pagehide 发生在这些 await 期间，旧任务仍会初始化新 key、赋值全局 session 并 showPairForm。页面留在 BFcache 时可观察该路径；不能用“普通关闭通常终止脚本”证明取消完整。reset 正在执行时旧 dashboard 也未立即禁用，旧事件/旧异步 UI 结果缺少统一的 reset 操作栅栏。

建议复现（未执行）：用受控 promise 延迟 initialize，先派发 pagehide，再返回已有 approved/consumed 记录，检查是否构造 WebSocket。另在 reset/initialize await 前后派发 pagehide，检查旧任务是否继续赋值并渲染。当前只核对了缺少检查的源码位置，没有实际运行这些事件时序；新连接或状态覆盖均不记为已观察结果。

最小修复范围：main 中为 boot/reset 建立生命周期/操作 generation；每个跨 await 的有副作用步骤核对归属。openManagement 最前面统一检查 pageActive/主动断开及操作归属，避免直接调用绕过 resume 的栅栏；reset 及时禁用旧 UI，旧任务不得覆盖新 session 或离开后启动认证。若 IndexedDB 已提交删除不可撤回，应明确完成当前事务与后续生成/渲染的边界，并让 BFcache pageshow 恢复到可用状态，不能只加 return 留下永久空 session。

关闭条件：延迟 boot、reset 删除、密钥生成/保存各阶段触发 pagehide，断言没有新 WSS、没有过期 UI/session 覆盖；pageshow 恢复恰一条有效流程；主动断开、online 与多个 close/旧请求结果交错时不复活旧连接或产生并行 reset。

### NA-REV-030-D01：Medium / Open — 长期用例仍使用旧编号和旧人工职责

位置：`testing/suites/node-access/tests.md` 的 Stable IDs/旧 NA-033；`testing/suites/client-trust/tests.md` 的旧 NA-034；`testing/suites/traceability/establish-node-access.md` 的能力分配与对应行。

当前 change tests 33 个唯一 ID；长期套件仍 34 个唯一 ID，把旧安全汇总 NA-033 分配给 node-access，并保留旧可理解性 NA-034 的技术操作/人员签署要求。不能以长期结构检查仍通过证明归档同步完成。

最小修复/关闭条件：主方按当前职责同步长期 suites 与追踪表，将安全检查关联 NA-002/012/014，当前 NA-033 仅关联用户理解确认，并保留旧→新编号解释；逐项核对与当前 tests 的步骤/预期，运行实际适用检查。该问题是归档资产同步阻断，不要求重新引入旧人工签署。

## 新版用例与历史证据的有效性

- 当前 tests 共 33 例，保留了预算/时钟/随机源、COMMIT/响应丢失、撤销栅栏、Windows 跨用户/远程 pipe、15 秒期限、浏览器异常存储和审批隔离等必要断言。精简是字段/职责去重，不等于这些断言已通过或获豁免。两个长期规格从首个 Requirement 起与各自增量正文一致。
- NA-033：读取 `2026-10-30-user-acceptance.md`，用户原话“NA-033 已经验证通过了，现在可以归档了吗”；接受其主观理解通过，旧 NA-034 对应当前 NA-033。原 NA-REV-B02 的技术人工签署阻断已被当前规则取代，不再开启；客观证据缺口继续由独立 Agent 承担。
- 不继承旧 4/25/5。当前逐例状态以同日 tester 报告的实际断言映射为准；读取时该报告为执行中，未形成最终逐例通过结论。
- 独立比较本轮与旧 71 项快照的共同路径：本轮清单内 26 个 app 文件与旧摘要相同；main、management/client、styles、management.test、合同、client-trust 增量、tests 及长期 client-trust 用例已变化。reconnect 代码/测试是旧 71 项之外的新增资产。未据共同文件不变推定嵌入的新 Web 构建产物相同。
- NA-REV-001/002/003/004 的负责 Rust 文件和关闭逻辑仍在，可保留代码问题 Closed；旧 Rust/存储/跨 SID/L6 证据仅复用其未变字节和相同范围。NA-RC-001/002 原修复仍在（deadline、failure 原因），保持其原修复范围 Closed；最新自动删除 pin 行为是 R01 的新增范围。旧 P01 已修复，不恢复该问题。
- NA-REV-N01 端口 0 就绪报告问题仍在 `app/src/node.rs:193`，Low、正式显式非零端口路径非阻断。没有据此触碰用户节点。
- NA-010/030 当前仍明确要求后续审批的测试适配器；源码 lib 模块和当前公开测试入口未发现该能力入口，tester 亦记录未找到。保留未执行/受阻，由主方建立明确入口或按用户授权调整范围，不能将无真实 Harness 误写成该策略断言通过。

## 本轮已执行、证据和限制

- reviewer 独立运行 Web typecheck、Vitest：7 文件/38 测试通过；OpenSpec strict、git diff --check 通过。没有以这 38 项替代 main 页面生命周期/自动清理验收，现测试未覆盖 R01/R02。
- 已读同日 integrated-development：隔离 target 的全量检查/构建、118 库+4 向量及根/Web audit 0 漏洞为主方实际记录，非 reviewer 独立执行。沙箱 audit DNS 失败后真实网络成功，故不列为最终 audit 失败。
- 同日 tester 报告读取时执行中，仅接受已经记录的 69/69 校验和环境事实；构建启动或旧报告数量不计作本轮验收通过。不频繁轮询，也未要求用户重复技术验收。
- 主方本轮 Gitleaks 8.30.1 官方 zip 60 秒零字节下载超时；cargo-audit 0.22.2 安装长时间无进展已取消。**扫描未执行**，不是扫描通过、用户拒绝或已发现漏洞。reviewer 未重复安装；供应链/完整秘密扫描阻断仍保留。

## 独立依赖与安全边界检查

- `cargo metadata --no-deps --locked --offline` 可读取当前 29 个普通直接依赖、3 个开发依赖及 windows-sys 目标依赖；锁定结果包括 p256 0.14.0、rustls 0.23.46、ring 0.17.14、SQLx 0.8.6、libsqlite3-sys 0.30.1、Tokio 1.54.1、rcgen 0.14.10、tungstenite 0.29.0。Cargo.toml 的兼容版本下界不当作最终解析版本。
- 已对直接 Rust 依赖本地上游 Cargo.toml 逐一读取许可声明，均为 MIT/Apache-2.0/ISC/BSD-3-Clause 及其组合；ring 的许可是 Apache-2.0 AND ISC，读取其 LICENSE 后确认还有 BoringSSL/once_cell 许可说明，不能一律简写为可选其一。此项不是完整传递依赖再分发许可清单审计。
- Web 已安装 TypeScript 7.0.2 为 Apache-2.0、Vite 8.3.4 和 Vitest 5.0.5 为 MIT；Node 22.20.0 满足其 package.json engines。Web 无新增运行时框架/第三方资源依赖。
- 上游核对：[RustCrypto P-256 README](https://github.com/RustCrypto/elliptic-curves/tree/master/p256)、[rustls README](https://github.com/rustls/rustls)、[SQLx](https://github.com/transact-rs/sqlx)、[Microsoft windows-rs](https://github.com/microsoft/windows-rs)、[axum](https://github.com/tokio-rs/axum)。维护来源可追踪，并不证明每一锁定版本无漏洞。SQLx 上游 SQLite 路径包含 C/unsafe 边界，不宣称全栈纯 Rust。
- P-256 本地 0.14.0 README 与上游均明确提示其曲线实现未独立审计、常量时间未全面评估。这是需披露的上游风险，不凭此直接认定当前产品有可利用漏洞，也不将 RustCrypto 项目归属写成第三方密码审计通过。
- 当前 rustls 显式选择 ring；SQLx 关闭其他数据库/宏默认功能、使用 SQLite；Win32 调用集中平台模块。已读 owner SID/DACL、reject_remote_clients、模拟后显式 revert、CurrentUser DPAPI 和临时秘密清零实现；实际不同 SID/远程 pipe/错误注入覆盖仍须对应 tester 证据，源代码分支存在不等于平台实测通过。
- 全量 `cargo metadata --locked --offline` 因缓存缺 zerocopy-derive 0.8.63 未完成，改用 no-deps 和本地缓存包声明完成上述直接依赖核对；没有为此联网安装，不据 partial 元数据宣布全传递图安全。
- CI secrets job 有固定 action commit、Gitleaks 8.30.1 和默认规则扩展；allowlist 针对历史精确测试值/路径，没有给当前产品目录整体豁免。CI 定义不是执行证据；本报告不宣布全历史/熵/二进制秘密扫描通过。

## 额外读取资产与证据摘要

以下摘要不修改产品清单，仅固定本报告已读输入。

```text
1019D8854ACA68D2519E599D0F3A1484E49667273708A49AC03818834AA38BE3  AGENTS.md
F546052BB6B5D6213A2657EF68FDBC59FD43A0D3B3E719FE5CEF9544FC7CA5BE  .github/workflows/ci.yml
C473773F48BE9E31AB47DAF40372F4771185410E888DB9F08D4852F25341520E  .gitleaks.toml
3913C4A1AF198D0F924CDD3197B8B4BBC6F48CAFB423170596137E0784923FF4  rust-toolchain.toml
12D3A4EFA6646B3ECE4782F70033B9785BF0D167B553C43E22579B031CEA5C4D  .nvmrc
7F04F14E02B9027CC2F05957213503121623A14AB761F35E868B2F42D0B93561  app/DEPENDENCIES.md
8BBDE7F5E93511A953B37CA209A0C50C533B3661FF738C43C405D026FEDA92AF  testing/probes/establish-node-access-storage/Cargo.toml
78C72125BAE6D05A8EE1D914CC07106618C0BE802243FA462FF0350B02E02680  testing/probes/establish-node-access-storage/Cargo.lock
376EBBDC0D096444CB85AE01A30B561D18DE3E9DE918ED13A8657F79453617C2  testing/runs/establish-node-access/2026-10-30-independent-tester.md（读取时执行中）
FDB98A886AB7236ABA5DC2133A477D9BD95E23E6347FD15D2F7C629C5A3A8F4F  testing/runs/establish-node-access/2026-10-30-integrated-development.md
48140549F96023FFD3E42E5E2E2F4A98EBD3155B4CFE1176E5049BCFB713FD0B  testing/runs/establish-node-access/2026-10-30-user-acceptance.md
```

## 交付结论与重验边界

本轮 **BLOCKED**：D01 归档同步、Gitleaks/Rust advisory 未执行，以及尚未最终形成的独立逐例证据；另有 R01/R02 需要定向复现或通过修复及回归消除的源码风险。当前没有新增“已经实际复现”的产品缺陷结论。用户理解确认已接受；不以旧安全人工签署继续阻断。

R01 最小修复只涉及 Web 连接层拒绝来源标记/错误优先级、页面清理前提及回归；R02 涉及页面异步操作所有权与取消/恢复。修复后建立新摘要，由本 reviewer 对新字节关闭问题；优先重验 NA-024/025/031、身份拒绝再配对和页面生命周期，不无条件重跑未变 Rust 全部场景。D01 由主方在解冻文档后同步。新增测试与 tester 最终报告按实际摘要复核，不把当前报告自动延伸为新版本通过。
