# RV1-WP4 Review 报告（变更 `node-link-owner` / WP4 握手与连接生命周期，工作包交付前）

> 说明：本轮 reviewer 子 Agent 无写文件工具（只读检视），报告由主 Agent 按其返回正文原样持久化到本路径。

## 结论

- **判定：FAIL**（Target Revision `319c77b715eb218ee81696aeec8257d268e0930a`）。
- **2 条 P1（MAJOR，阻断）**：`RV1-WP4-F1`（出站序号在入队前分配 → HighWater 拒绝永久烧掉一个序号，对端按 §2.2 必须 `sequence_invalid`）、`RV1-WP4-F2`（R40 的 `last_seen` 收尾写集未实现，且无生产写入路径）。
- **5 条 P2（非阻断，report-only）**：F3 在途握手配额被持有到连接结束（与自身文档口径不符）、F4 两条准入闸门（认证限流 10/min/IP、在途配额）无行为用例、F5 unknown-type 不记账序号的内部不一致、F6 两处不可达分支、F7 design D3 与实现的限流位置不一致。
- **附带复核**：`RV1-WP3-F2`/`F3`/`F4`/`F5`（修复于 `443c6c09`）在 target 上**逐条已解决**。
- 本报告不代表候选验证、`[PV5]`/E2E、主 Agent 的 `workflow check` 或合并已完成，也不宣称整个变更可归档。

## Findings（完整发现表）

| ID | 级别 | Location（target `319c77b`） | 问题摘要 | 主 Agent 裁决 |
|---|---|---|---|---|
| RV1-WP4-F1 | P1（阻断） | `conn/registry.rs:138-186`（send/encode/enqueue/next_sequence） | 出站 `connectionSequence` 在入队判定之前分配；`enqueue` 返回 `HighWater` 时计数器不回退 → 后续每帧带缺口，合规对端必须按 `sequence_invalid` 拒绝，且协议无重同步规则 → 该方向永久错位 | **修复**：准入判定与序号分配放入同一临界区（或入队成功才分配序号）；补红向回归用例（高水位→排空→再发→序号连续） |
| RV1-WP4-F2 | P1（阻断） | `conn/session.rs:680-724`、`use_cases.rs:622-728`、`admin/trust.rs:951-1050` | spec R40 与身份合同 §5.1 要求认证收尾写集含 `last_seen`；target 无任何生产路径推进 `owned_node.last_connected_at`（`put_node`/`upsert_node` 无生产调用方），管理视图 `lastSeenAt` 恒空 | **修复（选项①）**：认证收尾同事务推进 `owned_node.last_connected_at`（窄写路径：TrustStore 增方法或扩展写集），同步 CORE_PORTS §5/§7 与漂移门禁；补「首次认证后非空且不倒退」用例 |
| RV1-WP4-F3 | P2 | `conn/session.rs:290` + `:1344-1392` | 在途握手配额的 RAII 凭证绑定到 `run()` 全生命周期 → 实际是「同源并发连接数 ≤ 4」而非文档的「在途握手数 ≤ 4」；第 5 条同源已认证连接被误拒 | **修复**：握手完成即释放（acquire 移入握手阶段或 Ready 分支 drop）；补用例钉死 |
| RV1-WP4-F4 | P2 | `conn/tests.rs` 用例缺口 | 认证限流 10/min/IP 与在途配额两条准入闸门无行为用例（RV1-WP2 的 N5 要求逐条核对拒绝点与计数键） | **修复**：补两条 loopback 用例（第 11 次建连 4429+rate_limited+零副作用；并发 5 条未认证连接第 5 条被拒） |
| RV1-WP4-F5 | P2 report-only | `conn/wire.rs:94-99` vs `:227-236` | 未知 type 不记账序号 vs post_mvp 记账——同层两个口径，合规性存疑时双方静默错位到 90 秒关闭 | **修复**：与 post_mvp 同口径（信封可解析即记账）；同步 wire.rs 模块文档 |
| RV1-WP4-F6 | P2 report-only | `conn/session.rs:414-421`、`:560-576` | 两条 `Flow::Finished` 早退分支不可达（`pre_auth_allowed` 已保证） | **修复**：删除或改为「由 pre_auth_allowed 保证不可达」注释 |
| RV1-WP4-F7 | P2 report-only | `design.md:62` vs `session.rs:18-20` | design 写「upgrade 前拒绝」，实现是 upgrade 后会话第一步 4429 | **主 Agent 已修** design.md D3（写明实际接线位置与效果等价理由） |

## 附带复核（RV1-WP3 修复，全部已解决）

| 原问题 ID | 结论 | 依据 |
|---|---|---|
| RV1-WP3-F2 | 已解决 | §5.1 矛盾表述已收窄，另有读取顺序条目；design.md D12 同步 |
| RV1-WP3-F3 | 已解决 | `has_secret` 条目补生产用途说明 |
| RV1-WP3-F4 | 已解决 | 用例注释如实 + 4 个响应逐一断言状态码与安全头 |
| RV1-WP3-F5 | 已解决 | 409 分支补自愈语义注释 |

## 已核实无误（Correct，含证据）

1. 握手只经 `Authority` 三入口；不读系统时间、不持私钥、无第二套密码学；proof 字段全部取自本机签发的挑战记录。
2. 验签公钥只来自持久化信任快照（`TrustStore::peer_key`），握手消息自报字段不参与验签。
3. 认证前白名单（4401）/版本（4406）/15 秒（4408）/feature（feature_required + details.features）逐条有用例。
4. 序号规则（认证前省略/认证后必带、跳号/回退/不匹配 → sequence_invalid、消息级拒绝后连接可用）有用例 ①–⑩。
5. 凭据状态映射（Revoked → node_revoked + 4410、Unknown → node_unknown + 4401）各有用例且断言零副作用。
6. `consume_pairing` 同事务提交后才发 `node.ready`（失败即 4500 不半认证）；`node.authenticated`/`node.auth_failed` 两族写入用例存在（RV1-WP3C-F4 闭合）。
7. limits 只下调（构造期不变量）、固定常量不可配置（双向锁定用例）。
8. 心跳/静默/慢连接：ping/pong、真实入站时刻判定、队列双上限、对照组不受影响、v1 不发 backpressure。
9. 审计边界：审计行 actor/viaNode/target/localPrincipalRef=null/detailDigest=None；错误 body 只含登记字段；日志无秘密。
10. wire 修订（D13）四面一致 + 红向证据（取值 +1 → proof_invalid；固定向量逐字节复算）。
11. §2.5 结构上限执行点在 wire DTO 边界（Envelope::decode 前的 structure::check），三常量各有正反用例，不依赖 serde 默认值。
12. 无 unsafe、无秘密外泄、依赖方向正确、测试设施纪律（真实 loopback WSS + 真实本地通道配对落库）。

## Assessment 与限制

- [PV3] 日志内部自洽（两轮：`wp4-handshake.log` 218 lib、`wp4-wire-fix.log` workspace 全绿）；本轮 reviewer 只读未复跑。
- [PV5] 归 3.7/3.9，候选合入前必须补齐且 F1/F2 修复后重跑。
- L1 无 shell/Git（diff 未独立重算）；L2 未执行命令；L3 WP5/WP6/WP7 不在本轮；L4 R45 后半句（第 9 个并发命令被拒）归 WP6 检视接上；L5 残余风险登记：「会话阻塞在 socket 写」路径未被慢连接用例覆盖，是否给写出加超时由 WP5/WP6 或候选门禁决定。
- 阻断标准对照：命中「违反 specs 任一需求」（F2 的 R40 last_seen 子句、F1 的 R47 序号严格加一）。

## 结论与建议顺序

修掉 F1、F2，同批补 F4 用例；F3/F5/F6/F7 同批处理。修完后由**新的独立 reviewer** 按原 ID 复核（recheck），[PV3] 在新提交上重跑；[PV5] 与 WP5/WP6 的消费方（`ConnectionHandle::send`）在合入前须确认已按修复后的语义接线。

> 主 Agent 注：F7 已当场修复（design.md D3 补实际接线口径）；F1/F2/F3/F4/F5/F6 交 wp4-fix1 修复批，复核由 RV2-WP4 承担。
