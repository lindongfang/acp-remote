## R2-001–003 当前修订范围（2026-10-31）

用户同意本轮三项文档修复：空闲取消分支、管理存储故障下限，以及请求有效期、去重和审计保留边界。当前行为由本 change specs/design 及总体设计定义；元数据清理、容量账本和隔离恢复分别由所属 change 引用。主执行者串行维护共享来源，避免相互覆盖；不改代码、独立 tests/review、旧合同或任务勾选，不将本次文本修订当作独立复核通过。

恢复实施前，独立 tester 须映射新增场景到稳定用例：idle 无待确认输入的取消、idle 但输入未确认、自然完成竞态、受理/批准/派发后写入失败、故障代际安全控制、存储恢复和正常重启核验、截止时刻及清理并发、时钟回退、受保护记录和容量满。独立 reviewer 检视后再固定请求身份签发用途、事务及拒绝水位、新版 wire 合同与工作包；产品和真实 Harness 场景全部未执行，实现仍暂停。

---

## ARCH-001 当前修订范围（2026-10-30）

本轮用户授权只修订整体设计及相关规格，补齐原生工作与审批生命周期、缓存外分页、内容安全和自动回收能力边界。当前依据为本 change 的 proposal/specs/design 及总体设计；本 change 负责原生工作、有效请求、内容与资源安全及能力呈现，另两项由所属 change 定义。主执行者串行维护共享文档，避免同文件并行写入；本轮不恢复代码或数据迁移，不调整任务勾选。

新增规格 Scenario 尚须独立 tester 映射到稳定用例并重基准 tests，独立 reviewer 复核后再重排工作包、固定合同及安排真实验证；本轮未修改独立 tests、review 或历史快照，不宣称本轮独立检视通过。既有报告仅保留原范围，OpenSpec 文件齐备不解除实现暂停。后续验证需覆盖无 Run 的原生活动及后台审批、内容/URI/附件边界；缺页持续读取和回收证据分别跟随其所属 change，通用资源安全边界仍须先成立。

---

## PV-001 独立文档结论（2026-10-30）

独立 tester `/root/spec_tester` 编写通用用例 CL-058–060，独立 reviewer `/root/spec_reviewer` 按固定快照限定复核通过，未发现本轮阻断。本 change 定义通用版本优先级及验收，详见[本轮 review](review.md)顶部。OpenSpec strict 11/11、`git diff --check` exit 0；3条新增产品用例均 not-run，不签全面架构、完整重基准或实施准备通过，实现继续暂停。下述27文件固定摘要为本报告追加前值，追加仅转录结论，不改规则及用例正文。

---

## PV-001 协议优先级当前范围（2026-10-30）

用户要求能用最新协议时优先新协议，避免多协议 Harness 长期使用低版。当前行为以 harness-management 的“优先最高共同可用协议”为准：每次新连接优先最高已实现验证版本的标准协商，遵守合法选择及能力，规范回退与业务失败分开；新连接不按旧报告固定低版，也不强制对端升版或编造选择原因。旧请求、审批及副作用不跨版本重发。

本轮验证由独立 tester 仅在本 change tests.md 定义 CL-058–060，覆盖高版优先、规范回退、原因缺失、新版能力保真、重连与业务失败边界；本轮全部不执行。恢复实施前固定新版合同/实现/实际版本，后续由独立 tester 在隔离 Windows 节点及 Web/真实 Harness 环境验证，替身与真实边界分列，结果归 testing/runs/deliver-codex-session-loop/。主执行者串行修订公共规则和引用，tester 独占 tests，reviewer 只读，保持 role 所定独立性。

下方旧 RP/DC/SR 结论及摘要保留原快照，不覆盖本次新增优先级；不将既有“支持多版本”当作已验证优先高版。完整合同、旧用例及工作包重基准未完成，实现继续暂停，本轮不改代码、任务勾选或用户数据，不运行产品、构建或真实 Harness。独立文档结论在取得后另行记录，不把结构检查当产品或全面架构通过。

---

## DC-001 独立文档结论（2026-10-30）

独立 tester `/root/spec_tester` 编写本轮 CL-055–057，独立 reviewer `/root/spec_reviewer` 限定文档复核通过，无未关闭阻断；DC-R001（P2，重放反复撤销满额候选）已由原 reviewer 复核设计及 CP-025 关闭。详见[本轮 review](review.md)顶部。全量 OpenSpec strict 11/11、`git diff --check` exit 0；7条新增产品用例均 not-run，不签产品、全面架构或完整重基准通过，实现仍暂停。本记录仅转录独立结论，下方规则及计划原文不变；29文件固定摘要属于本记录追加前快照。

---

## DC-001 三项取舍当前范围（2026-10-30）

用户确认实际重放可刷新30分钟无活动计时，但不形成执行、有效审批或后台任务保护，满额仍可安全淘汰；默认继承节点环境并允许显式覆盖，不新增白名单或环境指纹准入；原生身份以显式接入来源+原生会话ID协调，同来源多入口复用，不自动证明不同来源背后物理存储等价，也不以缺少此证明阻断已配置来源。

本轮独立 tester 负责必要用例定义，拟验证 CL-055–057：默认环境继承及覆盖、同来源协调和来源区分、后续真实闭环。 产品用例本轮全部不执行；恢复实施前须固定新版合同、实现和环境，由独立 tester 在 Windows 专用数据根/配置、节点与 Web 环境验证，真实与替身边界分别报告，证据归 testing/runs/deliver-codex-session-loop/。本轮规格、设计和规划由主执行者串行修订，避免同文件重叠写；tester 仅写三份 tests，reviewer 只读，两者按 role 文件保持独立。

当前规则以更新后的规格及总体设计为准。下方 RP/SR 审查和摘要属于各自历史快照，冲突的物理存储证明、一概查看不保活断言不再适用。详细合同、旧用例及执行工作包全面重基准尚未完成，本轮只同步文档，独立复核结论另行记录；不恢复实现、不修改任务勾选、不删除实际数据，不把结构校验当产品验收或其他架构风险已解决。

---

## RP-001 独立文档结论（2026-10-30）

独立 reviewer `/root/spec_reviewer` 对用户已确认的重放准入及兼容修订作限定文档复核：通过，本轮无新增阻断；本 change 增补用例为 CL-047–054。详见[本轮 review](review.md)顶部。本结论只覆盖本轮规则与用例定义，不代表产品、安全实测、全面架构或执行工作包通过。全部产品用例 not-run；旧合同、旧用例及工作包全面重基准和其他架构整改尚未完成，实现继续暂停。以下方案、规格、用例和计划的固定快照是本报告追加前值，不能冒充报告追加后的文件摘要。

---

## RP-001 独立文档结论（2026-10-30）

独立 reviewer `/root/spec_reviewer` 对用户已确认的重放准入及兼容修订作限定文档复核：通过，本轮无新增阻断；本 change 增补用例为 CL-047–054。详见[本轮 review](review.md)顶部。本结论只覆盖本轮规则与用例定义，不代表产品、安全实测、全面架构或执行工作包通过。全部产品用例 not-run；旧合同、旧用例及工作包全面重基准和其他架构整改尚未完成，实现继续暂停。以下方案、规格、用例和计划的固定快照是本报告追加前值，不能冒充报告追加后的文件摘要。

---

## RP-001 重放兼容当前范围（2026-10-30）

本轮独立 tester 新增定义 CL-047–054；当前只检查文档，不执行产品用例。恢复实施及固定集成摘要后，由独立 tester 在 Windows 专用原生根、节点及 Web 环境执行相应合同/故障和真实闭环，逐 ID 证据归 testing/runs/deliver-codex-session-loop/；真实 Harness、通用重放支持及故障入口缺失记受阻或未执行，不能以替身通过冒充实际能力。执行任务及完整旧用例仍须先按当前规格重基准。

用户最新确认无标准历史重放的 Harness 暂不接入，统一使用 ACP 重放兼容，不增加厂商专用读取器。当前行为以本 change specs 及通用 harness-management/structured-history 为准，平台分页不要求原生按页读取。本文下方所有原快照及此前 SR 修订记录保留历史归属；其中当前连接降级、禁止全量重放再分页、原生分页硬前提和旧能力假设均不得作为本次执行依据。

本轮只修订文档；独立 tester 增补本轮用例，独立 reviewer 复核本次范围，产品用例仍未执行。详细合同、全套旧用例与工作包全面重基准及其他架构底座整改未完成，不恢复实现、不修改 checkbox，不删除原生数据或改写旧证据。平台身份、审计及离线平台备份不因 Harness 未准入而失效。

---

## 本轮文档修订独立复核（2026-10-30）

本 change 的 SR-001 文档修订和新增用例 CL-043–046 经独立 tester `/root/spec_tester` 编写、独立 reviewer `/root/spec_reviewer` 限定复核通过。对应的受审查快照、范围和问题关闭记录见 [本轮 review](review.md) 顶部。50 文件报告追加前聚合 SHA-256 为 `7e77ec113fd48b6796139cfb4465d2949dc3a788c7ee033e23ddc84136d77d2d`；追加本记录不改动规格、设计或用例。

本次只完成文档检查：全量 OpenSpec strict 11/11 与文档 `git diff --check` 通过；R-SR-DOC-001 和 R-SR-TEST-001 经独立复核关闭。新增产品用例全部未执行；旧合同、旧用例及执行工作包仍待全面重基准，不恢复实现、不勾产品任务、不把下方旧阶段记录当新版通过。下方 Project Verify 已登记本轮用例及未来执行安排，须在恢复实施前落实到重基准后的工作包。

---

> 五项审查修订补充（2026-10-30）：跨入口原生身份、非 Run 操作审批、节点共享连接视图、能力准入分层及按能力独立交付已写入当前 specs/design。下方旧用例与工作包尚未覆盖这些新场景和依赖调整，保持原 ID 供独立重基准，不据此开工或声明覆盖。

> 2026-10-30 规格重基准门禁：实现暂停。会话及历史以 Harness 为权威，见本 change 当前 specs 与 design。下方为原方案的plan.md内容及证据，凡依赖平台聊天持久化、入库版本、强刷、导入批次、平台删除或历史备份的部分均已失效；不可据旧任务勾选、旧检查或旧审查恢复实现。待独立 tester 修订用例、详细合同与工作包重排、reviewer 重新检视后再决定恢复。历史证据保留原范围，不改写旧结论。

## Execution Model

本轮文档修订范围为 SR-001（异常后重新准入），依据 proposal Exploration 及当前 session-execution/native-session-recovery 规格。独立 tester 仅增补 tests.md 的本轮用例，独立 reviewer 复查这一修订及用例；检查结果另记本文件和 review.md 的本轮节，不覆盖旧快照。以下原执行计划、WP2/readiness 及旧合同不因本轮文档修复恢复效力，仍须全面重基准后另行决定恢复实现。

2026-10-30网络重试后的阶段说明：以下旧“真实执行超时”状态保留历史意义，以本节为当前入口。独立readiness-review确认WP1编码前可行性门禁足以开始WP2；task1.2/1.3的CL-005产品门控及最终检查仍待后续产品实现，复选框不提前勾选。二者不是WP2编码前必须先有成品的循环条件；全部最终验收要求保留。仅开放WP2已审领域/存储及开发测试，WP3/WP4仍按各自真实合同/依赖与安全门禁推进。当前证据见 testing/runs/deliver-codex-session-loop/2026-10-30-readiness-review.md、2026-10-30-network-retry-acceptance.md。

WP2隔离工作区为 `C:/Users/jack/.codex/worktrees/codex-session-wp2/acp-remote`，产品基线HEAD `65b46c01d6b95fc1f1a8223b1c92a729a885c185`；最新未提交规划以 `D:/Project/acp-remote/openspec/changes/deliver-codex-session-loop/` 为准，只读交接，不能使用工作区旧规划覆盖主目录。coder只写WP2范围，CARGO_TARGET_DIR单独设置。主目录在该包交付前只维护探测/规划/证据，不并行修改产品；集成先复核实际diff与检查。

网络重试：系统现有用户代理需显式传入独立Codex进程；两版已取得回复、跨进程同原生ID恢复/上下文召回、可靠取消、权限取消和allow_once真实工具完成。当前主执行与独立报告见 testing/runs/deliver-codex-session-loop/2026-10-30-network-retry.md 及其引用，不改写此前直连失败报告。临时登录副本已清理。readiness范围仅WP2；Job Object、Web/SQLite产品及唯一裁决等最终断言不据此通过。

WP2输入事务阶段：七文件候选已获限定层级独立静态检视和12+2项独立SQLite层检查，逐摘要合入主目录；开发完整Rust检查报告156库+4向量通过，主目录完整Rust/Web检查另取实际集成日志。当前stage详见 testing/runs/deliver-codex-session-loop/2026-10-30-wp2-input-development.md 及两份独立报告。task2.1的最终Web/产品断言未执行，不勾完成；同一隔离工作区继续2.2，主目录此阶段只汇总证据，不并行改该包产品。

该七文件集成版本的`npm run check`和`npm run build:rust`已实际exit0，Web50、Rust156库+4向量，完整检查/构建日志和摘要见上述development记录。独立tester追加真实旧exe对实际schema2一致性副本拒绝且不改库/key的子项，DPAPI完整升级与启动接线仍待后包，不扩大独立结论。

WP2历史保存阶段：旧候选HIST-R001丢唤醒问题由原reviewer发现，修复并执行确定性回归后独立关闭；修复版13文件经独立34+2项SQLite/async层测试、逐hash合入主目录，完整集成check及普通exe构建exit0（Web50、Rust190库+4向量）。证据与限制见 testing/runs/deliver-codex-session-loop/2026-10-30-wp2-history-development.md 及两份独立报告。task2.2最终产品断言未完成，不勾；同一隔离工作区开始2.3，主目录暂不并行改产品。中间schema2草案布局拒绝不代表允许清库。

状态：会话闭环待实现；上游 `establish-node-access` 已于2026-10-30完成33/33验收与独立检视并归档。本轮WP1按最终access-contract、实际Rust平铺模块及schema1重基准；最新独立规划报告确认方案/用例定义PASS、PR-001–003设计Closed；真实能力仍BLOCKED，本文件仅汇总原独立结论。真实ACP/Codex能力门禁未通过前不启动受影响产品编码。最终上游证据及路径映射见session-contract §1。

主执行统筹；coder/tester/reviewer按openspec/role分工。tester/reviewer不继承coder对话。基线是proposal、specs、design、tests以及上游固定版本的完整文件摘要，未提交修改不以HEAD代替。保留用户改动；交付diff/可读报告，不自动提交发布。

## 本轮 ACP 规划修订

WP2审批阶段已完成内层同版独立审验与14文件逐摘要集成：tester10审批+4外部补验+34历史、reviewer独立10/34重跑均通过；主D完整check（Web50/Rust200+4）及普通Windows构建exit0。证据见2026-10-30-wp2-approval-development.md及tester/reviewer-wp2-approval.md。task2.3真实产品断言仍未完成；WP3设计/用例/编译入口问题W3R001–007已关闭，按下文隔离拆分开始公共协议与Windows平台实现，不提前签最终CL。主D暂保持固定2.3产品，前端仅冻结隔离未挂载组件。

2026-10-30 WP3前置独立预核查提出W3R-001–003，已在session-contract §7/8补实可信审批发行者、typed工具关联、撤销/取消与实际writer边界，以及按selected版本解码初始化响应；原reviewer已关闭设计缺口。独立tester已补相应CL-004/013–016/031/036窗口断言，产品执行仍not-run，用例复核另见reviewer-wp3-readiness.md最新追加。WP2 2.3尚未冻结交接，本记录不提前开放WP3产品写入，也不代替最终验收。

通用协议与版本策略写范围归 WP1/WP3，标准能力禁止按厂商分支；WP1核实 v1/v2 官方规范、实际版本标识及发布状态，WP3实现公共客户端和各版本策略，WP4使用通用能力/错误视图。WP5补充 CL-032–041，WP6重新检视本轮规划；全部新用例及真实多版本验收待执行，不沿用旧review对新文本签通过。升级复验和不兼容用户决策遵循 design 的通用 ACP 边界。无可核验v2规范或真实对端时记录阻断和影响并交用户决定，不静默只交v1。

## 最新保存展示修订

WP2持久化及提交事实内部保持，WP3继续正确同步版本，WP4正常页面不显示保存进度/状态/版本，仅异常显示错误。WP5新增CL-042并更新保存失败预期，WP6重新检视当前展示合同；不以旧review覆盖此次规格和用例修订，不修改保存周期、缓存限额或故障停止规则。

## 测试流程修订

按当前config和测试模板精简CL-001–042，需求与共用信息集中说明，保留ID、标题和行为断言；CL-031由LX调整为独立reviewer可工具验证的L5，不新增人工门禁。此次修订的独立用例/方案检视待完成，旧review不覆盖新文本；既有执行结果不改写。

依据本轮用例复核，补实CL-003保存/容量边界、CL-004事务及派发计数、CL-011强制刷新失败操作与资源责任断言；区分L3/L4整组件替身和L5真实组件局部故障，L6必需依赖保持真实。CL-031归WP6执行，WP5及PV3仅负责tester用例；用例仍42条，产品未执行，本次修订待独立复核。

## Parallel Assessment

2026-10-30新增Web隔离准备：`C:/Users/jack/.codex/worktrees/codex-session-web/acp-remote`，相同HEAD基线65b46c01d6b95fc1f1a8223b1c92a729a885c185；只写clients/web（不含tests/acceptance），最新合同从D主目录只读。与WP2的Rust写范围互不重叠。仅准备稳定wire与界面，WP3冻结前不集成、不签真实闭环；Rust/WSS装配仍遵循原串行顺序，合入后重跑全部受影响检查。该准备不允许自行补出尚未定义的payload投影结构，缺字段先回主执行协调合同。

Web准备据此暴露的投影字段缺口已集中补入session-contract §9；原reviewer复核关闭W3R-004–006并确认该节设计PASS，具体证据及固定摘要见reviewer-wp3-readiness.md。内容引用、节点登记元数据引用范围、版本/能力状态与manifest完整性均由合同统一定义，前端不自行推断。tester对应断言及产品执行分别追踪，不以设计PASS代签。

WP3准备拆分（WP2 2.3独立验收及集成检查通过后才开产品写入）：公共ACP客户端/策略先以泛型AsyncRead/AsyncWrite端口和有界测试传输实现，在WP2原工作区独占app/src/adapters/acp/、harnesses/；Windows进程树owner在独立`codex-session-platform`工作区独占app/src/platform/、app/Cargo.toml仅启用现有windows-sys的JobObjects及Globalization特性（后者供CompareStringOrdinal匹配Windows环境名大小写语义；锁定crate不升级）。二者不并写共享文件。协议端不包含进程启动，平台端不包含会话/审批数据库；公共接口是有所有权的异步stdin/stdout/stderr、关闭/等待/完整Job回收结果，最终资源装配由协议coder按Merge Order串行接管lib.rs/node.rs/access_policy.rs/trust.rs。平台模块声明在本工作区platform/mod.rs内完成；其他lib模块声明由最终装配owner处理。平台模块交付时单独冻结/独立检查，合入协议工作区后重跑两个包全部受影响检查。app/Cargo.toml与platform/mod.rs在平台包独占至该次合入，后续交接才可修改；app/Cargo.lock不应因仅特性变动产生新依赖。app/DEPENDENCIES.md的该特性说明同归平台包。这一拆分不开放WP3提前写入或绕过任何真实安全/平台验收。

WP1先核对上游与检查环境；其他包共享会话历史和查询合同，有写范围交叉，按Merge Order串行集成。已确定的Web接口可让WP4在独立工作区并行准备，但必须等WP3稳定后合入，合入后重跑受影响测试。WP5与WP6对同一固定集成版本可并行独立工作，不能边改实现边签验收。Rust构建使用各自CARGO_TARGET_DIR。

## Work Packages

WP2串行补全范围：运行层所需catalog/生命周期/通用Command存储端口按session-contract §10补齐，写owner单独在`C:/Users/jack/.codex/worktrees/codex-session-catalog/acp-remote`，先以主D固定2.3源码摘要覆盖新建干净基线；最新规划仍从D只读。仅写domain/catalog与application/catalog、session_storage/catalog及其tests、schema.rs/storage.rs/module声明（不改lib/node/协议/platform/Cargo/前端）。与WP3新目录不重叠；schema/ledger和原input/approval兼容调整均由此owner串行完成，不能由协议coder直写SQL。§10先经独立方案/用例复核再写新实现；合入后协议工作区单次同步通过审验的存储文件并重跑WP2全部检查。主D在该包交付前仅维护合同/证据，不并行改其产品。catalog的副作用去重/原子审计及未知不重派沿用现有CL，不新增未经确认能力。

WP3拆分的实际编译入口：现有app/src/lib.rs已声明pub mod platform，平台包通过已存在platform/mod.rs新增受cfg(windows)保护的process_owner模块声明，Cargo自动纳入，不需平台包改lib.rs；开发测试写在该模块及独占app/tests/platform_owner.rs（需时使用产品公开接口）。协议包由同一最终装配coder在lib.rs声明adapters模块及adapters/mod.rs，只有模块接入不提前装配运行资源；协议参数化开发测试随该模块编译。各包独立cargo check/fmt/clippy/test命令实际覆盖新入口后才可声称编译/测试通过，平台测试必须在Windows真实owner下执行，独立target且不得仅检查未引用文件。

| ID | Goal / Scenarios | Dependencies | Agent | Write Scope | Deliverable | Verify |
| --- | --- | --- | --- | --- | --- | --- |
| WP1 | 上游/schema/安全接线重基准，v1/v2官方规范与真实能力探测 | 上游已归档；允许先核验 | coder | 本change/proposal.md、design.md、session-contract.md、plan.md；testing/probes/codex-session-loop/、testing/runs/deliver-codex-session-loop/本次WP1报告与公开探测结果；README.md仅检查入口及升级引用；openspec/config.yaml仅context事实重基准，不改流程rules/operations | 最小wire/升级/同步合同及固定探测报告 | CL-001/002/005；探测不代真实闭环 |
| WP2 | 会话命令、输入、Run与审批历史事务 | WP1及独立方案/用例通过 | coder | app/src/domain/、application/、storage.rs、新增session_storage/、app/tests/unit/；lib.rs仅模块声明 | schema1无损升级与原子受理、开发单元测试 | CL-003/004/008–027及升级保留断言 |
| WP3 | ACP策略、进程所有权、审批安全接线与原生恢复 | WP2；对应真实能力门禁通过 | coder | app/src/adapters/acp/、harnesses/、platform/、access_policy.rs、trust.rs；node.rs/lib.rs资源接线；app/tests/integration/ | 双版策略、真实审批发行者/连接代际、资源关闭与恢复 | CL-005–016/020–024/028/029/032–041及现有身份撤销回归 |
| WP4 | Web闭环、认证管理入口扩展、最终组合 | WP2/WP3 | coder | clients/web/（排除tests/acceptance/）、app/src/management_ws.rs；node.rs/lib.rs最终装配；README.md、.github/workflows/ci.yml | session-contract入口、逐块权限/撤销writer协调、Web与检查 | CL-001/002/022/030及原帧/深度/出站队列回归 |
| WP5 | 独立用例维护、正式验收与长期回归 | 用例阶段WP1合同；验收阶段WP4冻结 | tester | 本change/tests.md；app/tests/acceptance/、clients/web/tests/acceptance/、testing/suites/；testing/runs/deliver-codex-session-loop/tester-*及本次2026-10-30-contract-test-review.md独占报告 | 补合同断言、逐例结果及回归资产 | CL-001–030/032–042；CL-031归WP6 |
| WP6 | 独立方案/用例及交付检视 | 方案阶段当前合同/用例；交付阶段WP4同版冻结，与WP5并行 | reviewer | 本change/review.md仅追加新快照结论；testing/runs/deliver-codex-session-loop/reviewer-*及本次2026-10-30-planning-review.md独占报告 | 执行CL-031、独立问题与复查 | 固定版本安全/平台/供应链/真实证据；不得代签tester |


写范围均为仓库相对路径，表中“本change”指openspec/changes/deliver-codex-session-loop/。实际已存在storage.rs/management_ws.rs/node.rs/lib.rs/access_policy.rs/trust.rs；新增目录不意味着替换安全入口。所有共享文件显式串行交接，不能并行写同文件；无需为并行重构安全模块。plan.md结果节只由主执行在收齐独立报告后更新，WP5/WP6均不写共享汇总。

## Merge Order

| Order | Work Package | Reason | Re-verify After Merge |
| --- | --- | --- | --- |
| 1 | WP1 | 先重基准合同与入口；README首次登记由WP1独占 | 严格结构检查及独立新方案/用例复核，真实门禁另验 |
| 2 | WP2 | storage.rs迁移与lib.rs模块声明先落稳 | schema1/新库、保留身份/撤销/审计、事务与故障用例及现有Rust检查 |
| 3 | WP3 | 接管lib.rs，独占access_policy.rs/trust.rs及node.rs资源接线；不能同时改WP2 storage.rs，需变更则退回WP2串行修复再重基准 | WP2/3受影响检查、身份撤销/审批及进程树/恢复 |
| 4 | WP4 | 接管node.rs/lib.rs最终装配；独占management_ws.rs；README在WP1后接管，Web可隔离准备 | Rust/Web真实检查、原32KiB/深度8/队列/撤销COMMIT后不发送及CL-030 |
| 5 | WP5、WP6 | 主执行冻结同一集成提交或完整SHA256快照；验收和交付检视并行只读产品，报告路径独占 | 各自逐例证据及问题ID；修复后更新快照，独立重测复查 |
| 6 | 主执行单一汇总 | 两份独立报告完成后，主执行独占plan.md结果节；tasks5.3所称共同更新指提供各自报告，不是并行写plan | 摘要对应同版本，必需失败/not-run不交付；提出者独立关闭问题 |

共享owner顺序：lib.rs为WP2→WP3→WP4；node.rs为WP3→WP4；storage.rs仅WP2（后包改动先退回串行修复）；management_ws.rs仅WP4；README为WP1→WP4；plan结果节仅主执行。后合入者必须重跑表列受影响检查，跨包修复后已有证据由tester/reviewer重新判断有效性。隔离准备也不能隐式拓宽写范围。

## Project Verify

本轮新增验收定义为 CL-043–046，覆盖 SR-001；当前只核对定义和来源，不运行产品验收。恢复实现并固定集成文件摘要后，由独立 tester 在 Windows 专用原生根及节点/Web 环境执行对应合同故障注入和真实闭环，逐 ID 证据归 testing/runs/deliver-codex-session-loop/；执行安排须纳入重基准工作包，缺少原生安全确认能力记受阻，不以握手替代。

| Check ID | Command / Working Directory | Scope / Pass Criteria |
| --- | --- | --- |
| PV1 | 根目录：openspec.cmd validate deliver-codex-session-loop --strict；git diff --check；tester核对CL-001/002 | 当前真实可执行，仅结构/差异检查 |
| PV2 | WP1从上游README和工程配置核对实际Rust/Web编译、格式、类型和测试入口，根目录及实际工程目录；CL-001–004 | 现有 npm run check、npm run build:rust 等入口按 README 执行；核对是否覆盖本 change，缺专用目标/合同入口记受阻/未执行，不虚构通过 |
| PV3 | 独立tester按tests.md共用执行安排及各例操作步骤执行CL-005–030、CL-032–042；独立Windows与浏览器profile | 引用精确需求和断言，正常链路真实；故障fixture不能代替真实Harness验收 |
| PV4 | 独立reviewer：按tests.md执行CL-031，固定与tester相同版本；源码/配置及真实浏览器、协议和脱敏证据 | 安全及交互检视有独立结论；不由tester或主执行自查代签，缺入口记受阻/未执行 |

固定同一提交或完整文件SHA-256清单，报告存testing/runs/deliver-codex-session-loop/。记录OS、工具、协议/适配器版本、逐例结果与清理；最终检查覆盖实际集成版本。不得动用户真实数据库、密钥或原生会话；专用测试资源本包拥有并在失败/取消时清理。

## Code Review

| Work Package | Reviewer Agent | Review Focus | Pass Condition |
| --- | --- | --- | --- |
| 全部编码前 | 独立reviewer | proposal/spec/design/tests及当前计划与上游依赖 | 每个引用场景存在且覆盖，无未闭环设计阻断 |
| 全部交付时 | 独立reviewer | 固定集成diff与实际安全/平台/验收证据 | 必需检查实际通过，reviewer问题由其独立复查关闭 |

## Main E2E

```yaml
mode: required
```

对应tests.md的L6真实Windows/Web链路，按角色使用真实浏览器/桌面工具；CL-031由独立reviewer通过工具完成L5安全及交互检视，当前无LX用例；可复用同版本、同范围且环境满足要求的有效证据，不要求用户重复手测。缺工具、环境、权限或入口记受阻/not-run，不转LX，不改成模拟通过。

## Risks

集成回归NET-I001：主D首次合平台5文件完整检查时，既有WSS超帧测试发生10054 reset（199通过/1失败），catalog独立分支同一未修改测试也失败。主执行独占management_ws.rs及node.rs相关开发测试诊断/修复，其他包不写这两文件；保留首次失败及原测试并发80次中5失败证据，不通过重跑碰绿或放宽4429预期关闭。修复不得放宽32KiB帧/深度8/队列/撤销边界；后续WP3/WP4接管这两文件前须同步新基线，原独立tester/reviewer复验该修复，最终全检查仍必须实际通过。

NET-I001隔离修复owner为net_close_fix，工作区`C:/Users/jack/.codex/worktrees/codex-session-net-close/acp-remote`，基线51文件摘要包含主D已审验WP2/平台/ACP，仅该owner改management_ws.rs和node.rs的关闭运输层/相关开发测试，必要时新增私有app/src/node/transport.rs模块。因axum公开WS层在容量错误后已终止且不暴露底层TLS，允许在现有node TLS连接所有权边界落实有界关闭/排空，而非扩大帧缓冲或加固定延时碰绿；不得新增旁路、认证规则或依赖。传输资源需由原连接任务拥有并等待，取消/超时仍能关闭真实socket；单次关闭沿用原2秒总预算、节点15秒总预算，临时排空使用固定小缓冲/显式字节上限且不得再解析/受理请求。新机制和证据须独立检视后才合主D，不修改既有归档历史报告。

上游已归档，本change会话仍待实现；当前最新规划问题已独立关闭，真实执行门槛仍未满足。真实依赖/版本探测、平台证据、安全与供应链在实施时逐项交付。现有上游编译入口须核对覆盖范围，不将上游构建或本change用例定义记成会话闭环通过。设计发现违反已确认产品约束时先修文档，不能以门禁失败为由降级承诺。

## Handoff

只交接用户确认（Windows+Web、Codex/ZCode/DeepSeek、分页10轮、保存500ms/64KiB）、AGENTS/config/角色、当前proposal/spec/design/tests、上游固定版本及写范围。交接测试数据/端口/目录所有权，不继承开发对话或代签结论。

## Findings

| ID | Source | Severity | Target Version | Finding | Fix Version | Status | Retest / Re-review |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 文档方案检视 | 独立reviewer | 文档结论 | 当前review快照 | 旧快照方案/用例/计划/任务检视通过；本轮修订另行复核，问题及关闭依据见review.md | 旧快照 | 本轮待复核 | 独立复查；产品未执行 |

## Verification Results

- 受测版本：上游已交付并归档，会话产品尚不存在。本轮规划及探测正式报告固定实际文件SHA256；上游33/33不是本change验收。
- 摘要排除范围：plan结果、tasks状态和review报告自身；spec/design/tests及实现/配置变化不排除。
- 执行者与环境：主执行结构自检、独立tester用例规划、旧快照独立reviewer方案检视通过（见review.md，不覆盖本轮修订，非产品验收）。
- 验证命令及目录：当前仅根目录OpenSpec严格校验和Git差异；实际退出码在整体文档检查记录，非产品结果。

| Case ID | Executor | Target Version | Status | Evidence / Limitations |
| --- | --- | --- | --- | --- |
| CL-001–030、CL-032–042会话产品部分 | 独立tester | 会话实现未建立 | not-run | 用例定义不是运行证据，未运行正常/异常/平台验收 |
| CL-031会话安全/交互 | 独立reviewer | 会话实现未建立 | not-run | 最新规划审查不是本例的产品验收 |

- 未执行项与已知限制：本change所有产品、安全、平台及真实Harness验收；结构检查不替代它们。
- 问题关闭依据：2026-10-30-planning-review.md的PR-001–003等待提出者对本轮摘要独立复核，不代关。
- 新版本证据判断：受影响spec/design/tests或上游/实现改变后，tester/reviewer独立确认重验范围。
- 归档后复核：未归档；后续核对spec/长期用例同步、移动映射及摘要一致性。


## WP1 当前有限证据

官方codex-acp 2.5.0配套codex 0.165.0，在拥有的隔离CODEX_HOME分别initialize v1/v2成功；session初始无登录探测新建均-32000 Authentication required；用户授权复用登录后两版创建成功，但v1 prompt响应及v2前台idle均超时，后续真实链路受阻。官方v2初始化/prompt-lifecycle为Draft，已可核验规范，不能再称v2依据不存在，但不能把协商或Draft规范当成真实执行/审批/取消/原生恢复通过。固定发布摘要、路径、矩阵和脱敏原始记录由主执行WP1报告提供，当前不标WP1全部完成。

PR-003详细合同见session-contract；其中断言建议需独立tester维护tests并经reviewer复核后才可开工。本文记录的是修订候选，不宣告规划门禁通过。

## 2026-10-30 WP1 最新汇总（取代上文旧阶段待复核状态）

- 独立方案/用例定义PASS，PR-001–003设计问题由原reviewer复查关闭，34/34场景、42个稳定用例；原报告及各轮固定摘要见 testing/runs/deliver-codex-session-loop/2026-10-30-planning-review.md。tester用例复核见同目录2026-10-30-contract-test-review.md。方案通过不等于CL-031产品验收。
- 上游最终合同/身份撤销/schema及三款探测范围已冻结，可读摘要与独立复核满足task1.1，进度1/14。任务1.2、1.3及后续任务保持待办；检查入口已核对，但会话专用合同、供应链及最终目标未完成，不能提前勾1.3。
- 独立tester现有工程基线实际通过：Web50、Rust144+4、fmt/clippy/类型/构建；产品源码前后摘要相同。执行环境为CI owner前置及独立CARGO_TARGET_DIR，失败尝试和有效日志见2026-10-30-wp1-baseline-checks.md。不签会话功能通过。
- codex-acp2.5.0/codex0.165.0分别真实协商1/2。初始无认证新建拒绝；用户随后授权复用登录，两版新建成功，但v1 prompt超时、v2未收到idle；审批/取消/原生恢复未执行。未附凭据的官方服务网络检查连接重置。证据见2026-10-30-wp1-capabilities.md及两个脱敏JSON，不因创建成功绕过真实执行门槛。
- 独立探测依赖和锁仅位于testing/probes/codex-session-loop；未改产品依赖或实现。临时登录副本两份已清理，任务自有可执行路径CIM查询无遗留（主执行证据；reviewer权限受限不代签），测试原生状态与脱敏日志保留，不动用户已有会话/数据。
- 继续条件：官方服务网络可用后，用相同固定版本重测真实执行、结构事件、审批、可靠取消、释放与同原生绑定恢复，再由独立角色确认门槛；当前不进入WP2–WP4产品编码。没有将网络故障解释成协议不支持或自动切换私有网关。
