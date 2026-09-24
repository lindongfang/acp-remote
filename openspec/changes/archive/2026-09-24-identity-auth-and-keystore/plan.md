## Scope and Contracts

- Specs Revision: 本变更 `specs/` 下 4 个新增能力规范的初版——`identity-pairing`（7 条需求 / 22 个场景）、`identity-handshake`（7 条需求 / 19 个场景，含「transcript 与仓库固定向量保持一致」）、`scope-expansion`（5 条需求 / 11 个场景）、`platform-keystore`（5 条需求 / 14 个场景）；`openspec/specs/` 之前没有这四个能力，全部为 ADDED，无 MODIFIED/REMOVED/RENAMED。
- Design Revision: `design.md` 的 D1–D9 与 Migration Plan 初版（边界与依赖、transcript 装配与固定向量、配对状态机与内存态所有权、握手入口与持久事实快照、授权展开表、keystore 结构与条目格式、平台差异与 DPAPI wrapper 选型、测试基座、门禁与文档同步面）。
- Convergence Check: 一致。核对结论：specs 的每条可观察行为在 design 中都有对应机制（创建/认领/落定/secret 生命周期 ↔ D3；可信公钥来源与检查顺序、重放、注入时钟 ↔ D4；固定向量与畸形输入 ↔ D2/D8；展开与漂移 ↔ D5；生成/签名同源、失败关闭、秘密不泄漏、条目与引用、默认不启用 ↔ D6/D7），且 design 的接口与数据约定（`PeerTrust` 快照入参、熵源端口、条目格式、fake 端口测试基座）足以支撑 WP 拆包与独立 test 设计。未解决冲突：无。已知取舍三条（`PeerTrust` 快照入参是对合同 §5.1 的定型收口、`KeyPurpose::DeviceIdentity` 删除、DPAPI wrapper 候选可能被 §20 核验否决）已写入 design 的 Decisions/Risks，不改变本次拆包与验收条件。
- Skip Specs: no

## Contract Changes

本变更**改变**以下已冻结内容（原/新值、原因与影响分析随实现写入 `verification.md` 的 Check Plan Changes）：

- `docs/IDENTITY_AND_AUTH_CONTRACT.md` §2（类型归属）：补 `PeerTrust` 快照与熵源端口；§4.1/§5.1（配对与握手入口的最终签名）：把「只接收结构化字段」定型为「结构化字段 + 调用方从持久化信任读到的当次快照」，理由与替代方案见 design D4；§7（keystore 端口）：定型最终 trait 形状并**删除** `KeyPurpose::DeviceIdentity`（第一阶段无调用方，合同 §7 自身要求「确认用不到时删掉」）。
- `docs/MODULE_ARCHITECTURE.md` §3/§3.1/§4.8/§4.12/§5：`identity-auth`/`identity-keystore` 从「待落地」改为已落地（本轮）；§3.1 登记 `getrandom` 与 DPAPI wrapper 的版本口径并修正密码学原语的「无 crate 使用」注释；§4.12 写入 DPAPI wrapper 选型结论。§5 矩阵的 `identity-auth`/`identity-keystore` 行与列**已存在**，本变更加固其断言，不新增行列。
- `Cargo.toml`：`[workspace] members` 依次增加两个 crate（按波次 W1 → W2 串行）；`[workspace.dependencies]` 新增 `getrandom` 与（`cfg(windows)` 的）DPAPI wrapper。
- `.gitleaks.toml`：新增自研 keystore 条目格式的**明文**形态规则，并在注释中记录「DPAPI 包裹后的字节不可用模式识别」这一限制。

不改变的契约：`schemas/`、`fixtures/`、`compatibility/` 的全部既有资产（含 `transcripts/v1/transcripts.json` 与 `commands/v1/commands.json`）、`core` 的端口签名与值对象、`storage-sqlite` 表结构、Sync/Node Link/Local Admin wire、`core` 的依赖 allow-list。

## Coverage Index

> **evidence 口径**：下面 R1–R90 各行的 `evidence` 使用**变更目录相对路径**（`reports/…`，由 `openspec-agentic workflow check` 按本变更目录解析；仓库根的 `reports/` 另有别的变更的同名文件，因此**不要在 Coverage Index 里写仓库相对完整路径**）。这些路径指向**首次交付该能力时**的日志（WP 轮，revision `5c9d2c4`/`627bb8e`）。能力本身在后继轮次未改变，后续各收口轮的等价证据（同一命令、同一 crate 版本）见 `verification.md` 的 Check 行：RV5/RV6 轮 revision `3a247a5`、RV6/RV7 轮 `69dd81f`、RV7/RV8 轮 `c1fd65d`（最新一轮的证据为 `rv8-*` 日志；此后只有只改 `openspec/changes/identity-auth-and-keystore/*.md` 的记录提交，代码等价性由编排者声明），因此这里不逐行改写（R86 一行已额外补指修复轮日志）；本文档**正文**提到同一批文件时仍写仓库相对完整路径（如 `openspec/changes/identity-auth-and-keystore/reports/…`），以免与仓库根 `reports/` 的同名文件混淆——两种写法的解析基准不同，是有意区分。


```agentic-coverage
version: 1
target_ref: refs/heads/main
rows:
  - id: R1
    source:
      path: specs/identity-pairing/spec.md
      heading: "### Requirement: 配对创建的请求集合与有效期"
    tasks: ["2.4", "2.5"]
    checks: [PV3]
    evidence: [reports/wp2-identity-auth-pairing.log]
  - id: R2
    source:
      path: specs/identity-pairing/spec.md
      heading: "#### Scenario: 设备配对只接受 scope"
      requirement: "### Requirement: 配对创建的请求集合与有效期"
    tasks: ["2.4", "2.5"]
    checks: [PV3]
    evidence: [reports/wp2-identity-auth-pairing.log]
  - id: R3
    source:
      path: specs/identity-pairing/spec.md
      heading: "#### Scenario: 节点配对只接受 grant"
      requirement: "### Requirement: 配对创建的请求集合与有效期"
    tasks: ["2.4", "2.5"]
    checks: [PV3]
    evidence: [reports/wp2-identity-auth-pairing.log]
  - id: R4
    source:
      path: specs/identity-pairing/spec.md
      heading: "#### Scenario: 有效期只能收窄"
      requirement: "### Requirement: 配对创建的请求集合与有效期"
    tasks: ["2.4", "2.5"]
    checks: [PV3]
    evidence: [reports/wp2-identity-auth-pairing.log]
  - id: R5
    source:
      path: specs/identity-pairing/spec.md
      heading: "#### Scenario: 创建后看不到其他设备的任何信息"
      requirement: "### Requirement: 配对创建的请求集合与有效期"
    tasks: ["2.4", "2.5"]
    checks: [PV3]
    evidence: [reports/wp2-identity-auth-pairing.log]
  - id: R6
    source:
      path: specs/identity-pairing/spec.md
      heading: "### Requirement: 认领校验与唯一对端绑定"
    tasks: ["2.4", "2.5"]
    checks: [PV3]
    evidence: [reports/wp2-identity-auth-pairing.log]
  - id: R7
    source:
      path: specs/identity-pairing/spec.md
      heading: "#### Scenario: 绑定不一致的认领被拒"
      requirement: "### Requirement: 认领校验与唯一对端绑定"
    tasks: ["2.4", "2.5"]
    checks: [PV3]
    evidence: [reports/wp2-identity-auth-pairing.log]
  - id: R8
    source:
      path: specs/identity-pairing/spec.md
      heading: "#### Scenario: 结构错误先于密码学被拒"
      requirement: "### Requirement: 认领校验与唯一对端绑定"
    tasks: ["2.4", "2.5"]
    checks: [PV3]
    evidence: [reports/wp2-identity-auth-pairing.log]
  - id: R9
    source:
      path: specs/identity-pairing/spec.md
      heading: "#### Scenario: 并发认领只有一个成功"
      requirement: "### Requirement: 认领校验与唯一对端绑定"
    tasks: ["2.4", "2.5"]
    checks: [PV3]
    evidence: [reports/wp2-identity-auth-pairing.log]
  - id: R10
    source:
      path: specs/identity-pairing/spec.md
      heading: "#### Scenario: 相同认领内容幂等"
      requirement: "### Requirement: 认领校验与唯一对端绑定"
    tasks: ["2.4", "2.5"]
    checks: [PV3]
    evidence: [reports/wp2-identity-auth-pairing.log]
  - id: R11
    source:
      path: specs/identity-pairing/spec.md
      heading: "#### Scenario: 认领携带公钥并在重启后可读"
      requirement: "### Requirement: 认领校验与唯一对端绑定"
    tasks: ["2.4", "2.5"]
    checks: [PV3]
    evidence: [reports/wp2-identity-auth-pairing.log]
  - id: R12
    source:
      path: specs/identity-pairing/spec.md
      heading: "### Requirement: 落定与信任创建的唯一性"
    tasks: ["2.4", "2.5"]
    checks: [PV3]
    evidence: [reports/wp2-identity-auth-pairing.log]
  - id: R13
    source:
      path: specs/identity-pairing/spec.md
      heading: "#### Scenario: 批准后同时存在信任与身份材料"
      requirement: "### Requirement: 落定与信任创建的唯一性"
    tasks: ["2.4", "2.5"]
    checks: [PV3]
    evidence: [reports/wp2-identity-auth-pairing.log]
  - id: R14
    source:
      path: specs/identity-pairing/spec.md
      heading: "#### Scenario: 拒绝不产生信任"
      requirement: "### Requirement: 落定与信任创建的唯一性"
    tasks: ["2.4", "2.5"]
    checks: [PV3]
    evidence: [reports/wp2-identity-auth-pairing.log]
  - id: R15
    source:
      path: specs/identity-pairing/spec.md
      heading: "#### Scenario: 过期不产生信任"
      requirement: "### Requirement: 落定与信任创建的唯一性"
    tasks: ["2.4", "2.5"]
    checks: [PV3]
    evidence: [reports/wp2-identity-auth-pairing.log]
  - id: R16
    source:
      path: specs/identity-pairing/spec.md
      heading: "#### Scenario: 最终集合不能超出请求集合之外的方向"
      requirement: "### Requirement: 落定与信任创建的唯一性"
    tasks: ["2.4", "2.5"]
    checks: [PV3]
    evidence: [reports/wp2-identity-auth-pairing.log]
  - id: R17
    source:
      path: specs/identity-pairing/spec.md
      heading: "### Requirement: SAS 派生双方独立"
    tasks: ["2.4", "2.5"]
    checks: [PV3]
    evidence: [reports/wp2-identity-auth-pairing.log]
  - id: R18
    source:
      path: specs/identity-pairing/spec.md
      heading: "#### Scenario: 同一输入得到同一 6 位结果"
      requirement: "### Requirement: SAS 派生双方独立"
    tasks: ["2.4", "2.5"]
    checks: [PV3]
    evidence: [reports/wp2-identity-auth-pairing.log]
  - id: R19
    source:
      path: specs/identity-pairing/spec.md
      heading: "#### Scenario: 未认领前不展示 SAS"
      requirement: "### Requirement: SAS 派生双方独立"
    tasks: ["2.4", "2.5"]
    checks: [PV3]
    evidence: [reports/wp2-identity-auth-pairing.log]
  - id: R20
    source:
      path: specs/identity-pairing/spec.md
      heading: "### Requirement: pairing secret 生命周期"
    tasks: ["2.4", "2.5"]
    checks: [PV3]
    evidence: [reports/wp2-identity-auth-pairing.log]
  - id: R21
    source:
      path: specs/identity-pairing/spec.md
      heading: "#### Scenario: 摘要不能恢复 secret"
      requirement: "### Requirement: pairing secret 生命周期"
    tasks: ["2.4", "2.5"]
    checks: [PV3]
    evidence: [reports/wp2-identity-auth-pairing.log]
  - id: R22
    source:
      path: specs/identity-pairing/spec.md
      heading: "#### Scenario: 批准后提前清除"
      requirement: "### Requirement: pairing secret 生命周期"
    tasks: ["2.4", "2.5"]
    checks: [PV3]
    evidence: [reports/wp2-identity-auth-pairing.log]
  - id: R23
    source:
      path: specs/identity-pairing/spec.md
      heading: "#### Scenario: 拒绝保留到原过期时间"
      requirement: "### Requirement: pairing secret 生命周期"
    tasks: ["2.4", "2.5"]
    checks: [PV3]
    evidence: [reports/wp2-identity-auth-pairing.log]
  - id: R24
    source:
      path: specs/identity-pairing/spec.md
      heading: "### Requirement: 失败计数与失效"
    tasks: ["2.4", "2.5"]
    checks: [PV3]
    evidence: [reports/wp2-identity-auth-pairing.log]
  - id: R25
    source:
      path: specs/identity-pairing/spec.md
      heading: "#### Scenario: 第 5 次失败后配对失效"
      requirement: "### Requirement: 失败计数与失效"
    tasks: ["2.4", "2.5"]
    checks: [PV3]
    evidence: [reports/wp2-identity-auth-pairing.log]
  - id: R26
    source:
      path: specs/identity-pairing/spec.md
      heading: "#### Scenario: 失败不泄漏存在性"
      requirement: "### Requirement: 失败计数与失效"
    tasks: ["2.4", "2.5"]
    checks: [PV3]
    evidence: [reports/wp2-identity-auth-pairing.log]
  - id: R27
    source:
      path: specs/identity-pairing/spec.md
      heading: "### Requirement: 身份变化与撤销的唯一恢复路径"
    tasks: ["2.4", "2.5"]
    checks: [PV3]
    evidence: [reports/wp2-identity-auth-pairing.log]
  - id: R28
    source:
      path: specs/identity-pairing/spec.md
      heading: "#### Scenario: 撤销后普通写入不能复活"
      requirement: "### Requirement: 身份变化与撤销的唯一恢复路径"
    tasks: ["2.4", "2.5"]
    checks: [PV3]
    evidence: [reports/wp2-identity-auth-pairing.log]
  - id: R29
    source:
      path: specs/identity-pairing/spec.md
      heading: "#### Scenario: 公钥变化被拒绝"
      requirement: "### Requirement: 身份变化与撤销的唯一恢复路径"
    tasks: ["2.4", "2.5"]
    checks: [PV3]
    evidence: [reports/wp2-identity-auth-pairing.log]
  - id: R30
    source:
      path: specs/identity-handshake/spec.md
      heading: "### Requirement: 每条新连接完整执行挑战签发"
    tasks: ["2.4", "2.6"]
    checks: [PV3]
    evidence: [reports/wp2-identity-auth-handshake.log]
  - id: R31
    source:
      path: specs/identity-handshake/spec.md
      heading: "#### Scenario: 未知对端也拿到挑战"
      requirement: "### Requirement: 每条新连接完整执行挑战签发"
    tasks: ["2.4", "2.6"]
    checks: [PV3]
    evidence: [reports/wp2-identity-auth-handshake.log]
  - id: R32
    source:
      path: specs/identity-handshake/spec.md
      heading: "#### Scenario: 挑战携带本节点身份证明"
      requirement: "### Requirement: 每条新连接完整执行挑战签发"
    tasks: ["2.4", "2.6"]
    checks: [PV3]
    evidence: [reports/wp2-identity-auth-handshake.log]
  - id: R33
    source:
      path: specs/identity-handshake/spec.md
      heading: "#### Scenario: 不引入长期会话凭据"
      requirement: "### Requirement: 每条新连接完整执行挑战签发"
    tasks: ["2.4", "2.6"]
    checks: [PV3]
    evidence: [reports/wp2-identity-auth-handshake.log]
  - id: R34
    source:
      path: specs/identity-handshake/spec.md
      heading: "### Requirement: 证明校验的可信公钥来源与检查顺序"
    tasks: ["2.4", "2.6"]
    checks: [PV3]
    evidence: [reports/wp2-identity-auth-handshake.log]
  - id: R35
    source:
      path: specs/identity-handshake/spec.md
      heading: "#### Scenario: 自选公钥不能通过"
      requirement: "### Requirement: 证明校验的可信公钥来源与检查顺序"
    tasks: ["2.4", "2.6"]
    checks: [PV3]
    evidence: [reports/wp2-identity-auth-handshake.log]
  - id: R36
    source:
      path: specs/identity-handshake/spec.md
      heading: "#### Scenario: 长度先于密码学被检查"
      requirement: "### Requirement: 证明校验的可信公钥来源与检查顺序"
    tasks: ["2.4", "2.6"]
    checks: [PV3]
    evidence: [reports/wp2-identity-auth-handshake.log]
  - id: R37
    source:
      path: specs/identity-handshake/spec.md
      heading: "#### Scenario: high-S 与 low-S 都被接受"
      requirement: "### Requirement: 证明校验的可信公钥来源与检查顺序"
    tasks: ["2.4", "2.6"]
    checks: [PV3]
    evidence: [reports/wp2-identity-auth-handshake.log]
  - id: R38
    source:
      path: specs/identity-handshake/spec.md
      heading: "#### Scenario: 零值签名分量被拒绝"
      requirement: "### Requirement: 证明校验的可信公钥来源与检查顺序"
    tasks: ["2.4", "2.6"]
    checks: [PV3]
    evidence: [reports/wp2-identity-auth-handshake.log]
  - id: R39
    source:
      path: specs/identity-handshake/spec.md
      heading: "#### Scenario: 非规范 base64url 被拒绝"
      requirement: "### Requirement: 证明校验的可信公钥来源与检查顺序"
    tasks: ["2.4", "2.6"]
    checks: [PV3]
    evidence: [reports/wp2-identity-auth-handshake.log]
  - id: R40
    source:
      path: specs/identity-handshake/spec.md
      heading: "### Requirement: 一次性挑战与重放拒绝"
    tasks: ["2.4", "2.6"]
    checks: [PV3]
    evidence: [reports/wp2-identity-auth-handshake.log]
  - id: R41
    source:
      path: specs/identity-handshake/spec.md
      heading: "#### Scenario: 同一证明第二次使用失败"
      requirement: "### Requirement: 一次性挑战与重放拒绝"
    tasks: ["2.4", "2.6"]
    checks: [PV3]
    evidence: [reports/wp2-identity-auth-handshake.log]
  - id: R42
    source:
      path: specs/identity-handshake/spec.md
      heading: "#### Scenario: 未知挑战被拒绝"
      requirement: "### Requirement: 一次性挑战与重放拒绝"
    tasks: ["2.4", "2.6"]
    checks: [PV3]
    evidence: [reports/wp2-identity-auth-handshake.log]
  - id: R43
    source:
      path: specs/identity-handshake/spec.md
      heading: "### Requirement: 时间判定使用注入时钟"
    tasks: ["2.4", "2.6"]
    checks: [PV3]
    evidence: [reports/wp2-identity-auth-handshake.log]
  - id: R44
    source:
      path: specs/identity-handshake/spec.md
      heading: "#### Scenario: 过期由注入时钟决定"
      requirement: "### Requirement: 时间判定使用注入时钟"
    tasks: ["2.4", "2.6"]
    checks: [PV3]
    evidence: [reports/wp2-identity-auth-handshake.log]
  - id: R45
    source:
      path: specs/identity-handshake/spec.md
      heading: "#### Scenario: 授权结果不随时间缓存"
      requirement: "### Requirement: 时间判定使用注入时钟"
    tasks: ["2.4", "2.6"]
    checks: [PV3]
    evidence: [reports/wp2-identity-auth-handshake.log]
  - id: R46
    source:
      path: specs/identity-handshake/spec.md
      heading: "### Requirement: 认证输出只包含事实与凭据状态"
    tasks: ["2.4", "2.6"]
    checks: [PV3]
    evidence: [reports/wp2-identity-auth-handshake.log]
  - id: R47
    source:
      path: specs/identity-handshake/spec.md
      heading: "#### Scenario: 已撤销设备与权限不足可区分"
      requirement: "### Requirement: 认证输出只包含事实与凭据状态"
    tasks: ["2.4", "2.6"]
    checks: [PV3]
    evidence: [reports/wp2-identity-auth-handshake.log]
  - id: R48
    source:
      path: specs/identity-handshake/spec.md
      heading: "#### Scenario: 范围缩减提示需要重新认证"
      requirement: "### Requirement: 认证输出只包含事实与凭据状态"
    tasks: ["2.4", "2.6"]
    checks: [PV3]
    evidence: [reports/wp2-identity-auth-handshake.log]
  - id: R49
    source:
      path: specs/identity-handshake/spec.md
      heading: "#### Scenario: 事实本身不携带授权结论"
      requirement: "### Requirement: 认证输出只包含事实与凭据状态"
    tasks: ["2.4", "2.6"]
    checks: [PV3]
    evidence: [reports/wp2-identity-auth-handshake.log]
  - id: R50
    source:
      path: specs/identity-handshake/spec.md
      heading: "### Requirement: transcript 与仓库固定向量保持一致"
    tasks: ["2.4", "2.6"]
    checks: [PV3]
    evidence: [reports/wp2-identity-auth-handshake.log]
  - id: R51
    source:
      path: specs/identity-handshake/spec.md
      heading: "#### Scenario: 固定向量被逐字节重算"
      requirement: "### Requirement: transcript 与仓库固定向量保持一致"
    tasks: ["2.4", "2.6"]
    checks: [PV3]
    evidence: [reports/wp2-identity-auth-handshake.log]
  - id: R52
    source:
      path: specs/identity-handshake/spec.md
      heading: "#### Scenario: 畸形输入被拒绝且无副作用"
      requirement: "### Requirement: transcript 与仓库固定向量保持一致"
    tasks: ["2.4", "2.6"]
    checks: [PV3]
    evidence: [reports/wp2-identity-auth-handshake.log]
  - id: R53
    source:
      path: specs/identity-handshake/spec.md
      heading: "### Requirement: 收尾副作用是认证的唯一写入点"
    tasks: ["2.4", "2.6"]
    checks: [PV3]
    evidence: [reports/wp2-identity-auth-handshake.log]
  - id: R54
    source:
      path: specs/identity-handshake/spec.md
      heading: "#### Scenario: 认证成功后才消费配对"
      requirement: "### Requirement: 收尾副作用是认证的唯一写入点"
    tasks: ["2.4", "2.6"]
    checks: [PV3]
    evidence: [reports/wp2-identity-auth-handshake.log]
  - id: R55
    source:
      path: specs/identity-handshake/spec.md
      heading: "#### Scenario: 认证失败无副作用"
      requirement: "### Requirement: 收尾副作用是认证的唯一写入点"
    tasks: ["2.4", "2.6"]
    checks: [PV3]
    evidence: [reports/wp2-identity-auth-handshake.log]
  - id: R56
    source:
      path: specs/scope-expansion/spec.md
      heading: "### Requirement: 授权词汇展开为命令级 scope"
    tasks: ["2.7"]
    checks: [PV3]
    evidence: [reports/wp2-identity-auth-expansion.log]
  - id: R57
    source:
      path: specs/scope-expansion/spec.md
      heading: "#### Scenario: 预设递归展开为命令名"
      requirement: "### Requirement: 授权词汇展开为命令级 scope"
    tasks: ["2.7"]
    checks: [PV3]
    evidence: [reports/wp2-identity-auth-expansion.log]
  - id: R58
    source:
      path: specs/scope-expansion/spec.md
      heading: "#### Scenario: 重复输入被去重"
      requirement: "### Requirement: 授权词汇展开为命令级 scope"
    tasks: ["2.7"]
    checks: [PV3]
    evidence: [reports/wp2-identity-auth-expansion.log]
  - id: R59
    source:
      path: specs/scope-expansion/spec.md
      heading: "#### Scenario: 展开结果不泄漏包名"
      requirement: "### Requirement: 授权词汇展开为命令级 scope"
    tasks: ["2.7"]
    checks: [PV3]
    evidence: [reports/wp2-identity-auth-expansion.log]
  - id: R60
    source:
      path: specs/scope-expansion/spec.md
      heading: "### Requirement: 未知名称显式拒绝"
    tasks: ["2.7"]
    checks: [PV3]
    evidence: [reports/wp2-identity-auth-expansion.log]
  - id: R61
    source:
      path: specs/scope-expansion/spec.md
      heading: "#### Scenario: 未知包名整体失败"
      requirement: "### Requirement: 未知名称显式拒绝"
    tasks: ["2.7"]
    checks: [PV3]
    evidence: [reports/wp2-identity-auth-expansion.log]
  - id: R62
    source:
      path: specs/scope-expansion/spec.md
      heading: "#### Scenario: 拼错的导出授权被拒"
      requirement: "### Requirement: 未知名称显式拒绝"
    tasks: ["2.7"]
    checks: [PV3]
    evidence: [reports/wp2-identity-auth-expansion.log]
  - id: R63
    source:
      path: specs/scope-expansion/spec.md
      heading: "### Requirement: 本地管理能力永不远程授予"
    tasks: ["2.7"]
    checks: [PV3]
    evidence: [reports/wp2-identity-auth-expansion.log]
  - id: R64
    source:
      path: specs/scope-expansion/spec.md
      heading: "#### Scenario: 本地能力不能被请求包覆盖"
      requirement: "### Requirement: 本地管理能力永不远程授予"
    tasks: ["2.7"]
    checks: [PV3]
    evidence: [reports/wp2-identity-auth-expansion.log]
  - id: R65
    source:
      path: specs/scope-expansion/spec.md
      heading: "#### Scenario: 远程命令不接受本地能力"
      requirement: "### Requirement: 本地管理能力永不远程授予"
    tasks: ["2.7"]
    checks: [PV3]
    evidence: [reports/wp2-identity-auth-expansion.log]
  - id: R66
    source:
      path: specs/scope-expansion/spec.md
      heading: "### Requirement: 展开表的唯一机器来源与漂移可见"
    tasks: ["2.7"]
    checks: [PV3]
    evidence: [reports/wp2-identity-auth-expansion.log]
  - id: R67
    source:
      path: specs/scope-expansion/spec.md
      heading: "#### Scenario: 表与合同资产不一致时门禁失败"
      requirement: "### Requirement: 展开表的唯一机器来源与漂移可见"
    tasks: ["2.7"]
    checks: [PV3]
    evidence: [reports/wp2-identity-auth-expansion.log]
  - id: R68
    source:
      path: specs/scope-expansion/spec.md
      heading: "#### Scenario: 展开结果与合同成员一致"
      requirement: "### Requirement: 展开表的唯一机器来源与漂移可见"
    tasks: ["2.7"]
    checks: [PV3]
    evidence: [reports/wp2-identity-auth-expansion.log]
  - id: R69
    source:
      path: specs/scope-expansion/spec.md
      heading: "### Requirement: 展开只影响授权输入，不产生授权结论"
    tasks: ["2.7"]
    checks: [PV3]
    evidence: [reports/wp2-identity-auth-expansion.log]
  - id: R70
    source:
      path: specs/scope-expansion/spec.md
      heading: "#### Scenario: 展开结果不构成授权"
      requirement: "### Requirement: 展开只影响授权输入，不产生授权结论"
    tasks: ["2.7"]
    checks: [PV3]
    evidence: [reports/wp2-identity-auth-expansion.log]
  - id: R71
    source:
      path: specs/scope-expansion/spec.md
      heading: "#### Scenario: 节点侧输出两个独立集合"
      requirement: "### Requirement: 展开只影响授权输入，不产生授权结论"
    tasks: ["2.7"]
    checks: [PV3]
    evidence: [reports/wp2-identity-auth-expansion.log]
  - id: R72
    source:
      path: specs/platform-keystore/spec.md
      heading: "### Requirement: 长期密钥的生成、读公钥与签名是同源操作"
    tasks: ["2.1", "2.10", "2.12"]
    checks: [PV4, PV5]
    evidence: [reports/wp3-identity-keystore.log, reports/pv5-windows-dpapi.log]
  - id: R73
    source:
      path: specs/platform-keystore/spec.md
      heading: "#### Scenario: 生成后读公钥与签名一致"
      requirement: "### Requirement: 长期密钥的生成、读公钥与签名是同源操作"
    tasks: ["2.1", "2.10", "2.12"]
    checks: [PV4, PV5]
    evidence: [reports/wp3-identity-keystore.log, reports/pv5-windows-dpapi.log]
  - id: R74
    source:
      path: specs/platform-keystore/spec.md
      heading: "#### Scenario: 不同条目是不同密钥"
      requirement: "### Requirement: 长期密钥的生成、读公钥与签名是同源操作"
    tasks: ["2.1", "2.10", "2.12"]
    checks: [PV4, PV5]
    evidence: [reports/wp3-identity-keystore.log, reports/pv5-windows-dpapi.log]
  - id: R75
    source:
      path: specs/platform-keystore/spec.md
      heading: "#### Scenario: 删除后条目不可用"
      requirement: "### Requirement: 长期密钥的生成、读公钥与签名是同源操作"
    tasks: ["2.1", "2.10", "2.12"]
    checks: [PV4, PV5]
    evidence: [reports/wp3-identity-keystore.log, reports/pv5-windows-dpapi.log]
  - id: R76
    source:
      path: specs/platform-keystore/spec.md
      heading: "#### Scenario: 签名只接受待签内容"
      requirement: "### Requirement: 长期密钥的生成、读公钥与签名是同源操作"
    tasks: ["2.1", "2.10", "2.12"]
    checks: [PV4, PV5]
    evidence: [reports/wp3-identity-keystore.log, reports/pv5-windows-dpapi.log]
  - id: R77
    source:
      path: specs/platform-keystore/spec.md
      heading: "### Requirement: 平台不可用时失败关闭"
    tasks: ["2.13"]
    checks: [PV4]
    evidence: [reports/wp3-identity-keystore.log, reports/pv5-windows-dpapi.log]
  - id: R78
    source:
      path: specs/platform-keystore/spec.md
      heading: "#### Scenario: 缺条目时不生成新身份"
      requirement: "### Requirement: 平台不可用时失败关闭"
    tasks: ["2.13"]
    checks: [PV4]
    evidence: [reports/wp3-identity-keystore.log, reports/pv5-windows-dpapi.log]
  - id: R79
    source:
      path: specs/platform-keystore/spec.md
      heading: "#### Scenario: 损坏条目不被静默替换"
      requirement: "### Requirement: 平台不可用时失败关闭"
    tasks: ["2.13"]
    checks: [PV4]
    evidence: [reports/wp3-identity-keystore.log, reports/pv5-windows-dpapi.log]
  - id: R80
    source:
      path: specs/platform-keystore/spec.md
      heading: "#### Scenario: 非 Windows 明确失败"
      requirement: "### Requirement: 平台不可用时失败关闭"
    tasks: ["2.13"]
    checks: [PV4]
    evidence: [reports/wp3-identity-keystore.log, reports/pv5-windows-dpapi.log]
  - id: R81
    source:
      path: specs/platform-keystore/spec.md
      heading: "### Requirement: 密钥与凭据不进入可观察的非安全位置"
    tasks: ["2.8", "2.12", "2.13"]
    checks: [PV3, PV4]
    evidence: [reports/wp2-identity-auth-ports.log, reports/wp3-identity-keystore.log]
  - id: R82
    source:
      path: specs/platform-keystore/spec.md
      heading: "#### Scenario: 调试输出不含密钥材料"
      requirement: "### Requirement: 密钥与凭据不进入可观察的非安全位置"
    tasks: ["2.8", "2.12", "2.13"]
    checks: [PV3, PV4]
    evidence: [reports/wp2-identity-auth-ports.log, reports/wp3-identity-keystore.log]
  - id: R83
    source:
      path: specs/platform-keystore/spec.md
      heading: "#### Scenario: 记录与错误不携带秘密"
      requirement: "### Requirement: 密钥与凭据不进入可观察的非安全位置"
    tasks: ["2.8", "2.12", "2.13"]
    checks: [PV3, PV4]
    evidence: [reports/wp2-identity-auth-ports.log, reports/wp3-identity-keystore.log]
  - id: R84
    source:
      path: specs/platform-keystore/spec.md
      heading: "#### Scenario: 普通数据库只保存引用"
      requirement: "### Requirement: 密钥与凭据不进入可观察的非安全位置"
    tasks: ["2.8", "2.12", "2.13"]
    checks: [PV3, PV4]
    evidence: [reports/wp2-identity-auth-ports.log, reports/wp3-identity-keystore.log]
  - id: R85
    source:
      path: specs/platform-keystore/spec.md
      heading: "### Requirement: 条目与引用之间没有分布式事务但可恢复"
    tasks: ["2.14"]
    checks: [PV4]
    evidence: [reports/wp3-identity-keystore.log]
  - id: R86
    source:
      path: specs/platform-keystore/spec.md
      heading: "#### Scenario: 引用提交失败不影响旧条目"
      requirement: "### Requirement: 条目与引用之间没有分布式事务但可恢复"
    tasks: ["2.14"]
    checks: [PV4]
    evidence: [reports/wp3-identity-keystore.log, reports/rv5-pv4.log, reports/rv5-mutation.log]
  - id: R87
    source:
      path: specs/platform-keystore/spec.md
      heading: "#### Scenario: 孤儿可回收且不影响现有身份"
      requirement: "### Requirement: 条目与引用之间没有分布式事务但可恢复"
    tasks: ["2.14"]
    checks: [PV4]
    evidence: [reports/wp3-identity-keystore.log]
  - id: R88
    source:
      path: specs/platform-keystore/spec.md
      heading: "### Requirement: 非硬件保护实现默认不启用"
    tasks: ["2.1", "2.10", "2.13"]
    checks: [PV4]
    evidence: [reports/wp3-identity-keystore.log]
  - id: R89
    source:
      path: specs/platform-keystore/spec.md
      heading: "#### Scenario: 默认档位不是进程内实现"
      requirement: "### Requirement: 非硬件保护实现默认不启用"
    tasks: ["2.1", "2.10", "2.13"]
    checks: [PV4]
    evidence: [reports/wp3-identity-keystore.log]
  - id: R90
    source:
      path: specs/platform-keystore/spec.md
      heading: "#### Scenario: 开发档位必须显式选择"
      requirement: "### Requirement: 非硬件保护实现默认不启用"
    tasks: ["2.1", "2.10", "2.13"]
    checks: [PV4]
    evidence: [reports/wp3-identity-keystore.log]
```

## Work Packages

| ID | Goal / Scenarios | Dependencies | Owner | Reviewer | Branch / Worktree | Write Scope | Inputs / Outputs | Verification |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| WP1 | 契约与依赖边界冻结：把 `IDENTITY_AND_AUTH_CONTRACT.md` §2/§4.1/§5.1/§7 的目标形状定型为已实现形状（含 `PeerTrust` 快照入参、熵源端口、删除 `KeyPurpose::DeviceIdentity`），把 `MODULE_ARCHITECTURE.md` §3/§3.1/§4.8/§4.12/§5 的实现状态与依赖口径改到位，并登记 `[workspace.dependencies]`；覆盖全部需求的契约前提（`platform-keystore` 的用途收口、`scope-expansion` 的唯一机器来源声明） | 无（W0 起点） | 实现 Agent（coder） | 非文档作者（RV1） | `feat/identity-auth-and-keystore` / 主 worktree | `docs/IDENTITY_AND_AUTH_CONTRACT.md`、`docs/MODULE_ARCHITECTURE.md`（§3/§3.1/§4.8/§4.12/§5，不含选型结论）、`Cargo.toml`（仅 `[workspace.dependencies]`） | 输入：`design.md` D1/D7/D9、两份合同文档原文、`compatibility/commands/v1/commands.json`；输出：可被 `check:boundaries`/`check:doc-links`/`check:command-catalog` 读取的口径 | [PV1]、[PV2]（W0 阶段成员未加入，PV2 只核对既有成员与 `core` 闭包） |
| WP2 | `identity-auth`：transcript 装配与固定向量消费（D2）、配对状态机（D3）、握手入口（D4）、授权展开表（D5）、keystore 与熵源端口定义、fake 端口测试基座（D8 前半）；覆盖 `identity-pairing`、`identity-handshake`、`scope-expansion` 的全部需求与场景 | WP1 | 实现 Agent（coder） | 非实现者（RV1） | 同上 | `crates/identity-auth/**`（含 `Cargo.toml`）、`[workspace] members` 中的本 crate 条目、`Cargo.lock` | 输入：`design.md` D2–D5/D8、`compatibility/transcripts/v1/transcripts.json`、`fixtures/{sync,node-link}/v1/transcripts/**`、`compatibility/commands/v1/commands.json`、`core` 的写集与值对象形状；输出：可被 `identity-keystore` 与切片 4–7 依赖的公共形状与常驻回归测试 | [PV3]（+ [PV1]） |
| WP3 | `identity-keystore`：端口实现与条目格式（D6）、DPAPI wrapper 选型实证（D7）、Windows DPAPI 后端与平台测试、非 Windows 失败关闭、条目与引用恢复语义、`.gitleaks.toml` 明文规则；覆盖 `platform-keystore` 的全部需求与场景 | WP1、WP2 | 实现 Agent（coder） | 非实现者（RV1） | 同上 | `crates/identity-keystore/**`（含 `Cargo.toml`）、`[workspace] members` 中的本 crate 条目、`Cargo.lock`、`.gitleaks.toml` | 输入：`design.md` D6/D7/D8 后半、`IDENTITY_AND_AUTH_CONTRACT.md` §7（WP1 定型版）、`SECURITY_DESIGN.md` §9.2/§13.1/§14.1/§20；输出：平台 keystore 适配器、选型结论、平台测试证据 | [PV4]、[PV5]（+ [PV1]）；选型不合格时回到用户决策并记 BLOCKED |
| WP4 | 文档与状态收口：把「已落地」标记与 DPAPI wrapper 选型结论写回 `MODULE_ARCHITECTURE.md` §3/§3.1/§4.12，更新 `README.md`「仓库当前状态」、`docs/DEVELOPMENT_PLAN.md` §2、`AGENTS.md` §4，并补齐 `.gitleaks.toml` 的限制注释；跑通全量统一入口 | WP2、WP3 | 实现 Agent（coder） | 非文档作者（RV1） | 同上 | `docs/MODULE_ARCHITECTURE.md`（§3/§3.1/§4.12 的结论与标记）、`README.md`、`docs/DEVELOPMENT_PLAN.md`、`AGENTS.md`、`.gitleaks.toml` | 输入：WP3 的选型结论与 WP2/WP3 的测试证据；输出：与代码一致的仓库状态与版本口径 | [PV1]、[PV2]（成员已加入后逐条核对 §5） |

写范围说明（如实登记重叠，不假装完全隔离）：`Cargo.toml` 的 `[workspace] members` 由 WP2 与 WP3 各写一次（按波次 W1 → W2 串行，不并发）；`Cargo.lock` 在 W1/W2 随成员与直接依赖各变更一次；`docs/MODULE_ARCHITECTURE.md` 由 WP1（§3.1 依接口口径、§4.8/§4.12 职责）与 WP4（已落地标记与选型结论）先后写入，两波之间串行；`.gitleaks.toml` 只由 WP3 写。

说明：Main E2E mode 为 `not-applicable`，因此不生成独立的测试工作包（TP）与 Test Design 分组；各工作包自带其行为测试（局部自检），最终替代验证由主 Agent 任务（7.1）统一执行并留证。

## Execution Waves

| Wave | Work Packages | 依赖满足条件 | 并发上限 | 说明 |
| --- | --- | --- | --- | --- |
| W0 契约与依赖冻结（串行，单执行者） | 1.1 → WP1（2.1–2.3） | 已完成的仓库工作区 | 1 | 文档与 `[workspace.dependencies]` 在 W0 只有一个写入者；本波完成条件 = [PV1]/[PV2] 绿（新 crate 尚未加入 `members`，因此矩阵新增行不受影响）+ 一个基线提交 |
| W1 `identity-auth` | WP2（2.4–2.9） | W0 的基线提交 | 1 | 本波把 `crates/identity-auth` 写入 `members`；公共形状（端口、入口的领域值返回与快照入参、错误分类）在本波内冻结，供 WP3 与切片 4–7 使用（写集由调用方按 §11.6 组装，见 design「实现期的口径收窄」） |
| W2 `identity-keystore` | WP3（2.10–2.15） | WP2 的端口与秘密类型冻结 | 1 | 本波把 `crates/identity-keystore` 写入 `members`；`Cargo.toml` 的两次成员写入按 W1 → W2 串行，不并发 |
| W3 文档收口 | WP4（2.16–2.17） | WP2、WP3 完成 | 1 | 已落地标记与选型结论只在实现确实完成后写入 |
| W4 交付前验证 | 3.1–3.8 | W0–W3 完成 | 1 | [PV1]–[PV5] 与 RV1；review 必须由不继承实现对话的 reviewer 完成 |
| W5 集成与合入 | 5.1–5.2 → 6.1–6.8 | 全部 3.x 完成 | 1 | `integrated` 单一交付单元；每个目标分支只允许一个集成执行者串行更新 |
| W6 最终替代验证与验收 | 7.1–7.3 → 8.1 | 6.7、6.8 | 1 | `not-applicable` 的替代验证 + `[e2e-owned]` 门禁 + 最终验收 |

**多 Agent 分派规则**（宿主支持子 Agent / 可开独立会话时）：

- 每个 WP 一个独立 coder 执行者与其自己的 worktree（`git worktree add`）；构建目录用 `CARGO_TARGET_DIR` 隔离，**不允许两个执行者共用同一个 `target/`**。本变更的三条代码轨道（WP1 文档/依赖、WP2、WP3）存在顺序依赖（合同口径 → 端口 → 平台实现），因此默认**串行**；无并发能力时按 roles 串行执行并如实记录，串行不改变各 WP 的完成条件与证据要求。
- 每个 WP 的交付前独立 review（RV1）必须由**不继承实现对话**的执行者完成；缺少隔离上下文时相关任务记 BLOCKED，不得以自审替代。
- 只有主 Agent 可写 `plan.md`/`tasks.md`/`verification.md`；子 Agent 返回结构化 handoff（固定提交、命令、日志路径、差异范围）。
- 文件单一写入者（跨轨道也不得并发写）：`docs/IDENTITY_AND_AUTH_CONTRACT.md`、`docs/MODULE_ARCHITECTURE.md`（WP1，随后 WP4 在 W3 串行接管）、`Cargo.toml` 的 `[workspace.dependencies]`（WP1）、`crates/identity-auth/**` 与 `members` 条目（WP2）、`crates/identity-keystore/**` 与 `members` 条目、`.gitleaks.toml`（WP3）、`README.md`/`docs/DEVELOPMENT_PLAN.md`/`AGENTS.md`（WP4）、`reports/*` 按检查 ID 一文件一写者。
- 本地 Windows x64 上的 DPAPI 证据（[PV5]）与其它使用同一用户 profile 的检查串行执行：DPAPI 的 current-user scope 是本机用户级资源，测试之间靠各自的临时 keystore 目录隔离，但不允许两轮测试并行写同一目录。

## Dependency Handoffs

| Downstream | Upstream | Accepted Revision / Evidence | Start Revision | Transfer / Inclusion Check | Invalidation |
| --- | --- | --- | --- | --- | --- |
| WP2 | WP1 | 2.1–2.3 的合同/依赖口径与 [PV1]/[PV2] 通过证据 | 变更分支起点 | `MODULE_ARCHITECTURE.md` §5 的 `identity-auth` 行与 `Cargo.toml` 的 `[workspace.dependencies]` 一致；`IDENTITY_AND_AUTH_CONTRACT.md` §5.1/§7 已是定型版 | 合同 §5.1/§7 或依赖口径变化 → WP2/WP3/WP4 复验 |
| WP3 | WP1、WP2 | WP2 冻结的端口 trait、`KeyHandle`/`SecretBytes`/`P1363Signature`/`KeystoreError`/熵源端口 + 3.3 的 [PV3] 证据 | W1 完成点 | `cargo build -p identity-keystore` 通过且只使用 `identity-auth` 的公开项（不 import 其内部模块） | 端口或秘密类型形状变化 → WP3/WP4 复验 |
| WP4 | WP2、WP3 | 两个 crate 的 [PV3]/[PV4]（+ [PV5]）证据与 RV1 review 报告、DPAPI wrapper 选型结论 | W2 完成点 | 文档中的「已落地」与版本口径与 `cargo metadata`/`cargo tree` 实际一致 | 选型结论或成员集合变化 → WP4 复验 |
| DU1 | WP1–WP4 | 各 WP 的检查证据与 RV1 review 报告 | W5 完成点 | `npm run verify` 全绿 + 变更 diff 无未登记文件 | 任一上游变化 → 候选重建、[PV1] 重跑 |

## Runtime Resources

| Checks / Work Packages | Resource | Isolation / Configuration | Exclusive Scheduling | Owner / Cleanup |
| --- | --- | --- | --- | --- |
| [PV1]–[PV5]、WP1–WP4 | Rust 构建目录 `target/` | 每个并行执行者设自己的 `CARGO_TARGET_DIR`；本变更默认串行，同一 `target/` 不得并发写 | 按 WP 分 worktree；同 crate 并发时独占构建目录 | 各执行者，只清理自身产生的临时目录与 worktree |
| [PV4]、WP3 的非平台用例 | 临时 keystore 目录（每个用例一个独立目录） | 用例自建临时目录并在结束时删除；不写仓库或共享临时路径 | 无（目录级隔离） | 实现 Agent |
| [PV5]、WP3 的 DPAPI 用例 | 本机 Windows x64 的当前用户 DPAPI 保护密钥与 `%LOCALAPPDATA%` 下的临时 keystore 目录 | 无法在 Linux CI 复现；必须在本地 Windows 执行并保存原始日志；每个用例一个目录，结束即删 | DPAPI current-user scope 是用户级资源：同一时刻只允许一轮 DPAPI 用例，避免共享目录 | 主 Agent／实现 Agent，用例结束删除自身目录 |

说明：本变更不涉及数据库服务、容器、端口、外部账号或网络等共享运行资源；`target/` 与临时 keystore 目录是本变更仅有的运行设施，已按上表隔离。

## Target Repository and Main Branch

- Code Repository: `D:\Project\acp-remote`
- Target Kind / Ref: local；`refs/heads/main`
- Version Confirmation Owner: 主 Agent（机械核实可派发 environment/recon）
- Confirmation Method / Evidence: `git rev-parse refs/heads/main`、`git status --porcelain` 与 `git worktree list`；实际提交与核实结果记录在 `verification.md`

## Merge Strategy

| Delivery Unit / Mode | WP / TP | Owners: Implementation / Test / Review / Merge | Start / Readiness | Candidate Checks / Critical E2E | Order |
| --- | --- | --- | --- | --- | --- |
| DU1 / integrated | WP1–WP4 | 实现 Agent / 实现 Agent（各 WP 自带测试）/ 独立 reviewer / 主 Agent（按当前授权） | 合同与依赖口径冻结 + `identity-auth` 公共形状冻结 + 选型实证完成 | [PV1]、[PV2]、[PV3]、[PV4]、[PV5]；无关键 E2E（mode = not-applicable） | 唯一单元，先候选后合入 |

- Integration Branch / Worktree: `feat/identity-auth-and-keystore`（主 worktree；如需并行分片则用 `CARGO_TARGET_DIR` 隔离构建目录）
- Source Revision / Handoff: 变更分支起点 = 计划确认时的 `refs/heads/main` 提交（在 6.1 记录固定值）
- Operation Boundary / Report Path: apply 覆盖本地合入；推送远端、回滚与发布另需明确授权；集成报告写入 `openspec/changes/identity-auth-and-keystore/reports/du1-integration.md`

采用 integrated 的理由：`Cargo.toml` 的 `members`、`MODULE_ARCHITECTURE.md` §5 的记载与两个 crate 的实现/测试由 `check:boundaries` 相互绑定；`identity-keystore` 的编译依赖 `identity-auth` 的公开端口，`.gitleaks.toml` 的规则依赖 WP3 定型的条目格式。任何一方单独先合入都会让 `main` 上的 `npm run check` 或 `cargo build --workspace` 变红，因此必须作为一个交付单元一次性合入。

## Verification Strategy

### Local Checks

- 实现期：`cargo fmt --all -- --check`、`cargo clippy --locked -p identity-auth -p identity-keystore --all-targets --all-features -- -D warnings`、`cargo test --locked -p identity-auth --all-features`、`cargo test --locked -p identity-keystore --all-features`。
- 合同侧：`node scripts/check-crate-boundaries.mjs`、`node scripts/check-doc-links.mjs`（改文档后必跑）、`node scripts/check-command-catalog.mjs`（确认 `commands.json` 与相关表格未被改动）。
- 平台侧：Windows 本机执行 `cargo test --locked -p identity-keystore --all-features dpapi -- --nocapture` 并保存原始日志。
- 审查重点自检：`grep -rn "unwrap()\|expect(\|panic!\|unreachable!\|from_der" crates/identity-auth/src crates/identity-keystore/src`（**可失败路径**零命中；已确认为不变的构造与测试内部断言不计入，命中处必须在 `verification.md` 逐条登记理由）、`rg -n "cfg\\(windows\\)|cfg\\(unix\\)|cfg\\(target_os" crates/identity-auth/src`（零命中）、`rg -n "\\.await" crates/identity-auth/src` 对照持锁位置（不得跨 `await` 持锁）。

### Project Verify

| Check ID | Stages / Work Packages | Command / Working Directory | Configuration / Environment | Scope / Pass Criteria | Evidence |
| --- | --- | --- | --- | --- | --- |
| PV1 | 候选、主分支、最终（WP1–WP4） | `npm run verify`（= `npm run check` + `cargo fmt --all -- --check` + `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` + `cargo test --locked --workspace --all-features`）@ `D:\Project\acp-remote` | Node ≥ 22.12、`rust-toolchain.toml` 固定的工具链；不联网 | 退出码 0 且逐子项无失败、无零测试、无全跳过；`npm run check` 的十道门禁全绿 | `openspec/changes/identity-auth-and-keystore/reports/du1-pv1.log`、`openspec/changes/identity-auth-and-keystore/reports/du1-main-verify.log` |
| PV2 | 候选、主分支（WP1、WP4） | `node scripts/check-crate-boundaries.mjs` @ 仓库根 | 需要本地 `cargo`（`cargo metadata`/`cargo tree`），不联网 | §5 矩阵逐条成立：`identity-auth` 行只依赖 `core`/`sync-protocol`/`node-link-protocol`/`acpr-transcript`（`acpr-wire` 格为空必须被拒），`identity-keystore` 行只依赖 `identity-auth`，`core` 闭包与 allow-list 逐项相等 | `reports/wp1-boundaries.log`、`openspec/changes/identity-auth-and-keystore/reports/du1-pv1.log` |
| PV3 | 各 WP 交付前、最终（WP2） | `cargo test --locked -p identity-auth --all-features` @ 仓库根 | 本地 Rust 工具链；只读 fixtures 与 `compatibility/` 资产 | 12 个 transcript 向量逐字节重算、SAS 期望值一致、畸形输入被拒、配对/握手/展开的失败路径与边界场景全部通过，且新增用例确实被执行（无 0 用例 / 无全跳过） | `openspec/changes/identity-auth-and-keystore/reports/wp2-identity-auth-pairing.log`、`openspec/changes/identity-auth-and-keystore/reports/wp2-identity-auth-handshake.log`、`openspec/changes/identity-auth-and-keystore/reports/wp2-identity-auth-expansion.log`、`openspec/changes/identity-auth-and-keystore/reports/wp2-identity-auth-ports.log` |
| PV4 | 各 WP 交付前、最终（WP3） | `cargo test --locked -p identity-keystore --all-features` @ 仓库根 | 本地 Rust 工具链；临时 keystore 目录（用例自建自删） | 条目格式与原子写、失败关闭（缺条目/损坏/非 Windows）、条目与引用恢复语义、秘密不进 `Debug` 的断言全部通过；Windows 平台用例可在此命令中被跳过，但跳过数必须如实记录 | `openspec/changes/identity-auth-and-keystore/reports/wp3-identity-keystore.log` |
| PV5 | 候选、主分支（WP3 的 DPAPI 用例） | Windows 本机：`cargo test --locked -p identity-keystore --all-features dpapi -- --nocapture` @ `D:\Project\acp-remote` | **只在 Windows x64 执行**；Linux CI 只跑 `#[cfg(not(windows))]` 失败关闭分支 | DPAPI（当前用户 scope）包裹/解包往返成功、读出的 65 字节公钥与签名一致（签名可被该公钥验证）、被篡改条目解包失败且不覆盖，且输出原始日志 | `openspec/changes/identity-auth-and-keystore/reports/pv5-windows-dpapi.log` |
| RV1 | 各 WP 交付前、主分支复核 | 独立 reviewer 按 `roles/reviewer.md` 在隔离上下文检视固定版本 diff 与契约 | 只读；不修改代码与证据 | 报告完整、无未解决阻断项；覆盖 specs 场景与 design 决策 | `openspec/changes/identity-auth-and-keystore/reports/rv1-wp1.md`、`openspec/changes/identity-auth-and-keystore/reports/rv1-wp2.md`、`openspec/changes/identity-auth-and-keystore/reports/rv1-wp3.md`、`openspec/changes/identity-auth-and-keystore/reports/rv1-wp4.md`、`openspec/changes/identity-auth-and-keystore/reports/rv1-du1.md` |

### Code Review

- 范围：WP1（合同定型的收口措辞是否与 design D4/D6 一致、依赖口径与 `[workspace.dependencies]`）、WP2（transcript 装配是否只从协议表取 domain/tag、验签公钥是否只来自快照、是否存在 `from_der`、重放与一次性消费顺序、展开表与 `commands.json` 的逐项一致、持锁是否跨 `await`）、WP3（条目格式与原子写、失败关闭路径、密钥与凭据是否可能进入 `Debug`/日志/错误、DPAPI 使用是否仅为「包裹 + 进程内签名」、选型实证是否留证）、WP4（状态表与版本口径是否与 `cargo metadata` 实际一致）。
- 关注点：① 是否有任何路径用握手消息自带的公钥验签；② 是否存在「先返回成功、后写库」或「内存已批准、库无信任」；③ 是否有非规范 base64url 被接受；④ 是否把私钥/Provider 凭据写入日志、错误、测试快照或普通 SQLite 字段；⑤ 平台 `cfg` 是否只出现在 `identity-keystore`；⑥ 是否为通过检查而放宽 `unsafe`/lint 或抬高 MSRV。
- 阻断标准：任一规格场景缺覆盖、任何 [PV2] 依赖差异、任何秘密材料出现在可观察位置、任何「平台不可用就静默生成新身份」的路径、任何把配对 secret 明文落库的实现、任何跨 `await` 持锁。
- 复核安排：修复后由**新的**隔离子 Agent 对新固定版本复核（与 `roles/reviewer.md` 及 `tasks.md` 3.4 的口径一致），并保留原报告与复核结论。

### Main E2E

```yaml
mode: not-applicable
reason: "本变更只落地两个库 crate（身份状态机与平台 keystore 适配器）。仓库中没有可端到端运行的产品入口——`server`（含 `local_admin`/`node_link`/`sync`/`acp_facade`）、`app`/Daemon、CLI 与前端均未实现，没有本地管理通道、没有监听器、没有可驱动配对的 CLI 子命令；`identity-auth` 的持久事实由切片 4/5 的组合根装配，当前只能由测试替身驱动。因此不存在可执行产品路径的用例。"
basis: "`openspec/config.yaml` 的 context 已记录「当前阶段没有可端到端运行的产品路径，Main E2E 按变更记 not-applicable，需用户逐变更批准并写入 plan.md 的 downgrade_approval」，且 `x-agentic.e2e.command` 为空（`openspec-agentic e2e --json` 实测：enabled=true、command=\"\"）。本变更是库级实现，不改任何 wire 协议与封闭词表，不影响 E2E 适用性判断。"
alternative_checks:
  - "cargo test --locked -p identity-auth --all-features [PV3]：transcript 12 向量逐字节重算（含 transcriptBase64url/transcriptSha256Hex/hmacSha256/sas）、签名域用 fixture 公钥验证固定 P1363 签名、畸形输入负例、配对创建/认领/落定/过期/重启终结/失败计数、握手签发与校验顺序、一次性 nonce 与重放、注入时钟过期、凭据状态映射、展开表与 commands.json 漂移。"
  - "cargo test --locked -p identity-keystore --all-features [PV4]：条目格式与原子写、缺条目/损坏条目的失败关闭、非 Windows 明确失败、条目与引用恢复语义、秘密不进 Debug/日志。"
  - "Windows 本机 cargo test -p identity-keystore --all-features dpapi [PV5]：把 SECURITY_DESIGN.md §9.2 的 Windows 档位（DPAPI 当前用户 scope 包裹 + 进程内签名）固化为常驻回归测试。"
  - "npm run check [PV2 等]：十道合同门禁，重点为 §5 依赖矩阵扫描、文档引用与 `commands.json` 词表未被改动。"
  - "npm run verify [PV1]：统一合同门禁 + fmt/clippy/全 workspace 测试，确认新增成员与依赖不破坏既有 crate。"
downgrade_approval: "2026-09-24，本会话，用户原话：「1和2都同意」。其中第 1 项对应本变更 Main E2E 记 not-applicable 及上述替代验证清单（本会话提问原文点名 `cargo test --locked -p identity-auth -p identity-keystore --all-features` + 本地 Windows DPAPI 测试 + `npm run check` + `npm run verify`），来源为用户对本会话提问中第 1 问的批准；本记录不沿用 2026-09-23 的首次确认，也不沿用 2026-09-24 另一次实现切片的批准。"
```

#### E2E Ownership and Cases

不适用（mode = not-applicable），本节按模板要求删除；实际替代验证已列为主 Agent 任务（7.1）并在 Coverage Index 中引用。

## Failure and Recovery

- 失败修复：按原 WP ID 重新派发（不新增 WP），修复后重跑该 WP 的 [PV3]/[PV4]/[PV5] 与 RV1；受影响的下游按 Dependency Handoffs 的失效列重开。
- 传递下游失效：合同 §5.1/§7 定型或端口/秘密类型形状变化 → WP3/WP4 与 DU1 的既有证据失效；DPAPI wrapper 选型变化 → WP3 的平台证据与 WP4 的版本口径重跑；§5 矩阵或依赖口径变化 → WP2/WP3 复验。
- 选型失败（DPAPI wrapper 不满足 §20 核验）：立即停止该路径并把证据交用户决策（换 wrapper、自写 wrapper crate + 新增 ADR、或抬高 MSRV）；不擅自放开 `unsafe_code = \"forbid\"`、不擅自抬 MSRV。
- 主分支失败：`main` 上的 `npm run verify` 失败时停止后续合入，按同一验证路径交付修复；必要时回滚该交付单元（`git revert` 或恢复候选前提交），不允许「红着继续合」。
- 共享资源异常：临时 keystore 目录残留时只删自身用例创建的目录；`target/` 污染时 `cargo clean -p identity-auth -p identity-keystore` 后重跑受影响检查。特别地，若某个 keystore 用例在 DPAPI 解包路径失败并留下无法解包的文件，必须删除该临时目录后再复跑，避免「上一次的坏条目」被当作本次结果。
- 平台限制：[PV5] 无法在 Linux CI 复现；若本地 Windows 不可用，该检查记 BLOCKED 并如实上报，不得用 Unix 路径的结果替代。
- 无 E2E 执行，故不涉及 E2E 失败上限；若后续有人把 mode 改为 required，必须先取得新的用户批准并补齐 E2E 任务与入口。
- 未执行项：`cargo-deny`（`deps`/`advisories`）与 `gitleaks`（`secrets`）本地没有等价物，只在 CI 运行；本变更新增的 `getrandom` 与 DPAPI wrapper 的许可证/来源/advisory 判定**只在 CI 完成**，本地 `npm run verify` 不是证据，最终验收中明确记录。

## Completion Criteria

- 全部任务勾选（`verification.md` 中逐 ID 关联证据；结果仅 PASS/FAIL/BLOCKED/NOT_APPLICABLE）。
- 最终主分支版本（`refs/heads/main` 的实际提交）上：[PV1]、[PV2]、[PV3]、[PV4] 全绿；[PV5] 在本地 Windows 上执行并留证（或如实记 BLOCKED）；RV1 无未解决阻断项；E2E 记 NOT_APPLICABLE 且替代验证（7.1）已完成并留证。
- 阻断问题清零：无未闭环的 FAIL/BLOCKED、无未登记漂移、无秘密材料进入日志/错误/测试快照/普通 SQLite 字段、无「平台不可用时静默生成新身份」的路径、无跨 `await` 持锁。
- 最终验收由主 Agent 按 `.agents/skills/agentic-verify/SKILL.md` 执行，并在验收块中以唯一 `[final-verification]` 任务记录。
- 本计划不构成合并、推送、回滚或发布授权。
