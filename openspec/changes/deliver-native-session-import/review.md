## DC-001 独立文档结论（2026-10-30）

用户确认三项取舍后，独立 tester `/root/spec_tester` 编写用例，独立 reviewer `/root/spec_reviewer` 按固定快照只读复核：**本轮限定文档通过，无未关闭阻断**。本 change 新例为 NI-018；本段由主执行者转录，不改写下方历史结论。

本轮只覆盖实际重放刷新计时且不形成任务保护、默认环境继承及显式覆盖、显式接入来源+原生会话ID三项决定。新增7例（5 L5、2 L6）及9行精确 Requirement/Scenario 关联经独立核对，三份旧 tests 正文后缀逐字节保留。DC-R001（P2）原“有新活动则撤销候选”可能使持续重放反复阻止满额回收，已修为区分活动排序与真实保护变化，并由 CP-025 覆盖唯一安全候选选中后持续重放；原 reviewer 已复查关闭。

已验证 OpenSpec strict 11/11、`git diff --check` exit 0。全部产品用例 not-run，未运行构建、真实 Harness 或数据迁移；不签整个架构、完整旧材料重基准或实施准备通过，现有产品实现继续暂停。

共同29文件结论追加前快照及3份 tests 摘要见[共同受审查快照](../deliver-codex-session-loop/review.md)顶部，聚合为 `749c7707fd1c8bf312fd7d9502186ec6694c173a5e34772a95d3312ed1678082`。追加本报告不把快照摘要冒充报告追加后的文件摘要。

---

## DC-001 三项取舍当前范围（2026-10-30）

用户确认实际重放可刷新30分钟无活动计时，但不形成执行、有效审批或后台任务保护，满额仍可安全淘汰；默认继承节点环境并允许显式覆盖，不新增白名单或环境指纹准入；原生身份以显式接入来源+原生会话ID协调，同来源多入口复用，不自动证明不同来源背后物理存储等价，也不以缺少此证明阻断已配置来源。

当前规则以更新后的规格及总体设计为准。下方 RP/SR 审查和摘要属于各自历史快照，冲突的物理存储证明、一概查看不保活断言不再适用。详细合同、旧用例及执行工作包全面重基准尚未完成，本轮只同步文档，独立复核结论另行记录；不恢复实现、不修改任务勾选、不删除实际数据，不把结构校验当产品验收或其他架构风险已解决。

---

## RP-001 独立文档结论（2026-10-30）

来源：用户确认“不提供重放的，我们暂时也不支持吧，那同意采用重放兼容”。独立 tester `/root/spec_tester` 编写本轮用例，独立 reviewer `/root/spec_reviewer` 只读复核，主执行者在此转录结论：**本轮限定文档复核通过，无新增阻断**。本 change 增补用例为 NI-015–017。

已核对通用重放准入、全量流式处理与有界可丢弃分页视图、读取代际及完成事实、失败和子能力限制、能力变化后的旧事实与资源责任；不增加厂商读取器或无重放降级。25 条新增定义（19 L5、6 L6）及39行 Requirement/Scenario 关联经独立逐项核对，其余3份 tests 范围一致。OpenSpec 全量 strict 11/11、`git diff --check` exit 0。全部产品用例 not-run，未执行构建或真实 Harness；原生身份、Run/后台活动、交付账本、双向 I/O 及完整旧资料重基准仍未完成，不恢复实现。

本轮65文件报告追加前固定摘要与9份 tests 摘要见[共同受审查快照](../deliver-codex-session-loop/review.md)顶部；聚合为 `f9d01a364731b7495470132551fbd690c86245ee2bc9eb8174abc03cd742ad04`。本段与 plan 中的转录不改写此前快照、历史结论或当前规格/用例。

---

## RP-001 独立文档结论（2026-10-30）

来源：用户确认“不提供重放的，我们暂时也不支持吧，那同意采用重放兼容”。独立 tester `/root/spec_tester` 编写本轮用例，独立 reviewer `/root/spec_reviewer` 只读复核，主执行者在此转录结论：**本轮限定文档复核通过，无新增阻断**。本 change 增补用例为 NI-015–017。

已核对通用重放准入、全量流式处理与有界可丢弃分页视图、读取代际及完成事实、失败和子能力限制、能力变化后的旧事实与资源责任；不增加厂商读取器或无重放降级。25 条新增定义（19 L5、6 L6）及39行 Requirement/Scenario 关联经独立逐项核对，其余3份 tests 范围一致。OpenSpec 全量 strict 11/11、`git diff --check` exit 0。全部产品用例 not-run，未执行构建或真实 Harness；原生身份、Run/后台活动、交付账本、双向 I/O 及完整旧资料重基准仍未完成，不恢复实现。

本轮65文件报告追加前固定摘要与9份 tests 摘要见[共同受审查快照](../deliver-codex-session-loop/review.md)顶部；聚合为 `f9d01a364731b7495470132551fbd690c86245ee2bc9eb8174abc03cd742ad04`。本段与 plan 中的转录不改写此前快照、历史结论或当前规格/用例。

---

## RP-001 重放兼容当前范围（2026-10-30）

用户最新确认无标准历史重放的 Harness 暂不接入，统一使用 ACP 重放兼容，不增加厂商专用读取器。当前行为以本 change specs 及通用 harness-management/structured-history 为准，平台分页不要求原生按页读取。本文下方所有原快照及此前 SR 修订记录保留历史归属；其中当前连接降级、禁止全量重放再分页、原生分页硬前提和旧能力假设均不得作为本次执行依据。

本轮只修订文档；独立 tester 增补本轮用例，独立 reviewer 复核本次范围，产品用例仍未执行。详细合同、全套旧用例与工作包全面重基准及其他架构底座整改未完成，不恢复实现、不修改 checkbox，不删除原生数据或改写旧证据。平台身份、审计及离线平台备份不因 Harness 未准入而失效。

---

## 本轮文档修订独立复核（2026-10-30）

来源：用户在全量活动 spec 审查后回复“同意，修复吧”。本 change 涉及 SR-003；当前规则、设计及增补用例 NI-011–014 的限定文档修订经独立 reviewer `/root/spec_reviewer` 复核通过。独立 tester `/root/spec_tester` 编写用例，不继承规格修订对话；reviewer 与 tester 为不同执行者。本记录由主执行者转录独立结论，不改写下方历史记录。

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
| openspec/changes/deliver-native-session-import/design.md | `005f782f25a6cdeee76c46ebd4942f34f53a7b1e3cc9450bf7f259e5dad28fa0` |
| openspec/changes/deliver-native-session-import/plan.md | `d4764a83d33da599c56971e3801bac15143587da7af9511a90ab7c51908bb289` |
| openspec/changes/deliver-native-session-import/proposal.md | `d045f23d5ff446fa9d698d7cc975f9901e437c9e278516c99c777f96db9e1e89` |
| openspec/changes/deliver-native-session-import/specs/native-session-import/spec.md | `8a0810cbba099c5acbeaed68b1f8ffae3d31fc5e36063f15826858671ac94fae` |
| openspec/changes/deliver-native-session-import/tasks.md | `0b69b6eed0b39e5b247a17d39b43fe500d3cbd9fcdac0c5cb739de2ed4169704` |
| openspec/changes/deliver-native-session-import/tests.md | `1315e6e90a5561aa693ca6b08cf49df360d3bf7b13b7a27e90dce9d2c460c30a` |

---

## 原有独立检视记录（仅原快照）

> 适用范围说明（2026-10-30）：下方为原快照的独立检视记录，原问题与结论未改写。本轮会话数据权威、激活和删除规格已修订，尚未独立复查；本文历史 PASS 不构成新方案开工或验收依据。

﻿# 独立方案与用例检视

状态：已独立复查当前proposal/specs/design/tests/plan/tasks快照，方案、用例、计划与任务设计检视通过，未发现未闭环设计阻断。本结论不构成本轮实施授权。真实上游交付、版本能力探测与正式实施前重基准保持必需；本轮不实施，不签产品能力或验收。

提案依赖agent tree与capacity，容量端口先申请；绑定预留/恢复/批次发布分别协调，失败所有权不丢失，不按文本去重。

已检查所有spec场景在tests中的精确引用与可观察断言；边界、并发、失败/取消由L3/L4覆盖，真实Windows/Web闭环由L6覆盖，人工安全/交互检查单列。辅助字段/引用检索仅检查结构，不替代人工语义检视。当前所有产品测试未执行不作为文档设计缺陷。独立reviewer只维护本报告，不改产品/测试。


## 最终计划与任务复查

已实际读取WP1–WP5及task完成判据；各包有真实checkbox引用，全部未勾选。节点/Web接口冻结后可隔离并行，共享README/锁文件/CI归WP1集中修订，注册/模式/网关及跨change共享文件串行集成并重跑受影响用例。Main E2E只有一个mode required配置，PV覆盖全部本change稳定用例ID，执行者/Windows真实环境/固定版本/证据/清理/独立人工LX/新版本重验均已安排；产品不存在的入口列为WP1待核定或建立，不记作现已执行。上游真实交付与具体Harness版本/能力证据继续是实施门禁。正式产品所有用例仍未执行。

## 文件快照

| 文件 | SHA-256 |
| --- | --- |
| design.md | `EFEA63896D32376E3769C045D3624E09F0870B14D6561BB5CE96199DA5A478EA` |
| plan.md | `A0675AE4299DBEEEA80BC6E3CE5667FF7FAAE60D26AD2B031069C5EA411C42C2` |
| proposal.md | `24F234843270E1FCAEFEDB2AC66181506F28BD1C71AEF8EDA657D495E0B5E3F5` |
| tasks.md | `82049BD807BE6A9340BD6C86EC8A6AFF75ACC3654CE5BD8B3A59B519A314BD76` |
| tests.md | `44D28B0E7EDF7112A515DE4F00D942EA335BDE23C203105F0499AB93DB2BF5F3` |
| specs/native-session-import/spec.md | `CBBB1C1417AC48B9FE5C85C4FEC2F6158A2DF501E5FE42B818336088B436EE8B` |
