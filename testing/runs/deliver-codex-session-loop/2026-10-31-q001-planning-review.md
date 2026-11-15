# Q-001 规划独立检视

结论：本次已检查的规格、合同、计划及 tester 候选用例定义通过，无新增阻断问题。该结论仅限下面 SHA-256 对应的规划文件，不代表产品实现、数据库迁移或真实 Harness 验收通过，不自动放行尚未冻结的 DTO/DDL、协议完成关联算法及队列交接实现。

日期：2026-10-31。执行者：独立 reviewer Agent。依据来自文件中的 Q-001 原始确认、AGENTS/config/reviewer 职责，不继承 coder 对话。没有修改仓库、数据库、进程或历史 runs/review；唯一写入为本报告。

## 范围与核对

- 逐份阅读主 change 八份 specs（含新增 acp-platform-constraints）、proposal/design、implementation/management-command/approval-owner 三份当前合同、plan/tasks，以及 README、总体设计、能力地图、术语和 config。
- 逐份核对八个下游 change 的 spec：原生接管、分页、子树、容量、生命周期、备份、DeepSeek、ZCode。未以旧代码是否符合新版作为本轮问题；未审查下游全部 plan/tests 的完整交付质量。
- 检查共享三个未发送项、编辑身份与版本、取消/发送顺序、满额新草稿拒绝、立即发送先确认旧工作结束、失败暂停、无 TTL 与节点重启不恢复且无专门提示、一次受理授权及当前读取/新操作撤销、严格批准保存、附加审计失败继续、未知审批原选项人工展示、正文/输出及间接视图预算删除、旧 ID 不复活及 v2 早终态/迟插入/自发工作不造关联，未发现当前规划间实质冲突。
- 既有认证、显式来源、真实请求、资源计数与内容/资源安全保留有负责规格；附件实际读取的独立安全预算未被写成聊天正文或输出缓存上限。未发现新造厂商白名单、环境指纹、物理存储等价证明、历史实测报告白名单、统一重放或原生分页接入门槛。
- 总体设计第193行仍有 RP-001 旧正文；该文第3/9行及当前主规格明确取代冲突规则，故本轮按停用历史记录处理，不将它判为仍激活门槛。未来整理可让局部历史标识更醒目；不要求改历史 runs/review。
- plan 的 WP2→WP3→WP4 共享 node/lib 写范围已在 Merge Order 串行，隔离与重验安排明确；Main E2E 只有一个 required YAML 块。任务6.1及后续新实现/验收仍待办，四项旧勾选均限定旧版本/子范围，没有把其当 Q-001 通过。

## tester 候选定义

读取 tester-candidate/tests.md 全文。独立只读核对得到：主规格66个 Requirement、121个 Scenario，在候选关联表全部有相同原名；107例，CL-001–086 标题与当前仓库旧 tests.md 逐字一致。另逐段检查对应操作步骤及断言，而非仅以关联表或文件存在性认定覆盖。

CL-087–107及旧例修订覆盖本轮队列、授权接管、严格保存、管理故障、正文预算、时间、重启及组合时序；真实/替身/模型边界明确。CL-100/101 不代签 CL-105–107 的真实分支，未能产生的真实分支明确未执行。无 LX 转嫁用户，缺具体新入口/DDL/协议合同则受阻；当前候选没有声称产品用例已经执行。

## 问题记录

没有新增必须修复问题，无需生成空的问题 ID。历史 Findings 保留原归属，本报告不关闭历史工程或产品缺口。若规划输入或候选用例发生实质变更，需按新摘要复查。

## 实际只读检查

- openspec.cmd validate --all --strict：11 passed、0 failed，仅结构检查。
- git diff --check：exit 0，无格式错误；Git 输出现有 LF/CRLF 提示，未改文件或索引。
- 自有 PowerShell 读取核对：66 Requirement / 121 Scenario / 107 Case；缺失原名映射0；旧86标题差异0。没有运行产品、浏览器、真实 Harness、SQLite 迁移、安全/供应链动态检查或 Windows 进程树验收。

## 输入快照 SHA-256

摘要在完成阅读与只读检查后取自当前字节；tester 候选未集成时仅以下列绝对候选路径为受审目标，集成后仍须核对同字节。表中仓库相对路径以 D:/Project/acp-remote 为根。

| 文件 | SHA-256 |
| --- | --- |
| AGENTS.md | 1019d8854aca68d2519e599d0f3a1484e49667273708a49ac03818834aa38be3 |
| openspec/config.yaml | 9d984823b83af8a704d0e95ab9c1cbda4fdd1a4816a5dc8b8fca56ee7e88c48b |
| openspec/role/reviewer.md | d36cd77fb71906fa3a6052c1febea0f62535fec450d9343b5648369d68320e31 |
| README.md | 93d2270e8963e47c43aaedc52a10522d3382e692569be06739262dd15ce90a7c |
| docs/HARNESS_CONTROL_PLANE_REDESIGN.md | d26ca42232ea7c510ab61ee4d83f1a14af918f2038e4924a920ffc173eebd2b6 |
| docs/REBUILD_CAPABILITY_MAP.md | c934b2022eeadd41dc738783b2645e912d7dc09153250d46ba3e84add17f084a |
| openspec/GLOSSARY.md | 42a23c5a46eef25bd1d1ed1167a875e454b7dc35c11eb8a0bae89da1c20c81a8 |
| openspec/changes/deliver-codex-session-loop/proposal.md | 872b1087d96da1151b63972fb86d03811b88e804f41faa5caef4ba2b5b19acee |
| openspec/changes/deliver-codex-session-loop/design.md | f44d512cd16376c70213f1c456fe4a98832c9c54d998e606c07a8dd4063a0d1a |
| openspec/changes/deliver-codex-session-loop/plan.md | 49a9dc8c060edfda7b966f86fdf69a5dc9bfb93a998a833d4982a94f12fcb1b6 |
| openspec/changes/deliver-codex-session-loop/tasks.md | 9ae5696b64b6ac3d41b719995a6d2062827d9b7cb6bf2c857167c173dacfd88d |
| openspec/changes/deliver-codex-session-loop/implementation-contract.md | 285b1c96be9e89ea36cf6bfadf73f39263bd6b006ae24b816a0ca41fca2ffa91 |
| openspec/changes/deliver-codex-session-loop/management-command-contract.md | b97e46500676de1882d3f0d3f1b5f28cc29a2a8c3361378bbba17dd9def9a726 |
| openspec/changes/deliver-codex-session-loop/approval-owner-contract.md | 2904811b7eb9fc645b660d5b7a74428c3557307275f313d7c234ecd21600d7af |
| openspec/changes/deliver-codex-session-loop/specs/acp-platform-constraints/spec.md | abd41bf8b13e29833b698e7eadbd5f3d861f186301dd81ff27d64e06c26aa5e1 |
| openspec/changes/deliver-codex-session-loop/specs/harness-management/spec.md | 42af8e99948e990af1713712d67b7b19dacdb56a2d043847f7a22e4db9027cb5 |
| openspec/changes/deliver-codex-session-loop/specs/management-audit/spec.md | 270faa951f5e83637bbf23713d7a27e0dd73ae938c204b2867a214bc879a439a |
| openspec/changes/deliver-codex-session-loop/specs/native-session-recovery/spec.md | c013e151444238361d96802e0e7519d8cc31fa42f6b788fe4413805cac79cb29 |
| openspec/changes/deliver-codex-session-loop/specs/permission-approvals/spec.md | 935bd6039aa07fc33e3706ba50d20766cbd58022ac8dc57450547bec2854ea3b |
| openspec/changes/deliver-codex-session-loop/specs/session-execution/spec.md | 176ccea61b97aff2794ee8a0837c65b29cd3fcf5a27b3935a5dec133cd4e3f8f |
| openspec/changes/deliver-codex-session-loop/specs/structured-history/spec.md | bfcd46d1735773b920f7400524dacb1fedf218452ecfdc1fa00842d8525ab13a |
| openspec/changes/deliver-codex-session-loop/specs/workspace-registration/spec.md | 4cf4e9e297efeb4a5c56fc89b706b3fd46fd866849708fe032efbcd78c1e9de9 |
| openspec/changes/deliver-agent-session-tree/specs/agent-session-tree/spec.md | ab90d02c6145d1f0d3b8c8cc73ae85cfae08b7fbe84789962089ba988be0ecf4 |
| openspec/changes/deliver-backup-recovery/specs/backup-recovery/spec.md | fe4283d9a70257a0f1e893ba8ffb101f3f26a44ae4a3162a81fe4066e1867bde |
| openspec/changes/deliver-history-navigation/specs/history-navigation/spec.md | 8b5e76c27a1ee8defcb24a29a7913a27eeac806e8652f08d051b4767a25e28ce |
| openspec/changes/deliver-native-session-import/specs/native-session-import/spec.md | c901d1da6837c09f9bfa1ed4099988ca5b7bc96cfd9b25f5d1ff74d3ba0e746e |
| openspec/changes/deliver-session-capacity/specs/session-resource-policy/spec.md | d65c2de336bcbb19340f961b4fb889b707315de0be7d38e49e36d517831d4512 |
| openspec/changes/deliver-session-data-lifecycle/specs/session-data-lifecycle/spec.md | 8463c9450a0f28dcec895b9bea1986cc9cad7b13ba117a3917f992b8e03bd00d |
| openspec/changes/integrate-deepseek-harness/specs/deepseek-harness-compatibility/spec.md | eb91f812453dfa10d35e8aa2193ce71c735f07b7f85369431ead5d11d774a91f |
| openspec/changes/integrate-zcode-harness/specs/zcode-harness-compatibility/spec.md | 80ac6d0264f7652cdbf4d45ca3a52a7e2e257d811a3b86728414310c123bf9d1 |
| C:/Users/jack/.codex/visualizations/2026/10/31/01a18fb8-1806-7d91-b146-a2d88e96b6f8/tester-candidate/tests.md | 7928246d0433783833a4b372c5cea11eba28cdde176525d1f424b831ea3614d6 |

## 集成后复查（2026-10-31，最终规划结论）

结论：Q-001 本轮规格及规划集成交付通过，无新增阻断。允许将本报告按原字节复制为 testing/runs/deliver-codex-session-loop/2026-10-31-q001-planning-review.md，在主 plan 登记该报告链接及规划复查结论，并仅将任务6.1标为完成；这些是本次检查的交付记账，不是产品验收，也不放行未冻结工程合同。6.2–6.5和其余产品待办保持原状态，未改其完成判据。报告自身及上述纯记账追加不要求递归摘要。

### 实际复查依据和结果

- 已直接读取仓库集成 tests.md 并核算 SHA-256，仍为 `7928246d0433783833a4b372c5cea11eba28cdde176525d1f424b831ea3614d6`，与初轮独立审查候选完全相同。重新运行原名映射核对仍为66 Requirement、121 Scenario、107例、缺失映射0。
- 对初轮摘要表逐份核对，只有总体设计、主 plan 和 acp-platform-constraints 三个输入发生变化；逐项重新阅读：总体设计原第193行已改为 Q-001 能力归属，消除初轮所记局部旧段；plan新增交付摘要准确限定候选/定义/初轮报告范围；限制清单新增确认来源及官方入口，没有改变 Requirement/Scenario 或引入额外准入。其他初轮输入摘要未变。
- 只读检查八个下游 change 的 proposal/design/plan/tasks 的实际新增/替换文字。统一的 Q-001 优先及历史失效说明和当前主规格一致；原用户引文、旧报告及未重新基准的完整实施计划仍保留历史归属。没有把仍存在的旧正文或其历史通过状态重新激活为新门槛。本结论仅覆盖此次引用修订，不宣称下游全部实施合同及用例已全面复审。
- 阅读 ACP README 新顶部声明，准确区分现有预算/未知审批实现与新版规则，不改变产品代码或制造已实现声明。原正文实现细节只按该声明的现状说明理解。
- 本地新增 proposal 相对链接及主 plan 的测试定义报告路径可解析。实际在线打开四个新增官方入口：v1 Prompt Turn、v2 Prompt Lifecycle、v2 Tool Calls 的 Requesting Permission 及 v2 Session Setup，均返回对应官方页面；v2仍标 Draft。此项仅核对入口可读和主题一致，不替代未来必须固定的协议修订/schema摘要及算法审查。
- 实际重复执行 openspec.cmd validate --all --strict：11 passed、0 failed；git diff --check：exit0，只有既有 LF/CRLF 提示。没有运行产品或改仓库/索引。产品、真实Harness、浏览器、SQLite迁移及进程树验收仍未执行，初轮列明的工程限制继续有效。

### 集成复查输入 SHA-256

下表是本次复查时的当前字节，覆盖初轮输入、实际集成用例、补读下游关联文件和 ACP README。之后仅上述 report/plan/任务6.1记账可变；其他行为或用例改变仍须新版本复查。测试定义说明保留原候选阶段上下文，不把其中“同目录tests.md”当作复制后 runs 目录下的新产品文件引用。

| 文件 | SHA-256 |
| --- | --- |
| AGENTS.md | 1019d8854aca68d2519e599d0f3a1484e49667273708a49ac03818834aa38be3 |
| openspec/config.yaml | 9d984823b83af8a704d0e95ab9c1cbda4fdd1a4816a5dc8b8fca56ee7e88c48b |
| openspec/role/reviewer.md | d36cd77fb71906fa3a6052c1febea0f62535fec450d9343b5648369d68320e31 |
| README.md | 93d2270e8963e47c43aaedc52a10522d3382e692569be06739262dd15ce90a7c |
| docs/HARNESS_CONTROL_PLANE_REDESIGN.md | 9351f3a0a50fd32d6c77cd7a4842244bde3855c856ffe3f54ff2746cefc3e6a3 |
| docs/REBUILD_CAPABILITY_MAP.md | c934b2022eeadd41dc738783b2645e912d7dc09153250d46ba3e84add17f084a |
| openspec/GLOSSARY.md | 42a23c5a46eef25bd1d1ed1167a875e454b7dc35c11eb8a0bae89da1c20c81a8 |
| openspec/changes/deliver-codex-session-loop/proposal.md | 872b1087d96da1151b63972fb86d03811b88e804f41faa5caef4ba2b5b19acee |
| openspec/changes/deliver-codex-session-loop/design.md | f44d512cd16376c70213f1c456fe4a98832c9c54d998e606c07a8dd4063a0d1a |
| openspec/changes/deliver-codex-session-loop/plan.md | 1a7bebf8b199f26c006ccd10d8a57e83ec2e04777013598860f4a9858e5e3e39 |
| openspec/changes/deliver-codex-session-loop/tasks.md | 9ae5696b64b6ac3d41b719995a6d2062827d9b7cb6bf2c857167c173dacfd88d |
| openspec/changes/deliver-codex-session-loop/implementation-contract.md | 285b1c96be9e89ea36cf6bfadf73f39263bd6b006ae24b816a0ca41fca2ffa91 |
| openspec/changes/deliver-codex-session-loop/management-command-contract.md | b97e46500676de1882d3f0d3f1b5f28cc29a2a8c3361378bbba17dd9def9a726 |
| openspec/changes/deliver-codex-session-loop/approval-owner-contract.md | 2904811b7eb9fc645b660d5b7a74428c3557307275f313d7c234ecd21600d7af |
| openspec/changes/deliver-codex-session-loop/specs/acp-platform-constraints/spec.md | a8f7da0def150d79b02bdce1e693a72fd2c6d37859032aab8dd42fbe42de2be5 |
| openspec/changes/deliver-codex-session-loop/specs/harness-management/spec.md | 42af8e99948e990af1713712d67b7b19dacdb56a2d043847f7a22e4db9027cb5 |
| openspec/changes/deliver-codex-session-loop/specs/management-audit/spec.md | 270faa951f5e83637bbf23713d7a27e0dd73ae938c204b2867a214bc879a439a |
| openspec/changes/deliver-codex-session-loop/specs/native-session-recovery/spec.md | c013e151444238361d96802e0e7519d8cc31fa42f6b788fe4413805cac79cb29 |
| openspec/changes/deliver-codex-session-loop/specs/permission-approvals/spec.md | 935bd6039aa07fc33e3706ba50d20766cbd58022ac8dc57450547bec2854ea3b |
| openspec/changes/deliver-codex-session-loop/specs/session-execution/spec.md | 176ccea61b97aff2794ee8a0837c65b29cd3fcf5a27b3935a5dec133cd4e3f8f |
| openspec/changes/deliver-codex-session-loop/specs/structured-history/spec.md | bfcd46d1735773b920f7400524dacb1fedf218452ecfdc1fa00842d8525ab13a |
| openspec/changes/deliver-codex-session-loop/specs/workspace-registration/spec.md | 4cf4e9e297efeb4a5c56fc89b706b3fd46fd866849708fe032efbcd78c1e9de9 |
| openspec/changes/deliver-agent-session-tree/specs/agent-session-tree/spec.md | ab90d02c6145d1f0d3b8c8cc73ae85cfae08b7fbe84789962089ba988be0ecf4 |
| openspec/changes/deliver-backup-recovery/specs/backup-recovery/spec.md | fe4283d9a70257a0f1e893ba8ffb101f3f26a44ae4a3162a81fe4066e1867bde |
| openspec/changes/deliver-history-navigation/specs/history-navigation/spec.md | 8b5e76c27a1ee8defcb24a29a7913a27eeac806e8652f08d051b4767a25e28ce |
| openspec/changes/deliver-native-session-import/specs/native-session-import/spec.md | c901d1da6837c09f9bfa1ed4099988ca5b7bc96cfd9b25f5d1ff74d3ba0e746e |
| openspec/changes/deliver-session-capacity/specs/session-resource-policy/spec.md | d65c2de336bcbb19340f961b4fb889b707315de0be7d38e49e36d517831d4512 |
| openspec/changes/deliver-session-data-lifecycle/specs/session-data-lifecycle/spec.md | 8463c9450a0f28dcec895b9bea1986cc9cad7b13ba117a3917f992b8e03bd00d |
| openspec/changes/integrate-deepseek-harness/specs/deepseek-harness-compatibility/spec.md | eb91f812453dfa10d35e8aa2193ce71c735f07b7f85369431ead5d11d774a91f |
| openspec/changes/integrate-zcode-harness/specs/zcode-harness-compatibility/spec.md | 80ac6d0264f7652cdbf4d45ca3a52a7e2e257d811a3b86728414310c123bf9d1 |
| C:/Users/jack/.codex/visualizations/2026/10/31/01a18fb8-1806-7d91-b146-a2d88e96b6f8/tester-candidate/tests.md | 7928246d0433783833a4b372c5cea11eba28cdde176525d1f424b831ea3614d6 |
| openspec/changes/deliver-codex-session-loop/tests.md | 7928246d0433783833a4b372c5cea11eba28cdde176525d1f424b831ea3614d6 |
| app/src/adapters/acp/README.md | 438845b3e831899bd8100c230e36b3aa66a8784a267de5ac0b76e479b1ddbfc5 |
| testing/runs/deliver-codex-session-loop/2026-10-31-q001-test-definition.md | 5c561b6ba26e2ebaa7228ee6a5d0f87ec13632bd92affde785f6928caeef8df0 |
| openspec/changes/deliver-agent-session-tree/proposal.md | e410fd126edf1890bc1548744996826b78de044d9c95402b1061f08139048a4d |
| openspec/changes/deliver-agent-session-tree/design.md | 1b76fab1b04bc92882630ea853631a7f63be70adb271080dd3ac6abaad734c2a |
| openspec/changes/deliver-agent-session-tree/plan.md | ec7cc8ab2ca9c5a10aef39ebddd1a8e7d35282731577b66f690e21bb77b5bc77 |
| openspec/changes/deliver-agent-session-tree/tasks.md | d9895902e445eb29cab95248e82715f5a139d6198be88c095c7532943f48e9c0 |
| openspec/changes/deliver-backup-recovery/proposal.md | afaadb1bce324f223764ce7d86c916103b47698dc7e77903bd915bb197f80bd1 |
| openspec/changes/deliver-backup-recovery/design.md | e7d7a8b5ebe28e1e64b88ec9ef3affd310442fc02201c7f8b48b6d325fa281e2 |
| openspec/changes/deliver-backup-recovery/plan.md | 105939cbec43879813af7545382a737bc019c4775996b0065b0129bbe882f4bd |
| openspec/changes/deliver-backup-recovery/tasks.md | 559c9bc22db553c3ff44904c4713a90cb5e7f2f2b5731e90dba40d73da63bf55 |
| openspec/changes/deliver-history-navigation/proposal.md | db6930b03735eaa42843f4dc317a5b7b81bd4cecd45cfebafa9a6cac1b9be12a |
| openspec/changes/deliver-history-navigation/design.md | 035def3cb8532a8944a9debd85201fcde9821f8c00b5a1b1ac7505032f1f9513 |
| openspec/changes/deliver-history-navigation/plan.md | c9f033b1f9fe4c00e075dc826bec891dd786fffa7fa4e781d56da0cba215baed |
| openspec/changes/deliver-history-navigation/tasks.md | ae53528f7f8c2de8483c4fd393516b5ccd35a7a3ea778f8ff1fdb7b8d337f6ac |
| openspec/changes/deliver-native-session-import/proposal.md | 313151fdda67f49bd31e7cd6c378a4047e10755960a7536eacb49fa86d13db73 |
| openspec/changes/deliver-native-session-import/design.md | 206e87ae286d1eb1a0c02c53b6cbf3f319c5bdedf11563224e5158db942ac184 |
| openspec/changes/deliver-native-session-import/plan.md | 50d05b05353cc952d2f7f7428c30cc0d3a95162f1dbab8e8d7e901009d6dd45e |
| openspec/changes/deliver-native-session-import/tasks.md | c3885b8856077ae2f91ec78edd15d9496b9cf1b6a5b3d6e9a68aaf8348768f13 |
| openspec/changes/deliver-session-capacity/proposal.md | 1c48ef70820cd15ae70f52dd7deab0e8ebf145807b7fa945359ca30ee25faf2a |
| openspec/changes/deliver-session-capacity/design.md | e3fc140afcd070db85fec294cad913fd161baf3a621375144ba5aa313508e6a3 |
| openspec/changes/deliver-session-capacity/plan.md | 7586411b2a306e609abeeb6886045d3e3bf67fc642d26315537fd71542ed2ab9 |
| openspec/changes/deliver-session-capacity/tasks.md | cb4e22d7069520d37894fb9ed7013654bd5c0d52d71ee625eedf17269526deef |
| openspec/changes/deliver-session-data-lifecycle/proposal.md | 7ded66ae94fa7643d1164709d351f6b12e3afcc9619890428e570759d7899f58 |
| openspec/changes/deliver-session-data-lifecycle/design.md | aa56e6dd7afdb5f9795ec43b4955d58d031893567be43190e700da419498cc0f |
| openspec/changes/deliver-session-data-lifecycle/plan.md | 4551a707512b19c4fb4918779bd674f773a1f1a440d233e6798db542efba61a9 |
| openspec/changes/deliver-session-data-lifecycle/tasks.md | b6adff72d0727a5944841a5d067c47f655325f91e209b56f6f3aa24602ed32d2 |
| openspec/changes/integrate-deepseek-harness/proposal.md | 2fa8683893fa1fbc8c4e6343081b19794a0d925d9883ba0653af2373c190d2c8 |
| openspec/changes/integrate-deepseek-harness/design.md | 729f85e82661cf9a969c8f1f8dc8108413e2b12836699d34de0df9d0d23de5a5 |
| openspec/changes/integrate-deepseek-harness/plan.md | 4c79b1a0a3e26f4bed5a3e192441535b970456681cd319377202a79db5cdfb6e |
| openspec/changes/integrate-deepseek-harness/tasks.md | cc841d0fae6acb04179aedd1dee0ce20adb12c7b455f208d1d01faa350d426d7 |
| openspec/changes/integrate-zcode-harness/proposal.md | 507d6fb801a50329a03dc545297410d6598c4a7464585dcc667cb6a9e73f5b4b |
| openspec/changes/integrate-zcode-harness/design.md | b30fe67aac7dd59497ab94aefd4f2206fca93feb950191787ae2cec3bf7d1e0d |
| openspec/changes/integrate-zcode-harness/plan.md | 565b142000b9e6f43b93724d83a0f20c36f0b8e3e9cc0eec326e3cd6bbfa6ff1 |
| openspec/changes/integrate-zcode-harness/tasks.md | 2ab741aa9ade948dbfe007ce11678a8745646123cee1524f576d73b56432c22a |
