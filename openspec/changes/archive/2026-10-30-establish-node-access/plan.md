## Execution Model

当前已实现节点、配对、认证、撤销、关闭和同源Web，独立验收发现的问题均已闭环。R20开发全量144项Rust库、4项签名向量、50项Web测试通过；正式独立33例全部PASS，原reviewer已关闭最后NA012环境阻塞，当前15/15任务完成，处于待归档状态，尚未执行归档。最新状态见本文件Verification Results首节，后续日期节保留历史含义。未提交工作按每包源码清单、普通二进制和资源固定，HEAD本身不足以标识版本；保护用户现有修改。

主执行者统筹，coder、独立tester、独立reviewer按 openspec/role 分工。当前tester/reviewer使用不继承开发对话的独立Agent；正式实现也遵循该隔离。直接修改提交可读diff，不要求为了串行工作制造patch。禁止未经用户要求提交、推送或发布。

## Parallel Assessment

当前WP5并行分为原tester的配对/事务/管理、平台tester的浏览器存储及A/B、协议tester的预算/IPC、侧tester的供应链/秘密/并发认领，以及WP6独立reviewer。各自只在独占probe/runtime/target和报告文件写入；产品源码只由主方串行修复。端口分别为30451/52、30453/54、30457/58、30455/56，用户30447不参与测试。冻结副本保存在`.tmp/frozen-implementation-rN-20261030`，各包83项逐文件校验，生成dist与二进制单列；历史清单不覆写，修复后定向复测，避免把变化中的工作树冒称同一版本。

WP1完成工程和合同边界后，WP2节点内核与WP3 Web可在不同隔离工作区并行；接口只读共同合同，不同时改共享配置。WP4集成串行合入并重跑两侧检查。WP5 tester与WP6 reviewer对同一固定集成版本可并行，reviewer不能以tester自评代替检视。

当前开发侧已按 WP1-WP4 实现并集成，角色写范围保持互斥；WP5 tester 与 WP6 reviewer 必须读取同一重新生成的固定快照，不能继承 coder 对话。正式并行 Rust 构建使用独立 `CARGO_TARGET_DIR`。

## Work Packages

| ID | Goal / Scenarios | Dependencies | Agent | Write Scope | Deliverable | Verify |
| --- | --- | --- | --- | --- | --- | --- |
| WP1 | 工程入口与依赖/协议边界 | 独立方案/用例检视通过 | coder | app/Cargo.toml、app/Cargo.lock、app/src/lib.rs、clients/web/package.json、clients/web/package-lock.json、clients/web/tsconfig.json、.github/workflows/ci.yml、README.md | 可编译工程、真实检查入口和依赖审查记录 | NA-001、NA-002；README登记命令实际运行并验证非零有效检查 |
| WP2 | Windows节点、配对、认证、撤销与关闭 | WP1 | coder | app/src/（除lib.rs）、app/tests/、本change实施记录 | 领域、网关、SQLite、DPAPI/IPC和开发单元测试 | NA-003–017、NA-021–030，真实Windows控制入口及可控故障操作 |
| WP3 | Web配对与可信客户端管理 | WP1 | coder | clients/web/src/、clients/web/tests/、clients/web/index.html、clients/web构建配置 | WebCrypto/IndexedDB配对及管理界面和开发测试 | NA-018–029、NA-031；正常浏览器真实操作，不能用协议模拟替代 |
| WP4 | 组合装配、同包静态资源与运维文档 | WP2、WP3 | coder | app/src/、clients/web/、README.md、.github/workflows/ci.yml | 集成diff、真实构建检查入口与使用文档 | NA-001、NA-002、NA-015、NA-032；重跑WP2/WP3所有受影响检查 |
| WP5 | 独立验收及安全/人工证据 | WP4 | tester | app/tests/acceptance/、clients/web/tests/acceptance/、testing/runs/establish-node-access/、testing/suites/、tests.md | 可重复验收资产、逐例报告与长期用例同步 | NA-001–033；真实Windows+受信TLS+Web，记录全量not-run/fail/pass |
| WP6 | 独立交付检视与问题闭环 | WP4，正式通过依赖WP5证据 | reviewer | 本change/review.md、plan.md结果节 | 同版本diff检视、问题关闭及可读摘要 | 逐项核对spec场景、合同、用例、实际结果，无阻断或必需未执行 |

## Merge Order

| Order | Work Package | Reason | Re-verify After Merge |
| --- | --- | --- | --- |
| 1 | WP1 | 首次建立真实工程与检查 | NA-001、NA-002 |
| 2 | WP2、WP3 | 独立目录可并行，合同变动先串行协调 | 各自单元/组件检查，合同变化双方重验 |
| 3 | WP4 | 与前包重叠必须串行集成 | 全部适用Rust/Web检查及NA-032 |
| 4 | WP5、WP6 | WP5验收目录与WP2/3属于嵌套路径，必须在开发停止写入后执行 | 固定集成版本的适用全量检查，修复后独立重测/复查 |

## Project Verify

| Check ID | Command / Working Directory | Scope / Pass Criteria |
| --- | --- | --- |
| PV1 | 仓库根：openspec.cmd validate establish-node-access --strict；git diff --check；tester执行NA-001/002 | 当前可运行，仅文档结构/差异；不能替代产品验证 |
| PV2 | WP1按app与clients/web真实配置建立编译、格式、静态和类型检查并登记README；tester执行NA-001/002 | 当前入口已存在；WP1判据仍是命令真实可执行且有效覆盖，当前通过只证明开发检查，不替代独立验收 |
| PV3 | tester按tests.md操作步骤执行NA-003–031；独立Windows环境 | 协议探针/故障入口已有开发实现，但正式逐例执行受独立环境、可信证书和审批额度限制，未执行项不得写成通过 |
| PV4 | Windows真实受信TLS及桌面Chromium，NA-032–033 | NA-032真实闭环有实际证据；NA-033仅用户主观可理解性确认；安全与依赖检查见NA-002/012/014及独立交付检视 |

实际证据位置testing/runs/establish-node-access/；清理只针对本次目录、端口、profile、进程。不得删除实际用户密钥、原生会话或数据库。最终检查针对待交付集成结果，不能只测各分支。

## Code Review

| Work Package | Reviewer Agent | Review Focus | Pass Condition |
| --- | --- | --- | --- |
| WP1–WP4编码前 | 独立reviewer | 方案、精确场景覆盖、依赖、所有权、独立用例 | review.md无未闭环设计阻断，计划含真实入口建立任务 |
| WP1–WP6交付时 | 独立reviewer | 当前集成diff、安全、实际结果、秘密与平台证据 | 同版本必需检查通过，reviewer关闭自己的问题；自查不能代签 |

## Main E2E

```yaml
mode: required
```

NA-032使用真实浏览器或桌面工具与真实节点；安全与依赖检查归NA-002/012/014及独立交付检视，用户不承担技术验收或签署；NA-033（原NA-034）仅用户本人的提示可理解性判断保留LX。缺Windows、可信证书、工具或权限记受阻/未执行，不自动转人工，不改为mock通过。移动端不在范围。

## Risks

当前无未闭环的必要验收或产品检视阻断。跨SID剩余命令已用用户授权的现有账户补齐，R20批准后中断、多标签迟到写入及R18迟到拒绝恢复均由原独立角色闭环。证据仍保留明确边界：远程pipe为真实回环SMB正反控制，不冒称另一机器；RustSec为固定公告快照，不能保证未来无漏洞；后续Harness为明确适配器，不冒充真实集成。结构检查不代表行为通过，不静默删除或放宽必要场景。

## Handoff

交接用户确认（Windows+Web；device pair→输入码→本机批准）、AGENTS/config/角色、proposal/specs/design/access-contract/tests、实际快照及资源归属。tester/reviewer不继承coder对话；同一固定版本独立工作。

## Findings

| ID | Source | Severity | Target Version | Finding | Fix Version | Status | Retest / Re-review |
| --- | --- | --- | --- | --- | --- | --- | --- |
| NA-SEC-01–08、NA-TEST-01 | 独立reviewer | 原设计阻断 | 初稿及首次接入合同 | 节点引导、proof字段、Windows身份、撤销、Web密钥、预算及分页用例 | 当前合同/用例修订 | 已由独立reviewer复查关闭，逐项依据见review.md | 设计关闭，产品实测仍未执行 |

## Verification Results

### 2026-10-30 R20 跨SID补验收口（最新）

- 用户授权使用现有abc账户，主方仅负责真实凭据登录启动，密码不写入探针或验收资产；原独立平台tester负责夹具、断言、后验和报告。报告`2026-10-30-cross-sid-r20-acceptance.md`：实际SID尾1006，与owner尾1002不同，固定R20普通exe摘要不变，两命令pair/stop均真实TTY、exit1、无超时，在私有数据目录解析阶段os5拒绝。未将此误写为到达服务端token比较，后者按既有独立子项复用。
- abc后owner同节点status/list正常，全部业务表有序内容逻辑摘要及审计不变；owner stop正常，节点/端口/pipe释放、锁独占重开。初次final记录watchdog仍存在，后续两次缺席证据追加保存，不反写原记录；用户既有账户、资料和体验节点未修改。NA012原五命令及其他有限平台断言由报告给出逐项复用依据，当前必要动态缺口已补齐。
- 原primary tester已核读并更新唯一表`2026-10-30-current-independent-status.md`为33 PASS / 0 PARTIAL / 0 FAIL / 0 NOT-RUN。原reviewer在`2026-10-30-cross-sid-final-review.md`独立核对17项证据、真实SID/固定binary/owner正控制/后续清理及复用链，明确NA012 PASS、最后环境阻塞Closed，产品范围PASS；review.md当前索引同步。
- 据原NA009/011/017与新NA012独立PASS完成2.1，独立全例及检视满足4.2/4.3，现15/15。最终文档与验收资产另列`2026-10-30-ready-metadata.sha256`，由独立`2026-10-30-ready-metadata-review.md`有限读回；旧R20与旧metadata清单不回写。完整产品检查仍为R20的144库+4向量/Web50，之后仅新增验收探针/报告/元数据；最终定义、strict和差异格式及秘密扫描单列，不冒称为新的产品全套执行。
- 当前目标为待归档，尚未归档、提交或推送。既有abc账户及用户体验节点保留；仅本次测试资源收敛，未删除任何用户资料。

### 2026-10-30 R20 当前检查点（历史，跨SID资源补齐前）

- 正式源码83项清单SHA256 `73727BCF617F3F96BCC24C4CBB7B38BA044A56856C6CB4D628869832C8B368A4`，冻结`.tmp/frozen-implementation-r20-20261030`逐条83/83；普通嵌入单包exe SHA256 `561A6024E983DD5239C09D1388CA5D256625F690EE9345E64B9654BC43AAFBE3`。完整`npm run check`与普通构建均退出0，144库+4向量、Web50、格式/Clippy/类型/构建通过；日志为implementation-r20-check/build。
- 原tester已在`2026-10-30-web-evidence-merge.md`核读正式嵌入R16/R17链及R18定向、存储和生命周期证据，NA013/026/031/032均PASS。合并下述R20原提出者复测关闭后为32 PASS / 1 PARTIAL（NA012）/ 0 FAIL，独立统一表由原tester更新。原NA-E2E-001由提出tester关闭，迟到新身份与普通profile全退出持久性均有真实浏览器证据。
- 原NA007-CANCEL-001为已提交批准但IPC结果未交付时真实Ctrl-C退出且无revoke指引。R20修复final-event等待的取消分支，先给诚实条件指引、关闭配对流，再以总5秒owner list查询确认同指纹trusted ID；未知不虚报取消或批准。原reviewer源码/helper限定PASS；原tester在`2026-10-30-approval-cancel-r20-retest.md`以固定R20普通exe真实TTY完成control/precommit/committed三分支、实际owner查询及同key重启恢复，退出均0，原问题已Closed。旧R17 FAIL原样保留，不改写历史。
- NA012仍仅缺另一真实Windows SID的pair/stop必要命令。当前jack两个执行器均非管理员，现有资源不足；已请求可用的不同账户交互终端/测试环境，不要求密码或用户手工执行验收，也不因缺环境转LX。
- 任务3.2、4.1由tester合并报告明确确认可完成，主方已勾选；R20原问题实际闭环后，原tester亦确认2.3的NA005/007/018–026及3.1的NA018–026判据满足。独立reviewer在`2026-10-30-final-metadata-review.md`确认4.4结果/闭环/长期定义和编号映射同步条件PASS，主方据此勾选，现12/15。仅剩2.1/4.2/4.3受NA012环境阻塞；未归档、提交、推送或替换用户体验节点。
- 最终文档及验收资产清单单列`2026-10-30-final-metadata.sha256`，不回写旧R20源码83项；清单自身及其独立读回报告不入条目，避免自引用。末次完整检查仍为R20的144库+4向量/Web50；之后只改文档/摘要，长期定义、OpenSpec strict及差异检查再次通过。秘密扫描按快照时点记录，旧扫描不外推新字节。

### 2026-10-30 R19 检查点（历史）

- R19源码83项清单SHA256 `298EA007D4E1270E15D37DE93FC680A76A1121D452ECF2B96AD8F2E60B1B32C1`，冻结目录`.tmp/frozen-implementation-r19-20261030`逐字83/83；其`bin/acp-remote.exe`为普通debug-embed单包，SHA256 `377C780CB3FFEBB9936BA131DC46955A2EF3E455134DA56DE1E49805786C38A7`。后续计划和勾选元数据不倒写旧清单。
- 实际全量`npm run check`及普通构建退出0，144项Rust库、4项JWS、Web7文件50项；日志`.tmp/implementation-r19-check.log`、`implementation-r19-build.log`。R18仅补迟到旧拒绝遇到新身份已配对时按当前状态恢复，原reviewer独立PASS；R19仅补NA008 writer发送先赢的cfg(test)栅栏/计数，普通运行逻辑和Web与R18相同。
- 独立统一表`2026-10-30-current-independent-status.md`已更新为27 PASS / 6 PARTIAL。R19的NA008 writer先赢栅栏由原tester独立实际1 PASS、143 filtered、exit0，有限缺口闭合，见r19-na008报告。剩余NA007、012、013、026、031、032，分属提交后CLI取消、不同SID命令、loopback对照、批准失败浏览器状态和最终多页/整包闭环；不把开发通过计入整例。
- 任务2.4、2.5的判据已由原tester明确确认；3.3据独立NA027–029及用户NA033通过、前端检查与使用说明收口。主方勾选后OpenSpec apply实际7/15完成、8项待办。源码扫描当时648个可版本化文件、7.65MB零命中；后续新增报告与测试会在最终集成检查时补扫，不冒称旧扫描覆盖后续文件。
- 两执行环境重新只读实测仍为同一个jack SID、Administrator=false；NA012异SID pair/stop资源缺口继续保留。远程pipe本机SMB正控制已有独立tester及reviewer通过，不再列缺另一物理机器。用户NA033主观理解已通过，无需额外人工技术签署。

### 2026-10-30 R17 集成检查点（历史）

- 产品固定清单：`testing/runs/establish-node-access/2026-10-30-implementation-r17.sha256`，83项，SHA256 `5074890C209A702335F13042A26DBA64278EFD7A7B0A70A4E42E2F698BA3E26F`。普通debug-embed exe SHA256 `38AA9D02F334E3C81FDC900ED4F8943BB78339650981BC0046A609381D03E6C2`。完整冻结副本83/83已复制验证，dist另作为生成资产提供。本节是在该冻结后更新的计划元数据，不将旧清单plan摘要称为覆盖本节。
- 开发实际执行`npm run check`与普通构建退出0：Rust库143、固定JWS4、Web7文件50、fmt/Clippy/类型/构建全部通过。原始日志`.tmp/implementation-r17-check.log`、`implementation-r17-build.log`。受限沙箱早期全量出现DPAPI/pipe环境失败，保留失败日志；后续成功采用真实owner环境，不把受限失败计通过。
- 独立已关闭：CON001控制台字符处理、LIFE001裸Drop就绪窗口、SC001扫描allowlist、SC002依赖记录、RV001撤销受理/入队协调、RV002首次订阅漏revision、NA-022-001终态claim重试、NA-BUDGET-IPC-001第32管道击穿节点，以及SC003/debug外部Web目录依赖。各自由原提出者在同日r5a/r9/r12/r14/r15报告定向关闭，不以主方自评代签。
- 独立当前新增通过：NA003/004严格JWS与挑战、NA005终态/碰撞/非交互、NA009真实DPAPI崩溃阶段、NA010/030审批边界、NA016默认关闭期限失败、NA022旧材料与留存边界等。原tester最新自身阶段表为15 PASS/18 PARTIAL；这还未合并侧tester的NA002/014/015/017/021/032等结果，禁止把该历史阶段数当最终33例总表。
- 动态补验报告：`2026-10-30-runtime-secret-acceptance.md`（NA014）、`claim-origin-acceptance.md`（NA021与来源/替代节点受控表）、`r6-e2e-acceptance.md`（实际R9 A/B/损坏/停止组合）、`budget-acceptance.md`（入口限流、128连接、IPC32/33及真实300秒补验）、`r12-na022-independent-acceptance.md`。普通profile完全退出和InPrivate存储为不同断言，分别按报告复用。
- 原SC003表明旧debug exe摘要不足以冻结页面。R15启用debug-embed；原reviewer实际构建复制产品、隐藏并修改其自有dist后仍返回原页面/JS/CSS，问题Closed。旧浏览器报告保留当时真实观察及资源归因限制，不倒称旧exe早已嵌入。
- 多页问题：R17把initialize/rotation/save纳入同origin协调、普通更新比较当前key，旧页面重新读取，不新增按钮；原reviewer已独立重跑迟到save并关闭NA-R16-001，见r17-review。NA-E2E-001仍待原tester对固定嵌入整包真实多页复测。慢writer两种真实WSS4429分支已由独立预算tester运行通过，暂停writer/刷新命令窗口适配器边界保留，不冒称自然网络拥塞。
- 环境缺口：当前两种执行环境均为jack且无管理员角色；另一真实SID的pair/stop仍缺。历史另一SID文件/pipe DACL、status/list/revoke与可读密文DPAPI拒绝可按未变负责字节复用，不说当前有第二账户可执行，不转人工LX。已发出一次测试环境资源询问，未获回应不推定可创建账户。
- 远程pipe子项已补：同owner-only DACL的正控制在本机UNC SMB路径双向通信，普通R17产品在同路径拒绝，本地owner仍可用。独立reviewer在platform-smb-scope-review确认满足当前PIPE_REJECT_REMOTE_CLIENTS实效断言；明确本机SMB loopback，不宣称另一机器/SID，也不新增另一物理机器门禁。
- 用户NA033已回答通过，见`2026-10-30-user-acceptance.md`，不再要求旧安全签字。长期suite已同步11+22例；R13修正旧12字段/34例checker，只检查三字段、定义和引用一致，独立正/负控制通过，不验证产品场景。
- 原tester在`2026-10-30-r17-management-independent-acceptance.md`独立确认NA027/028/029通过，并明确任务2.5的NA011/015–017证据满足判据。主方已据此勾选2.5，OpenSpec apply实际回读5/15完成、10项待办。其余任务待33例统一汇总和各自完成判据核对，不等待全change通过才勾已完成的独立工作包。
- 仍需原tester合并全部33例结果、补齐其余必要断言与环境条件，再由WP6对最终集成结果给出交付结论。不提前归档、提交或推送；用户30447体验节点及实际身份未替换。

### 当前用例去重与编号映射（2026-10-29）

- 删除原NA-033安全与依赖汇总用例：依赖维护/许可/平台/漏洞、密码库与锁文件归NA-002，DACL/token revert/DPAPI及不同用户隔离归NA-012，秘密与脱敏处理归NA-014；合同§1–7整体安全边界由WP6独立reviewer检视。检查范围和未完成证据要求保留，不因删除汇总用例视为通过。
- 原NA-034“配对提示与错误可理解性”前移为当前NA-033，仍只要求用户判断本人能否理解提示，回答通过/不通过；客观行为由Harness验证。当前共33例。
- 下方历史记录、review.md、testing/runs以及旧长期套件保持原编号：旧NA-033已合并删除，旧NA-034对应新NA-033，禁止把旧NA-033结果套用到新NA-033。后续WP5同步长期套件时须带此映射核对，当前通过数量不因去重重算。
- 旧人工交接表中要求用户逐项技术审查和签署的安排被本节取代；无需用户安全签字。NA-REV-B02等旧问题仍由原独立reviewer依据新边界复核，不在本次代关。供应链扫描与跨用户测试缺口保持受阻/未执行；原4 PASS / 25 PARTIAL / 5 NOT-RUN仅为旧快照统计。

### 2026-10-29 独立检视后的关闭修复

- 最终 reviewer 已接受当前 NA-001/017/019/032 整例 PASS 及 L6 清理证据，交付状态仍为 BLOCKED（4 PASS / 25 PARTIAL / 5 NOT-RUN）。NA-REV-001–004、D01、S01、T01、P01 关闭；B01–B05 仍保留精确验收缺口。另有 Low/非当前正式配置阻断的 NA-REV-N01：配置接受监听端口 0，但就绪地址返回配置值 0，需后续修复或显式拒绝。

- 最新独立 L6 已写入 tester 报告：NA-032 整例 PASS，受信 Let's Encrypt/Tailscale、两个独立 Chromium profile、本机显式批准、认证/list/订阅、A 撤销 B、停止重启后 A 恢复/B 拒绝以及资源释放均有本版本证据。NA-017/019 亦 PASS；当前整例统计 4 PASS / 25 PARTIAL / 5 NOT-RUN，不把浏览器关闭后正常持久性扩写为 NA-031 的全部故障场景通过。
- 存储探针权限/清理修复后 normal 模式 58 条断言、failure/timeout 各 9 条清理断言通过；原 reviewer 已关闭 NA-REV-P01。NA-007/027 保持 PARTIAL。冻结产品源码仍为 71 项，验证资产摘要单列。

- 辅助验收工具被 reviewer 提出 NA-REV-P01（临时私钥目录继承宽权限及超时清理）；原 tester 正在修复和复测，不影响冻结产品代码。主流程也核对本轮受信测试私钥，确认继承宽权限后，仅对该明确由本轮生成的 `.tmp/.../faraday.tail74842e.ts.net.key` 设置 jack owner-only 受保护 DACL，字节摘要不变；未操作正常用户部署密钥。该修复不追溯宣称此前临时材料已受保护。

- 独立存储 tester 已交付 `testing/probes/establish-node-access-storage/` 可重跑探针，实际 52 条断言通过（含前提与清理，不等于 52 个业务用例）。真实 SQL 语句/COMMIT 失败回滚、撤销幂等重放/载荷冲突、9999→10000 满额、不驱逐未过期结果及过期槽回收均有子断言证据；NA-007/027 仍为 PARTIAL，未将外键失败当磁盘满或过期行夹具当真实时钟推进。独立报告：`testing/runs/establish-node-access/2026-10-29-storage-acceptance.md`，新验证资产摘要单列，冻结产品源码未变。

- 用户已明确选择亲自执行 NA-033/034 人工验收。交接表：`testing/runs/establish-node-access/2026-10-29-human-acceptance-handoff.md`；当前仅落实执行人，结果和签署仍待提交，不算通过。

- 最新独立 tester 回归报告已落盘：`testing/runs/establish-node-access/2026-10-29-independent-tester-regression.md`。真实 jack 用户下库 118 项及 JWS 4 项通过，五项精确定向回归通过；当前版本已实际验证跨 SID 数据目录、管道拒绝和可读密文副本的 DPAPI 解密拒绝，以及真实正常停止/响应投递失败和资源释放。完整用例状态仍须逐断言校准，不能以主要子集通过替代整例通过。

- 恢复后独立 reviewer 已关闭 NA-REV-001–004、NA-REV-D01 和 NA-REV-S01；当前无开放代码问题，但 NA-REV-B01–B05 的验收缺口仍需独立证据收敛。报告与逐项依据见 `testing/runs/establish-node-access/2026-10-29-independent-reviewer.md`。
- 已只读核对长期 `node-access` 与 `client-trust` 规格：从首个 Requirement 起的正文与对应增量逐字一致；长期套件保持 34 个稳定 ID、15 个 Requirement、28 个 Scenario 的结构一致。该检查不代表行为验收通过。

- 第二轮 reviewer 已关闭 NA-REV-001/002，并新增 NA-REV-003（停止响应互相中止）、NA-REV-004（超大 WebSocket 关闭码）、NA-REV-D01（合同陈旧状态）。coder 已修复后续三项，尚待独立复查；真实用户完整验证升至 118 项库测试与 4 项 JWS 向量，证据见 `testing/runs/establish-node-access/2026-10-29-review-regression-development.md`。
- 当前固定版本为 71 项源码清单 `6B1D981658CE1ACA98D390B3C56650908924B0A7486A55A87836DEEC74200BB9`，以及 22 项配置清单 `13AD9DDE14AFC5235217CBA748B8EAB830162F56625FF4183BAB386172DFB3F1`。二者共同固定 93 个文件；前轮 69/70 项摘要仅用于历史证据。
- 用户确认产品设计文档的变化来自其本人调整规格，并授权继续。新增内容涉及会话回收资格和待授权保护，不改变本 change 的节点接入合同；保留原修改，仅重新固定配置摘要。若后续调整涉及接入安全边界，应单独核对影响，不能自动视为实现已覆盖。

- reviewer 对原清单 69/69 校验一致，提出 NA-REV-001（High，订阅前停止信号可能丢失）与 NA-REV-002（Medium，停止响应写失败被忽略）；coder 已修复，尚待原 reviewer 独立复查关闭。
- 新增三项确定性回归测试；真实 Windows 用户会话完整运行 116 项库测试、4 项 JWS 向量全部通过，格式与 Clippy 通过。证据：`testing/runs/establish-node-access/2026-10-29-shutdown-regression-development.md`。
- 原清单 CD304DFE…93EA7E5 因源码修复失效；tester 已停止旧版本检查，原结果仅保留为旧版本记录。重新固定清单后再执行独立复测。
- cargo-audit 安装已实际尝试，但 crates.io 的 abscissa_core 下载反复超时；公告库扫描仍未执行。人工与跨用户验收未完成，不进入待归档状态。
- 后续有界重试 cargo-audit 安装仍无下载进展，已取消以释放构建缓存锁。另按 CI 固定版本尝试获取 Gitleaks 8.30.1，取得官方校验和文本，但 PowerShell 与 curl 下载 Windows 可执行包均超时、无可验证扫描器，因此本地完整密钥扫描仍未执行；不将 shell 最后返回码或校验和下载成功记为扫描通过。

### 2026-10-29 受信 TLS 与 Tailscale 开发复核

- coder 在非沙箱 Windows 用户环境执行：`cargo fmt --all --check`、`cargo check --all-targets --locked`、`cargo clippy --all-targets --locked -- -D warnings` 通过；Rust 库 113 项无过滤通过，`cargo test --all-targets --locked` 同时通过 113 项库测试和 4 项 JWS 固定向量；Web 6 个文件 31 项、类型检查和生产构建通过；OpenSpec strict 校验通过；根与 Web `npm audit` 均为 0 个漏洞。
- 真实 Chromium 首次配对暴露 `globalThis.fetch` 作为对象方法调用时的 `Illegal invocation`。实现改为在包装函数内调用 `globalThis.fetch(...args)`，新增 receiver-sensitive 回归测试，Web 测试数由 30 增至 31。
- 完整 Rust 套件还暴露测试在 `daemon stop` 与 IPC 停止确认之间抢跑，偶发得到 Windows `os error 232`。测试现先等待 `stop_received` 再启动关闭期限；连续 10 次定向执行和完整套件通过。此前 `send_replace` 与 `select!` 前状态检查仍保留，避免立即关闭请求丢失。
- 真实受信 Tailscale 开发证据见 `testing/runs/establish-node-access/2026-10-29-trusted-tailscale-development.md`：Tailscale `1.104.1`、origin `https://faraday.tail74842e.ts.net:28444`、Let's Encrypt 证书、Chromium A/B 配对、本机显式批准、WSS 认证/订阅、非导出私钥、A 撤销 B、`4410 / revoked`、重载 `4401`、正常停止、端口释放、同目录重启及撤销持久性均通过。
- 证书错误未被忽略，`isSecureContext === true`；证书和私钥只位于 `.tmp/`，不进入交付清单。当前仍属开发自测，不能替代独立 tester/reviewer。尚未完成第二个 Windows 用户、跨用户 DACL/管道/DPAPI 拒绝、完整并发/故障矩阵及 `cargo-audit`；NA-010/NA-030 依赖后续 Harness，NA-033/NA-034 尚无人工签字。
- 当前 tasks 仍为 4/15；实现和开发验证已完成的范围不得在独立复测、同版本检视或人工签字缺失时提前勾选。最终处置由用户归档。

### 2026-10-29 已保存身份自动恢复连接
- 按用户要求移除手动重试认证按钮，使用原身份自动恢复临时断线；保留每次连接的新挑战认证、限流等待和终止条件。
- Web 38 项测试及真实浏览器断线恢复、主动断开、撤销验证通过；聚焦独立审查两项发现已关闭。证据见 testing/runs/establish-node-access/2026-10-29-reconnect-development.md 与同日 reconnect-review.md。整体验收状态不因此改为完成。

### 2026-10-29 配对页样式与重新配对入口
- 用户确认参照 main 旧配对页，改为居中圆角卡片、蓝色按钮和深浅主题，保留当前协议。失效或撤销身份直接回到配对表单。
- Web 类型检查、38 项测试、构建及嵌入资源编译通过；真实浏览器桌面/375px 无横向溢出，主题切换通过；截图与本轮再次配对端到端未完成。详见 testing/runs/establish-node-access/2026-10-29-pair-page-style-development.md。整体独立验收仍未完成。

### 2026-10-30 新版33例独立核对与定向修复收口

- 用户当前NA-033主观可理解性已确认通过，原话及版本关联见 testing/runs/establish-node-access/2026-10-30-user-acceptance.md；不恢复旧安全人工签字，不要求用户重复技术操作。
- 冻结与角色隔离：tester与reviewer分别只写独立报告/自有探针，主方修复Web并串行重新冻结。旧69项27AAB…、r2的77项409A…及r3的77项351D…清单保留，各报告只覆盖其对应版本。独立角色最终校验r3 77/77匹配；本节是该校验后追加的计划元数据，不改变产品源码，旧清单的plan摘要不能声称覆盖本节。
- 完整集成检查：npm run check真实退出0，Web 7文件46项、Rust库118项及JWS固定向量4项、格式/Clippy/类型/构建均通过，原始日志.tmp/acceptance-r3-check.log。初轮npm run build:rust通过；r2/r3由tester独立构建嵌入Web。根/Web真实联网npm audit均0漏洞。OpenSpec strict与git diff --check通过。
- 独立发现与闭环：NA-REV-030-R01（未验证节点关闭不应清理私钥/pin）、R02（boot/reset跨页面离开取消）、R03（页面恢复不得绕过deadline及终端停止）和D01（长期编号/职责同步）均由原reviewer复查Closed。连接层保留节点证明错误，页面身份工作串行/取消，恢复入口复用原调度器；长期套件按当前定义同步为11+22例。见同日independent-reviewer报告的r3节。
- 真实与受控边界：tester在受信TLS/Tailscale/ego完成4401/4410后新key重新配对且仍需TTY明确批准；实际生产页面配合受控传输、真实IndexedDB延迟回调及合成生命周期事件验证保留pin/取消与单次恢复。r3的60秒deadline、终端停止使用受控时钟，不声明真实BFcache导航或实际等待60秒。reviewer另动态执行真实main/reconnect的9场景通过。不是全矩阵通过。
- 当前33例最终结果：4 PASS（001/018/019/033）、27 PARTIAL、2 NOT-RUN（010/030），本轮0个确认产品FAIL。逐例缺少断言见 testing/runs/establish-node-access/2026-10-30-independent-tester-r3.md；不能将局部PASS提升为整例。NA010/030是缺少应用审批策略测试入口的实现缺口，不是用户/证书/工具环境限制。
- 仍阻断：其余预算/随机源/故障/并发/Windows隔离/浏览器异常存储必要矩阵，审批测试入口，以及Rust advisory/Gitleaks扫描。官方Gitleaks下载超时零字节；cargo-audit临时安装无进展已取消，未执行扫描，不记为通过。独立报告已记录直接依赖许可/平台/维护来源，不能据此宣称全传递依赖漏洞检查通过。
- 后续顺序：先建立NA010/030明确可调用的策略验收入口；按tester逐例断言补齐可控故障和Windows/浏览器矩阵；在可访问公告库/扫描器环境完成供应链；对最终版本独立复验后才勾相应任务和归档。tasks仍4/15，不以实现自测代替完成判据。
- 清理与限制：tester自有30451/30452节点、pipe、锁、ego空间已清理；敏感运行材料留在受保护.tmp，探针源码不含运行身份。用户30447保持运行；本次修复源码未替换该正在运行的体验节点。未提交、推送、归档。
