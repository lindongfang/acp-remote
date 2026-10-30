## RP-001 独立文档结论（2026-10-30）

来源：用户确认“不提供重放的，我们暂时也不支持吧，那同意采用重放兼容”。独立 tester `/root/spec_tester` 编写本轮用例，独立 reviewer `/root/spec_reviewer` 只读复核，主执行者在此转录结论：**本轮限定文档复核通过，无新增阻断**。本 change 仅补充影响范围，不机械新增重复用例。

已核对通用重放准入、全量流式处理与有界可丢弃分页视图、读取代际及完成事实、失败和子能力限制、能力变化后的旧事实与资源责任；不增加厂商读取器或无重放降级。25 条新增定义（19 L5、6 L6）及39行 Requirement/Scenario 关联经独立逐项核对，其余3份 tests 范围一致。OpenSpec 全量 strict 11/11、`git diff --check` exit 0。全部产品用例 not-run，未执行构建或真实 Harness；原生身份、Run/后台活动、交付账本、双向 I/O 及完整旧资料重基准仍未完成，不恢复实现。

本轮65文件报告追加前固定摘要与9份 tests 摘要见[共同受审查快照](../deliver-codex-session-loop/review.md)顶部；聚合为 `f9d01a364731b7495470132551fbd690c86245ee2bc9eb8174abc03cd742ad04`。本段与 plan 中的转录不改写此前快照、历史结论或当前规格/用例。

---

## RP-001 重放兼容当前范围（2026-10-30）

用户最新确认无标准历史重放的 Harness 暂不接入，统一使用 ACP 重放兼容，不增加厂商专用读取器。当前行为以本 change specs 及通用 harness-management/structured-history 为准，平台分页不要求原生按页读取。本文下方所有原快照及此前 SR 修订记录保留历史归属；其中当前连接降级、禁止全量重放再分页、原生分页硬前提和旧能力假设均不得作为本次执行依据。

本轮只修订文档；独立 tester 增补本轮用例，独立 reviewer 复核本次范围，产品用例仍未执行。详细合同、全套旧用例与工作包全面重基准及其他架构底座整改未完成，不恢复实现、不修改 checkbox，不删除原生数据或改写旧证据。平台身份、审计及离线平台备份不因 Harness 未准入而失效。

---

## 本轮文档修订独立复核（2026-10-30）

来源：用户在全量活动 spec 审查后回复“同意，修复吧”。本 change 涉及 SR-005；当前规则、设计及增补用例 BR-011–015 的限定文档修订经独立 reviewer `/root/spec_reviewer` 复核通过。独立 tester `/root/spec_tester` 编写用例，不继承规格修订对话；reviewer 与 tester 为不同执行者。本记录由主执行者转录独立结论，不改写下方历史记录。

- 本轮总体范围：SR-001–005，7 个 change 及 2 份总体文档；31 条新增定义（24 条 L5、7 条 L6），35 行组级来源表经逐项核对无引用错误。
- 已验证：OpenSpec 全量严格结构校验 11/11 通过；限定文档 `git diff --check` 通过；旧 tests 正文由 tester 对照写入前快照确认逐字节保留。
- 仅文档问题闭环：R-SR-DOC-001（P3，本轮说明重复）及 R-SR-TEST-001（P2，AT 关联超出实际断言）均经原独立 reviewer 修后复核关闭。前者去重；后者移除无覆盖映射并将自定义分页限定为 AT-017、AT-019。
- 未执行：全部新增产品用例、真实 Harness 及原生范围/资源/备份能力验收。旧合同、旧用例及旧工作包全面重基准尚未完成；本结论不授权恢复实现，也不签署产品、安全实测或整个计划通过。

### 受审查文件快照

固定的是本轮结论追加前的 50 文件快照；计划文件的下列摘要同样为本记录追加前值。汇总算法为按相对路径排序，逐行 `relativePath|lowercaseSHA256\n`，UTF-8 无 BOM、LF 且含末 LF，聚合 SHA-256：`7e77ec113fd48b6796139cfb4465d2949dc3a788c7ee033e23ddc84136d77d2d`。各 change 本地表及共同 docs 行合起来构成完整清单；review.md 不在这 50 文件内。报告追加后仅核对记录差异，不把旧快照摘要冒充最终文件摘要。

| 相对路径 | SHA-256 |
| --- | --- |
| docs/HARNESS_CONTROL_PLANE_REDESIGN.md | `e863c2d36951d4bcf62d4adcb40a7bf09aaf39565b5ec922770eba015a3966d1` |
| docs/REBUILD_CAPABILITY_MAP.md | `58a13c8b6851a8cba0b90e89d6c527c361e4855478397b027fce82387daa6fa1` |
| openspec/changes/deliver-backup-recovery/design.md | `a9f91899cae0ab643f1e3199e45581f47fde22e20f225b04384401e04eccac4e` |
| openspec/changes/deliver-backup-recovery/plan.md | `129a0994637d1cc8878b134d61fc00983e405ab83a1668ac4b020bd138a57fe3` |
| openspec/changes/deliver-backup-recovery/proposal.md | `5f5b91a07606ef88b85ece35c6581fab55d565906effac8b0e07dc5d9bd204f2` |
| openspec/changes/deliver-backup-recovery/specs/backup-recovery/spec.md | `d4092a08ca2089d6e9c17516d7fc3d19851feacb1c2992239de6a8e61185678e` |
| openspec/changes/deliver-backup-recovery/tasks.md | `15c650fc6dfe669b7a5df2f7d7b181daa6c1e3e949c08df23bf5ff1f6c6bcb84` |
| openspec/changes/deliver-backup-recovery/tests.md | `c616e9b2aba54014fcc4a1af3200ae9124361382af063cdb314583d7bfc1bfc5` |

---

## 原有独立检视记录（仅原快照）

> 适用范围说明（2026-10-30）：下方为原快照的独立检视记录，原问题与结论未改写。本轮会话数据权威、激活和删除规格已修订，尚未独立复查；本文历史 PASS 不构成新方案开工或验收依据。

﻿# 独立方案与用例检视

状态：当前proposal/spec/design/tests/plan/tasks快照的方案、用例、计划与任务设计检视通过，无未闭环设计阻断；不是本轮实施授权。当前无产品运行证据，不把未运行本身当文档缺陷，也不签产品验收或自动上线授权。

独立reviewer实际读取四个spec场景与BR-001–010步骤/预期，覆盖包校验、完成标志、路径/重解析点、附件租约与回收竞态、失败取消、范围分项、旧审批/身份不可执行及真实隔离验证。每条有角色、环境、清理与入口；真实平台与原生组件能力未验证不得被fixture替代。

| ID | 问题与修复复核 | 状态 |
| --- | --- | --- |
| BR-DES-01 | 最初不备份密钥却恢复node_meta/clients，未说明如何避免缺key启动失败或静默替换身份。design §5现明确离线只读检验资产不进daemon，meta无key仍拒绝，旧nodeId/信任/幂等仅来源历史；新节点启用另行新空根初始化/重新配对。BR-008/009明确无自动启动、换钥、继承旧认证或旧审批，已实际读回。 | 设计/用例关闭；产品未运行 |

平台库与附件同一存储协调边界取快照及租约，原生组件分别证明一致性，不能将复制活跃DB/WAL当备份。恢复不覆盖实际目录，不自动启用生产；可选原生组件不支持要显式列出。依赖真实交付、版本与上游模式重基准、产品/安全/人工验收保留为实施门禁。


## 最终计划与任务复查

已实际读取WP1–WP5及task完成判据；各包有真实checkbox引用，全部未勾选。节点/Web接口冻结后可隔离并行，共享README/锁文件/CI归WP1集中修订，注册/模式/网关及跨change共享文件串行集成并重跑受影响用例。Main E2E只有一个mode required配置，PV覆盖全部本change稳定用例ID，执行者/Windows真实环境/固定版本/证据/清理/独立人工LX/新版本重验均已安排；产品不存在的入口列为WP1待核定或建立，不记作现已执行。上游真实交付与具体Harness版本/能力证据继续是实施门禁。正式产品所有用例仍未执行。

## 文件快照

| 文件 | SHA-256 |
| --- | --- |
| design.md | `BDBBF54CA0019BBE65164D6F13079E063BF4ABCDF121973DBD2837CFFA6AB09F` |
| plan.md | `E792EB1F38BB519B9CF00A8CF12CE8C3A38AFAC6309121079A2A0E1BDF21AC62` |
| proposal.md | `320CAA60562C24A2FE2B1F5FF9C6FD305C4578E44A889C3856E78EB612C2176A` |
| tasks.md | `3BBF7D43EDC105432E3CD46858C6FC3EB3698D160422F5E2808194D5D972D99B` |
| tests.md | `9EF550EF0C5FDC339E0B24AD859FF1E558047D7D0EE7EBE719373C0228D21AB5` |
| specs/backup-recovery/spec.md | `6948C31CD140BC12004FBA1035A450B44B625B9573F9C1AB44D9C6928BEAB05C` |
