# identity-pairing 增量

## MODIFIED Requirements

### Requirement: 配对创建的请求集合与有效期（无授权集合）

系统 SHALL 只接受与其目标一致的请求集合：设备配对与节点配对都 MUST NOT 携带任何授权集合字段；出现任何 `grants` / `scopes` / `requestedScopes` / `requestedPacks` 类字段时 MUST 拒绝创建且不产生配对记录。节点展示名由本机配置提供，MUST NOT 经配对请求提交。配对有效期固定为 5 分钟，调用方只能收窄不能延长。创建结果 SHALL 给出配对标识、展示用的配对载荷，以及可由网页入口使用的配对码；配对码 MUST NOT 成为新的持久字段，也 MUST NOT 改变配对 secret 的熵或生命周期。配对 secret 明文只在内存中存在。创建时若派生出的配对码与另一个活跃配对的码相同，系统 SHALL 重新生成配对 secret（从而得到新码）直到唯一，MUST NOT 让两个活跃配对共享同一个码。

#### Scenario: 两种配对都不接受授权集合

- **WHEN** 调用方以设备或节点为创建一次性配对，并提交任何授权集合字段（`grants` 或 `scopes`）
- **THEN** 创建被拒绝，且不产生任何配对记录

#### Scenario: 节点名不来自配对请求

- **WHEN** 一次节点配对创建成功
- **THEN** 对端看到的展示名来自创建方本机配置，MUST NOT 取自请求体

#### Scenario: 有效期只能收窄

- **WHEN** 调用方请求一个小于 5 分钟的有效期，随后再请求一个大于 5 分钟的有效期
- **THEN** 前者按请求值生效，后者被拒绝或按 5 分钟上限生效，绝不会长于 5 分钟

#### Scenario: 创建后看不到其他设备的任何信息

- **WHEN** 一次配对创建成功、尚无任何对端认领
- **THEN** 查询该配对只返回状态与过期时间，对端标识、名称、指纹与展示材料均为空

#### Scenario: 创建结果同时给出配对码但不落库

- **WHEN** 一次设备配对创建成功
- **THEN** 创建结果同时给出配对标识、配对载荷与配对码；持久化仍只保存配对记录与其 secret 摘要，MUST NOT 新增或写入配对码明文字段

#### Scenario: 活跃配对之间码碰撞时重新生成

- **WHEN** 新建配对的派生码与另一个仍持有 secret 的活跃配对相同
- **THEN** 系统重新生成 secret 与码，直到码在活跃配对中唯一；用户不会被要求处理这次碰撞

## ADDED Requirements

### Requirement: 配对码派生

系统 SHALL 从 pairing secret 以域分离方式派生网页入口使用的配对码：固定 domain tag 下的 HMAC-SHA256，取前 40 bit，编码为 8 位 Crockford base32（字母表不含 `I`/`L`/`O`/`U`；输入归一化把 `I`/`L` 映射为 `1`、`O` 映射为 `0`）。配对码 MUST NOT 作为 claim/status 的 HMAC 密钥，MUST NOT 替代或缩短 pairing secret；claim 与主机证明仍使用完整的 256 bit secret。Crockford 编解码 SHALL 由本项目实现，MUST NOT 为这 40 bit 引入新的编解码依赖。

#### Scenario: 派生结果稳定且长度固定

- **WHEN** 对同一个 pairing secret 重复派生配对码
- **THEN** 每次都得到同一组 8 位 Crockford 字符；不同 secret 得到不同配对码的概率与 40 bit 空间一致

#### Scenario: 配对码不承担证明职责

- **WHEN** 客户端只用配对码（没有完整 pairing secret）尝试计算 claim 或 status 的 HMAC
- **THEN** 服务端拒绝该证明；配对码 MUST NOT 被当作 HMAC 密钥或证明材料

### Requirement: 配对码单次消耗与内存查找

配对码查找 SHALL 只覆盖本进程内存中仍持有 secret 的 active pairing，MUST NOT 从持久化摘要重建配对码、secret 或配对绑定。一次成功查找 MUST 原子地消耗该码：同一码的第二次查找 MUST 失败，即使第一次的响应没有送达调用方。查找 MUST 使用常数时间比较；失败 MUST 与「配对不存在」不可区分。

#### Scenario: 成功查找后码被消耗

- **WHEN** 一次配对码查找成功返回绑定与 secret
- **THEN** 该码立即从可查找集合中移除；同一码的第二次查找失败，且失败形状与不存在的码相同

#### Scenario: 查找失败不泄露存在性

- **WHEN** 分别用不存在的配对码与存在但已失效/已消耗的配对码执行查找
- **THEN** 两者返回同一失败，调用方无法区分配对是否存在或处于哪个状态

### Requirement: 配对码与 secret 的生命周期

配对码与其查找表 MUST 与 secret 同生同灭：secret 在过期、已批准配对首次认证成功、拒绝后的过期时间或进程重启时被清除时，配对码查找 MUST 同时失效。过期维护 SHALL 接到清理 secret 的入口（`due_pairings`/`unrecoverable_after_restart` 与启动时的 `reset_memory`），MUST NOT 只清理持久记录而把内存 secret 与配对码留到进程重启之后。

#### Scenario: secret 清除后配对码失效

- **WHEN** 配对因过期、被拒绝后到达原过期时间，或已批准配对完成首次认证而清除 secret
- **THEN** 该配对码不再能查到任何配对，后续查找与不存在的码返回同一失败

#### Scenario: 过期维护清除内存 secret

- **WHEN** 配对到达 `expires_at` 并由过期维护扫描
- **THEN** 清理路径 MUST 同时清除该配对的 secret 与配对码映射，MUST NOT 只把持久记录改成过期

#### Scenario: 重启后不能凭摘要恢复配对码

- **WHEN** 进程在配对未确认时重启
- **THEN** 内存中的 secret 与配对码查找表都被清空，MUST NOT 从持久化摘要重建配对码或 secret
