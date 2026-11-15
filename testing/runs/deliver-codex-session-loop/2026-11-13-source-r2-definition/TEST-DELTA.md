# Source R2 独立验收增量定义

2026-11-13。状态：定义完成、待 reviewer 方案/用例检视；不是产品编码放行、执行结果或 finding 关闭。只定义 Source discover/create，在同一 Node 的原 configuration、Native runtime、Registry owner、Source launcher、唯一 ACP receiver 上补服务。R1 schema 11 已有代码不等于独立 R1 已交付；R2 正式执行须先绑定 R1 独立结论及新固定源码清单。

## 依据、范围与有限退出门

- 行为依据：活动 change `specs/structured-history/spec.md` 的“基础原生会话发现与读取”“结构身份与来源顺序”“读取撤销”，`specs/harness-management/spec.md` 的“优先最高共同可用协议”“能力准入与证据状态分开”“不支持的标准操作直接报错”，`design.md` 第 1/3/4/6/7/8 节、`implementation-contract.md`；实施增量依据 `../2026-11-13-source-entry-rebase-v1/REBASE-PLAN.md` R2。
- 保留 CL-061/062/063/065/085 原 ID、标题和完整判据。下列 `R2-*` 是本次局部验收标识，不能勾完 CL-061 或 task 3.1/4.1/4.2。CL-061 的无执行绑定发现只覆盖其发现半段；CL-062/063 的历史重建/旧视图、CL-065 的按 ID 加载/审批和 CL-085 的真实 Harness/Web 均不由本批代签。
- 不包括 R3 认证 WSS 动作、页面 DTO/Web、无 target 的外部原生 ID 历史读取、rename/delete/configure；不扩成各阶段×各错误×各 actor 的全排列。发现新风险时先定位此表中的合同缺口，再经 reviewer 增量确认。
- 第一批仅 R2-V01/V02/V03，分别一条真实 child 纵向链。随后必须 R2-M01…M07。V01…V03 通过只是“最小链路可执行”，M01…M07 全部具备有效证据且阻断 finding 独立闭环后才能声明 R2 交付。
- 本次只读当前 Root 和旧候选，未执行 Cargo、list、测试或产品功能。当前 Root 未提交且共享工作区有大量其他修改；不以 HEAD 等同受测版本。输入与当前具体缺口见 `ENTRY-AND-OBSERVABILITY.md` 和 `INPUTS.json`。

## 共用执行边界与入口绑定

拟经生产 `NodeManagementSessionServices` 的 Source 服务门面进入：`register(client, selection, action)` → `execute(client, operationId)` → 原 receipt `wait()`；查询与核验分别 `query` / `reconcile`；原结果经 `open_result` / `release_result` 交给现有 Doc。名称是旧候选 ABI 参考，当前 Root 尚无这些 Source 门面；编码若改名须在执行清单绑定等价实际入口，不能以新 mock manager 替代实际 Node 接点。

实际 Windows Node、真实自有 SQLite、实际 WindowsSourceLauncher 子进程/管道、原 ACP transport/receiver 和生产 registry/Doc 所有权必须真实；child 为显式标注的可控 ACP 合同对端，不是真实 vendor Harness。服务身份取生产 bundle 和其原认证 socket 的 identity-bound client；不新增 R3 JSON action。数据根、端口和完整子进程树均为测试自有，阻塞标记配 RAII release 兜底，失败也清理；不访问用户数据库/原生会话。

实现后先由 tester 从实际 libtest list 绑定完整限定名，再使用 `--exact` 运行；每个选择器必须实际执行 1 个测试（分支数另外记录）。不得复用旧 v14S 裸名。使用仓库锁定工具链、`--locked --offline` 和独占 target；固定源码/测试 overlay/PE SHA-256，记录实际执行数、自然退出、Windows 树退出、必要原始证据摘要。不得以只编译/list、零测试、历史日志或检查启动当通过；本次定义不要求现在运行这些命令。

## 第一批：真实纵向三例

### R2-V01 同来源真实 discover，原 result 经 Doc 消费

- 用例级别：L4（真实 Node/SQLite/child/transport，ACP 对端是合同夹具）。
- 操作步骤：在自有授权 source/harness/profile revision/workspace/launch_path 中，仅登记 Discover；记录登记后零 child/零 session RPC。经原 socket execute，child 正常 initialize 并应答标准 `session/list`；其列表含一个外部既存 SID、未知/重复 key、转义、大整数原词法、数值 `1.2300e+09` 和不透明 `nextCursor`。等待原 receipt，读 query 小事实，再从 `open_result` 取 Doc descriptor，按生产 credit/ACK 流完整消费原 result；随后对同 operationId 再 execute/query，结束原 Node。另用该 opaque cursor 发一条新的 Discover 验证透传，第二请求是显式新操作而非自动翻页；child 第二页带新标题 canary，用来对照旧 snapshot不被就地伪改。
- 预期结果：首个操作仅一次 list、零 new/load/resume/prompt；实际协商版本/当前能力可追溯，不因缺历史报告挡住标准列表。列表 SID 不要求平台 target/input/run，零伪 target、ReadTargetProof、Command、Run。原 reserve 的 scope、typed RPC、Sessions kind 和 response/ACK 事实一致，outcome 仅在真实确认后 Succeeded。Doc 类型为 Source result，未伪装 target/revision；原 result 字节保真，未知字段/数字/顺序不静默降文本，cursor 原样往返。原响应正文/typed 列表不进 SQLite，库只保必要管理事实；重复 execute 返回同一 receipt，不补 list；消费/释放后 result 不凭元数据重新构造。最终生产 owner join 与树退出可观测。

### R2-V02 真实 create response 才产生原生身份

- 用例级别：L4（真实 Node/SQLite/child/transport，延迟窗口为合同夹具）。
- 操作步骤：在同一已配置来源登记 Create，经原 socket execute。child 收到 `session/new` 后发出 waiting 标记且持住响应；记录 Written、尚无 response/native SID。释放 child，使其用原 RPC 返回带未知字段的实际 `sessionId`。核原 receipt、SQL 小事实和 Doc 原 result；同 ID 重复 execute，再释放结果并关闭 Node。
- 预期结果：必要登记/Admit/Arm 未可靠提交前零 session/new；登记本身不启动 child。响应前不得报告创建成功或猜 SID；响应后 SID 仅来自原 scope+typed RPC+Created kind 的真实响应，ACK 与实际结果一致。new 请求使用选定 profile 的完整有效配置，MCP/额外根/显式空集语义保留，不改成“保持不变”。全程一次 new、零 list/load/resume/prompt、零伪 target/run；重复返回原 receipt。raw/typed 保真但不保存正文；结果与原 created claim 归原 source instance/connection。正常 close 必须 join 原工作而非只 seal facade。

### R2-V03 WrittenUnknown 与同 ID 重复绝不重发

- 用例级别：L4（真实 child 收到 new 后退出，不用预制 receipt）。
- 操作步骤：child 真实完整读取 `session/new` 并写 create-count=1，然后在返回响应前退出。等待原 transport/receipt，依次 query、重复 execute 同 operationId、显式 reconcile；记录每一步原 receipt 身份、原 SQL identity/knowledge、原连接事实及 child 调用数，最后关闭 Node。重启分支留 R2-M07，不在首批混入。
- 预期结果：必须实际证明 Written（accepted_length=frame_length），不能把更弱 transport Unknown 标成 Written。outcome=Unknown、response/native SID 不存在；不是创建成功、不是确定未执行、不产生替代会话/target。重复 execute 保留同 receipt，query/reconcile 零额外 new/list/load/resume，不能新握手/降版补发。原任务和 SQL 回执在 caller 等待消失后仍被 owner 保留；无法确认的 SQL/claim 仍保护，deadline 只返回预算失败，不假称已释放。

## 必须后续项：固定七组，不先铺全矩阵

这些用例是 R2 交付必须项；“第一批之后”不是可省略项。故障仅放在指定切点，不遍历所有 SQL role。

### R2-M01 受理权限、scope 与来源准入拒绝

- 用例级别：L3 + L4。
- 操作步骤：纯合同/编译负例验证不能凭任意 bytes、伪 scope/typed RPC/kind 构造有效 reply proof 或重复线性交接；实际 Node 分别传 foreign Node/socket client、撤销 actor、缺显式来源、不匹配 profile revision/workspace、错误同短 SID 来源。原 socket seal 在受理前先赢与 execute 已接管后 seal 两个顺序各做一例；同 Node 有权 actor B 作 query/read，并对照其不能冒登记 actor抢 execute。另只在 Admit INSERT、Arm UPDATE 两个必要 SQL 点各用真实 SQLite拒绝一次，记录原 SQL事实及 child/RPC计数。
- 预期结果：受理前拒绝零 child/零 session RPC；Admit失败零 child/new，Arm失败已有原source也必须零new；错误不混称 Harness unsupported。同 source+SID 共享原所有权，不同显式 source 不猜物理等价；B只能按当前授权范围 query/read，不能抢登记execute。Owned 先赢后原 actor 下线/撤销不取消已受理工作或派发再验旧 actor；所有新的 query/reconcile/读块按当前权限校验。fake proof/二次 handoff 被类型边界或实际原身份检验拒绝。

### R2-M02 Register / Query caller cancellation 与真实 no-SQL-start

- 用例级别：L4。
- 操作步骤：四个固定取消窗口：Register 原 Node task/receipt 安装后、SQL 进入前；Register 实际 SQL 进入后/报告前；Query 原 task/receipt 安装后、SQL 进入前；Query 实际 SQL 进入后/报告前。每次 cancel/drop 仅 caller future，持住 barrier 观察，再释放/join 原 worker。另在 task 调度受理之前拒绝一次 Register，取得同 invocation 的 `NeverScheduled` 事实；再对 Query 作同类拒绝一次。注册 ID 必须由 owner 在调用前/受理时发出可关联的非敏感观察 identity，不能因 caller 未收到 ID 丢失该操作。
- 预期结果：取消 caller 不丢已安装原 slot/receipt，不中断事实收束、不自动 execute/RPC，原 identity 不换。Register 进入后可能实际提交，取消不能声称回滚；实际提交的登记可被当前有权方核验。Query 只读，无修复/新 RPC；取消后其原读连接/worker 仍被 owner join。`NeverScheduled` 必须是调度前原 owner 判定并与拒绝 identity 绑定，配同 invocation/Source SQL role 的 SQL-start=0 和零该 role原连接/任务（不把受理前的合法 actor验权SQL算进去）；“inventory 空/observe None/Task Weak 升级失败”都不是 no-SQL-start 证据。

### R2-M03 原 SQL 三故障轴、query 不洗状态、reconcile 不重发

- 用例级别：L4（真实 SQLite + 原 connection owner）。
- 操作步骤：仅在 Create 的 Response 小事实写入选定 role 做三轴独立 fixture：A 真实 COMMIT 失败（不是仅预先返回错误）；B 真实 COMMIT 成功但报告丢失；C 原连接实际 close ACK 已发生、close报告不可确认且 app task/SQLite worker 尚未 join。保留原 task、typed write proof、SQL receipt/close receipt identity。各轴在 caller 返回后 query 两次，再显式 reconcile；C 在 join barrier 释放前后各观察一次，并取得原close task最终报告，不用第二条连接替代。以原 connection 的实际事务/关闭报告和数据库行共同判定，不用新 SELECT 猜原提交。
- 预期结果：query 是新只读操作，只返回已证明事实或原 Unknown/Err，不能凭 SELECT 成功把原事务/cleanup 改为确认，也不能隐式 reconcile。A 按真实 rollback 事实、B 按实际提交与报告丢失分别呈现；两者不得混称。reconcile 先 join 原 task，再确认原 receipt/原 close 及 worker，才以保存的 typed proof 修复必要管理事实，零 session RPC。C 未 join 时不得报告 cleanup confirmed/放 claim；join 后仅原 receipt 可以证明收束。没有 proof 不能从“库里缺行”补造响应、成功或重发。三轴的任务、回执与 SQL role 均可精确关联，`SSF-SQL-RECONCILIATION001` 仍由独立提出方复核关闭。

### R2-M04 不支持、远端错误与错误 reply 关联

- 用例级别：L3 + L4。
- 操作步骤：同一来源运行三个固定分支：未广告 list 但标准 new 可用；list 返回真实 JSON-RPC remote error（保留原 code/message/data）；在有效 reservation 上注入旧 scope/不同 typed RPC 或错误 response kind 的迟到响应。协商沿现有最高共同版本规则，不构造厂商白名单。
- 预期结果：无 list 时零 list IO，明确 Harness 能力限制，new 仍独立可用；平台未实现与 Harness 未支持不混淆。远端错误是失败事实和原 error result，不能伪成功/吞字段/降版重发。错关联响应不接入该 operation/SID/Doc，仍保留真实诊断和未知状态；不凭内容形状猜绑定。

### R2-M05 Source result provenance、逐块当前权限与最后析构

- 用例级别：L3 + L4。
- 操作步骤：V01 的真实 result owner 建两个当前有权读取者，A 首块后撤销；B 在无 credit 的等待期间，对同 ACP 发另一实际 Create 验证接收/控制继续。依次 socket seal、显式 release、最后 credit/ACK，并用原 disposal task identity 观察最后 raw/typed owner 的析构槽、原 epoch 撤销唤醒及最终 join。负例将结果交给 foreign Node/registry/Doc owner。
- 预期结果：读取每块当前权限，A 不再取得字节，B 不因 A 撤销失效；无 credit 不堵 ACP receiver/控制。foreign owner 和二次线性交接拒绝；小事实不拥有正文。seal/ACK/release 只退休本原结果，最后大型 payload 析构在 Node-owned blocking retire/disposal 槽完成且可 join，不在短 lifetime/actor 锁或接收循环析构。旧 Queue/Native Doc 绿色结果不能代签该新 producer。

### R2-M06 长列表 CPU 与部分失败析构不占接收循环

- 用例级别：L4。
- 操作步骤：仅一份 4097 条合法列表（反证旧条数截断，不定义新上限）加一份末条非法导致 partial Vec 的负例。用明确的 decode/Vec-growth/retire start barrier 持住实际 Node-owned 工作槽；在原接收循环送一个可立即处理的独立控制/响应，观察其进入；释放 barrier 后记录成功/失败和最后 raw/partial Vec 析构 task identity。
- 预期结果：不截断合法列表、不把工程边界伪称 ACP 限制；长解码、Vec 增长和失败 partial Vec/最终大对象析构均不在接收循环、短 lifetime/actor 锁。持住 CPU 槽时原控制仍能实际进入（用事件顺序判定，不只看耗时）。取消/停止仍保留 CPU 和 retire 原 task 供 close join；不得仅给未来加异步包装就声称卸载。移植前 coder 必须选定现有 owner 工作/retire 槽及观测 API，reviewer 检视。

### R2-M07 Source close/claim 与重启保护

- 用例级别：L4。
- 操作步骤：正常对照复用 V01/V02。故障只选三处：Source result 无 credit 等待、M03-C 的原 SQL join barrier、一个原 producer/CPU retire 未完成。各自触发 production Node stop/source close，观察 seal、IO stop、完整树退出、producer/SQL/close worker join、metadata 收束及 claim 释放的顺序；deadline 前后查原观察 identity，不二次创建假 closer。补一个跨 profile 的真实 Created同SID冲突，保留两次真实响应并观察原claim，禁止改SID/造替代会话。最后用自有已登记 WrittenUnknown/Interrupted 数据关闭后重开 Node，query/reconcile 同 ID。
- 预期结果：source close 等原 producer、IO、tasks、SQL、worker 和必要 metadata 实际收束后才放 claim；同 source+SID 在未确认时不能被新 owner 占用。截止只发布 pending/unconfirmed，保留原 task/receipt/claim 及后续 join/收束责任；不可因 source authority closed、空 inventory、新 SELECT 成功或新任务完成早放 claim。冲突两条原Created响应均保存小事实，第二条明确Ownership/Unknown，不抹原响应、不抢claim。无 credit reader 被原 epoch seal 唤醒，结果正文正常退休。重启保留必要原事实/unknown、不重发 session RPC、不重建已失 raw result；未确认保持保护。Windows 完整自有树退出与共享其他 source 不受影响分别给证据。

## 七组风险映射与交付记录

旧七组参考经旧 `source-session-recovery-acceptance-v1/program/PLAN.md`、`CORE-COVERAGE.md` 与独立 v4 review 实际读取；旧 `MINIMUM-MAPPING.md` 原件未在已定位目录找到，不能宣称逐字继承其完整覆盖。本批按当前 REBASE-PLAN 明确七组合同：1 身份/来源/同源（CL-061/056，V01/V02/M01/M07）；2 discover/raw/cursor（CL-061/095，V01/M06）；3 受理/取消/SQL-before-IO（CL-075，M01/M02/M03）；4 create/unknown/reconcile（CL-076/082，V02/V03/M03/M07）；5 unsupported/error/当前权限（CL-005/032/035–040/060，M01/M04/M05）；6 Doc provenance/非阻塞/retire（P2D-001/002/004–011、P2X-001/002，V01/M05/M06）；7 close/claim/restart（P2X-002、CL-077/068，M03/M07）。这些仅为旧稳定ID的Source局部关联，不改原ID定义。上述操作给定有限切点，不扩散全排列；reviewer 若发现旧原件另有本批必须义务，应作为具体增量指出。

R2 放行前 reviewer 逐项确认：首批三例真实边界；当前门面接线与唯一 receiver；M02 调度前观察证明；M03 原 SQL 三轴；M05/M06 新 producer/CPU/retire 责任；M07 close/claim；实际写集没有默默扩到 R3。测试实现和结果另存新运行目录，不回写本定义的 expected；缺入口记 blocked/not-run，不转 LX。Red Check 要实际使旧 Root 因服务缺失/关键断言失败，不能把一个无法发现的选择器或零测试作为 Red。
