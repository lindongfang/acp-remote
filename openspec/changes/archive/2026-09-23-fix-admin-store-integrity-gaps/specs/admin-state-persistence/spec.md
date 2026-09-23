## ADDED Requirements

### Requirement: 撤销与删除后的写入不得复活资源

系统 SHALL 把 Export 撤销与 Import 完整移除视为终态：`put_export` MUST NOT 清除已撤销 Export 的 `revoked_at`，也 MUST NOT 经普通写入让已撤销的 Export 重新可用；撤销时间与撤销审计 MUST 只由 `revoke_export` 产生。失去 Import 的 imported 写路径（写入 imported 会话元数据或提交交付收据）MUST 在同一写事务内先确认该 `(ownerNodeId, exportId)` 仍归属某个 Import，否则 MUST 失败关闭并返回 `NotFound(Export(exportId))`：MUST NOT 重建会话行、交付索引或命令引用，MUST NOT 推进 `local_sequence`。

#### Scenario: 已撤销 Export 不能被后续写入清除撤销标记

- **WHEN** 一个 Export 已被撤销（库内 `revoked_at` 非空），随后同一 `exportId` 的 `put_export` 写集携带未撤销记录
- **THEN** 调用返回 `Conflict(AlreadyExists)`，库内该行仍保留首次撤销时间，该 Export 仍不可用，也不写入该写集携带的状态与审计

#### Scenario: 撤销只能经 export.revoke 落库

- **WHEN** 一个 `put_export` 写集携带 `revoked_at` 非空的记录（无论库内该 Export 是否已撤销）
- **THEN** 调用返回 `InvalidRequest` 且整事务零写入，撤销时间与 `export.revoked` 审计只能由 `revoke_export` 产生

#### Scenario: 完整移除后的迟到回调不能重建索引

- **WHEN** 一个 Import 被完整移除后，携带其 `(ownerNodeId, exportId)` 的迟到会话同步（`upsert_session`）或迟到的交付收据（`commit_receipt`）到达
- **THEN** 两者都返回 `NotFound(Export(exportId))`，`imported_session`、`imported_delivery_index` 与 `imported_command_ref` 保持为空，且不产生新的 `local_sequence`

### Requirement: 设备与节点记录的活动时间只前进

系统 SHALL 保证设备记录的 `last_seen_at` 与节点角色记录的 `last_connected_at` 单调不减：写入携带更晚的时间戳时推进该值，携带更早的时间戳或无该值时保留已存值。任一侧为空 MUST 按「未知」处理——已存值为空时写入新值，传入值为空时保留已存值；比较 MUST 按固定宽度 UTC 毫秒文本的时间序进行。该列的处理 MUST NOT 使调用失败，也 MUST NOT 丢弃同一次写入的其他字段。

#### Scenario: 旧值为空时首次写入不被丢弃

- **WHEN** 一条还没有 `last_seen_at`/`last_connected_at` 的记录收到携带时间戳的写入
- **THEN** 该时间戳落库，读回等于写入值（不因已存值为空而丢失）

#### Scenario: 更早的时间戳不使活动时间倒退

- **WHEN** 已存活动时间为 T2，随后一次写入携带更早的 T1（T1 < T2）
- **THEN** 读回仍是 T2，该记录的其他字段按写集更新，调用返回成功

#### Scenario: 空值不抹掉既有活动时间

- **WHEN** 已存活动时间为 T2，随后一次写入不携带该时间戳
- **THEN** 读回仍是 T2
