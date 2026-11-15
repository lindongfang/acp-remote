# ACPI v10 独立测试定义审查

**结论：PASS。**

**未关闭阻断项：无。**

**范围：**本次只做编码前验收定义与规划接缝审查。没有运行 Cargo、产品测试、真实 Harness 或任何会触发产品行为的检查，因此不声称任何产品行为已经通过。审查只把十个用例、测试接缝、关闭顺序和清理判据视为可执行；产品实现、正式验收、Web UI、真实 Harness 和归档仍须另行完成。

## 输入与摘要核验

已读取仓库规则、tester/reviewer 角色、活动 proposal/plan/tests/design/tasks、相关规格、完整 v8 包、v10 合同与候选测试、历史审查、`SOURCE.json`、`SEAM-EXCERPTS.md`，并对照当前源码核实被引用的生产入口和逐案先决条件。

- v8 `MANIFEST.json` 实测 1960 字节，SHA-256 为 `2DEBB940EFE10664DFE6237E079F0F0D4291C3A8457AC28ABE6E6D1CD06E881D`；清单内 12/12 文件的字节数与 SHA-256 均匹配。
- v10 `MANIFEST.json` 实测 1961 字节，SHA-256 为 `8A7DE56D29B67542AEA9091DE96E8C3F6183B47DB399E6AF381AEA25D03A595A`；`revision` 为 `acpi-route-v10`，清单内 12/12 文件的字节数与 SHA-256 均匹配。
- v10 `SOURCE.json` 实测 7448 字节，SHA-256 为 `A885481609DCC8E5F082DE6B34483229D6A53D5A9543C6818224EC3C6DAD70B8`；其中 40/40 输入文件的字节数与 SHA-256 均匹配。
- v10 `CONTRACT.md` 实测 13910 字节、SHA-256 为 `8496885E9F0191362520982422722E9A99606B1DCD69FB4A97F92A3147210B2E`；`tests-candidate.md` 实测 17469 字节、SHA-256 为 `63F38E3F35BE48C64032E2182327A8274C51087721129F4910BF7C03F7AC95C2`，二者均与 MANIFEST 相符。
- 摘要链已不再沿用 v9 的旧 revision、旧合同摘要或旧测试长度。v10 候选、活动输入和实际源码对应同一组已核验摘要。

## Requirement / Scenario 映射

候选规格的 8 个 Requirement、13 个 Scenario 均有稳定用例关联：

| Requirement / Scenario | 用例 |
|---|---|
| 上传服务原子可用：服务无法完整发布、仅控制面启动 | R01、R10 |
| 连接持有唯一上传所有权：两条连接不能共享 uploadId、上传途中连接结束 | R04、R06、R10 |
| 四条上传命令使用稳定 JSON：查询反映真实准备阶段、释放与重复释放 | R01、R03、R04、R10 |
| 文本错误映射稳定：正文解析失败 | R01、R03、R05、R08、R09、R10 |
| ACPI 数据帧与 ACK 使用唯一出站顺序：成功接收及最后块重试、ACK 与随后文本结果排队 | R01、R02、R05、R07、R09 |
| 二进制错误和关闭优先级明确：停止先于待处理坏帧、上传错误时节点仍运行 | R02、R05、R06、R09 |
| 撤销先阻断出站再完成生命周期封存：ACK 发送前撤销先提交 | R04、R08、R09 |
| 异步命令取消不产生无主任务：finish 已交接但响应观察取消 | R03、R06、R10 |

未发现孤立 Scenario、重复用例 ID、以 ACK 冒充正文完成，或把未实现替代物写成产品成功的映射。

## R01–R10

| 用例 | 结论 | 可执行性审查 |
|---|---|---|
| R01 | **PASS** | 使用生产服务 bundle、真实 TLS/WSS 和既有 `Case`；`begin → Data → ACK → finish → status ready/bodyId → register/consume` 顺序完整。ACK 为精确 32 字节，状态 offset 与正文完成分别断言；其他连接用于无效 payload，重复及 foreign 安装后继续检查上传、队列读取和队列管理三路径。清理沿用 `Case` 的 shutdown、`join_actual()`、数据库独占和目录锁释放。 |
| R02 | **PASS** | 末块原样重试与三种错误分支均使用独立连接；事实、offset 和错误阶段均明确。错误分支断言零 ACK/JSON、唯一 4400、offset 不前移、`receiving` 清理和连接结束。 |
| R03 | **PASS** | 通过现有一次性 `CpuEntered` barrier 确定观察 preparing，释放后经真实 status 得到 ready/failed；长度不符仍保持 receiving。`storage.prompt_preparation_receipt().join()`、`observation().all_joined()` 和 receipt identity 均为现有可调用入口，不以 ready 或计数零冒充任务 join。 |
| R04 | **PASS** | A/B 双连接隔离、释放幂等、未知/foreign 身份、A2 正常断线后由新 A3 使用旧 ID 均分开执行。文本 `body_unavailable`、Binary 零 ACK/JSON 加唯一 4400、旧 owner seal 和其余连接继续工作均有明确结果。 |
| R05 | **PASS** | L5 主链已改到生产 `queue_management/development_tests.rs` 的 `Case`；该 `Case` 启动原 `NodeProcessOwner`、SQLite、真实认证 WSS 和生产 session-services bundle。`observe_next_document_socket_for_test()`、identity-bound client、窄 `attach_shared_read(&OpenedView)`、ACPV credit、`stream.ready()` 和 upload dispatch 零计数形成同一 attachment 路径；精确 16384 字节有效 prompt JSON、ACK、finish、ready/bodyId 断言完整。错误帧和 32769/16385 边界分别覆盖 4400/4429。endpoint 不公开 attachment 或服务句柄，服务仍由原 bundle 提供；`Case` 负责最终 join 和数据库/目录清理。 |
| R06 | **PASS** | 明确移除 peer Close 作为 `PendingReads` barrier 的取消来源，改由真实 Node stop 触发 biased shutdown。四项命令各有 BeforeCall/AfterCall 边界；finish AfterCall 先确认同一 preparation receipt 已 Registered，停止后释放 CPU，并以原 shutdown report 实际 join。InputAcceptedBeforeAck 的等待由原 `ConnectionStop` 取消，断言零 ACK、1001 和 owner seal。坏帧 stop-first 与 format-first 分开，不向已关闭连接补帧；connection、writer、owner 和 preparation 均要求原任务 join 或明确记录 unknown。 |
| R07 | **PASS** | `pause_next_write` 明确定位在 writer 已取帧、actor gate 之前。首 ACK 形成 1 in-flight，随后 256 次真实幂等重试形成 256 queued，下一真实申请触发 4429；close 优先、未发 ACK drop 和 queued slot/byte permit 回收均有判据。FIFO 分支另确认 ACK 在后续 status 前发送。byte 饱和只暂持原 semaphore 的 `MAX_OUTBOUND_BYTES - 31` permits，并明确 RAII 归还，不伪造业务帧或另建 sink。 |
| R08 | **PASS** | 合法但不存在 target 与语法无效 target 分别为 `invalid_target`/`invalid_request`。ACK 已出队但 writer 尚未取得 actor gate 时提交真实 revoke，释放后断言无 ACK、唯一 4410、owner seal、entries/receiving 清零及 connection 实际 join；反向顺序让已取得 gate 的发送先完成再提交撤销，避免等待同一 gate 自锁。B 的后续成功证明 actor 隔离。 |
| R09 | **PASS** | 接缝精确声明于 `Owner::typed_entry_guard`：完成 connection/owner/client 校验后、访问上传表或认证存储前消费至多一次故障。`NodeInputUploads::arm_route_fault_for_test(client, fault)` 以原 connection ID 绑定并返回原子命中 receipt，RAII 负责撤销未消费故障；正常 build 不含该字段、方法或分支。三种故障与 status/Data 的 3×2 矩阵、1 次命中、4410/4500、文本 `necessary_storage_unavailable`、Binary 4400、无结果/ACK、连接隔离以及 owner seal/entries/receiving 清理均已固定。 |
| R10 | **PASS** | 合同明确把完整服务集合构造、`reject_services` 检查和失败返回放在 `TcpListener`/router 创建前；这修复了当前基线中 `BeforeServices` 原本位于 bind 之后的时序，若不按该合同重排则测试不得算执行。固定地址连接失败、同一完整 bundle 支撑三项服务、`ready()` 失败、preparation `all_joined()`、`join_actual()` 和最终 storage/owner 收敛均有实际断言。control-only 使用现有 `NodeRuntime::start_control_only_for_test`、真实 pair/auth WSS，逐条验证 begin/finish/status/release 的 `product_not_ready`、`result=null`、无 uploadId/ACK；A2/A3 断线重连沿用 R04。 |

## v7 缺口

| 缺口 | 状态 | 审查结论 |
|---|---|---|
| G1：R06 避免 Close/`PendingReads` 自死锁并定义 accept-data 停止取消 | **CLOSED** | 禁止由同一 reader 读取 peer Close；真实 Node stop 的 biased shutdown 取消原 future，InputAcceptedBeforeAck 由原 `ConnectionStop` 取消，之后检查 1001、owner seal 和原任务 join。 |
| G2：R10 首次发布失败、listener 不可用及真实关闭 | **CLOSED** | 完整 bundle 后、listener/router 前的单次失败注入，固定地址不可连接，preparation report、lifecycle `join_actual()`、storage 资源和 owner 收敛均有断言。 |
| G3：R05 真实 ACPV 与精确 16384 字节完成 | **CLOSED** | 使用生产 `Case` 的原 authority/attachment，不依赖无 bundle 的 socket unit `Harness`；合法 credit、ready、零 upload dispatch、最大合法 Data、ACK、finish 和 ready/bodyId 均在同一路径。 |
| G4：R07 writer 边界、1+256、4429、close/drop/permit | **CLOSED** | writer 取帧后 actor gate 前的边界可确定形成 1 in-flight + 256 queued；后续 4429、close 优先、queued ACK drop 和 RAII permit 回收全部明确。 |
| G5：R03/R10 join 入口与 R09 固定矩阵 | **CLOSED** | R03/R10 调用现有 preparation shutdown report 和 lifecycle actual join；R09 固定三故障来源乘 status/Data 的六格矩阵、命中计数和清理。 |

## 历史发现关闭情况

| 发现 | 状态 | 定义/规划级依据 |
|---|---|---|
| ACPI-V6-001 | **CLOSED** | MANIFEST revision、候选字节数/SHA-256、SOURCE 输入字节数/SHA-256及候选来源说明均一致；v10 形成了可复核的冻结链。 |
| ACPI-V6-002 | **CLOSED** | `UploadRoutingError` 保留 `Business`、`Revoked`、`StorageUnavailable` 三种来源，typed 方法共享检查/状态变更，不在失败后二次查询推断来源。 |
| ACPI-V6-003 | **CLOSED** | 八个 Requirement、十三个 Scenario 均已映射；R05 已接生产 bundle，R06/R07/R10 的边界确定，R09 矩阵完整。 |
| ACPI-V6-004 | **CLOSED** | R08 分开断言 `invalid_target` 与 `invalid_request`。 |
| ACPI-V6-005 | **CLOSED** | R01 明确要求更新 `queue_read_valid_unimplemented_and_invalid_payload_are_distinct`：合法 `input.upload.begin` 改为成功并取得 uploadId，合法未实现改用 parser 已支持的 `view.open(targetId, reason=first_open)`，同时保留数字 target 的 `invalid_request` 和不存在 target 的 `invalid_target`。当前源码仍含旧反向断言，但 v10 已把具体修改写入允许写集和验收定义，不存在契约冲突。 |
| ACPI-V8-001 | **CLOSED** | R09 已固定 `Owner::typed_entry_guard`、一次性 connection-bound receipt、3×2 矩阵、命中 API 和 seal/entry/lifetime 清理。 |
| ACPI-V8-002 | **CLOSED** | R10 已固定生产 control-only 入口、真实认证 WSS、四命令 payload 和逐条结果。 |
| ACPI-V8-003 | **CLOSED** | R01 自身包含 finish、status ready/bodyId 和 register/consume，不再借用 R05 补足。 |
| ACPI-V9-001 | **CLOSED** | revision 已更新为 `acpi-route-v10`；MANIFEST 12/12、SOURCE 40/40 全部匹配，CONTRACT 已如实区分 v4 原字节 spec 与经 v5–v10 修订的 tests。 |
| ACPI-V9-002 | **CLOSED** | R05 不再使用缺少 session-services bundle 的 `socket_development::Harness`；主线固定到生产 `Case`，新增 endpoint 只注册同一 attachment 的 OpenedView，不拼装或替换服务 bundle。 |
| ACPI-V9-003 | **CLOSED** | 合法 `input.upload.begin` 的反向 `platform_unimplemented` 断言已明确要求移除并改为成功；合法未实现分支改用 `view.open`，没有删除原区分意图。 |

## 判定说明

当前源码是编码前基线：仍有两个独立 queue `OnceLock`、上传命令仍返回 `platform_unimplemented`、`BeforeServices` 仍在 listener bind 之后，具名开发回归也仍保留旧断言。这些都是 v10 已明确要求修改的实现工作，不是验收定义矛盾。关键是 v10 合同已经给出精确目标顺序、声明点、故障/观察 API、预期状态与清理判据；实现若偏离，R10、R09 或具名回归必须失败，不能用实现便利放宽。

因此，在本次定义审查范围内，ACPI-R01–R10 均可由一名不继承开发对话的 tester 按固定摘要执行，且无需削弱要求。

**这是定义与规划审查，没有执行任何产品测试或构建。**
