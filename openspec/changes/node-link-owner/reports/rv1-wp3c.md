# RV1-WP3C Review 报告（变更 `node-link-owner` / WP3 合同扩展阶段，任务 2.25+2.26）

> 说明：本轮 reviewer 子 Agent 无写文件工具（只读检视），报告由主 Agent 按其返回正文原样持久化到本路径。

## 结论

- **判定：PASS**（Target Revision `13f0a16b55203a8224771a23fc432fac8b5184bb`），无 CRITICAL/MAJOR；5 项非阻断发现（F1 需主 Agent 在归档前处理、F2 需在 WP4 接线前钉死、F4 归 WP4）。

## Shared Report

- **task_id**: 2.25 / 2.26（WP3 合同扩展，DU1）
- **role**: reviewer（独立只读检视；未参与实现）；**phase**: review；**stage**: work-package
- **agent_context**: 新建任务级 reviewer 子 Agent（3a61aa1a-0e86-46e6-95fd-a62987180600；隔离上下文启动）；工作目录 `D:\Project\acp-remote-wt\node-link-owner`（工作树干净，HEAD == target）；未修改任何文件、未执行命令。
- **base / target**: `041aeb043d7b585b43faaaf1f40a330854b80784` / `13f0a16b55203a8224771a23fc432fac8b5184bb`
- **scope**: `core/src/{model/identity.rs,model/tests.rs,use_cases.rs,ports.rs,broker.rs}`、`storage-sqlite/src/{migrate.rs,session_store.rs,admin/{trust,audit,mod}.rs}`、`storage-sqlite/tests/{migration,admin_store,enum_coverage,commit}.rs`、`server/src/local_admin/test_support.rs`、三份权威文档 diff、`openspec/specs/storage-schema-v2-migration/spec.md`、`fixtures/storage/**`
- **result**: PASS；**evidence_paths**: 本报告 + `reports/wp3-contract-handoff.md`、`reports/wp3-contract.log`
- **resource_cleanup**: 无资源创建、无写入。

## handoff_index

```yaml
handoff_index:
  - task_id: "2.25"
    role: reviewer
    phase: review
    stage: work-package
    target_revision: "13f0a16b55203a8224771a23fc432fac8b5184bb"
    evidence_type: REVIEW
    evidence_id: RV1-WP3C
    report_path: "reports/rv1-wp3c.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "在 target 13f0a16 上对 core use_cases/model/ports 做静态核对：绑定校验、consume_pairing 状态机、幂等不重复写审计、LocalCli 原行为留存；断言改动清单逐条与目标版本一致；另记 5 项非阻断发现"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.26"
    role: reviewer
    phase: review
    stage: work-package
    target_revision: "13f0a16b55203a8224771a23fc432fac8b5184bb"
    evidence_type: REVIEW
    evidence_id: RV1-WP3C
    report_path: "reports/rv1-wp3c.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "在 target 13f0a16 上独立抽查 §5↔ports.rs、§7↔migrate.rs 的关键行（PairingConsumption/consume_pairing、两张审计表 CHECK、owned_command 三值）、v2→v3 重建与序列回填、夹具策略；测试证据取自 reports/wp3-contract.log（本轮未复跑）"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.25"
    role: reviewer
    phase: review
    stage: work-package
    target_revision: "13f0a16b55203a8224771a23fc432fac8b5184bb"
    evidence_type: CHECK
    evidence_id: PV3
    report_path: NOT_AVAILABLE
    result: BLOCKED
    evidence_status: PENDING
    applicability_basis: "正式 [PV3] 轮次归 WP3 交付任务 2.9/3.10；本轮 reviewer 只读、未执行测试。门禁：WP3 交付前 2.9 与候选合入前的 3.10/6.x"
    source_evidence: NOT_APPLICABLE
```

## Findings

| ID | 级别 | 位置 | 问题 / 证据 | 主 Agent 裁决 / 处理 |
|---|---|---|---|---|
| RV1-WP3C-F1 | P2（归档前必须处理） | `openspec/specs/storage-schema-v2-migration/spec.md`「版本常量与 migration 幂等」仍断言版本为 2 | 本 diff 把三个版本常量推进到 3，但没有该能力的 MODIFIED 增量；plan.md 还写「无 MODIFIED」 | **已处理**：主 Agent 补写 MODIFIED 增量（`specs/storage-schema-v2-migration/spec.md`，版本→3 + v2→v3 重建语义），plan.md 与 Coverage Index 同步 |
| RV1-WP3C-F2 | P2（WP4 消费前置） | `use_cases.rs:1122-1129`、`admin/trust.rs:1064-1070`（对端只比 `actor.node`）vs `identity.rs:120-122` 的字段语义未钉死 | 契约未钉死节点配对对端 id 对应 `Actor::Node` 哪个字段；选错则 Owner 侧配对消费全失败 | **裁决**：钉死 `node` = 对端（claimant/连接对端）节点 id（与 `via_node`、`broker.rs:572-577` 的既有口径一致）；在 `CORE_PORTS_AND_STORAGE.md` §3.5 写明；交 WP4 接线并按此核对 |
| RV1-WP3C-F3 | P2 | `CORE_PORTS_AND_STORAGE.md:798` v1→v2 步骤 4 描述了一次已不存在的中途落盘 | 仅文档精度 | 修复（改写步骤 4 为「两段同事务、最终由 v2→v3 段统一写 3」；wp3-fix1） |
| RV1-WP3C-F4 | P2 | §9 判据 14「每个取值都要有写入用例」vs `node.auth_failed` 暂无写入用例 | 该动作的写入归 WP4（2.11 审计接线） | 列入 WP4 派发输入（R83 对应用例必须含 node.auth_failed 写入） |
| RV1-WP3C-F5 | P2（建议级） | `migration.rs:3-7` 模块注释指代歧义（「它」读作 too-new，实为 empty 副本） | 仅注释歧义 | 修复（wp3-fix1） |

## Correct（已核实无误，含证据）

- ActorKind/Actor/AuditAction 新变体落地正确；文档注释明确「审计表四值 / owned_command 三值」。
- 文档 ↔ 代码归一化：§5.3 DTO/签名与 ports.rs 逐字相同；§7.3/§7.4 两表 CHECK 与 migrate.rs 一致；owned_command 仍三值；§7.2 升级判据与实现一致。
- `owned_command` 未放宽；enum_coverage 期望拆分有具名理由。
- LocalCli 既有路径不变（require_pairing_access 对 LocalCli 返回 Ok，其余与 require_local 逐字相同）。
- claimant 绑定校验（不符即拒，同一拒绝形状，不构成配对 id 预言机）。
- consume_pairing 状态机自洽；存储面在 BEGIN IMMEDIATE 内重做对端比对与状态守卫；幂等不重复写审计在三处语义一致并有双面固定测试。
- v2→v3 重建正确（列集合/顺序不变、全列拷贝、索引重建、序列一次取一次回填、v1 连续两段同事务）；测试有红向判别力。
- 夹具策略合规（无新增二进制；生成器只重写 from-v1；过新用例改临时副本）。
- 连带改动均为编译必然（broker.rs 穷尽匹配、两个替身、两处 actor_from_columns）。
- 无 unsafe；审计行不含秘密正文；pairing secret 不进 DTO/审计/日志。
- 断言改动清单与目标版本逐条核对相符。

## Assessment 摘要与限制

- coder 日志（wp3-contract.log）内部自洽：core lib 100、storage 各套件计数与 handoff 一一对应；check:drift 报「§7 的 36 条 DDL 与 §5 的 15 trait/88 签名逐条一致」。
- 限制：无 shell/Git，base→target diff 未独立重算；未执行任何命令（PV3 正式轮次归 2.9/3.10）；E2E 不适用（已批准 not-applicable）。
- 代码检视 PASS 不等于 Project Verify PASS；本结论只对应 target revision。
