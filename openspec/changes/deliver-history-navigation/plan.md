# 历史导航实施计划

## Execution Model

主执行者按 openspec/role/ 分配角色。独立 reviewer 合审规格、设计及独立 tester 的用例后，coder 实现并自测。集成后由独立 tester 和 reviewer 对同一提交验收和检视。

## Parallel Assessment

轮次模型与协议字段冻结后，快照存储和 Web 分别在独立工作区开发。节点集成需要快照接口，浏览器联调需要节点命令，两者串行。

## Work Packages

| ID | Goal / Scenarios | Dependencies | Agent | Write Scope | Deliverable | Verify |
| --- | --- | --- | --- | --- | --- | --- |
| WP1 | 可证明轮次、方向、参数与游标范围 | 无 | coder | app/src/domain、app/src/adapters/acp、app/src/management_ws | 分页模型、命令合同与单测 | HN-003 至 HN-007 |
| WP2 | 旧库首次翻页、增量索引及换代 | WP1 | coder | app/src/transcript_cache | 原子迁移与有界区间读测试 | HN-008、HN-009 |
| WP3 | 独立只读子历史查询、按来源选页、权限复核、跨代定位、实时衔接 | WP1、WP2 | coder | app/src/node、app/src/adapters/acp、app/src/application/native_dispatch、app/src/transcript_cache、app/src/application/shared_views、app/src/management_ws | 标准命令接线与集成测试 | HN-010 至 HN-024、HN-029 |
| WP4 | 父子独立翻页、折叠恢复与迟到响应处理 | WP1；联调依赖 WP3 | coder | clients/web/src、clients/web/tests | 分页组件、控制器与交互测试 | HN-002、HN-025 至 HN-027 |
| WP5 | 独立验收与交付检视 | WP1 至 WP4 | tester、reviewer | testing、当前变更 plan.md 与 tasks.md | 固定提交的执行证据与审查结论 | HN-001 至 HN-029 |

## Merge Order

| Order | Work Package | Reason | Re-verify After Merge |
| --- | --- | --- | --- |
| 1 | WP1 | 提供轮次和分页合同 | 模型与协议单测 |
| 2 | WP2 | 存储依赖轮次分类 | v1 迁移及索引一致性 |
| 3 | WP3 | 串行接入管理命令和存储 | 冷读、重放、撤权与并发读取 |
| 4 | WP4 | 在真实节点复核协议匹配 | Web 单测及浏览器翻页 |
| 5 | WP5 | 固定集成提交 | 全套独立验收与检视 |

## Project Verify

| Check ID | Command / Working Directory | Scope / Pass Criteria |
| --- | --- | --- |
| PV1 | 根目录运行 node testing/suites/verify-suite.mjs --change deliver-history-navigation 和 npm run check:spec | 场景映射与规划结构通过 |
| PV2 | 根目录运行 npm run check、npm run build:rust、git diff --check，并执行 tests.md HN-001 至 HN-026 | 工程与协议、存储、节点和 Web 断言通过 |
| PV3 | 按 tests.md HN-027、HN-028 从生产 Web 入口操作 | 受控和真实 ACP 链路各自有独立结果 |

## Code Review

| Gate / Scope | Reviewer Agent | Review Focus | Pass Condition | Status | Reviewed Snapshot / Notes |
| --- | --- | --- | --- | --- | --- |
| 编码前门禁 | 独立 reviewer | 父步骤入口、游标权限、轮次证明与用例充分性 | PV1 通过且语义阻断清零 | passed | 原范围 7aeff9a2 及子查询增量 405957a1 经独立 reviewer 通过；HN-PR01 由原提出者关闭 |
| 集成交付门禁 | 独立 reviewer | 受测提交、真实链路、资源与安全 | 独立验收通过且阻断项关闭 | pending | 尚无受测提交 |

## Main E2E

```yaml
mode: required
```

使用隔离 data_dir、真实 Windows 节点与生产同源 Web。HN-027 使用受控 ACP，HN-028 使用已授权的真实 Codex；结束后验证子进程树退出并清理测试所建数据。

## Risks

跨代际按轮序号定位只保证当前范围；覆盖目标删除和迟到响应。手输 ID 属于不可信输入。HN-020 验证目标关系及访问权限，HN-027 从历史面板验证合法子目标可读。

## Handoff

交付本地功能分支受测 commit SHA、干净状态、当前变更规则与 tests.md。独立人员从固定提交执行，原始证据存于 `testing/runs/deliver-history-navigation/`。

## Findings

| ID | Source | Severity | Commit SHA | Finding | Fix Commit | Status | Retest / Re-review |
| --- | --- | --- | --- | --- | --- | --- | --- |
| HN-R01 | 独立 reviewer | 高 | 4dd96797 + 规划工作区 | 缺少可靠分轮证据 | 7aeff9a2 | closed | 原 reviewer 完成规划复核 |
| HN-R02 | 独立 reviewer | 高 | 4dd96797 + 规划工作区 | 实时新增轮次缺少分组边界 | 7aeff9a2 | closed | 原 reviewer 完成规划复核 |
| HN-R03 | 独立 reviewer | 高 | 4dd96797 + 规划工作区 | 游标范围编码与方向校验 | 7aeff9a2 | closed | 原 reviewer 完成规划复核 |
| HN-R04 | 独立 reviewer | 高 | 4dd96797 + 规划工作区 | 旧页遗漏迟到的工具补丁 | 7aeff9a2 | closed | 原 reviewer 完成规划复核 |
| HN-R05 | 独立 reviewer | 中 | 4dd96797 + 规划工作区 | 子首页与重新定位语义混用 | 7aeff9a2 | closed | 原 reviewer 完成规划复核 |
| HN-F01 | 独立 tester / HN-022 | 高 | 1808ce2 | 冷读旧页时，页内工具能显示最终状态，但指向页后 rawOutput 的引用缺少可读取的原始字节 | 1feb0075 | closed | 原 tester 在 871593b 以真实管理 WebSocket 按 rawRecordId、byteStart、byteEnd 回读并解码约 48 KB 的 UTF-8 rawOutput；跨包字节不含相邻字段 |
| HN-F02 | 独立 tester / HN-012 | 高 | 1808ce2 | 根忙时按要求拒绝；根空闲但仅有根重放能力、无子快照或子事件时，子首次读取返回 unsupported_capability。原用例预期成功 | 70ab4edd | open | 根忙拒绝、自动投递子事件后读取成功、无子数据时受限均已独立验证；原报告人在 126ed0eb 的干净检出独立运行 10 项子查询测试全部通过，覆盖 v1/v2 协商、根忙与空闲、共享读者撤权、根工作互斥、九类中断或失败，以及发布时撤权或抢先换代。补充 WSS 探针已验证 v1/v2 六类协商或方法拒绝、无扩展时复用快照、30 秒无进展、错误关联 ID 和 17 种坏批次；逐批核对父会话、另一来源同名子会话与子快照原子性。最终固定提交整组复跑中 |
| HN-F03 | 独立 tester / HN-027 | 高 | 74e7c2bb | 双浏览器跨代定位到正确的 11–15 轮后提示读取或展示准备失败，第 15 轮新正文未显示 | dc6bb39d | open | 30ff6bd0 独立双 Edge 复验通过，旧游标定位到第 15 轮，显示新代 11–15 全部正文，无旧正文；待原提出者关闭 |
| HN-F04 | 独立 tester / HN-028 | 高 | 5f1a4a05 | 真实 Codex 在节点重启并显示释放后仍拒绝外部追加，第 13 轮八次尝试均返回 active writer | 30ff6bd0 | open | 实施侧在 1ee6bbf1 使用真实 Codex 和隔离配置 thread_unload_delay_secs=0，通过外部第 13 轮、刷新显示 9–13、新游标续读 4–8 及退出清理。集成侧 HN-015 释放后重新校准与换代分页回归通过。30ff6bd0 的独立真实复验通过。另在隔离默认配置下，关闭响应后 65.006 秒发起外部续写并成功，精确卸载时刻未测；待原提出者关闭 |
| HN-F05 | 独立 tester / HN-018 | 高 | 强制倒序实节点探针 e330569 | 刷新换代后，迟到的旧代定位有页元数据，给出额度后 20 秒无正文，复现 3/3 | b4092cc5 | closed | 静态页独立持有发送状态，沿用原快照及授权检查。原探针在实施侧连续三次通过，集成侧三项 HN-018 定向测试通过，含新代 291–300、另一目标输入及迟到的旧代 491–500 正文与 SnapshotEnd。原报告人在干净检出 b4092cc5 上用未改动的原探针独立复验通过（1/1，1.65 秒），复读仍为新代。日志 hn018-b4092cc5-retest.log，原三份失败记录保留 |
| HN-F06 | 独立 tester / HN-014 | 高 | fdf612f8（集成 513df523） | 独立第三台配对设备打开普通视图并完成自身重绑。两组均已输入 26/27，原分页流在 8 秒内未收到轮次标记及完整正文，也未收到切换通知 | 待确认跨读取换代后的页面行为 | open | 两方向旧游标与新连接页均正常。候选 c0e31437 在两组原探针各连续通过两次，但已完成重放时 reopen 直接复用内存视图，缺少 structured-history 要求的原生重读。候选未集成，待同时验证同内容重放的实时追加和外部更新后的重新读取；真正换代的页面行为另待确认 |
| HN-F07 | 独立 tester / HN-020 | 高 | 64719399 | 仅有助手帧的子会话 C4 首次分页成功，违反缺少可证明轮次时返回 unsupported_capability 的合同 | 766eddf0 | closed | 原报告人在干净检出复跑原探针矩阵 26/26 通过，C4 已按原断言拒绝；父页 21–25 首位置稳定、空子零帧、20 次展开及子实时 11/12 均通过。集成侧定向回归 22/22 通过。另有独立冷缓存专项探针待执行结果 |
| HN-PR01 | 独立 reviewer | 高 | 1feb0075 | 发起者撤权与共用查询终止条件冲突 | 405957a1 | closed | 原审查人复核通过，A 撤权即时关闭，仍有权的 B 完成原查询 |

## Verification Results

当前为集成预验收，最终统一受测提交待锁定。独立 tester 在 1808ce2 的隔离检出加入验收探针，实测 HN-011 的未证身份拒绝、已证根首次重放、离线续页、淘汰后定位及 50/50 满额拒绝、释放一个名额重试。HN-018 的定位途中撤销子场景实测返回 4410，另一客户端仍可读，原生对端未收到取消。HN-022 的原始失败及独立复验见 HN-F01。同一受测提交上，固定 SQLite 读事务期间的同代追加与并发换代均已通过。正式门禁状态等待验收代码合入和统一提交复验。

独立 tester 另已验证以下子场景。HN-012 中，根执行期间拒绝读取冷子会话；根空闲且自动投递子目标专属用户和助手帧后，子首页显示第 1–10 轮，整树占 1 个名额。未投递且无子快照时返回 unsupported_capability。HN-013 中，5 MiB 内容传输中途断线后可完整重读第 6–15 轮。HN-016 的父会话在无跨加载稳定 ID 的条件下遍历 1,000 轮、100 页，中途快照淘汰后重新定位，逐帧核对通过，共发生两次 session/load。子方向在 126ed0eb 已独立验证 100 页元数据及中途淘汰后的定位；补充 WSS 探针逐条核对 100 页共 2000 条原始用户与助手帧，淘汰后定位 501–510，观测到两次子查询首批，无子 session/load。

HN-018 的并发定位与失败不留下半截代际已由独立 tester 实测。HN-021 原探针使用 product.status，在既有平台返回 platform_unimplemented。按增量规格对「实际可用会话操作」的要求，改以同一连接上的 session.capacity.inspect 与会话列表验证，保留原失败记录。首次运行中，pure_memory 收到首轮 input.consume 响应后，20 秒内对端未收到 session/prompt，该次初始化失败记录保留。独立 tester 复验时，在新建纯内存根会话的同一原生连接投递 25 组用户与助手帧，确认 50 帧落盘后重启。三页离线读取、显式刷新后复用旧游标、淘汰后返回 unsupported_capability、同一管理连接查询容量与会话列表均通过。纯内存冷快照标明 continuable=false；直接提交输入返回 settled/queued，对端未收到 resume 或 prompt。resume_only 则实际恢复会话并将新提示送达对端。复验探针已集成（53dfbc95），两种能力在集成分支复跑通过。

HN-001 在 871593b 的全仓检查被 HN-028 验收驱动的两处 Clippy 错误阻断（default_trait_access、too_many_lines），随后独立 tester 在干净检出 234a7eca 运行 npm run check，退出 0。Cargo 单编译作业、Rust 三个测试线程；Web 46 个文件共 1072 项通过，Rust 主库 1114 项全部通过（494.01 秒），其余默认目标通过。日志 all-check-234a7eca-jobs1-t3.log。该提交早于独立子历史查询与多视图集成。

独立预验收逐项报告已在集成工作区保留副本 testing/runs/deliver-history-navigation/independent-precheck/39b14237-results.md。HN-008 后续独立补测中，基线 4dd96797 的测试二进制打开 v2 WAL 库，完成配对、提示与批准。物理关闭后数据库 SHA-256 不变，临时目录已移除。候选版本上的 v3 库和迁移事务冲突两个场景也通过。HN-016 的子方向逐帧遍历后续实测见上。HN-018 强制倒序时的原始失败与独立复验见 HN-F05。HN-023 已覆盖双客户端交错翻页和撤权，1799/1800 秒边界已有容量领域证据，实节点 1799 秒仍占 1 名额、1830 秒扫描后为 0，库中确认 idle_timeout、finished 和物理关闭。首次收尾因合法控制变更通知先到而中断。234a7eca 的 1860.35 秒独立复跑完成 50 次交错请求和持续翻页，确认原空闲计时未变；1799 秒占用为 1，1830 秒扫描后物理关闭、容量为 0，随后 B 的现有连接撤权收到 4410。该次未重连；766eddf0 上另行实测，撤权读者用新连接携带原游标收到 4401。其余复合步骤的覆盖范围以逐项记录为准。

独立 tester 在 39b14237 运行的 1107 个 Rust 库测试全部通过。宿主 NoDefaultCurrentDirectoryInExePath=1 影响两项 platform_owner 集成测试；仅在测试子进程的 PATH 加入夹具工作目录后，两项单独复跑通过。最终集成提交的默认环境全仓检查仍待执行。

HN-008 补充探针已合入 9500a6b。集成侧串行执行未来库及失败迁移两个场景通过；并行运行时失败迁移场景在节点退出超预算，单独运行 STC-004 也在同一关停断言失败。e330569 在关停前等待原生审批回复送达与输入队列结清，沿用原关停预算和断言。独立 tester 连续 10 批并行三例通过（30/30），集成提交 06a7aa73 连续 5 批并行三例通过（15/15）。原失败日志缺少当时的队列状态。真实 Codex 验收驱动已合入 c7139bac。

独立 tester 在 1d1e03eb 实跑 Rust 的 hn_ 过滤集，26/26 通过，另有 1088 项未选中。HN-010 完整组合覆盖 25 次原生提示、2 条前导记录、同代后台校准、节点重启后跨客户端复读，以及暂停流控额度时的容量淘汰。该提交的 Web 检查、生产构建与 OpenSpec 检查通过。集成提交 464daa4b 的 app 和 clients/web 与该受测提交一致。本批测试代码还有三处 Clippy 风格问题待修正。

任务 1.5 开发自测先验证旧实现缺少初始化声明，再通过节点管理 WSS 连接受控 ACP v1/v2 验证子首次读取。两名读者共享查询时，发起者撤权收到 4410，另一读者完成分页，查询结束后根可继续工作。千轮子历史在快照淘汰后用旧游标得到失效结果，重新定位后读到末轮。协议测试覆盖坏块及迟到响应，数据库测试覆盖最终授权失败回滚、摘要相同时保留代际和较新代际抢先发布。

子历史补证的中间日志 child-interim-matrix.log 记录 4 项通过、3 项失败。首批空结束、根重放与配置切换的夹具断言已校准，最终同提交复跑前保持 HN-F02 开放。
