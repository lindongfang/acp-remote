## RP-001 独立文档结论（2026-10-30）

来源：用户确认“不提供重放的，我们暂时也不支持吧，那同意采用重放兼容”。独立 tester `/root/spec_tester` 编写本轮用例，独立 reviewer `/root/spec_reviewer` 只读复核，主执行者在此转录结论：**本轮限定文档复核通过，无新增阻断**。本 change 增补用例为 HN-030–034。

已核对通用重放准入、全量流式处理与有界可丢弃分页视图、读取代际及完成事实、失败和子能力限制、能力变化后的旧事实与资源责任；不增加厂商读取器或无重放降级。25 条新增定义（19 L5、6 L6）及39行 Requirement/Scenario 关联经独立逐项核对，其余3份 tests 范围一致。OpenSpec 全量 strict 11/11、`git diff --check` exit 0。全部产品用例 not-run，未执行构建或真实 Harness；原生身份、Run/后台活动、交付账本、双向 I/O 及完整旧资料重基准仍未完成，不恢复实现。

本轮65文件报告追加前固定摘要与9份 tests 摘要见[共同受审查快照](../deliver-codex-session-loop/review.md)顶部；聚合为 `f9d01a364731b7495470132551fbd690c86245ee2bc9eb8174abc03cd742ad04`。本段与 plan 中的转录不改写此前快照、历史结论或当前规格/用例。

---

## RP-001 独立文档结论（2026-10-30）

来源：用户确认“不提供重放的，我们暂时也不支持吧，那同意采用重放兼容”。独立 tester `/root/spec_tester` 编写本轮用例，独立 reviewer `/root/spec_reviewer` 只读复核，主执行者在此转录结论：**本轮限定文档复核通过，无新增阻断**。本 change 增补用例为 HN-030–034。

已核对通用重放准入、全量流式处理与有界可丢弃分页视图、读取代际及完成事实、失败和子能力限制、能力变化后的旧事实与资源责任；不增加厂商读取器或无重放降级。25 条新增定义（19 L5、6 L6）及39行 Requirement/Scenario 关联经独立逐项核对，其余3份 tests 范围一致。OpenSpec 全量 strict 11/11、`git diff --check` exit 0。全部产品用例 not-run，未执行构建或真实 Harness；原生身份、Run/后台活动、交付账本、双向 I/O 及完整旧资料重基准仍未完成，不恢复实现。

本轮65文件报告追加前固定摘要与9份 tests 摘要见[共同受审查快照](../deliver-codex-session-loop/review.md)顶部；聚合为 `f9d01a364731b7495470132551fbd690c86245ee2bc9eb8174abc03cd742ad04`。本段与 plan 中的转录不改写此前快照、历史结论或当前规格/用例。

---

## RP-001 重放兼容当前范围（2026-10-30）

用户最新确认无标准历史重放的 Harness 暂不接入，统一使用 ACP 重放兼容，不增加厂商专用读取器。当前行为以本 change specs 及通用 harness-management/structured-history 为准，平台分页不要求原生按页读取。本文下方所有原快照及此前 SR 修订记录保留历史归属；其中当前连接降级、禁止全量重放再分页、原生分页硬前提和旧能力假设均不得作为本次执行依据。

本轮只修订文档；独立 tester 增补本轮用例，独立 reviewer 复核本次范围，产品用例仍未执行。详细合同、全套旧用例与工作包全面重基准及其他架构底座整改未完成，不恢复实现、不修改 checkbox，不删除原生数据或改写旧证据。平台身份、审计及离线平台备份不因 Harness 未准入而失效。

---

## 本轮文档修订独立复核（2026-10-30）

来源：用户在全量活动 spec 审查后回复“同意，修复吧”。本 change 涉及 SR-003/SR-004；当前规则、设计及增补用例 HN-026–029 的限定文档修订经独立 reviewer `/root/spec_reviewer` 复核通过。独立 tester `/root/spec_tester` 编写用例，不继承规格修订对话；reviewer 与 tester 为不同执行者。本记录由主执行者转录独立结论，不改写下方历史记录。

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
| openspec/changes/deliver-history-navigation/design.md | `18d100f8143fd62b9a0db9fb4ccb4ae996f4d0720e5bcc88ab19e85225a62422` |
| openspec/changes/deliver-history-navigation/plan.md | `d74bdbfc135cf67bd4ee3e90cf257dde613e048c5335684c02b475e146080523` |
| openspec/changes/deliver-history-navigation/proposal.md | `129c39941d8ce55fb35855cb7ccfaefd2b51651314b5926076b5a8ba84bf1502` |
| openspec/changes/deliver-history-navigation/specs/history-navigation/spec.md | `c958246326bb22a0eeead5f43446dad062f08d2abed74143e035c344bb535de9` |
| openspec/changes/deliver-history-navigation/tasks.md | `12990845ca6944175ed9b47a3c00fd436f606315ded68387827a15123530de63` |
| openspec/changes/deliver-history-navigation/tests.md | `1159b078405189529e1bef0539504f17d83ecf4e7a2480efd6441c6370e8d151` |

---

## 原有独立检视记录（仅原快照）

> 适用范围说明（2026-10-30）：下方为原快照的独立检视记录，原问题与结论未改写。本轮会话数据权威、激活和删除规格已修订，尚未独立复查；本文历史 PASS 不构成新方案开工或验收依据。

﻿# 独立方案与用例检视

状态：当前快照方案、用例、plan/tasks设计检视通过；不是本轮实施授权或产品验收。上游Codex真实交付及最终条目/版本合同重基准仍是开工门禁。未签任何查询性能、真实平台或产品结果。

审查者独立读取proposal、spec、design、tests、plan/tasks及上游约束，不继承coder对话。所有7个场景有精确引用与对应步骤；HN-003/004补边界、查询量和订阅交接栅栏，HN-012要求真实数据库/Web/订阅，禁止数组截页或历史浏览启动Harness。plan的WP1–6都有task完成判据，重叠路径串行、Web独立准备后集成、固定版本的tester/reviewer并行及实际入口待建立边界明确。

## 问题闭环

| ID | 初始问题 | 修复及独立复查 | 状态 |
| --- | --- | --- | --- |
| HN-DES-01 | 单轮全部元数据无上限，正文1MiB预算不能保证帧/内存有界。 | design §2改为同一10轮逻辑页内16KiB/64描述块、16KiB正文块、冻结页句柄与版本/代际；定义空闲60秒/总5分钟、断线/取消/队列满释放；HN-006已补10000条元数据、块间更新、完整性、到期及重同步断言，实际读回复查。 | 设计/用例关闭；实际资源测量未执行 |

10轮保持完整关联而非单响应无限大小；当前在途轮计入，稳定轮次边界与条目ID/内容版本阻止旧基线覆盖新实时值。缓存丢失/修订淘汰显式重同步，无重放伪造已提交尾部。所有产品检查与人工验收仍待实现后执行。

## 文件快照

| 文件 | SHA-256 |
| --- | --- |
| design.md | `4A99059D52E747D53D993BAC5128362AE65F9D22FF9B34D52E7B2D1D4F71DDB5` |
| plan.md | `4167F37109C5CEA1D621745E5E189E15714547152CC27BCC5CEF4ADD46040B42` |
| proposal.md | `26BF7992CFB4F24F6DCDE84EBB1EA8A06E05299AD1315532031DEC5D9B80749F` |
| tasks.md | `AABBDBF516E9F25DF9755BA548A191D4BD6CB88541B6C0189D9672B2DDA3E84D` |
| tests.md | `10442935730E3FFA03B54F3787D23BC106F030003BC7DA0BF68958E98BBA78C1` |
| specs/history-navigation/spec.md | `952AB101237CE046A96B50D185BAC0A095E02707AE655BBA15BF5A95E9B577FA` |
