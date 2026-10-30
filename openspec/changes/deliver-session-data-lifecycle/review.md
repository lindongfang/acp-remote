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

来源：用户在全量活动 spec 审查后回复“同意，修复吧”。本 change 涉及 SR-002；当前规则、设计及增补用例 DL-012–016 的限定文档修订经独立 reviewer `/root/spec_reviewer` 复核通过。独立 tester `/root/spec_tester` 编写用例，不继承规格修订对话；reviewer 与 tester 为不同执行者。本记录由主执行者转录独立结论，不改写下方历史记录。

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
| openspec/changes/deliver-session-data-lifecycle/design.md | `bb7ea9212282c2adcf7ff4ecfcff5cd79ef6be27d4edc7b881018005136ead65` |
| openspec/changes/deliver-session-data-lifecycle/plan.md | `f6a501725fc440b473fb877a5bc3f8eab812f49bab4ba6bd6322e3b58b7bbbbf` |
| openspec/changes/deliver-session-data-lifecycle/proposal.md | `0af5843a52eee5725d4c7cfddd3cf5728c189a92482513b3bde80dc7956f6371` |
| openspec/changes/deliver-session-data-lifecycle/specs/session-data-lifecycle/spec.md | `2c64e35600134313cba274e0e4dbeef2424ad361e007ab5261fd6ce8f7f6f11a` |
| openspec/changes/deliver-session-data-lifecycle/tasks.md | `8cbbf0475d8f3376ea62bd485f5ecb97a48c2687e7b42e346bc8fa5e4305b86b` |
| openspec/changes/deliver-session-data-lifecycle/tests.md | `8d995153802e42f883c1d9191a781695b170e0c92b5d2b495067559fa31d8730` |

---

## 原有独立检视记录（仅原快照）

> 适用范围说明（2026-10-30）：下方为原快照的独立检视记录，原问题与结论未改写。本轮会话数据权威、激活和删除规格已修订，尚未独立复查；本文历史 PASS 不构成新方案开工或验收依据。

﻿# 独立方案与用例检视

状态：已独立复查当前proposal/specs/design/tests/plan/tasks快照，方案、用例、计划与任务设计检视通过，未发现未闭环设计阻断。本结论不构成本轮实施授权。真实上游交付、版本能力探测与正式实施前重基准保持必需；本轮不实施，不签产品能力或验收。

默认不级联与审计最小保留属重设计§8/§10授权的详细选择，不虚称用户逐项确认。DL-DES-01已实际复查design与DL-006：根子孙/重复委派/重启保留parent/root无正文墓碑，Session不可浏览不改原生根。

已检查所有spec场景在tests中的精确引用与可观察断言；边界、并发、失败/取消由L3/L4覆盖，真实Windows/Web闭环由L6覆盖，人工安全/交互检查单列。辅助字段/引用检索仅检查结构，不替代人工语义检视。当前所有产品测试未执行不作为文档设计缺陷。独立reviewer只维护本报告，不改产品/测试。


## 最终计划与任务复查

已实际读取WP1–WP5及task完成判据；各包有真实checkbox引用，全部未勾选。节点/Web接口冻结后可隔离并行，共享README/锁文件/CI归WP1集中修订，注册/模式/网关及跨change共享文件串行集成并重跑受影响用例。Main E2E只有一个mode required配置，PV覆盖全部本change稳定用例ID，执行者/Windows真实环境/固定版本/证据/清理/独立人工LX/新版本重验均已安排；产品不存在的入口列为WP1待核定或建立，不记作现已执行。上游真实交付与具体Harness版本/能力证据继续是实施门禁。正式产品所有用例仍未执行。

## 文件快照

| 文件 | SHA-256 |
| --- | --- |
| design.md | `DCB8FC975A38CBEC8E1435E1EC483A66976678BDC6FE6CE10D97C71972AF9F95` |
| plan.md | `D72A0653DC1FB8D727313CC3E1F3F3E04ECE2D7567EFFA06BA30207899EE7F08` |
| proposal.md | `722EA4C913775AA0C6FD98EA696C615A1420538F8F0D7B4BE2E5911C682C6F2B` |
| tasks.md | `147CC2D975497AFC7C1C8F347E8AB58A2A2434844B609FB67E1D6F5A196A0763` |
| tests.md | `F3FA3A021434A0AF86F5CE8F1A651A70E1EF4809DA0A7AB346B406517F2E9046` |
| specs/session-data-lifecycle/spec.md | `E6464E24DEA15398942C93FA23E77A5D6C6896E08E426D5FE1763A11626D0471` |
