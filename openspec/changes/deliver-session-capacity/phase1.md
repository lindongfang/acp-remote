# 第一阶段实施边界（2026-11-16）

用户原文：“同意，按这个第一阶段范围继续实现，自动回收保留为未完成项。”

本阶段交付全入口默认50主执行树名额、同来源原生身份及所有者复用、未知活动保护、满额拒绝、显式释放后的必要管理协调和 Web 资源事实/限制展示。通用计时及安全条件只作开发验证。真实自动回收、完整活动及关闭窗口证明、原32例中的完整正向回收及真实L6继续未完成，整个 change 不归档。本文取代其他产物中“分阶段尚未批准”和“完整门禁阻止一切编码”的当前状态描述，历史报告保持原快照。

## 实施合同

- 容量为节点管理事实，不由页面、进程数、队列长度或缓存计算。预留与加载共同占默认50额；无额直接返回 `session_capacity_full`，不等待、不排队、不取消旧工作。
- 添加 schema12 必要管理表。SQLite `BEGIN IMMEDIATE` 串行检查、预留、原生身份绑定与归还；每次调用仍由既有 registry 物理连接所有者管理。提交或物理关闭未确认时，不执行新的原生加载。
- 创建以原 operation ID 预留，在原生 `session/new` 写入前可靠保存；原 Created 响应绑定来源+原生ID及原 source instance。结果未知保留预留和原所有者。Discover 不加载会话，不占额。
- 同一操作重复复用原预留；Created原生ID若与另一owner冲突，或者响应解析/必要保存未完成，保留两项潜在资源责任，不覆盖owner、不把第二预留直接归还。
- 已有身份先查原所有者：同来源同身份即使从另一配置入口访问，也复用原 source/资源/名额；不能用新配置覆盖已运行资源。不同来源同短ID分别计数；旧代际未协调不允许新所有者接管。
- 执行加载在 resume 写入前预留；SourceOnly 标准重放在其 `session/load`/`session/resume` 写入前同样预留。只取得读取能力、辅助进程初始化、无需加载查询不占名额。重放失败/取消后，只要原生加载可能发生就保留名额。
- 主树证明与排他归属分开。官方 ACP v1/v2 Session Setup 的标准 `session/new` 创建有独立上下文/历史/状态的新会话；仅在原始标准请求配置不含未解释扩展关系语义、原 request/response/scope/config 一致时，以该原始成功响应建立独立主会话事实，不从任意 Created DTO 或 target ID 推导。本阶段保留未知扩展原样，但有未解释扩展的创建返回 `session_tree_unverified`，不静默删除字段或发出可能创建未知关系资源的请求。已知子目标必须由可靠关系映射到主树。当前没有可靠关系证据的外部身份不得猜为根，实际加载返回 `session_tree_unverified`；不加载的查询仍按自身能力提供。不为子建立独立名额或关闭策略。本阶段验证原始标准 `session/new` 的独立主会话证明、身份绑定及未知/扩展拒绝边界。当前未实现可信父子关系映射端口；已知子目标及无可靠主树证明的外部身份不得实际加载。父子共享名额的正向验证和真实子树支持保留为后续未完成项。协议语义来源：`agentclientprotocol.com/protocol/v1/session-setup`、`agentclientprotocol.com/protocol/v2/session-setup`，仅证明独立会话，不证明后台静止或安全自动回收。
- 显式释放沿既有认证/撤销及取消工作语义。原生关闭、runtime必要关闭事实、外层释放操作结果及容量更新分别协调；全部确认后才返额。外层保存失败/结果未知继续占额；重试只协调原操作，不重复原生副作用。创建/重放拥有的资源即使没有普通执行 `ResourceSlot` 也保持所有者及关闭责任：在原 source 下持有按 native 标识的独立关闭操作/封存依据，通过原协议 writer/receiver 关闭，不为关闭而先 resume，不关闭同进程其他会话。外层 Released 重试也必须幂等协调容量，不能因此前结果已提交而短路；容量归还与释放结果在同一必要事务完成。
- 重启恢复容量表及既有原生资源/创建操作；未确认收敛的旧代际继续占用，不凭时间、进程连接消失或新内存账本释放。升级已有未关闭资源先纳入容量，超过50时保留旧资源并拒绝新增。
- 生产自动回收始终受限，无扫描或定时关闭。Web 展示默认上限、已占用（含预留/未协调）、自动回收未启用及显式释放入口；这些不增加页面会话生命周期状态，不返回正文、密钥或启动参数。
- 领域计时以单调时间建模；实际收到重放刷新，纯页面/心跳/缓存不刷新。完整活动和关闭窗口任一未知均不产出安全候选；开发替身不被当成真实 Harness 证明。

## 工作包及串行顺序

沿用 WP1–WP5 角色。WP2 的存储实现可在 `.tmp/capacity-storage/` 的 HEAD 原字节副本隔离编写，容量接口先冻结，再与主执行者的原生运行/Web接线并行。存储包只交付patch/文件清单，由主执行者检查并串行合入，不并行写产品工作区。合入顺序为 schema/storage → 原生入口/释放 → 认证管理协议/Web，随后重跑真实集成检查。独立 tester 仅写 tests.md 与专属 phase1 验收目录；reviewer 只读产品并写专属报告。正式有副作用验收在集成快照后串行运行。

WP2 精确范围：`app/src/domain/session_capacity.rs`、`app/src/application/native_dispatch/capacity.rs`、既有 native_dispatch source/read/reply/resource_close/runtime/mod 接线、`app/src/session_storage/session_capacity*.rs`、schema.rs/mod.rs、storage.rs 的版本声明及必要迁移回归、node/session_services.rs。复用现有数据库连接和关闭所有者，不引入新依赖。

WP3 精确范围：`app/src/management_ws/session_protocol.rs`、session_commands.rs、`clients/web/src/management/` 对应协议/client/error/容量展示、main.ts 及对应真实测试；按需维护 styles.css。UI 沿用现有控件和样式。

WP4 测试写范围：tests.md、`testing/runs/deliver-session-capacity/tester/2026-11-16-phase1/`。WP5 写范围：`testing/runs/deliver-session-capacity/reviewer/2026-11-16-phase1*.md`。

## 第一阶段验收与完成定义

独立 tester 对原32例分支建立阶段矩阵，不删除或放宽完整预期。必需验证：默认50及最后一额并发；Create预留与绑定；所有加载入口无绕过；跨配置原owner复用；不同来源隔离；未知主树拒绝；失败/取消/重启/必要存储故障保额；显式释放协调后返额一次；认证与Web错误/限制；生产无自动关闭。新测试须有旧实现失败的 Red Check。

实际检查入口来自 README/package.json：`npm run check`、`npm run build:rust`，Web 独立 check/test/build；Rust产品测试使用专属数据/进程及串行运行，避免已有并发/环境异常掩盖结果。报告逐项区分定义检查、开发fixture、真实节点/SQLite/管道和真实 Harness；必需阶段检查失败/未执行不签阶段完成。

原 CP-011/023/026/032 完整真实回收/子树正向分支保留 not-run/deferred，不记阶段豁免为完整通过。后续完整交付仍须真实50会话、实际30分钟、安全回收及独立验收。阶段进度使用新增6.x任务，原完整范围任务不因阶段部分实现勾选。
