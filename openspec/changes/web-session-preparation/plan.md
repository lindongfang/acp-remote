## Execution Model

- 使用仓库固定的 Oh My Pi（omp）`task` batch 派发，按真实依赖将可并行工作包放在同一批；不新增调度器或第二套进度台账。
- 每个会写文件的工作包一次独立 spawn，`isolated: true`，`task.isolation` 为 patch 模式且 `apply: false`；不直接编辑主检出、不产生 commit/分支。
- 交付 `<id>.patch`、`agent://<id>`、`history://<id>`，主 Agent 先 `git apply --check`，再按 Merge Order 串行 `git apply`。
- 隔离区没有 `target/` 和 `node_modules/`，不运行仓库级验证；Project Verify 仅主检出执行。不得以隔离区“未能编译”冒充代码通过。
- 每个包另起内置 `reviewer` spawn；输入 patch 绝对路径、报告和相关规范。`changes-requested` 用新隔离 spawn 修复，再用新 reviewer；最多三轮，否则重新拆包。
- `session-activation-lifecycle` 完成并通过对应入口验收是实现基线前提，当前其未完成验收不标成通过。派发前记录整个未提交工作区快照，禁止 stash/reset/覆盖用户修改；本轮只生成规划，不派发实现。

## Work Packages

| ID | Goal / Scenarios | Dependencies | Agent | Write Scope | Deliverable | Verify |
| --- | --- | --- | --- | --- | --- | --- |
| WP1 | D1–D9 合同冻结；feature/帧/start+continue/证明/首条DDL/暂存端口/浏览器边界一致 | none（基线前提另见上文） | wp-coder | `compatibility/`、`schemas/sync/v1/`、`schemas/transcripts/transcripts.schema.json`（仅放宽 tag 上限）、`fixtures/sync/v1/`、`crates/sync-protocol/`、`clients/app/src/protocol/`、`scripts/check-contract-assets.mjs`；`docs/INITIAL_DESIGN.md`、`docs/FRONTEND_DESIGN.md`、`docs/SYNC_PROTOCOL.md`、`docs/CORE_PORTS_AND_STORAGE.md`、`docs/MODULE_ARCHITECTURE.md`、`docs/SECURITY_DESIGN.md`、`docs/IDENTITY_AND_AUTH_CONTRACT.md`、`docs/CONFIG_REFERENCE.md`、`docs/ACP_COMPATIBILITY_MATRIX.md`、`docs/adr/0014-web-session-preparation-before-first-message.md`、`GLOSSARY.md`、`README.md`（仅 sync-protocol 状态计数） | patch + 共享fixture、固定签名向量、端口/DDL合同与兼容表；新端口写 §5.11 `rust-planned`、新表 DDL 写独立顶层 `sql-planned` 小节（不进 §7）；无虚假实现声明 | PV1、PV2（PV1 保持全绿；未落地项用计划围栏标注） |
| WP2 | 准备目录、首条原子受理、正文/附件保留、准备附件暂存端口、派发围栏/继续及旧库升级 | contract:WP1 | wp-coder | `crates/core/`、`crates/storage-sqlite/`、`scripts/check-contract-drift.mjs`；`docs/CORE_PORTS_AND_STORAGE.md`、`docs/MODULE_ARCHITECTURE.md` | patch + 无准备落盘/事务故障/回显去重/崩溃窗口/旧库升级/暂存无DB行与24h回收测试；v16 migration 后把计划DDL移入 §7、把已实现的暂存/首条端口从 §5.11 `rust-planned` 移入常规 §5 块、删除计划小节并更新 drift 版本断言 | PV3、PV1 |
| WP3 | 专用准备证明签发/验签，节点/设备/来源/上下文绑定与撤销 | contract:WP1 | wp-coder | `crates/identity-auth/`、`crates/identity-keystore/`；`docs/IDENTITY_AND_AUTH_CONTRACT.md`、`docs/SECURITY_DESIGN.md` | patch + 固定向量、篡改/跨身份/跨域/重启验证与无任意签名入口测试 | PV4、PV1 |
| WP4 | Agent 临时端点、真实模式配置、资源代/容量/释放与首条转移 | contract:WP1；code:WP2 | wp-coder | `crates/agent-host/`；`docs/MODULE_ARCHITECTURE.md`、`docs/ACP_COMPATIBILITY_MATRIX.md` | patch + 真实stdio fake场景、旧代迟到/关闭失败/进程树测试；不写会话store | PV5、PV3 |
| WP5 | Sync 控制面/首次提交/附件暂存，组合根/启动收敛/关闭装配 | code:WP2,WP3,WP4 | wp-coder | `crates/server/`、`crates/app/src/`；`crates/app/tests/support/sync.rs`、`crates/app/tests/sync_commands_e2e.rs`；`docs/SYNC_PROTOCOL.md`、`docs/CONFIG_REFERENCE.md` | patch + 认证/共享fixture/新旧客户端/首条API跨层测试；feature仅在装配齐备后宣告 | PV6、PV1–PV5 |
| WP6 | 浏览器独立准备草稿存储与标签页所有权；刷新/离开/附件完整恢复 | contract:WP1 | wp-coder | `clients/app/src/platform/`、`clients/app/src/domain/`、`clients/app/src/attachments/`；`docs/FRONTEND_DESIGN.md`、`docs/SECURITY_DESIGN.md` | patch + Blob/配额/版本/晚写/duplicate tab/后台与清理测试；不扩摘要/imported缓存 | PV7 的平台/草稿套件与 typecheck、PV1；完整前端套件在 WP7 登记 wire.ts 新帧后全绿 |
| WP7 | 新建/配置/首条页面状态机，固定URL、换Agent/目录、未知URL回目录 | code:WP5,WP6 | wp-coder | `clients/app/src/features/`、`clients/app/src/sync-client/`、`clients/app/src/composition.ts`、`clients/app/app/`、`clients/app/src/components/`、`clients/app/e2e/page/`；`docs/FRONTEND_DESIGN.md` | patch + feature/page真实调用链测试，未协商显式降级，旧会话不改；`clients/app/src/sync-client/wire.ts` 的封闭命令/帧词表必须同步登记 `session.start`、`session.preparation`、`session.preparation.result`（WP1 只交付 protocol 层） | PV7、PV6、PV1 |
| WP8 | 真实Codex/Web隔离E2E、故障矩阵、文档状态收口 | code:WP5,WP7 | wp-tester | `crates/app/tests/session_preparation.rs`、`crates/app/tests/session_preparation_browser.rs`、`clients/app/e2e/session-preparation-live.mjs`、`scripts/probe-session-preparation.mjs`、`docs/evidence/session-preparation.md`、`docs/SESSION_LIFECYCLE_DISCUSSION.md`；本变更 `tasks.md` 验收证据链接 | patch + 可重跑探针/浏览器与跨层测试/脱敏证据，未执行保留未完成 | PV8、PV9 |

WP1 后可同批派发 WP2、WP3、WP6；WP4 真实依赖 WP2 的端口类型而非只依赖文档，不能提前造另一套。WP5 等待后端包，WP7 等待服务与平台，WP8 最后收口。

包内测试和对应文档随本包交付，不留给 WP8 首次验证。现有未提交的错误分类修复不是本变更实现；不把它作为旧会话迁移任务。若合同实施需要改新增列表外的机器校验脚本或清单，先更新写范围与合入次序，不静默扩包。

用户已选定方案A。D3.1与D6.1–D6.4中的闭合结果、准备target字段、跨连接验证顺序、error相关字段、三新增错误（`preparation.stale`/`preparation.already_committed`/`preparation.store_rebuilt`）及单次上传/无全文件复制要求是本轮已定合同；WP1负责将其编码到唯一机器定义与fixture，不再推迟选择上传通道或committed语义。本轮不增加consumed/ID消费墓碑，也不新增正式会话删除或独立首条关联清理路径。

## Merge Order

| Order | Work Package | Reason | Re-verify After Merge |
| --- | --- | --- | --- |
| 1 | WP1 | 先冻结唯一合同；准备签名与首条模式不能边编码边猜 | PV1、PV2；合同用 `rust-planned`/`sql-planned` 围栏冻结，PV1 保持全绿，不宣称已实现 |
| 2 | WP2 | core/storage先实现；与WP1重叠CORE_PORTS、MODULE文档；v16 migration 需同步 drift 版本断言 | PV3、PV1；计划 DDL 移入 §7 并删除计划小节后 drift 仍绿 |
| 3 | WP3 | 验签/签名实现；与WP1重叠IDENTITY、SECURITY | PV4、PV1 |
| 4 | WP4 | 依赖WP2；与WP1/WP2重叠MODULE，与WP1重叠ACP矩阵说明 | PV5、PV3、PV1 |
| 5 | WP5 | 装配后端；与WP1重叠SYNC/CONFIG，消费WP2/WP3/WP4 | PV6及PV1–PV5 |
| 6 | WP6 | 平台可早派发，文档合入必须晚于WP1/WP3：FRONTEND/SECURITY重叠 | PV7、PV4、PV1 |
| 7 | WP7 | 与WP1/WP6重叠FRONTEND，其他代码写范围与后端/平台无重叠 | PV7、PV6、PV1 |
| 8 | WP8 | 独占新增E2E/证据和讨论记录，无其他包写这些新测试文件 | PV8、PV9 |

已登记全部重叠：CORE_PORTS(WP1→WP2)、MODULE(WP1→WP2→WP4)、IDENTITY(WP1→WP3)、SECURITY(WP1→WP3→WP6)、ACP矩阵(WP1→WP4)、SYNC/CONFIG(WP1→WP5)、FRONTEND(WP1→WP6→WP7)。脚本重叠另登记：`scripts/check-contract-assets.mjs` 由 WP1 扩展帧族登记，`scripts/check-contract-drift.mjs` 由 WP2 随 v16 更新版本断言；两者无其他包写入。`schemas/transcripts/transcripts.schema.json` 的 tag 上限由 WP1 从 16 放宽到 32（新 domain 使用 17–25），同样无其他包写入。后合入包须基于已合入文件重新产最终 patch 并重跑对应验证，不能用旧上下文强贴文档。

## Project Verify

下列命令只在主检出执行。新增测试目标与脚本由对应 WP 创建后执行；当前规划轮不宣称这些文件已存在或测试通过。真实 Agent 与浏览器使用独立 data_dir/身份/测试目录，不重启用户服务，不复制或输出认证内容。Windows 正在运行的 `target/debug/acp-remote.exe` 会阻止覆盖构建；使用独立 `CARGO_TARGET_DIR` 并配置测试二进制路径，不能为跑测试擅自停止用户 Daemon。

| Check ID | Command / Working Directory | Scope / Pass Criteria |
| --- | --- | --- |
| PV1 | 根：`npm.cmd run check` | schemas/commands/errors/features/assets/ACP/docs/boundaries/drift/OpenSpec全绿；中间合同先行的漂移必须在最终PV9关闭 |
| PV2 | 根：`cargo test --locked -p sync-protocol`；`clients/app`：`npm.cmd test -- src/protocol/contract.test.ts` | 同manifest Rust/TS解码、未知字段、精确值、准备凭据与错误/版本fixture一致 |
| PV3 | 根：`cargo test --locked -p core -p storage-sqlite` | 首条正文先存、唯一受理/阶段CAS、事务回滚、回显去重、无prepared落盘、owned旧库升级、附件引用TTL |
| PV4 | 根：`cargo test --locked -p identity-auth -p identity-keystore` | 专用签名向量、完整绑定、撤销/错误域/篡改与服务重启；无平台依赖进入纯状态机 |
| PV5 | 根：`cargo test --locked -p agent-host -p acp-protocol` | 真实stdio配置/关闭、旧代隔离、超时/拒绝收口、Windows完整进程树，既有ACP保真不变 |
| PV6 | 根：`cargo test --locked -p server -p app` | 临时控制无收据、认证先于读取、首次提交与附件暂存、准备资源转移/关闭、旧create/Node Link/Zed兼容 |
| PV7 | `clients/app`：`npm.cmd run typecheck`、`npm.cmd run typecheck:e2e`、`npm.cmd test`、`npm.cmd run export:web` | 草稿/Blob刷新、页签隔离、晚写清理、mode/config真实值、URL不变、未知URL与断网分支、page假主机断言 |
| PV8 | 根：`cargo test --locked -p app --test session_preparation`；`cargo test --locked -p app --test session_preparation_browser`；`node scripts/probe-session-preparation.mjs --help`；按脚本说明配置隔离环境运行 `node clients/app/e2e/session-preparation-live.mjs` | WP8新增；真实服务/SQLite/浏览器与Codex空准备跨进程、附件首条、全部故障窗口、同页同ID；无隐藏prompt、无用户会话操作；未跑项标未完成 |
| PV9 | 根：`npm.cmd run verify`；`clients/app`：`npm.cmd run check` | 最终仓库+Rust与完整前端门禁；不声称CI供应链/公告/密钥扫描已本地执行 |

## Code Review

每行使用新的内置 reviewer spawn；作者自评不能代替。输入必须包含 patch 绝对路径、`agent://<id>`、`history://<id>` 与本变更规范；输出 `pass / changes-requested / blocked` 及有定位的 findings。

| Work Package | Reviewer Agent | Review Focus | Pass Condition |
| --- | --- | --- | --- |
| WP1 | reviewer | 用户需求追踪、闭合词表/feature/transcript、端口纯度、旧协议兼容、DDL与证明字段足以编码 | 无未闭环CRITICAL/MAJOR；无关键安全/原子性决策留给实现猜测 |
| WP2 | reviewer | 未提交不落盘、同事务正文/附件、CAS、首条回显、全崩溃窗口、无旧会话推断 | 无未闭环CRITICAL/MAJOR且故障注入可重跑 |
| WP3 | reviewer | 无任意签名入口、原文transcript、身份绑定、不是bearer认证、撤销/重启及平台边界 | 无未闭环CRITICAL/MAJOR |
| WP4 | reviewer | 临时端点真实设置与关闭、代隔离、容量转移、无hidden prompt/进程泄漏 | 无未闭环CRITICAL/MAJOR |
| WP5 | reviewer | 协商/认证优先、prepared与owned隔离、附件无假会话、启动不自动派发、app无业务规则 | 无未闭环CRITICAL/MAJOR |
| WP6 | reviewer | origin/节点/设备/标签页隔离、复制tab、Blob完整性、quota/晚写/冻结/BFCache与删除 | 无未闭环CRITICAL/MAJOR，无imported缓存放宽 |
| WP7 | reviewer | 同页同ID、保存与派发区别、重试核实、陌生URL、换Agent/目录清空、真实设置不回退 | 无未闭环CRITICAL/MAJOR |
| WP8 | reviewer | 真实与fake分层、固定故障窗口、Codex原生边界、证据脱敏、不把未跑写通过 | 所有必需场景有可读证据，未跑项保持待办 |

## Main E2E

```yaml
mode: required
```

Web 使用 omp browser prelude，进程树/真实 Codex 资源检查使用 computer prelude（按仓库 `.omp/e2e.yml`），均针对隔离测试节点。截图、export成功或单纯脚本退出0不能代替以下可观察断言：

| Case | 必需观察 |
| --- | --- |
| E1 准备不登记 | 目录/其他设备/SQLite无owned准备行；后端分配的URL存在且真实模式可切换 |
| E2 刷新与独立tab | 文字/非默认设置/附件内容完整恢复；另tab不共享归属；A离开不删B；复制URL跳目录 |
| E3 变更与清理 | 更换目录/Agent换后端ID并清空；主动离开再新建为空；后台保留草稿 |
| E4 重建 | Codex空临时会话的原进程消失后，同ID重新准备；不调用旧正式resume兜底，不自动prompt |
| E5 首条保存 | 附件全齐、内容/命令/任务/引用原子提交后才派发；正文无Agent回显仍可读，回显不重复；单次成功每附件一份上传，无转正全文件复制/重哈希，分段计时 |
| E6 故障围栏 | 保存前/保存后/dispatch_started后/发送后各窗口强制停止隔离节点；判定未派发/uncertain准确，重试或重启无第二次执行 |
| E7 页面与兼容 | 保存成功/失败URL和页面实例不变，快照随后可见；未知ID仅权威不存在跳`/`；旧服务端/旧会话/Node Link/Zed回归 |
| E8 准备附件跨连接 | 现有附件端点双feature独立认证，target两分支互斥；篡改/跨设备/清单混用/未重绑拒绝；断线重绑同偏移，旧连接迟到写被拒；过代先restore后重传 |
| E9 committed收口 | 首条响应丢失后restore返回committed及first/current请求关联，同页正式读取；正式受理优先于旧代/lease/目录变化；上传/release/start竞争不重复认领或释放正式端点 |

## Risks

- 已运行二进制、在途未提交基线和旧schema可能干扰验证：独立构建/data_dir并记录上游基线，不覆盖用户可执行文件或数据。
- 文档/协议多包重叠：严格按 Merge Order 重新产 patch，所有调整先更新本表写范围。
- 固定向量/新接口未冻结造成伪并行：WP1 未检视通过前不派实现；WP4 等 WP2 真端口，WP7 等真实服务。
- 本机缺少真实Agent/浏览器/凭据：保留 WP8 未完成并记录原因，不让假主机替代真实空准备重启验收。
- 当前轮为规划：所有 tasks 为待办；不能把规划校验通过说成实现或E2E完成。
