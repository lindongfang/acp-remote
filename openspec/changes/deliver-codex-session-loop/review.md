## PV-001 独立文档结论（2026-10-30）

用户要求优先使用新协议后，独立 tester `/root/spec_tester` 编写通用用例，独立 reviewer `/root/spec_reviewer` 按固定快照只读复核：**本轮限定文档通过，未发现本轮阻断**。本段由主执行者转录，不改写下方历史结论。

已核对最高已实现验证版本优先发起、遵守对端合法选择及原因未提供、规范回退、重连重新优先、存量连接不热切、新版实际能力保真，以及业务失败不降版重发。DeepSeek/ZCode 引用与通用规则一致，程序实测报告未成为运行时版本白名单。CL-058–060共3例（2 L5、1 L6）、6行精确 Requirement/Scenario 关联经独立核对；旧 DC/RP/SR tests 后缀逐字节保留。专项不机械新增重复用例或代签其他真实程序。

OpenSpec strict 11/11、`git diff --check` exit 0；全部产品用例 not-run。未运行构建、产品或真实 Harness，不签全面架构、完整旧资料重基准或实施准备通过，实现继续暂停。

受审查范围为本 change、integrate-deepseek-harness、integrate-zcode-harness 的 proposal/design/specs/tests/plan/tasks（24文件），加总体设计、能力地图及 config（3文件），共27文件，排除3份 review。独立聚合 SHA-256 为 `de5bc83b206e7f059309becacf4c3b6866b377abec4b883dc283959f8cef1343`，与主执行者一致。算法：仓库相对路径正斜杠排序，每行 `path|小写SHA256\n`，UTF-8无BOM、LF且含末LF。此值属于本结论及3份 plan 报告追加前快照，后续仅追加报告，不能冒充追加后文件摘要。

通用 tests 冻结 SHA-256：`59d007f8888859c671efc0236d1fcf3dbd0ee52009b3edd32b39c27bb83825b8`。新增3例仅定义未来验收，未执行或升级协议/程序。

---

## PV-001 协议优先级当前范围（2026-10-30）

用户要求能用最新协议时优先新协议，避免多协议 Harness 长期使用低版。当前行为以 harness-management 的“优先最高共同可用协议”为准：每次新连接优先最高已实现验证版本的标准协商，遵守合法选择及能力，规范回退与业务失败分开；新连接不按旧报告固定低版，也不强制对端升版或编造选择原因。旧请求、审批及副作用不跨版本重发。

下方旧 RP/DC/SR 结论及摘要保留原快照，不覆盖本次新增优先级；不将既有“支持多版本”当作已验证优先高版。完整合同、旧用例及工作包重基准未完成，实现继续暂停，本轮不改代码、任务勾选或用户数据，不运行产品、构建或真实 Harness。独立文档结论在取得后另行记录，不把结构检查当产品或全面架构通过。

---

## DC-001 独立文档结论（2026-10-30）

用户确认三项取舍后，独立 tester `/root/spec_tester` 编写用例，独立 reviewer `/root/spec_reviewer` 按固定快照只读复核：**本轮限定文档通过，无未关闭阻断**。本 change 新例为 CL-055–057；本段由主执行者转录，不改写下方历史结论。

本轮只覆盖实际重放刷新计时且不形成任务保护、默认环境继承及显式覆盖、显式接入来源+原生会话ID三项决定。新增7例（5 L5、2 L6）及9行精确 Requirement/Scenario 关联经独立核对，三份旧 tests 正文后缀逐字节保留。DC-R001（P2）原“有新活动则撤销候选”可能使持续重放反复阻止满额回收，已修为区分活动排序与真实保护变化，并由 CP-025 覆盖唯一安全候选选中后持续重放；原 reviewer 已复查关闭。

已验证 OpenSpec strict 11/11、`git diff --check` exit 0。全部产品用例 not-run，未运行构建、真实 Harness 或数据迁移；不签整个架构、完整旧材料重基准或实施准备通过，现有产品实现继续暂停。

受审查范围为本 change、deliver-session-capacity、deliver-native-session-import 的 proposal/design/specs/tests/plan/tasks（24文件），加 README、总体设计、能力地图、GLOSSARY、config（5文件），共29文件，排除3份 review。独立聚合 SHA-256 为 `749c7707fd1c8bf312fd7d9502186ec6694c173a5e34772a95d3312ed1678082`，与主执行者一致。算法为仓库相对路径正斜杠排序，每行 `path|小写SHA256\n`，UTF-8无BOM、LF且含末LF。此值固定本结论及3份 plan 报告追加前快照；后续仅追加报告，原正文和其余文件摘要须保持。

| tests 所属 change | SHA-256 |
| --- | --- |
| deliver-codex-session-loop | `e2e8a4400fbb1e4928f5eb03f987b23efa4458ff3f2fcaea8100c989290549fc` |
| deliver-native-session-import | `eac0ff85f2fbf84cfd7d7da97228f95d98caa3304d21d147386c22cc1c9fd41d` |
| deliver-session-capacity | `d2bfe0055003aa1a899e243887a0ba8545941eb5be01df7d76b5cd7c5c7de8af` |

---

## DC-001 三项取舍当前范围（2026-10-30）

用户确认实际重放可刷新30分钟无活动计时，但不形成执行、有效审批或后台任务保护，满额仍可安全淘汰；默认继承节点环境并允许显式覆盖，不新增白名单或环境指纹准入；原生身份以显式接入来源+原生会话ID协调，同来源多入口复用，不自动证明不同来源背后物理存储等价，也不以缺少此证明阻断已配置来源。

当前规则以更新后的规格及总体设计为准。下方 RP/SR 审查和摘要属于各自历史快照，冲突的物理存储证明、一概查看不保活断言不再适用。详细合同、旧用例及执行工作包全面重基准尚未完成，本轮只同步文档，独立复核结论另行记录；不恢复实现、不修改任务勾选、不删除实际数据，不把结构校验当产品验收或其他架构风险已解决。

---

## RP-001 独立文档结论（2026-10-30）

来源：用户确认“不提供重放的，我们暂时也不支持吧，那同意采用重放兼容”。独立 tester `/root/spec_tester` 编写本轮用例，独立 reviewer `/root/spec_reviewer` 只读复核，主执行者在此转录结论：**本轮限定文档复核通过，无新增阻断**。本 change 增补用例为 CL-047–054。

已核对通用重放准入、全量流式处理与有界可丢弃分页视图、读取代际及完成事实、失败和子能力限制、能力变化后的旧事实与资源责任；不增加厂商读取器或无重放降级。25 条新增定义（19 L5、6 L6）及39行 Requirement/Scenario 关联经独立逐项核对，其余3份 tests 范围一致。OpenSpec 全量 strict 11/11、`git diff --check` exit 0。全部产品用例 not-run，未执行构建或真实 Harness；原生身份、Run/后台活动、交付账本、双向 I/O 及完整旧资料重基准仍未完成，不恢复实现。

受审查范围为9个活动 change 的 proposal/design/specs/tests/plan/tasks（60文件），加 README、2份总体 docs、GLOSSARY、config，共65文件；排除9份 review。聚合 SHA-256 为 `f9d01a364731b7495470132551fbd690c86245ee2bc9eb8174abc03cd742ad04`，独立计算与主执行者一致。算法：仓库相对路径正斜杠排序，每行 `path|小写SHA256\n`，UTF-8 无 BOM、LF 且含末 LF。此值固定的是本报告及9份 plan 结论追加前快照，报告追加后仅核对报告差异及其余文件摘要不变。

| tests 所属 change | SHA-256 |
| --- | --- |
| deliver-agent-session-tree | `23d719e1f69675d19305c062404b1f1c2fd3ccca9ea4ef5cc7348d81821d119d` |
| deliver-backup-recovery | `fd65d5871e5474eb7433af22fe2b246bba85a470ab54f48181b53c3f1e94222d` |
| deliver-codex-session-loop | `dc3732c676870304ae958118b4a830fff2083e9bcdbdac9cb76870f78f8c27cc` |
| deliver-history-navigation | `ae80d35a2d375d48a3b36cf5fcb46a11c74cf318d56744d7081c563f6f8f8c11` |
| deliver-native-session-import | `a438b202a43f11bec120a773022b36a2e44bb0257cded7cf65422cd147c23d02` |
| deliver-session-capacity | `ea0ca7e9f007cb79f84f7ab9e8ef033f5baf63a712c5e4537ff47331f51a4e3f` |
| deliver-session-data-lifecycle | `97a88c9ecee372f73c95fde60789c82adac45ce7191d15f4bce9589bb1360811` |
| integrate-deepseek-harness | `cc114d9537ec7b29251d952e4d3d7302df3b3aa403005cca153befe33a482ba2` |
| integrate-zcode-harness | `809dc4a016249e45782735a85898736dc8fd30aee22203e168947e0e86bc954e` |

独立 reviewer 另对7份含上轮 SR 用例的旧正文后缀核对与此前冻结摘要逐字节匹配；另2份旧正文的保留依据为独立 tester 记录。以上均不签署旧材料整体有效或产品验收通过。

---

## RP-001 独立文档结论（2026-10-30）

来源：用户确认“不提供重放的，我们暂时也不支持吧，那同意采用重放兼容”。独立 tester `/root/spec_tester` 编写本轮用例，独立 reviewer `/root/spec_reviewer` 只读复核，主执行者在此转录结论：**本轮限定文档复核通过，无新增阻断**。本 change 增补用例为 CL-047–054。

已核对通用重放准入、全量流式处理与有界可丢弃分页视图、读取代际及完成事实、失败和子能力限制、能力变化后的旧事实与资源责任；不增加厂商读取器或无重放降级。25 条新增定义（19 L5、6 L6）及39行 Requirement/Scenario 关联经独立逐项核对，其余3份 tests 范围一致。OpenSpec 全量 strict 11/11、`git diff --check` exit 0。全部产品用例 not-run，未执行构建或真实 Harness；原生身份、Run/后台活动、交付账本、双向 I/O 及完整旧资料重基准仍未完成，不恢复实现。

受审查范围为9个活动 change 的 proposal/design/specs/tests/plan/tasks（60文件），加 README、2份总体 docs、GLOSSARY、config，共65文件；排除9份 review。聚合 SHA-256 为 `f9d01a364731b7495470132551fbd690c86245ee2bc9eb8174abc03cd742ad04`，独立计算与主执行者一致。算法：仓库相对路径正斜杠排序，每行 `path|小写SHA256\n`，UTF-8 无 BOM、LF 且含末 LF。此值固定的是本报告及9份 plan 结论追加前快照，报告追加后仅核对报告差异及其余文件摘要不变。

| tests 所属 change | SHA-256 |
| --- | --- |
| deliver-agent-session-tree | `23d719e1f69675d19305c062404b1f1c2fd3ccca9ea4ef5cc7348d81821d119d` |
| deliver-backup-recovery | `fd65d5871e5474eb7433af22fe2b246bba85a470ab54f48181b53c3f1e94222d` |
| deliver-codex-session-loop | `dc3732c676870304ae958118b4a830fff2083e9bcdbdac9cb76870f78f8c27cc` |
| deliver-history-navigation | `ae80d35a2d375d48a3b36cf5fcb46a11c74cf318d56744d7081c563f6f8f8c11` |
| deliver-native-session-import | `a438b202a43f11bec120a773022b36a2e44bb0257cded7cf65422cd147c23d02` |
| deliver-session-capacity | `ea0ca7e9f007cb79f84f7ab9e8ef033f5baf63a712c5e4537ff47331f51a4e3f` |
| deliver-session-data-lifecycle | `97a88c9ecee372f73c95fde60789c82adac45ce7191d15f4bce9589bb1360811` |
| integrate-deepseek-harness | `cc114d9537ec7b29251d952e4d3d7302df3b3aa403005cca153befe33a482ba2` |
| integrate-zcode-harness | `809dc4a016249e45782735a85898736dc8fd30aee22203e168947e0e86bc954e` |

独立 reviewer 另对7份含上轮 SR 用例的旧正文后缀核对与此前冻结摘要逐字节匹配；另2份旧正文的保留依据为独立 tester 记录。以上均不签署旧材料整体有效或产品验收通过。

---

## RP-001 重放兼容当前范围（2026-10-30）

用户最新确认无标准历史重放的 Harness 暂不接入，统一使用 ACP 重放兼容，不增加厂商专用读取器。当前行为以本 change specs 及通用 harness-management/structured-history 为准，平台分页不要求原生按页读取。本文下方所有原快照及此前 SR 修订记录保留历史归属；其中当前连接降级、禁止全量重放再分页、原生分页硬前提和旧能力假设均不得作为本次执行依据。

本轮只修订文档；独立 tester 增补本轮用例，独立 reviewer 复核本次范围，产品用例仍未执行。详细合同、全套旧用例与工作包全面重基准及其他架构底座整改未完成，不恢复实现、不修改 checkbox，不删除原生数据或改写旧证据。平台身份、审计及离线平台备份不因 Harness 未准入而失效。

---

## 本轮文档修订独立复核（2026-10-30）

来源：用户在全量活动 spec 审查后回复“同意，修复吧”。本 change 涉及 SR-001；当前规则、设计及增补用例 CL-043–046 的限定文档修订经独立 reviewer `/root/spec_reviewer` 复核通过。独立 tester `/root/spec_tester` 编写用例，不继承规格修订对话；reviewer 与 tester 为不同执行者。本记录由主执行者转录独立结论，不改写下方历史记录。

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
| openspec/changes/deliver-codex-session-loop/design.md | `6165cae5d8ff8be9fcd302b44a4a42bbce6e0f445e8997561925812032d7884b` |
| openspec/changes/deliver-codex-session-loop/plan.md | `bd01b50fd2927d002b836482e4e480f99b6f9d921d4e92024f149807b0a13143` |
| openspec/changes/deliver-codex-session-loop/proposal.md | `f8755ce692aff98d29b804c3295dfe3137fe7ccc40b5db9b2081fc720f8c7f44` |
| openspec/changes/deliver-codex-session-loop/specs/harness-management/spec.md | `d1fcc4c610fdc71ef4b5af93d6ac125b1e7887a2387482aff975681142e4558c` |
| openspec/changes/deliver-codex-session-loop/specs/management-audit/spec.md | `66dd2dd8a304389f9a3b1c50ff80641a89ae85f1314bf882a2366981211d5715` |
| openspec/changes/deliver-codex-session-loop/specs/native-session-recovery/spec.md | `b4641b75362b01223588710527f50f0fb0e41dd15a63528c5c63ecec1a208de7` |
| openspec/changes/deliver-codex-session-loop/specs/permission-approvals/spec.md | `e377809f78a10d1e336fca0ba4ef1912013f3c989a9d91cbac39ae79a66abd14` |
| openspec/changes/deliver-codex-session-loop/specs/session-execution/spec.md | `afd5145b3d468738f3091648ec114c4a167711a1d8207289ecd0e33553cf6064` |
| openspec/changes/deliver-codex-session-loop/specs/structured-history/spec.md | `a40d8b096bcc06310f87fd055b0f06aafe23d0375ca0aec779bca7b805ec5a7c` |
| openspec/changes/deliver-codex-session-loop/specs/workspace-registration/spec.md | `4cf4e9e297efeb4a5c56fc89b706b3fd46fd866849708fe032efbcd78c1e9de9` |
| openspec/changes/deliver-codex-session-loop/tasks.md | `ff6d4969302844596cf158556ee55a8f85fa24c146f878c0e50cd6337ea98360` |
| openspec/changes/deliver-codex-session-loop/tests.md | `1d71fc46a0da8e56e6e5abe0b98fdd3afe79f7bab328dc01ed5db7eb77d6fc93` |

---

## 原有独立检视记录（仅原快照）

> 适用范围说明（2026-10-30）：下方为原快照的独立检视记录，原问题与结论未改写。本轮会话数据权威、激活和删除规格已修订，尚未独立复查；本文历史 PASS 不构成新方案开工或验收依据。

﻿# 独立方案与用例检视

状态：当前文档快照的方案/用例/计划/任务检视通过；不是实施授权、兼容性通过或产品验收。真实Codex/ACP版本、协议协商及原生恢复能力是adapter编码前门禁，未满足不得进入适配器实现；上游节点接入须真实交付并重基准。没有产品运行证据不是本轮文档缺陷。

审查读取本change proposal/specs/design/tests/plan/tasks及重设计、AGENTS、OpenSpec规则；不继承coder对话，未修改产品或用例。所有spec场景在tests有精确Requirement/Scenario引用，CL-003/004覆盖保存500ms或64KiB先满足、结束即时保存、缓存阈值、事务/审批/派发竞态，逐场景CL-005–029补正常及失败结果，CL-030要求真实Windows/Codex/Web闭环，CL-031人工检视不允许Agent代签。未验证与不支持明确分开，没有把研究或fixture认作真实能力。

设计保留命令与Run、协议载荷与领域、持久化与Web投影边界；保持前台互斥、取消不等于结束、资源运行状态不等于轮次结果、原生恢复不灌平台历史及重放来源隔离。上游授权/撤销逐次复核。plan明确WP1–6、重叠路径串行合入、固定版本和独立验收，tasks有各WP真实checkbox引用和完成判据。未存在产品检查被列为待建立入口，未虚报可运行。

本次未发现未闭环设计问题。实际性能、500ms触发、固定向量/协议合同、Windows进程树、依赖安全、真实审批/取消/恢复及所有产品用例仍待实现与独立验收；届时必须按固定版本重新检视，不能据本报告签产品通过。

## 文件快照

| 文件 | SHA-256 |
| --- | --- |
| design.md | `03E49960C1B37E8AA2CD0B4EBC090CBBD3905766C7B4B1F66BC24B96D72A26C7` |
| plan.md | `C258314B11983A3AEDA98CF7A038268B816A7F55E5C4AD29FE44DD6274A705A3` |
| proposal.md | `5855FF5AD4BFB71E786102D72B68E0A09E2074E08807390F44BD9C532569E78D` |
| tasks.md | `241B04DCCF3E63F42ADCA00B271529FC55A5AC8D978529C14D41B259D332FFBD` |
| tests.md | `976EAD2D13D8482683BC6CC5EEA5212F1D484F7EF692E2CBF6E614436219145C` |
| specs/harness-management/spec.md | `2FE4F312AEC5E232F06A0F9581DF6ADFC8A3423A1CC5403BC3CB4309050104C1` |
| specs/management-audit/spec.md | `66DD2DD8A304389F9A3B1C50FF80641A89AE85F1314BF882A2366981211D5715` |
| specs/native-session-recovery/spec.md | `8FEFC8E4CCD951644A16FA56429F0C62CA0821BBBF0FAF07D986BA6B77363C2B` |
| specs/permission-approvals/spec.md | `E61FB389DFBAD686F883FEA77E2D679AFEEADBE540D029E91F6632FACCFC7601` |
| specs/session-execution/spec.md | `61067A30D797F0AD3789226CAA3B1491299AA4F45400F9230221298823ED00A6` |
| specs/structured-history/spec.md | `67F05D151AAFE3D069BE1E962529D991F45B71940BBA2378CBDF1589067FD410` |
| specs/workspace-registration/spec.md | `4CF4E9E297EFEB4A5C56FC89B706B3FD46FD866849708FE032EFBCD78C1E9DE9` |
