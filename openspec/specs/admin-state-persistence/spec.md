# admin-state-persistence Specification

## Purpose

管理状态（设备、节点、配对、Export/Import 与本地配置）必须以「一次端口调用 = 一个事务 = 一个完整写集」的方式持久化，使撤销、授权与审计在崩溃与重启前后保持一致，任何一步失败都不留下半条授权。

## Requirements

### Requirement: 管理写集的原子性与失败关闭

系统 SHALL 在同一个事务内提交一次管理写集的状态变更、集合字段与审计行；任一约束失败（CHECK、唯一键、外键、条件更新）或审计写入失败时，事务 MUST 整体回滚，端口 MUST 返回具名错误而不是部分成功。

#### Scenario: 审计写入失败时状态不落库

- **WHEN** 一次管理写集（设备撤销、节点撤销或配对确认）包含一条无法写入的审计行
- **THEN** 调用返回错误，设备/节点/配对/Export/Import 行与关联行保持调用前的内容，且没有任何审计行被写入

#### Scenario: 约束冲突时不留下半条授权

- **WHEN** 一次配对确认写集在创建信任记录或写入身份材料时违反唯一键、外键或条件更新
- **THEN** 调用返回冲突或相应错误，配对状态、信任记录与身份材料都停留在调用前状态，不存在「配对已批准但没有持久信任」的组合

### Requirement: 配对认领与落定的单事务语义

系统 SHALL 只对「存在、未过期、仍为 `created`、本机绑定一致」的配对执行认领，并在同一事务内写入唯一对端行、把配对推进到待确认状态、追加认领审计；并发认领 MUST 至多一个成功。落定 SHALL 区分批准与拒绝/过期：只有批准才创建信任记录并把对端公钥转入信任材料。

#### Scenario: 并发认领只有一个成功

- **WHEN** 两个携带同一配对的合法认领被串行处理
- **THEN** 恰好一个返回成功并写入一行对端记录，另一个返回 `AlreadyClaimed` 冲突，配对名下始终只有一行对端记录

#### Scenario: 拒绝或过期不创建信任

- **WHEN** 一次落定携带拒绝结论，或目标配对已过有效期
- **THEN** 调用返回拒绝或过期结果，不新增信任记录，配对行进入相应终态并写入对应审计，该配对此后不能被再次认领

### Requirement: 撤销与重启恢复

系统 SHALL 先提交撤销及其审计，再阻断后续访问；已提交的撤销 MUST 在重启后继续生效，撤销产生的记录 MUST 不被容量压力删除。重启 SHALL 终结「未确认且已过期」的配对，并保留已批准的信任与本地配置。

#### Scenario: 重启后撤销仍然有效

- **WHEN** 一个设备或节点被撤销后 daemon 用同一数据库重启
- **THEN** 该身份的持久状态仍为已撤销（含撤销时间与原因），新的认证或命令请求被拒绝，且不需要重新写入撤销行

#### Scenario: 重启终结已过期的未确认配对

- **WHEN** 库中存在状态为待确认且有效期早于启动时刻的配对
- **THEN** 恢复流程把该配对终结为过期并写入过期审计，已批准的配对与信任记录不受影响

### Requirement: Export 与 Import 的归属与完整移除

系统 SHALL 在创建或更新 Export、添加 Import 时于同一事务校验并写入管理行与全部关联行；同一对端 Export MUST 只归属一个 Import，重复归属显式冲突。完整移除 Import SHALL 在同一事务删除管理行、关联行与对应的无正文交付索引和命令引用，但 MUST 保留审计行。

#### Scenario: 同一 Export 归属冲突被拒

- **WHEN** 一个 Import 写集把已被别的 Import 占用的对端 Export 关联到自己名下
- **THEN** 调用返回重复归属冲突，两个 Import 的管理行与关联行都不变

#### Scenario: 完整移除后审计仍在

- **WHEN** 一个 Import 被完整移除
- **THEN** 该 Import 的管理行、关联行、交付索引与命令引用都不再存在，而此前产生的审计行仍可按时间与类别查询到

### Requirement: 管理审计的取值闭合与无内容

系统 SHALL 只允许已登记的安全动作取值写入审计；涉及已登记安全动作的管理写集 MUST 至少携带一条成功或失败/拒绝审计，且审计行 MUST 不含会话正文、凭据、pairing secret、QR payload 或本机路径明文。

#### Scenario: 未登记动作无法写入

- **WHEN** 适配器尝试写入一个不在闭合枚举内的审计动作
- **THEN** 该写入被拒绝，库内不产生该审计行

#### Scenario: 失败请求的审计不含内容

- **WHEN** 一次管理写集因约束冲突或审计不可写而失败
- **THEN** 对应的失败或拒绝审计只含动作、actor、目标、结果与可选摘要，不含任何内容明文或凭据

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
