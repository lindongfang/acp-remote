# WP4-FR-002：服务端 typed 错误映射方案检视

日期：2026-11-13。原 finding 提出者、独立 reviewer。只审方案，不修改产品、测试定义或协议；未构建、未执行测试。

**结论：允许按下表实施局部服务端修复及适用开发单元测试；WP4-FR-002 / P2 当前仍 open。** 受审版本为环境继承候选 `SOURCE-FINAL-v2.json`（清单自身 SHA-256 `f1f96636ed110b6786523ccd2f7a0ff50f2ebb442c5465fb5e6b305308beb90a`，514 叶），以及本次读取时 Root 的同名文件；关键输入摘要见 `DELIVERY-ADDENDUM-INPUTS.json`。基线 HEAD `65ec7815bb0280f4519989d93a7b4ed3b4b37994` 不独自代表未提交工作树。

## 依据和最小范围

`app/src/management_ws/session_commands.rs:462–493` 的现有 `view_error_code` 已输出配置、目标、来源、范围、停止、释放、长度等固定码，却把 `ViewError::NativeFailure(_)` 无条件压成 `protocol_failure`。原配置启动失败因此丢失 typed 类别。允许只在这一 NativeFailure 分支内对 `DispatchError` 穷尽匹配，可用紧邻的私有纯 helper；保留其余所有 ViewError 映射以及 `view_failure:496–514` 的认证失效关闭逻辑。

`app/src/node/native_views.rs:173–184` 已提供 view admission 的语义依据：InvalidConfiguration、InvalidTarget、InvalidSource、InvalidScope、NodeStopping、LengthUnrepresentable 分别保留；Stopped→SourceReleased；其他多数→ReadFailure。释放操作的 `release_error_code:117–160` 属于另一业务语境，不能直接复制到 view。`management_ws.rs:1508–1517` 只序列化固定 `error.code`，不需要携带原始错误文本。

## 建议穷尽表（全部 29 个 DispatchError 变体）

| DispatchError | view 管理错误码 | 判断依据 |
| --- | --- | --- |
| ProtocolFault(_) | protocol_failure | 保留原协议故障语义，不按内部子原因重新定业务码 |
| ProtocolUnavailable | protocol_failure | 本次方案明确保留 |
| ProtocolRejected | protocol_failure | 沿用 view admission 既有分类 |
| InvalidConfiguration | invalid_configuration | 对应现有 ViewError 同名固定码 |
| InvalidTarget | invalid_target | 同上 |
| InvalidSource | invalid_source | 同上 |
| InvalidScope | invalid_scope | 同上 |
| NodeStopping | node_stopping | 同上 |
| Stopped | source_released | 沿用 node/native_views 的既有 view 语境；不能归成 node_stopping |
| LengthUnrepresentable | length_unrepresentable | 对应现有 ViewError 同名固定码 |
| RuntimeAlreadyStarted | read_failed | 运行时状态不足以证明客户端请求冲突 |
| StorageUnavailable | read_failed | 本轮保守 view 失败分类；不借 release/storage 的另一套码 |
| CommitUnconfirmed | read_failed | 同上，不改变事务事实或恢复规则 |
| SqlCloseUnconfirmed { .. } | read_failed | 两个 commit_confirmed 值均固定码，保留服务内 typed 事实 |
| CorruptState | read_failed | 不外泄内部元数据状态 |
| Conflict | read_failed | 不凭错误名推断请求冲突 |
| InvalidResource | read_failed | resource 不等价于 target，现有 view admission 也是兜底 |
| InvalidPromptReservation | read_failed | 无现成 view 同义码，保持保守失败 |
| StaleSelection | read_failed | 不擅自等同 stale_cursor/read_stale |
| RevisionChanged | read_failed | 不擅自等同 request_conflict |
| NotReady | read_failed | 不自动转为 product_not_ready 或重试策略 |
| RequiredStateUnavailable | read_failed | 无需新造管理码 |
| SequenceExhausted | read_failed | 沿用现有 view admission 对 DispatchError 的保守归类，见下方边界说明 |
| EnqueueRejected | read_failed | 不等同另一个 ViewError::QueueRejected |
| ObservationOrder | read_failed | 内部运行时状态 |
| ObservationUnapplied | read_failed | 内部运行时状态 |
| ObservationDropped | read_failed | 内部运行时状态 |
| ResourceUnconfirmed | read_failed | 未确认收敛不证明 source 已释放 |
| SourcePlatform { .. } | read_failed | 不输出 operation/code，保持固定公开错误面 |

`SequenceExhausted` 的直接 `ViewError` 虽已有 sequence_exhausted，但不能仅凭名字把不同层级耗尽事实归为同一客户端含义。本轮选择 read_failed 与现有 node helper 一致，属于已授权的“其余 runtime 状态”保守方案。若后续希望这类错误透传 sequence_exhausted，需单独说明可观察语义、调用来源及客户端合同，不能顺手扩大本次修复。Stopped 在 source 锁 poison 等路径也可能出现；沿用当前 view 映射不会修复该上游语义混用，本轮不改 domain 错误定义。

## 最小可执行验证与交付门

1. 先以新纯映射测试验证旧实现确实失败，至少 InvalidConfiguration 必须 red；建议同一表覆盖全部 29 个变体，旧实现除三个协议类之外应触发相应差异。保存实际 red 输出与当时输入摘要。
2. 实现 match 显式列出每个变体，不用 `_` 兜底，使新增变体成为编译期检视点。ProtocolFault 可保留 payload wildcard；测试当前 17 个 NativeProtocolFailure 子变体均仍为 protocol_failure，不重分类其 Io/Allocation 等内部子原因。
3. SqlCloseUnconfirmed 的 bool 两值均测试；SourcePlatform 至少两个不同 operation/code 组合均为 read_failed。断言返回固定码，并在序列化层确认仅包含既有 envelope 和 `error.code`，无原始 operation、数值错误码或 display/debug 文本。
4. 适用服务端开发单测应调用真实 `view_error_code(ViewError::NativeFailure(...))`，避免只测一个未接线的 helper。增加保留现有直接 ViewError 映射的断言，认证关闭逻辑不修改。跑适用编译、单元回归、fmt，并再次执行原 native_release 7 项以保护完整路由。
5. 固定新版本、实际差异、red/green 日志和 PE 身份，再交独立 tester/reviewer 限域复查；本报告只有方案通过，不把未来测试当作已执行，也不关闭 FR002。

## 协议与产品边界

现有 Web `clients/web/src/management/protocol.ts:68–85,684–706` 的 ManagementErrorCode/knownManagementError 是封闭集合，缺 invalid_configuration、invalid_source、invalid_scope、source_released、length_unrepresentable、protocol_failure 等服务端现有码，收到它们会抛 invalid_response。这是已有合同缺口；单改服务端不会使完整 Web 错误呈现通过。本轮明确只覆盖服务端映射和适用单测，不假称端到端修复。后续需独立确定完整管理错误码合同、Web 解析和呈现/重试策略，不能临时放开任意字符串或静默吞码。

当前方案不新增码、不输出敏感底层字段、不改 ACL、认证、释放行为、协议版本或上游错误含义，因此无需为上述明确表内的局部实现再作产品决策。若要把 InvalidResource 当目标、ResourceUnconfirmed 当释放成功、Stopped 当节点停止、或把 storage/runtime 类透传为其他业务码，则超出本次已审范围，须先提供合同/产品依据。FR001 的三叶放行与此方案互不包含。
