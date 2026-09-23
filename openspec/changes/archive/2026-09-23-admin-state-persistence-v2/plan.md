# admin-state-persistence-v2 执行计划

## Scope and Contracts

- Specs Revision: 本变更 `specs/` 下 5 个新增能力规范的初版（`admin-state-persistence`、`peer-identity-material`、`local-agent-config`、`workspace-resolution`、`storage-schema-v2-migration`）；`openspec/specs/` 之前为空，本次全部为 ADDED。
- Design Revision: `design.md` 的 D1–D7（依赖边界、写集端口形状、公钥构造、错误映射、DDL/migration 组织、workspace 解析、文档并入策略）与 Migration Plan 初版。
- Convergence Check: 一致。specs 的行为要求（原子写集、公钥绑定、失败关闭、迁移保留、无正文、路径不泄漏）与 design 的接口/数据约定（写集 DTO、`ResolvedWorkspace`、v2 DDL 与迁移顺序、`CredentialResolver` 交集规则）无冲突；唯一跨文档硬依赖是 §5/§7 与实现的逐条一致（漂移门禁），已在 WP5/WP4/WP2 的写范围内闭合。
- Skip Specs: no

## Contract Changes

本变更**改变**以下已冻结契约（原/新值、原因与影响分析随实现写入 `verification.md` 的 Check Plan Changes）：

- `docs/CORE_PORTS_AND_STORAGE.md` §5.3 的 `TrustStore`/`ExportStore` 签名、§7.2 的版本常量、§7.3/§7.4 的 DDL、§3.5/§3.6 的值对象行、§9 的判据编号、§11 的地位（目标形状 → 已实现）。
- `core::ports` 的破坏性签名变化（写集 DTO、角色维度、新端口）与 `core::model` 的新值对象；`CreateSessionRequest` 的 workspace 字段。
- `scripts/check-crate-boundaries.mjs` 的 `CORE_ALLOWED_CLOSURE`（core 新增 `p256`/`sha2` 闭包）。

不改变的契约：Sync/Node Link wire schema、`schemas/`、`fixtures/{sync,node-link,acp,local-admin}/`、`compatibility/` 的封闭词表与错误码。

## Coverage Index

```agentic-coverage
version: 1
target_ref: refs/heads/main
rows:
  - id: R1
    source:
      path: specs/admin-state-persistence/spec.md
      heading: "### Requirement: 管理写集的原子性与失败关闭"
    tasks: ["2.12", "3.9"]
    checks: [PV4]
    evidence: [reports/wp6-admin-store-tests.log]
  - id: R2
    source:
      path: specs/admin-state-persistence/spec.md
      heading: "#### Scenario: 审计写入失败时状态不落库"
      requirement: "### Requirement: 管理写集的原子性与失败关闭"
    tasks: ["2.12", "3.9"]
    checks: [PV4]
    evidence: [reports/wp6-admin-store-tests.log]
  - id: R3
    source:
      path: specs/admin-state-persistence/spec.md
      heading: "#### Scenario: 约束冲突时不留下半条授权"
      requirement: "### Requirement: 管理写集的原子性与失败关闭"
    tasks: ["2.12", "3.9"]
    checks: [PV4]
    evidence: [reports/wp6-admin-store-tests.log]
  - id: R4
    source:
      path: specs/admin-state-persistence/spec.md
      heading: "### Requirement: 配对认领与落定的单事务语义"
    tasks: ["2.19", "3.9"]
    checks: [PV4]
    evidence: [reports/wp6-admin-store-tests.log]
  - id: R5
    source:
      path: specs/admin-state-persistence/spec.md
      heading: "#### Scenario: 并发认领只有一个成功"
      requirement: "### Requirement: 配对认领与落定的单事务语义"
    tasks: ["2.19", "3.9"]
    checks: [PV4]
    evidence: [reports/wp6-admin-store-tests.log]
  - id: R6
    source:
      path: specs/admin-state-persistence/spec.md
      heading: "#### Scenario: 拒绝或过期不创建信任"
      requirement: "### Requirement: 配对认领与落定的单事务语义"
    tasks: ["2.19", "3.9"]
    checks: [PV4]
    evidence: [reports/wp6-admin-store-tests.log]
  - id: R7
    source:
      path: specs/admin-state-persistence/spec.md
      heading: "### Requirement: 撤销与重启恢复"
    tasks: ["2.19", "3.9"]
    checks: [PV4]
    evidence: [reports/wp6-admin-store-tests.log]
  - id: R8
    source:
      path: specs/admin-state-persistence/spec.md
      heading: "#### Scenario: 重启后撤销仍然有效"
      requirement: "### Requirement: 撤销与重启恢复"
    tasks: ["2.19", "3.9"]
    checks: [PV4]
    evidence: [reports/wp6-admin-store-tests.log]
  - id: R9
    source:
      path: specs/admin-state-persistence/spec.md
      heading: "#### Scenario: 重启终结已过期的未确认配对"
      requirement: "### Requirement: 撤销与重启恢复"
    tasks: ["2.19", "3.9"]
    checks: [PV4]
    evidence: [reports/wp6-admin-store-tests.log]
  - id: R10
    source:
      path: specs/admin-state-persistence/spec.md
      heading: "### Requirement: Export 与 Import 的归属与完整移除"
    tasks: ["2.20", "3.9"]
    checks: [PV4]
    evidence: [reports/wp6-admin-store-tests.log]
  - id: R11
    source:
      path: specs/admin-state-persistence/spec.md
      heading: "#### Scenario: 同一 Export 归属冲突被拒"
      requirement: "### Requirement: Export 与 Import 的归属与完整移除"
    tasks: ["2.20", "3.9"]
    checks: [PV4]
    evidence: [reports/wp6-admin-store-tests.log]
  - id: R12
    source:
      path: specs/admin-state-persistence/spec.md
      heading: "#### Scenario: 完整移除后审计仍在"
      requirement: "### Requirement: Export 与 Import 的归属与完整移除"
    tasks: ["2.20", "3.9"]
    checks: [PV4]
    evidence: [reports/wp6-admin-store-tests.log]
  - id: R13
    source:
      path: specs/admin-state-persistence/spec.md
      heading: "### Requirement: 管理审计的取值闭合与无内容"
    tasks: ["2.5", "2.15", "3.7"]
    checks: [PV2]
    evidence: [reports/wp5-contract-drift-before-after.md]
  - id: R14
    source:
      path: specs/admin-state-persistence/spec.md
      heading: "#### Scenario: 未登记动作无法写入"
      requirement: "### Requirement: 管理审计的取值闭合与无内容"
    tasks: ["2.5", "2.15", "3.7"]
    checks: [PV2]
    evidence: [reports/wp5-contract-drift-before-after.md]
  - id: R15
    source:
      path: specs/admin-state-persistence/spec.md
      heading: "#### Scenario: 失败请求的审计不含内容"
      requirement: "### Requirement: 管理审计的取值闭合与无内容"
    tasks: ["2.5", "2.15", "3.7"]
    checks: [PV2]
    evidence: [reports/wp5-contract-drift-before-after.md]
  - id: R16
    source:
      path: specs/peer-identity-material/spec.md
      heading: "### Requirement: 公钥值对象的构造校验与指纹派生"
    tasks: ["2.4", "3.1"]
    checks: [PV4]
    evidence: [reports/wp1-core-tests.log]
  - id: R17
    source:
      path: specs/peer-identity-material/spec.md
      heading: "#### Scenario: 接受合法未压缩公钥"
      requirement: "### Requirement: 公钥值对象的构造校验与指纹派生"
    tasks: ["2.4", "3.1"]
    checks: [PV4]
    evidence: [reports/wp1-core-tests.log]
  - id: R18
    source:
      path: specs/peer-identity-material/spec.md
      heading: "#### Scenario: 拒绝压缩点与非法长度"
      requirement: "### Requirement: 公钥值对象的构造校验与指纹派生"
    tasks: ["2.4", "3.1"]
    checks: [PV4]
    evidence: [reports/wp1-core-tests.log]
  - id: R19
    source:
      path: specs/peer-identity-material/spec.md
      heading: "### Requirement: 配对携带公钥并绑定到信任材料"
    tasks: ["2.19", "3.9"]
    checks: [PV4]
    evidence: [reports/wp6-admin-store-tests.log]
  - id: R20
    source:
      path: specs/peer-identity-material/spec.md
      heading: "#### Scenario: 认领后重启仍能取到验签公钥"
      requirement: "### Requirement: 配对携带公钥并绑定到信任材料"
    tasks: ["2.19", "3.9"]
    checks: [PV4]
    evidence: [reports/wp6-admin-store-tests.log]
  - id: R21
    source:
      path: specs/peer-identity-material/spec.md
      heading: "#### Scenario: 指纹与公钥不一致无法落库"
      requirement: "### Requirement: 配对携带公钥并绑定到信任材料"
    tasks: ["2.19", "3.9"]
    checks: [PV4]
    evidence: [reports/wp6-admin-store-tests.log]
  - id: R22
    source:
      path: specs/peer-identity-material/spec.md
      heading: "### Requirement: 双角色身份一致与撤销覆盖"
    tasks: ["2.19", "3.9"]
    checks: [PV4]
    evidence: [reports/wp6-admin-store-tests.log]
  - id: R23
    source:
      path: specs/peer-identity-material/spec.md
      heading: "#### Scenario: 双角色共享身份材料"
      requirement: "### Requirement: 双角色身份一致与撤销覆盖"
    tasks: ["2.19", "3.9"]
    checks: [PV4]
    evidence: [reports/wp6-admin-store-tests.log]
  - id: R24
    source:
      path: specs/peer-identity-material/spec.md
      heading: "#### Scenario: 按节点撤销覆盖两种角色"
      requirement: "### Requirement: 双角色身份一致与撤销覆盖"
    tasks: ["2.19", "3.9"]
    checks: [PV4]
    evidence: [reports/wp6-admin-store-tests.log]
  - id: R25
    source:
      path: specs/peer-identity-material/spec.md
      heading: "#### Scenario: 角色指纹不一致被拒"
      requirement: "### Requirement: 双角色身份一致与撤销覆盖"
    tasks: ["2.19", "3.9"]
    checks: [PV4]
    evidence: [reports/wp6-admin-store-tests.log]
  - id: R26
    source:
      path: specs/peer-identity-material/spec.md
      heading: "### Requirement: 身份变化不自动接受"
    tasks: ["2.19", "3.9"]
    checks: [PV4]
    evidence: [reports/wp6-admin-store-tests.log]
  - id: R27
    source:
      path: specs/peer-identity-material/spec.md
      heading: "#### Scenario: 已绑定身份换钥被拒"
      requirement: "### Requirement: 身份变化不自动接受"
    tasks: ["2.19", "3.9"]
    checks: [PV4]
    evidence: [reports/wp6-admin-store-tests.log]
  - id: R28
    source:
      path: specs/peer-identity-material/spec.md
      heading: "#### Scenario: 已撤销身份不能经普通写入复活"
      requirement: "### Requirement: 身份变化不自动接受"
    tasks: ["2.19", "3.9"]
    checks: [PV4]
    evidence: [reports/wp6-admin-store-tests.log]
  - id: R29
    source:
      path: specs/peer-identity-material/spec.md
      heading: "### Requirement: 凭据与身份材料的存放边界"
    tasks: ["2.6", "2.21", "3.9"]
    checks: [PV4]
    evidence: [reports/wp6-admin-store-tests.log]
  - id: R30
    source:
      path: specs/peer-identity-material/spec.md
      heading: "#### Scenario: 库内不出现秘密材料"
      requirement: "### Requirement: 凭据与身份材料的存放边界"
    tasks: ["2.6", "2.21", "3.9"]
    checks: [PV4]
    evidence: [reports/wp6-admin-store-tests.log]
  - id: R31
    source:
      path: specs/local-agent-config/spec.md
      heading: "### Requirement: Profile 写入校验与唯一默认"
    tasks: ["2.21", "3.9"]
    checks: [PV4]
    evidence: [reports/wp6-admin-store-tests.log]
  - id: R32
    source:
      path: specs/local-agent-config/spec.md
      heading: "#### Scenario: 默认 profile 唯一且切换原子"
      requirement: "### Requirement: Profile 写入校验与唯一默认"
    tasks: ["2.21", "3.9"]
    checks: [PV4]
    evidence: [reports/wp6-admin-store-tests.log]
  - id: R33
    source:
      path: specs/local-agent-config/spec.md
      heading: "#### Scenario: 非法绑定在写入时被拒"
      requirement: "### Requirement: Profile 写入校验与唯一默认"
    tasks: ["2.21", "3.9"]
    checks: [PV4]
    evidence: [reports/wp6-admin-store-tests.log]
  - id: R34
    source:
      path: specs/local-agent-config/spec.md
      heading: "### Requirement: 首次初始化种子的幂等"
    tasks: ["2.21", "3.9"]
    checks: [PV4]
    evidence: [reports/wp6-admin-store-tests.log]
  - id: R35
    source:
      path: specs/local-agent-config/spec.md
      heading: "#### Scenario: 空种子也标记已初始化"
      requirement: "### Requirement: 首次初始化种子的幂等"
    tasks: ["2.21", "3.9"]
    checks: [PV4]
    evidence: [reports/wp6-admin-store-tests.log]
  - id: R36
    source:
      path: specs/local-agent-config/spec.md
      heading: "#### Scenario: 重复打开不重导种子"
      requirement: "### Requirement: 首次初始化种子的幂等"
    tasks: ["2.21", "3.9"]
    checks: [PV4]
    evidence: [reports/wp6-admin-store-tests.log]
  - id: R37
    source:
      path: specs/local-agent-config/spec.md
      heading: "### Requirement: Provider 引用的无凭据与失效关闭"
    tasks: ["2.10", "2.21", "2.22", "3.9"]
    checks: [PV4]
    evidence: [reports/wp6-admin-store-tests.log]
  - id: R38
    source:
      path: specs/local-agent-config/spec.md
      heading: "#### Scenario: 引用失效即不可用"
      requirement: "### Requirement: Provider 引用的无凭据与失效关闭"
    tasks: ["2.10", "2.21", "2.22", "3.9"]
    checks: [PV4]
    evidence: [reports/wp6-admin-store-tests.log]
  - id: R39
    source:
      path: specs/local-agent-config/spec.md
      heading: "#### Scenario: 引用版本推进"
      requirement: "### Requirement: Provider 引用的无凭据与失效关闭"
    tasks: ["2.10", "2.21", "2.22", "3.9"]
    checks: [PV4]
    evidence: [reports/wp6-admin-store-tests.log]
  - id: R40
    source:
      path: specs/local-agent-config/spec.md
      heading: "### Requirement: 凭据到子进程环境变量的解析边界"
    tasks: ["2.10", "2.22", "3.9"]
    checks: [PV4]
    evidence: [reports/wp6-admin-store-tests.log]
  - id: R41
    source:
      path: specs/local-agent-config/spec.md
      heading: "#### Scenario: 白名单是上限"
      requirement: "### Requirement: 凭据到子进程环境变量的解析边界"
    tasks: ["2.10", "2.22", "3.9"]
    checks: [PV4]
    evidence: [reports/wp6-admin-store-tests.log]
  - id: R42
    source:
      path: specs/local-agent-config/spec.md
      heading: "#### Scenario: 引用失效时失败关闭"
      requirement: "### Requirement: 凭据到子进程环境变量的解析边界"
    tasks: ["2.10", "2.22", "3.9"]
    checks: [PV4]
    evidence: [reports/wp6-admin-store-tests.log]
  - id: R43
    source:
      path: specs/local-agent-config/spec.md
      heading: "#### Scenario: 日志只记录变量名与数量"
      requirement: "### Requirement: 凭据到子进程环境变量的解析边界"
    tasks: ["2.10", "2.22", "3.9"]
    checks: [PV4]
    evidence: [reports/wp6-admin-store-tests.log]
  - id: R44
    source:
      path: specs/local-agent-config/spec.md
      heading: "### Requirement: workspace 记录的本机归属"
    tasks: ["2.21", "3.9"]
    checks: [PV4]
    evidence: [reports/wp6-admin-store-tests.log]
  - id: R45
    source:
      path: specs/local-agent-config/spec.md
      heading: "#### Scenario: 使用不存在目录时明确失败"
      requirement: "### Requirement: workspace 记录的本机归属"
    tasks: ["2.21", "3.9"]
    checks: [PV4]
    evidence: [reports/wp6-admin-store-tests.log]
  - id: R46
    source:
      path: specs/local-agent-config/spec.md
      heading: "#### Scenario: 远程目录不泄漏本机路径"
      requirement: "### Requirement: workspace 记录的本机归属"
    tasks: ["2.21", "3.9"]
    checks: [PV4]
    evidence: [reports/wp6-admin-store-tests.log]
  - id: R47
    source:
      path: specs/workspace-resolution/spec.md
      heading: "### Requirement: 解析归属与后端输入"
    tasks: ["2.13", "3.3"]
    checks: [PV4]
    evidence: [reports/wp3-core-tests.log]
  - id: R48
    source:
      path: specs/workspace-resolution/spec.md
      heading: "#### Scenario: 后端只收到已解析路径"
      requirement: "### Requirement: 解析归属与后端输入"
    tasks: ["2.13", "3.3"]
    checks: [PV4]
    evidence: [reports/wp3-core-tests.log]
  - id: R49
    source:
      path: specs/workspace-resolution/spec.md
      heading: "#### Scenario: 未登记别名在调用后端前失败"
      requirement: "### Requirement: 解析归属与后端输入"
    tasks: ["2.13", "3.3"]
    checks: [PV4]
    evidence: [reports/wp3-core-tests.log]
  - id: R50
    source:
      path: specs/workspace-resolution/spec.md
      heading: "### Requirement: 路径校验与规范化"
    tasks: ["2.13", "3.3"]
    checks: [PV4]
    evidence: [reports/wp3-core-tests.log]
  - id: R51
    source:
      path: specs/workspace-resolution/spec.md
      heading: "#### Scenario: 规范化结果作为权威值"
      requirement: "### Requirement: 路径校验与规范化"
    tasks: ["2.13", "3.3"]
    checks: [PV4]
    evidence: [reports/wp3-core-tests.log]
  - id: R52
    source:
      path: specs/workspace-resolution/spec.md
      heading: "#### Scenario: 拒绝非绝对路径与父目录引用"
      requirement: "### Requirement: 路径校验与规范化"
    tasks: ["2.13", "3.3"]
    checks: [PV4]
    evidence: [reports/wp3-core-tests.log]
  - id: R53
    source:
      path: specs/workspace-resolution/spec.md
      heading: "### Requirement: 解析失败的分类"
    tasks: ["2.13", "3.3"]
    checks: [PV4]
    evidence: [reports/wp3-core-tests.log]
  - id: R54
    source:
      path: specs/workspace-resolution/spec.md
      heading: "#### Scenario: 已声明别名解析失败报服务端错误"
      requirement: "### Requirement: 解析失败的分类"
    tasks: ["2.13", "3.3"]
    checks: [PV4]
    evidence: [reports/wp3-core-tests.log]
  - id: R55
    source:
      path: specs/workspace-resolution/spec.md
      heading: "#### Scenario: 未声明别名报参数错误"
      requirement: "### Requirement: 解析失败的分类"
    tasks: ["2.13", "3.3"]
    checks: [PV4]
    evidence: [reports/wp3-core-tests.log]
  - id: R56
    source:
      path: specs/workspace-resolution/spec.md
      heading: "### Requirement: 路径不泄漏"
    tasks: ["2.13", "3.3"]
    checks: [PV4]
    evidence: [reports/wp3-core-tests.log]
  - id: R57
    source:
      path: specs/workspace-resolution/spec.md
      heading: "#### Scenario: 事件与错误都不含路径"
      requirement: "### Requirement: 路径不泄漏"
    tasks: ["2.13", "3.3"]
    checks: [PV4]
    evidence: [reports/wp3-core-tests.log]
  - id: R58
    source:
      path: specs/storage-schema-v2-migration/spec.md
      heading: "### Requirement: 版本常量与 migration 幂等"
    tasks: ["2.14", "2.17", "3.5"]
    checks: [PV4]
    evidence: [reports/wp4-migration-tests.log]
  - id: R59
    source:
      path: specs/storage-schema-v2-migration/spec.md
      heading: "#### Scenario: 连续两次打开 schema 文本不变"
      requirement: "### Requirement: 版本常量与 migration 幂等"
    tasks: ["2.14", "2.17", "3.5"]
    checks: [PV4]
    evidence: [reports/wp4-migration-tests.log]
  - id: R60
    source:
      path: specs/storage-schema-v2-migration/spec.md
      heading: "#### Scenario: 升级中途失败整体回滚"
      requirement: "### Requirement: 版本常量与 migration 幂等"
    tasks: ["2.14", "2.17", "3.5"]
    checks: [PV4]
    evidence: [reports/wp4-migration-tests.log]
  - id: R61
    source:
      path: specs/storage-schema-v2-migration/spec.md
      heading: "### Requirement: v1 数据保留与 Import 归属迁移"
    tasks: ["2.16", "2.18", "3.5"]
    checks: [PV4]
    evidence: [reports/wp4-migration-tests.log]
  - id: R62
    source:
      path: specs/storage-schema-v2-migration/spec.md
      heading: "#### Scenario: 升级后重放与幂等仍一致"
      requirement: "### Requirement: v1 数据保留与 Import 归属迁移"
    tasks: ["2.16", "2.18", "3.5"]
    checks: [PV4]
    evidence: [reports/wp4-migration-tests.log]
  - id: R63
    source:
      path: specs/storage-schema-v2-migration/spec.md
      heading: "#### Scenario: 缺少可信来源的 Import 保持不可用"
      requirement: "### Requirement: v1 数据保留与 Import 归属迁移"
    tasks: ["2.16", "2.18", "3.5"]
    checks: [PV4]
    evidence: [reports/wp4-migration-tests.log]
  - id: R64
    source:
      path: specs/storage-schema-v2-migration/spec.md
      heading: "### Requirement: 过新版本拒绝打开"
    tasks: ["2.18", "3.5"]
    checks: [PV4]
    evidence: [reports/wp4-migration-tests.log]
  - id: R65
    source:
      path: specs/storage-schema-v2-migration/spec.md
      heading: "#### Scenario: 过新库零写入"
      requirement: "### Requirement: 过新版本拒绝打开"
    tasks: ["2.18", "3.5"]
    checks: [PV4]
    evidence: [reports/wp4-migration-tests.log]
  - id: R66
    source:
      path: specs/storage-schema-v2-migration/spec.md
      heading: "### Requirement: 损坏与权限不符时的失败关闭"
    tasks: ["2.22", "3.9"]
    checks: [PV4]
    evidence: [reports/wp6-admin-store-tests.log]
  - id: R67
    source:
      path: specs/storage-schema-v2-migration/spec.md
      heading: "#### Scenario: 损坏库的写路径全部被拒"
      requirement: "### Requirement: 损坏与权限不符时的失败关闭"
    tasks: ["2.22", "3.9"]
    checks: [PV4]
    evidence: [reports/wp6-admin-store-tests.log]
  - id: R68
    source:
      path: specs/storage-schema-v2-migration/spec.md
      heading: "#### Scenario: 权限过宽在 Unix 上失败关闭"
      requirement: "### Requirement: 损坏与权限不符时的失败关闭"
    tasks: ["2.22", "3.9"]
    checks: [PV4]
    evidence: [reports/wp6-admin-store-tests.log]
  - id: R69
    source:
      path: specs/storage-schema-v2-migration/spec.md
      heading: "### Requirement: 管理表纳入保留与容量"
    tasks: ["2.14", "3.5"]
    checks: [PV4]
    evidence: [reports/wp4-migration-tests.log]
  - id: R70
    source:
      path: specs/storage-schema-v2-migration/spec.md
      heading: "#### Scenario: 超限时拒绝新写入而不是删信任"
      requirement: "### Requirement: 管理表纳入保留与容量"
    tasks: ["2.14", "3.5"]
    checks: [PV4]
    evidence: [reports/wp4-migration-tests.log]
  - id: R71
    source:
      path: specs/storage-schema-v2-migration/spec.md
      heading: "#### Scenario: 度量包含管理表分量"
      requirement: "### Requirement: 管理表纳入保留与容量"
    tasks: ["2.14", "3.5"]
    checks: [PV4]
    evidence: [reports/wp4-migration-tests.log]
  - id: R72
    source:
      path: specs/storage-schema-v2-migration/spec.md
      heading: "### Requirement: imported 家族保持无正文"
    tasks: ["2.20", "3.9"]
    checks: [PV4]
    evidence: [reports/wp6-admin-store-tests.log]
  - id: R73
    source:
      path: specs/storage-schema-v2-migration/spec.md
      heading: "#### Scenario: 黄金列清单逐项相等"
      requirement: "### Requirement: imported 家族保持无正文"
    tasks: ["2.20", "3.9"]
    checks: [PV4]
    evidence: [reports/wp6-admin-store-tests.log]
  - id: R74
    source:
      path: specs/storage-schema-v2-migration/spec.md
      heading: "#### Scenario: 投递正文后库内无正文"
      requirement: "### Requirement: imported 家族保持无正文"
    tasks: ["2.20", "3.9"]
    checks: [PV4]
    evidence: [reports/wp6-admin-store-tests.log]
  - id: R75
    source:
      path: specs/storage-schema-v2-migration/spec.md
      heading: "#### Scenario: 清空与移除都保留审计"
      requirement: "### Requirement: imported 家族保持无正文"
    tasks: ["2.20", "3.9"]
    checks: [PV4]
    evidence: [reports/wp6-admin-store-tests.log]
```

## Work Packages

| ID | Goal / Scenarios | Dependencies | Owner | Reviewer | Branch / Worktree | Write Scope | Inputs / Outputs | Verification |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| WP5 | 合同并入与依赖边界：§11.5–§11.9 → §5.3/§7.2/§7.3/§7.4/§3.5/§3.6/§9/§11；同步身份/模块/配置/AGENTS §12；更新 core allow-list | 无 | 实现 Agent（coder） | 非文档作者（RV1） | `feat/admin-state-persistence-v2` / 主 worktree | `docs/CORE_PORTS_AND_STORAGE.md`、`docs/IDENTITY_AND_AUTH_CONTRACT.md`、`docs/MODULE_ARCHITECTURE.md`、`docs/CONFIG_REFERENCE.md`、`AGENTS.md`、`scripts/check-crate-boundaries.mjs` | 输入：§11 目标形状、§9 判据 13；输出：可被漂移门禁读取的 §5/§7 正文与 allow-list | 3.7（PV2/PV3）；3.8 review |
| WP1 | `core::model` 与依赖：`PeerPublicKey`/`PairingPeer.public_key`、错误枚举与 `AuditAction` 新取值、本地配置值对象、`CreateSessionRequest` 改造、core 依赖 p256/sha2 | WP5（allow-list 结论） | 实现 Agent（coder） | 非实现者（RV1） | 同上 | `crates/core/Cargo.toml`、`crates/core/src/model/*.rs` | 输入：§11.5/§11.6/§11.9 形状；输出：可编译的值对象与其不变式测试 | 3.1（PV4）；3.2 review |
| WP2 | `core::ports` 目标签名：写集 DTO、`WriteContext`/`PendingAudit`、`TrustStore`/`ExportStore` 替换、新增 `LocalConfigStore`/`CredentialResolver`、测试替身跟进 | WP1 | 实现 Agent（coder） | 非实现者（RV1） | 同上 | `crates/core/src/ports.rs`、`crates/core/src/broker.rs`（仅测试替身） | 输入：§11.6 签名；输出：与 §5.3 逐条一致的 `ports.rs` | 3.3（PV4）；3.4 review |
| WP3 | `core::use_cases`：管理入口改走写集、`remove_import` 撤回为一次 `ImportRemoval`、`port_error_public` 显式分支、`create_session` 的 workspace 解析 | WP2 | 实现 Agent（coder） | 非实现者（RV1） | 同上 | `crates/core/src/use_cases.rs`、`crates/core/src/broker.rs`（`port_error_public`） | 输入：§11.2 第 3/4/5 条、§11.9；输出：原子管理路径与解析结果 | 3.3（PV4）；3.4 review |
| WP4 | `storage-sqlite` v2：版本常量、管理表 DDL、审计表 12-step 重建、`imported_import` 拆分迁移、幂等升级、v2 夹具 | WP5（DDL 定稿） | 实现 Agent（coder） | 非实现者（RV1） | 同上 | `crates/storage-sqlite/src/migrate.rs`、`fixtures/storage/v2/` | 输入：§11.7/§11.8；输出：v2 DDL 常量、可重放迁移、三件夹具 | 3.5（PV4）；3.6 review |
| WP6 | `storage-sqlite` 管理 store：实现 `TrustStore`/`ExportStore`/`LocalConfigStore` 与失败关闭映射 | WP2、WP4 | 实现 Agent（coder） | 非实现者（RV1） | 同上 | `crates/storage-sqlite/src/{lib.rs,error.rs,session_store.rs}` 与新增 store 模块 | 输入：§11.2 写集语义、§11.7 DDL；输出：一个事务一个写集的实现 | 3.9（PV4）；3.10 review |

说明：Main E2E mode 为 `not-applicable`，因此不生成独立的测试工作包（TP）与 Test Design 分组；各工作包自带其行为测试（局部自检），最终替代验证由主 Agent 任务（7.1）统一执行并留证。

## Execution Waves

| Wave | Work Packages | 依赖满足条件 | 并发上限 | 说明 |
| --- | --- | --- | --- | --- |
| W0 契约与 DDL 冻结（串行，单执行者） | 2.14（`migrate.rs` 的 v2 DDL 常量与 12-step 重建语句起草）→ 2.1 → 2.2 | 已完成的 2.3–2.13（本仓库工作区） | 1 | 漂移门禁把 §5/§7 与 `ports.rs`/`migrate.rs` 逐条绑定，因此 **DDL 文本必须先冻结**，合同才能并入；本波完成条件 = PV2/PV3 绿 + 在该状态上落一个基线提交 |
| W1 并行实现 | WP4 剩余（2.15–2.18）∥ WP6（2.19–2.22）∥ 已完成 WP1–WP3 的独立 review（3.2、3.4） | W0 的基线提交 | 3 | 三条轨道文件不重叠：`migrate.rs`+`fixtures/storage/v2/` ／ 新增 store 模块+`lib.rs`+`error.rs` ／ `reports/rv1-*.md`（只读） |
| W2 交付前验证 | 3.5、3.6（WP4）∥ 3.9、3.10（WP6） | W1 对应轨道完成 | 2 | PV 与独立 review 并行；review 必须由不继承实现对话的 reviewer 完成 |
| W3 集成与合入 | 3.7、3.8 → 5.1–5.2 → 6.1–6.8 | 全部 3.x 完成 | 1 | `integrated` 单一交付单元；每个目标分支只允许一个集成执行者串行更新 |
| W4 最终验证 | 7.1–7.3 → 8.1 | 6.7、6.8 | 1 | not-applicable 的替代验证 + `[e2e-owned]` 门禁 + 最终验收 |

**多 Agent 分派规则**（宿主支持子 Agent / 可开独立会话时）：

- 每个轨道一个独立 coder 执行者与其自己的 worktree（`git worktree add`）；构建目录用 `CARGO_TARGET_DIR` 隔离，**不允许两个执行者共用同一个 `target/`**。
- 每个 WP 的交付前独立 review（RV1）必须由**不继承实现对话**的执行者完成；缺少隔离上下文时相关任务记 BLOCKED，不得以自审替代。
- 只有主 Agent 可写 `plan.md`/`tasks.md`/`verification.md`；子 Agent 返回结构化 handoff（固定提交、命令、日志路径、差异范围）。
- 文件单一写入者（跨轨道也不得并发写）：`crates/core/src/ports.rs`/`broker.rs`（WP2/WP3，已冻结）、`crates/storage-sqlite/src/migrate.rs` 与 `fixtures/storage/v2/*`（WP4）、`crates/storage-sqlite/src/{lib.rs,error.rs}` 与新增 store 模块（WP6）、`docs/*` 与 `scripts/check-crate-boundaries.mjs`（W0 合同轨道）、`reports/*` 按检查 ID 一文件一写者。
- `Cargo.lock` 在 W0 已因 `p256`/`sha2` 变更（本轮工作区）；并行轨道不得再次解析依赖（禁止 `cargo update`/升级版本）。
- 无并行能力时按 roles 串行执行并如实记录；串行不改变上面每条轨道的完成条件与证据要求。

## Dependency Handoffs

| Downstream | Upstream | Accepted Revision / Evidence | Start Revision | Transfer / Inclusion Check | Invalidation |
| --- | --- | --- | --- | --- | --- |
| WP1 | WP5（2.2） | 2.2 的 allow-list 草案与 `cargo tree -p core --edges normal` 输出 | 变更分支起点 | 逐项核对闭包成员与 §9 判据 13 的名字集合 | allow-list 或依赖版本变化 → WP1/WP2/WP4 复验 |
| WP4 | WP5（2.1） | §7.3/§7.4 定稿的 DDL 文本 | 变更分支起点 | `node scripts/check-contract-drift.mjs` 的语句条数与逐条文本 | §7 DDL 变化 → WP4/WP6 与夹具复验 |
| WP6 | WP2、WP4 | `ports.rs` 签名定稿 + `migrate.rs` v2 常量 | W2 完成点 | `cargo build -p storage-sqlite` 与 `PortError` 映射清单核对 | 端口签名或 DDL 变化 → WP6 复验 |
| DU1 | 全部 WP | 各 WP 的 PV4 证据与 RV1 review 报告 | W4 完成点 | `npm run verify` 全绿 + 变更 diff 无未登记文件 | 任一上游变化 → 候选重建、PV1 重跑 |

## Runtime Resources

| Checks / Work Packages | Resource | Isolation / Configuration | Exclusive Scheduling | Owner / Cleanup |
| --- | --- | --- | --- | --- |
| PV1–PV4、WP1–WP6 | Rust 构建目录 `target/` | 每个并行执行者设自己的 `CARGO_TARGET_DIR`；同一 `target/` 不得并发写 | 按 WP 分 worktree，同 crate 并发时独占构建目录 | 各执行者，只清理自身产生的临时数据库与 worktree |
| WP4、WP6 的迁移与失败关闭用例 | 临时 SQLite 文件与 `fixtures/storage/v2/` 夹具 | 每个用例使用独立临时目录；夹具为只读输入，生成脚本不覆盖既有 v1 夹具 | 无（文件级隔离） | 实现 Agent |
| PV1（`npm run check`） | Node ≥ 22.12 与仓库内 `node_modules` | 只读脚本，不写仓库以外的状态 | 无 | 主 Agent |

说明：本变更不涉及数据库服务、容器、端口或外部账号等共享运行资源；`target/` 与 SQLite 临时目录是唯一共享设施，已按上表隔离。

## Target Repository and Main Branch

- Code Repository: `D:\Project\acp-remote`
- Target Kind / Ref: local；`refs/heads/main`
- Version Confirmation Owner: 主 Agent（机械核实可派发 environment/recon）
- Confirmation Method / Evidence: `git rev-parse refs/heads/main`、`git status --porcelain` 与 `git worktree list`；实际提交与核实结果记录在 `verification.md`

## Merge Strategy

| Delivery Unit / Mode | WP / TP | Owners: Implementation / Test / Review / Merge | Start / Readiness | Candidate Checks / Critical E2E | Order |
| --- | --- | --- | --- | --- | --- |
| DU1 / integrated | WP1–WP6 | 实现 Agent / 实现 Agent（各 WP 自带测试）/ 独立 reviewer / 主 Agent（按当前授权） | 契约定稿 + 依赖边界确认 + 夹具生成方法就绪 | PV1、PV2、PV3、PV4；无关键 E2E（mode = not-applicable） | 唯一单元，先候选后合入 |

- Integration Branch / Worktree: `feat/admin-state-persistence-v2`（主 worktree；如需并行分片则用 `CARGO_TARGET_DIR` 隔离构建目录）
- Source Revision / Handoff: 变更分支起点 = 计划确认时的 `refs/heads/main` 提交（在 6.1 记录固定值）
- Authorization / Report Path: 合并与推送授权仍受当前会话限制；集成报告写入 `reports/du1-integration.md`

采用 integrated 的理由：合同（§5/§7）、端口签名、实现与夹具由漂移门禁逐条绑定，任何单独一方先合入都会让 `main` 上的 `npm run check` 变红，因此必须作为一个交付单元一次性合入。

## Verification Strategy

### Local Checks

- 实现期：`cargo fmt --all -- --check`、`cargo clippy --locked -p core -p storage-sqlite --all-targets --all-features -- -D warnings`、`cargo test --locked -p core -p storage-sqlite --all-features`。
- 合同侧：`node scripts/check-contract-drift.mjs`、`node scripts/check-crate-boundaries.mjs`（改动中期允许漂移门禁报差异，最终必须为 0）。

### Project Verify

| Check ID | Stages / Work Packages | Command / Working Directory | Configuration / Environment | Scope / Pass Criteria | Evidence |
| --- | --- | --- | --- | --- | --- |
| PV1 | 候选、主分支、最终（WP1–WP6） | `npm run verify`（= `npm run check` + `cargo fmt --all -- --check` + `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` + `cargo test --locked --workspace --all-features`）@ `D:\Project\acp-remote` | Node ≥ 22.12、`rust-toolchain.toml` 固定的 1.98.1；不联网 | 退出码 0 且逐子项无失败、无零测试、无全跳过 | `reports/du1-pv1.log`、`reports/du1-main-verify.log` |
| PV2 | 候选、主分支（WP5、WP4） | `node scripts/check-contract-drift.mjs` @ 仓库根 | 只读脚本 | §7 的 2 个 sql 块与 `migrate.rs` 逐条一致；§5 的 trait/struct 声明与 `ports.rs` 集合相等；退出码 0 | `reports/wp5-contract-drift-before-after.md` |
| PV3 | 候选、主分支（WP1、WP5） | `node scripts/check-crate-boundaries.mjs` @ 仓库根 | 需要本地 `cargo`（`cargo metadata`/`cargo tree`），不联网 | 依赖方向矩阵无违规；`core` 的普通依赖闭包与 allow-list 逐项相等 | `reports/wp5-boundaries.log` |
| PV4 | 各 WP 交付前、最终（WP1–WP4、WP6） | `cargo test --locked -p core -p storage-sqlite --all-features` @ 仓库根 | 本地 Rust 工具链；SQLite 临时目录 | 变更相关 crate 全部测试通过，新增用例确实被执行（无 0 用例 / 无全跳过） | `reports/wp1-core-tests.log`、`reports/wp3-core-tests.log`、`reports/wp4-migration-tests.log`、`reports/wp6-admin-store-tests.log` |
| RV1 | 各 WP 交付前、主分支复核 | 独立 reviewer 按 `roles/reviewer.md` 在隔离上下文检视固定版本 diff 与契约 | 只读；不修改代码与证据 | 报告完整、无未解决阻断项；覆盖 specs 场景与 design 决策 | `reports/rv1-wp1.md`、`reports/rv1-wp23.md`、`reports/rv1-wp4.md`、`reports/rv1-wp5.md`、`reports/rv1-wp6.md`、`reports/rv1-du1.md` |

### Code Review

- 范围：WP5（合同/门禁文档改动）、WP1（值对象不变量与 core 依赖）、WP2/WP3（端口纯度与事务边界）、WP4（DDL 与迁移安全）、WP6（写集原子性、错误映射、失败关闭）。
- 关注点：① §5/§7 与实现逐条一致；② 新 `ConflictKind`/`UnavailableKind` 取值没有被通配臂静默吞掉；③ 审计是否真的与状态同事务、有无「先提交再补审计」；④ imported 家族无正文（列与行为双检查）；⑤ 凭据/路径/secret 不落库、不进日志；⑥ 迁移是否保留既有序号、`audit_id` 与审计行。
- 阻断标准：任一规格场景缺覆盖、任一漂移门禁差异、任何秘密材料落库或进日志、任何「可能返回成功但没有持久信任」的路径。
- 复核安排：修复后由同一 reviewer 对新固定版本复核，并保留原报告与复核结论。

### Main E2E

```yaml
mode: not-applicable
reason: "本变更只落地 core 与 storage-sqlite 两个库 crate；server、app、daemon 与 CLI 尚未实现，仓库中不存在可端到端运行的产品入口（无监听器、无 CLI 子命令、无前端），因此无法在真实入口执行端到端场景。"
basis: "openspec/config.yaml 的 context 已记录「当前阶段没有可端到端运行的产品路径，Main E2E 按变更记 not-applicable，需用户逐变更批准」，且 x-agentic.e2e.command 为空；本变更范围是库级持久化合同，不改 wire 协议。"
alternative_checks:
  - "cargo test --locked -p core -p storage-sqlite --all-features：覆盖迁移保留/幂等/过新拒绝、事务原子性与注入失败、重启恢复与撤销、失败关闭、公钥与配对绑定、workspace 解析、黄金列清单与无正文行为。"
  - "npm run verify：统一合同门禁 + fmt/clippy/test，确认 §5/§7 与实现逐条一致、依赖方向与 allow-list 成立。"
  - "node scripts/check-contract-drift.mjs：对新增端口签名与 DDL 的逐条断言。"
  - "fixtures/storage/v2/from-v1.sqlite3 的升级夹具重放：在最终主分支版本上重跑 v1→v2 升级并用行数/序号/audit_id 断言保留性。"
downgrade_approval: "2026-09-23，本会话，用户原话：「本变更（admin-state-persistence-v2）同意 Main E2E 记 not-applicable，替代验证为 core/storage-sqlite 的 cargo 测试 + npm run check + 合同漂移门禁 + v1→v2 迁移夹具。」"
```

#### E2E Ownership and Cases

不适用（mode = not-applicable），本节按模板要求删除；实际替代验证已列为主 Agent 任务（7.1）并在 Coverage Index 中引用。

## Failure and Recovery

- 失败修复：按原 WP ID 重新派发（不新增 WP），修复后重跑该 WP 的 PV4 与 RV1；受影响的下游（含 DU1）按 Dependency Handoffs 的失效列重开。
- 传递下游失效：端口签名或 DDL 一旦变化，WP6 与 DU1 的既有证据失效，必须重建候选并重跑 PV1–PV4。
- 主分支失败：`main` 上的 `npm run verify` 失败时停止后续合入，按同一验证路径交付修复；必要时回滚该交付单元（`git revert` 或恢复候选前提交），不允许「红着继续合」。
- 共享资源异常：SQLite 临时目录与夹具异常（磁盘满、文件占用）时清理由测试创建的临时目录，夹具只读不改；`target/` 污染时用 `cargo clean -p core -p storage-sqlite` 后重跑受影响检查。
- 无 E2E 执行，故不涉及 E2E 失败上限；若后续有人把 mode 改为 required，必须先取得新的用户批准并补齐 E2E 任务。
- 未执行项：`cargo-deny` 与 `gitleaks` 本地没有等价物，只在 CI 运行；本变更在最终验收中明确记录为「未在本地执行」。

## Completion Criteria

- 全部任务勾选（`verification.md` 中逐 ID 关联证据；结果仅 PASS/FAIL/BLOCKED/NOT_APPLICABLE）。
- 最终主分支版本（`refs/heads/main` 的实际提交）上：PV1、PV2、PV3、PV4 全绿；RV1 无未解决阻断项；E2E 记 NOT_APPLICABLE 且替代验证（7.1）已完成并留证。
- 阻断问题清零：无未闭环的 FAIL/BLOCKED、无未登记漂移、无秘密材料落库或进日志。
- 最终验收由主 Agent 按 `.agents/skills/agentic-verify/SKILL.md` 执行，并在验收块中以唯一 `[final-verification]` 任务记录。
- 本计划不构成合并、推送、回滚或发布授权。
