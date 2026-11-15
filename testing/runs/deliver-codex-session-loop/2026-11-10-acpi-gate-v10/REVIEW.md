# ACPI v10 独立编码门审查

**结论：PASS。**

**阻断项：无。**

**放行范围：**仅放行 `acpi-route-v10` 所定义的 Rust 认证 WSS 管理输入上传路由编码前门。此结论不表示产品实现、构建、R01—R10、Web UI、真实 Harness 或最终独立验收已经通过；归档仍须在实现、同版验收及交付检视完成后另行判断。

## 审查口径

本次以不继承开发对话的独立 reviewer 口径，读取仓库 `AGENTS.md`、OpenSpec 配置及 reviewer/tester 角色、活动 proposal/design/plan/tests/tasks、相关规格、v10 合同与候选、历史审查报告和实际源码。历史报告只用于核对原问题，不作为 v10 通过证据。

当前 HEAD 为 `65ec7815bb0280f4519989d93a7b4ed3b4b37994`。工作区有大量既有已修改和未跟踪文件；被审查源码均按当前实际字节读取，没有覆盖用户修改。当前 `management_ws.rs`、`node/lifecycle.rs` 等仍是编码前基线：双队列 `OnceLock`、上传返回 `platform_unimplemented`、listener 先于服务装配、具名回归保留旧反向断言。这些正是 v10 已明确要求修改的内容，不是候选定义冲突。

本报告没有运行 Cargo、构建、产品测试、真实 TLS/WSS、真实 Harness 或任何会触发产品行为的检查，也没有修改产品代码、活动规格、验收定义或 Git 索引。除本文件外未写其他文件。

## 固定输入

实际读取并计算 SHA-256 的结果如下。

| 输入 | 字节 | SHA-256 |
| --- | ---: | --- |
| `MANIFEST.json` | 1961 | `8A7DE56D29B67542AEA9091DE96E8C3F6183B47DB399E6AF381AEA25D03A595A` |
| `SOURCE.json` | 7448 | `A885481609DCC8E5F082DE6B34483229D6A53D5A9543C6818224EC3C6DAD70B8` |
| `CONTRACT.md` | 13910 | `8496885E9F0191362520982422722E9A99606B1DCD69FB4A97F92A3147210B2E` |
| `DESIGN-DELTA.md` | 3298 | `287D71E89DC3BA25E9E4724C8017B3F4D4D63813708B6F52ACA314FCC7D9354D` |
| `spec-candidate.md` | 11259 | `E450C05ED0FE24D87A31264137441A3EF597CD0FB9E40234B13A162EAC39976A` |
| `tests-candidate.md` | 17469 | `63F38E3F35BE48C64032E2182327A8274C51087721129F4910BF7C03F7AC95C2` |
| `AGENTS.md` | 4485 | `1019D8854ACA68D2519E599D0F3A1484E49667273708A49AC03818834AA38BE3` |
| `openspec/config.yaml` | 11985 | `2838C774A014E389BE5EC702BD2F293F1686012D7D8F6C01FB500A8C21980A2F` |
| `openspec/role/reviewer.md` | 2860 | `D36CD77FB71906FA3A6052C1FEBEA0F62535FEC450D9343B5648369D68320E31` |

`MANIFEST.json` 的 revision 为 `acpi-route-v10`，其中 12/12 文件的字节数和 SHA-256 与实际一致。`SOURCE.json` 中 40/40 输入也与工作区实际字节数和 SHA-256 一致。`SOURCE.json` 的 `scope` 文本仍写作 “ACPI v8 coding-gate and test-definition inputs”；这是非阻断的来源说明遗留，本版身份由已核验的 `MANIFEST.json` revision 和完整文件摘要链确定，40 项实际输入没有失配。

同时读取的历史输入摘要如下；它们只作为原问题和历史条件的输入，不是 v10 通过证据。

| 历史输入 | SHA-256 |
| --- | --- |
| `TESTER-REVIEW-v8.md` | `590F6A94E9D247C82AD4303945E7EFEF9427A6069F47EE69FE29D36093C300D9` |
| `TESTER-REVIEW-v9.md` | `0E3BF6296189FC8891D0DD4FC43F09ED94A949D7E18B12E5361E7070F0402D63` |
| `TESTER-REVIEW.md` | `5D50EDAFD6BDE460664620F39EAEF2AFDEBC7F370A7866FB61D0745CEA8F1467` |
| `REVIEW-v6.md` | `4E9E0CDDFFD3AE5FFED0221FEC8FB84FF6E3A0383DA328CFE5571F39D3ABAC07` |
| `TESTER-REVIEW-EVIDENCE.md` | `EC51DBFD4FBDF320D3BA792808266B6F4569BA7CA616E3F8439BD80C467F4BEE` |
| `REVIEW-v4-reference.md` | `C7AD87A6AF88A9E168D11E7F4623065FCA0D2AF1C77C63ACEB212D458A8D2037` |
| `TEST-DEFINITION-REVIEW-reference.md` | `87C76AF705D25DB41CB8A659A51A93B082DEC34B5D20238FDCB76E43E69E2E7A` |
| `CONTRACT-v4-reference.md` | `6BD506F56EC4B5AE7BA318E75C004C96463D9B692CF42F8998ECE6BA6B502C69` |
| `SEAM-EXCERPTS.md` | `3FDF9E4994F4BF6CA9338021423AF7B98B0ED47B506A9CCBA6251CE5588E01B2` |

## 需求与用例映射

`spec-candidate.md` 的 8 个 Requirement、13 个 Scenario 均有稳定用例关联：

| Requirement / Scenario | 用例 |
| --- | --- |
| 上传服务原子可用：服务无法完整发布、仅控制面启动 | R01、R10 |
| 连接持有唯一上传所有权：两条连接不能共享 uploadId、上传途中连接结束 | R04、R06、R10 |
| 四条上传命令使用稳定 JSON：查询反映真实准备阶段、释放与重复释放 | R01、R03、R04、R10 |
| 文本错误映射稳定：正文解析失败 | R01、R03、R05、R08、R09、R10 |
| ACPI 数据帧与 ACK 使用唯一出站顺序：成功接收及最后块重试、ACK 与随后文本结果排队 | R01、R02、R05、R07、R09 |
| 二进制错误和关闭优先级明确：停止先于待处理坏帧、上传错误时节点仍运行 | R02、R05、R06、R09 |
| 撤销先阻断出站再完成生命周期封存：ACK 发送前撤销先提交 | R04、R08、R09 |
| 异步命令取消不产生无主任务：finish 已交接但响应观察取消 | R03、R06、R10 |

未发现孤立 Scenario、重复用例 ID、把 ACK 当正文准备成功、把 response 丢失当业务回滚，或把控制面/owner 单元测试冒充真实 WSS 的映射。

## R01—R10 可执行性

| 用例 | 结论 | 独立核对 |
| --- | --- | --- |
| R01 | **PASS** | 生产 `Case`、真实 TLS/WSS 和生产 session-services bundle 可串起 `begin → Data → ACK → finish → status ready/bodyId → register/consume`。ACK 为精确 32 字节，接收 offset 与正文 ready 分开断言；非法 purpose、多余字段、数字 `totalBytes`、重复/foreign bundle 和上传/队列读取/队列管理三路径继续可用的对照步骤明确。 |
| R02 | **PASS** | 最后块原样重试和同 offset 异内容、跳 offset、finish 后 Data 使用独立连接。每格固定零 ACK/JSON、唯一 4400、offset 不前移、`receiving` 清理和连接结束的观察，能够区分幂等重试与冲突。 |
| R03 | **PASS** | 现有一系列 `CpuEntered` 暂停点可停在原 preparation CPU worker；preparing、ready/bodyId、failed/invalid_request 和长度不符保持 `invalid_upload_state` 均有真实 status 判据。`storage.prompt_preparation_receipt().join()` 与 `observation().all_joined()` 是现有原任务 join 入口，CPU receipt identity 也明确要求保持。 |
| R04 | **PASS** | A/B 双连接隔离、释放和重复释放、未知/foreign 身份、A2 正常关闭后由新 A3 重放旧 ID 均分开执行。文本 `body_unavailable`、Binary 零 ACK/JSON 加唯一 4400、旧 owner seal 和剩余连接继续工作均有明确结果。 |
| R05 | **PASS** | 主链已从缺少生产 bundle 的 socket `Harness` 改到 `queue_management/development_tests.rs` 的 `Case`。该 `Case` 当前实际启动 `NodeProcessOwner`、`NodeRuntime`、SQLite、真实认证 WSS 和生产 `OwnedNodeSessionServices`。现有 `observe_next_document_socket_for_test()` 返回 identity-bound `SocketViewClient` 与 `Outbound`；新增的 `DocumentSocketEndpoint::attach_shared_read(&OpenedView)` 可在 `SocketAttachment` 内复用现有 `attach_read` 的 belongs-to 校验和 stream 注册，不暴露 attachment，也不接受外来 authority 或替换服务 bundle。精确 16384 字节有效 JSON、ACK、finish、ready/bodyId 以及 4400/4429 边界均可确定观察。 |
| R06 | **PASS** | 明确禁止用同一个忙 reader 的 peer Close 解开 `PendingReads` 暂停，统一由真实 Node stop 取消；`run_authenticated` 当前已使用 biased shutdown 分支。四命令 BeforeCall/AfterCall、InputAcceptedBeforeAck、stop-first 与 format-first 的顺序、1001、owner seal、preparation receipt join 和 connection/writer join 均可分别取证。失效暂停点必须报告失败，不能用计数零代签。 |
| R07 | **PASS** | 当前 `pause_next_write` 已在 writer 从原出站队列取帧之后、文本 actor gate 之前暂停。由此可固定形成首 ACK `in-flight=1`、后续 256 个真实幂等 ACK queued，下一次申请触发 4429。FIFO 另行断言 ACK 在后续 status 前；close 优先、未入队/未发送 ACK drop、queued slot 与 byte permit 回收均有明确判据。byte 饱和只使用原 semaphore permits，并有 RAII 归还。 |
| R08 | **PASS** | 合法但不存在的 target 与语法非法 target 分别断言 `invalid_target`、`invalid_request`。ACK 已出队但 writer 尚未取得 actor gate 时由 B 真实 revoke，释放后无 A ACK、4410、协议结束后 owner seal、entries/receiving 清零和实际 join；反向顺序让已取得 gate 的发送先完成再提交 revoke，避免持同一 gate 自锁。B 的后续成功证明 actor 隔离。 |
| R09 | **PASS** | `Owner::typed_entry_guard` 的声明点可在完成 connection/owner/client 校验后、读取上传表或再次访问认证存储前消费至多一次故障。`LiveSocketContext::connection_id()` 已是 crate 可见，`InputData::decode_client` 在 owner 调用前只返回借用视图，不会改写表或消耗故障。`NodeInputUploads::arm_route_fault_for_test(client, fault)`、连接绑定的原子 receipt、RAII 撤销，以及 `ActorRevoked` / `ActorStorageUnavailable` / `UploadStorageUnavailable` 三值与 status/Data 3×2 矩阵、命中数 1、4410/4500/4400、文本 `necessary_storage_unavailable`、无结果/ACK、连接隔离和 seal/entries 清理均可实现。该字段、方法和分支限定 `#[cfg(test)]`，不会进入正常构建。 |
| R10 | **PASS** | 合同已把完整服务对象构造、`reject_services` 检查和失败返回放到 `TcpListener`/router 创建之前，并要求正常入口只在 bundle 原子发布后开放。当前 storage 已登记 preparation shutdown port，完整服务对象开始后若立即返回错误，局部 `PreparedBodyHandle` drop 会 seal，storage 持有的原 supervisor 可完成 `prompt_preparation_receipt().join()`；lifecycle 的原 `join_actual()` 也可实际收敛。固定地址不可连接可验证未开放 HTTPS。现有 `start_control_only_for_test` 会在同一 `ready()` 中跳过服务构造/安装而保留真实 TLS/WSS，可用于逐条验证四命令的 `product_not_ready`、`result=null`、无 uploadId/ACK。 |

## 原子 bundle 与模块边界

v10 采用一个 `OnceLock<NodeManagementSessionServices>` 是对当前“queue reads 与 queue management 两个独立 `OnceLock`”的必要替换。实现的可行边界已经闭合：

- facade 在 `node/session_services.rs` 定义，字段私有，只由原 `OwnedNodeSessionServices` 生成，不提供三份外来句柄的公共构造器。
- facade 保存原 `socket_authority`；`ManagementService` 安装前先验证 authority，再对唯一 `OnceLock::set` 发布。重复或 foreign 安装不会部分替换原 bundle。
- `NodeQueueReadService`、`NodeQueueManagement` 均可 clone 并可在 facade 上返回投影；`UploadOwner` 只需在 `node.rs` 增加 crate 内重导出即可跨模块使用原私有实现。
- 上传、队列读取和队列管理仍指向同一个原 `OwnedNodeSessionServices` 内的实例与服务依赖，不需要公开 `NodeInputUploads` 或新增可变安装入口。
- R05 的 endpoint 只增加 `#[cfg(test)] attach_shared_read(&OpenedView)`，在 `management_ws` 父模块内调用现有私有 `SocketAttachment::attach_read`，没有扩大生产 API。

## v7 tester 缺口

| 缺口 | 状态 | 依据 |
| --- | --- | --- |
| G1：R06 reader/Close 自死锁与 accept-data 停止取消 | **CLOSED** | 不再用 peer Close 解开 PendingReads；真实 Node stop 的 biased shutdown 取消暂停 future，InputAcceptedBeforeAck 由原 ConnectionStop 取消。 |
| G2：R10 首次发布失败及真实启动清理 | **CLOSED** | 完整 bundle 后、listener/router 前的单次失败注入、固定地址不可连接、preparation `all_joined()`、lifecycle `join_actual()` 均已给出可观察判据。 |
| G3：R05 真实 ACPV 与精确 16384 字节完成 | **CLOSED** | 生产 `Case` 的原 authority/attachment 与窄 endpoint，合法 ACPV credit、`stream.ready()`、upload dispatch 0、ACK、finish、ready/bodyId 在同一路径。 |
| G4：R07 writer 暂停、1+256、4429、close/drop/permit | **CLOSED** | 现有 writer 边界支持固定 in-flight/queued 状态；后续真实申请、close 优先、queued ACK drop 和 permit 回收均有断言。 |
| G5：join 入口与 R09 固定矩阵 | **CLOSED** | R03/R10 使用原 receipt/lifecycle join；R09 固定三故障来源乘 status/Data 的六格矩阵、精确注入点和清理。 |

## 历史问题复核

| 发现 | 状态 | 本版依据 |
| --- | --- | --- |
| ACPI-V6-001 | **CLOSED** | MANIFEST revision、12/12 内部摘要及 SOURCE 40/40 实际摘要均匹配；审查范围和写集已固定。 |
| ACPI-V6-002 | **CLOSED** | `UploadRoutingError` 保留 `Business`、`Revoked`、`StorageUnavailable` 三来源；typed 方法共享检查与状态变更，不从失败后查询推断来源。 |
| ACPI-V6-003 | **CLOSED** | 8 个 Requirement、13 个 Scenario 均有可执行用例；R05、R09、R10 的前述 fixture/seam 缺口已闭合。 |
| ACPI-V6-004 | **CLOSED** | R08 明确区分语法错误 `invalid_request` 与合法不存在 target 的 `invalid_target`。 |
| ACPI-V6-005 | **CLOSED** | 具名回归已列入允许写集：`input.upload.begin` 改为成功并取得 uploadId；合法未实现改用 `view.open`，同时保留数字 target 的 `invalid_request` 与不存在 target 的 `invalid_target`。 |
| ACPI-V8-001 | **CLOSED** | R09 已固定生产注入点、connection-bound one-shot receipt、命中断言和 owner/lifetime 清理。 |
| ACPI-V8-002 | **CLOSED** | R10 已固定现有 control-only factory 入口、真实 pair/auth WSS、四命令 payload 与逐项结果。 |
| ACPI-V8-003 | **CLOSED** | R01 自身包含 finish、status ready/bodyId 和后续 register/consume，不再借用 R05 补足。 |
| ACPI-V9-001 | **CLOSED** | revision 已更新为 `acpi-route-v10`；MANIFEST 12/12、SOURCE 40/40 匹配，CONTRACT 已如实区分 v4 spec 原字节副本与经 v5—v10 修改的 tests。 |
| ACPI-V9-002 | **CLOSED** | R05 主线已迁到生产 `Case`；endpoint 只向同一原 attachment 注册 read，不拼装或替换完整 bundle。 |
| ACPI-V9-003 | **CLOSED** | 现有反向 `platform_unimplemented` 断言已有明确替换方案，不删除“合法未实现/语法错误/目标不存在”的区分。 |

以上均为定义与编码门层面的关闭。动态测试尚未执行，不能用本表宣称对应产品行为通过。

## 合入与后续门

本次 PASS 允许进入 v10 规定的顺序：先合入候选规格、proposal 能力登记、design 增量、集成合同和 tests，并在 plan/tasks 中记录独立复核及 WP4/WP5/WP6 同版安排；随后按已冻结写集串行实现。合入时不得改变候选的验收预期，不得把两套队列服务安装、外来句柄拼装、第二 writer 或正常构建故障分支带入生产路径。

实现完成后仍必须由独立 tester 对新的同版源码摘要执行 R01—R10，并保存真实 TLS/WSS 有序输出、assertion、join 分类、permit 计数和日志索引；再由独立 reviewer 对同一产品快照做交付检视。Web UI、最终独立验收、真实 Harness、安全/供应链验收及归档均不在本次放行范围内。
