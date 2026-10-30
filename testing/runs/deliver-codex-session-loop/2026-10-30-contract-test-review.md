# WP1 详细合同与验收用例复核

日期：2026-10-30。独立 tester，输入为已就绪 session-contract、最新 proposal/design/plan、现有 CL-001–042，以及 AGENTS/config/tester 职责；不继承 coder 对话。仅维护 tests.md 与本报告，不改产品、规划或 tasks，不代关 reviewer 的 PR-001–003。

结论：原用例对新增详细合同有明确覆盖缺口，已补齐本轮要求的可执行断言，保留全部42个稳定 ID/标题和原产品预期。本 tester 未发现仍阻断这批用例定义的矛盾；**不表示规划门禁或产品验收通过**，须由独立 reviewer 对当前摘要复核。真实 ACP 执行、审批、取消与恢复门禁仍未通过，不授权跳过。

## 覆盖核对及修订

| 合同 | 原缺口与本轮用例 |
| --- | --- |
| §1 上游及真实审批安全接线 | CL-031补可信身份不能绕过真正审批发行者；CL-004/030补现有schema1身份/撤销保留。CL-001/002仍区分上游基线、合同检查与本change最终交付；既有检查报告不外推。 |
| §2 同认证入口封闭扩展 | CL-031补action/payload/未知字段/类型、nullable、安全整数、错误与广告decisionId；CL-009补会话/内容/订阅路径的身份复核。现有CL-005–022/028–029保持行为及失败预期。 |
| §3 大内容、预算与撤销 | CL-024补12KiB分块、32KiB信封、深度8、未知结构字节往返、顺序/幂等/异块冲突、长度/摘要、引用权限用途、上传并发/容量、60分钟硬截止/120秒无新块空闲到期与清理。CL-009补逐块client_gate及撤销COMMIT与实际writer发送屏障；CL-031补原速率/队列/连接预算和无旁路；CL-030补真实大内容和撤销窗口。区分无权限读取及input引用注入，保留有权其他客户端读取共享输出。 |
| §4 事务、去重与升级 | CL-004补Turn/内容引用/审计同事务，以及全新库与schema1副本两路径、两处版本原子更新、既有身份/信任/撤销/审计/revision保留、DDL/版本/COMMIT失败、未知/矛盾/残缺结构无修改、旧程序拒绝schema2；CL-017补超过24小时去重、未知交付不重派及100000条上限；CL-030补真实升级链路。 |
| §5 顺序与同步 | CL-027补snapshotId/代数/会话/revision一致性、并发分页、60秒分页游标与独立60分钟固定读取租约、缺事件/重连/重启重取原子提交/live基线、源位置间隙不复用。CL-042补迟到实时版本不回退、提交事实分离、过大元数据分段且无残缺JSON。原CL-012/023/024/027/034继续保真和恢复语义。 |
| §6 故障停止 | CL-003补单调时钟2/3次、4999/5000ms、249/250ms边界、单在途提交、成功复位、硬上限抢先及强制失败；去掉“实施时另定判据”的旧占位。CL-026补真实受控故障及15秒关闭预算；CL-011补共享实例不能单会话强杀且责任保留。 |
| §7 交接建议与其他用例 | 已按实际合同独立分配断言，没有把建议当执行证据。CL-032–041继续版本/协商/兼容/真实矩阵和用户决策门禁；不以本次会话wire扩展替代ACP逐版本合同。CL-042继续正常页面隐藏保存状态、仅异常报错。 |

## 已验证和未验证

- 比较修改前后全部 `### CL-` 标题：均42条且逐标题完全一致。只追加步骤/预期，并把CL-003旧失败判据占位替换为本合同引用；没有删除或降低原断言。
- `node_modules/.bin/openspec.cmd validate deliver-codex-session-loop --strict`：退出0、change valid；`git diff --check`：退出0。
- 本次仅文档/用例检视，未执行新增产品断言、未建模拟“通过”结果。大内容真实传输、schema迁移、保存失败窗口与真实Harness待实现后执行；前次上游基线检查仍只覆盖其原产品源码摘要。
- 原始结构/场景引用为人工核对，不宣称存在专用机器覆盖门禁。没有新增LX或要求用户重复手测。

## 固定候选摘要

文件均位于 `openspec/changes/deliver-codex-session-loop/`，SHA-256如下；若这些文件后续变化，本结论需重新判断适用范围。报告自身不纳入摘要。

| 文件 | SHA-256 |
| --- | --- |
| session-contract.md | BB8F6661EB66C43EE4CFAD7B31FA30C4B07AD5FACDADD3A96D200F7FB264A7CA |
| design.md | 32902C6335C3D2AF8EABF1464C41ABD7812EBC4E8E78AB64CEE66C7C1521109E |
| proposal.md | 8D4FB67C551E4120790F07216781EED753C0829A93BDD6C4A136D6FE572A1028 |
| plan.md | 9A16A81939910037601F936A16F9E824552CFA6A912ADCECE2512161933B4151 |
| tests.md（本轮补充后） | 6E6DAD8DE4B68ADE19021BF5518A4115173A1E1329C0F799FC7B26956E0A3AF6 |

产品交付仍被未执行的必需真实能力及后续独立检视阻断；本文“未发现用例定义阻断”仅限上述固定文本，不能替代提出者关闭原规划问题。

## 同日读取租约与原子基线修订复核

合同修订后重新读取§3/§5，并确认其他四份规划来源摘要不变；本表摘要已更新为修订后的合同和tests。此节取代上轮对60秒内容引用/上传期限及“先已提交快照再订阅”的过时理解，不回写任何产品执行通过结论。

CL-024现验证begin后固定60分钟硬截止和120秒无有效新块空闲到期，重复块不刷新空闲、有效块不延长硬截止；按原120 command/分钟完成16MiB/1366块并计入begin/finish及共享管理流量，独立验证限流、最大对象成功和超时重建上传。CL-027同步分离60秒历史游标与60分钟内容租约，避免残留矛盾。

CL-042增加首次订阅/afterRevision相等/快照/重连的静止未提交尾部可见性；订阅登记、捕获、基线排队交界分别设屏障，核对session.baseline先于增量且不丢更新。增加慢读最大对象、租约截止/逐块不续期、游标独立失效、撤销/断连作废及重新认证取基线、不混接不同版本、当前尾部能重取与缓存不足完整性错误。节点重启后不存在的未提交尾部仍明确异常，不宣称恢复。

重新执行OpenSpec strict和git diff --check均退出0，42个ID/标题保持。当前用例定义未发现剩余阻断，待reviewer对新摘要独立复核；真实上传、竞态及慢速读取尚未执行。未发现授权范围外的产品规则变更，新增合同可执行参数仍由reviewer最终检查。
## 同日 pageSize 最终合同补充

用户已确认前端传参及默认10的分页语义，在02最小wire中仍须保留，完整父子/展开由03交付。读取最终合同后，仅向CL-027/042追加最小参数断言，42稳定ID/标题不变：history/snapshot/subscribe省略默认10，合法1–200正安全整数，null/0/负/小数/超限在查询前invalid_request；父轮DESC、轮内源位置ASC，当前轮计入且不切轮。下一逻辑页游标只锁目标/方向/认证范围/排他轮次边界，允许新合法pageSize；同页分块和快照冻结原选中集合与有效大小，续块不重选/漏条目。基线不可把显式值回退10；02 Web可始终传10，不冒认03能力。

最终session-contract SHA-256 `A1D47843629CFE32A1B01ECE5A8CB021BF82804C969AD3132166BB0991D3E2C0`；追加后的tests SHA-256 `C757EEA6D4EEEAFE3DE9E7815BD5E11A1E89DF0A1000374BDC5B3CCD8E8ADBC9`。OpenSpec strict与git diff --check实际退出0；仅文档断言维护，没有pageSize产品运行结果。此追加不改变2.2保存故障预期，也不将旧候选HIST-R001失败或任何产品not-run改写成通过。
## WP3预核查新增§7–8：独立用例补充

读取主执行新增session-contract §7原生审批发行/关联/交付边界、§8初始化版本引导，以及reviewer-wp3-readiness.md的W3R-001–003后维护断言。本次是落实既有权限、关联和逐版本语义，不新增产品行为，不将文档修订记作实现通过。原断言建议节现为§9；本报告此前快照中的节号保留历史归属。

- CL-004/014/015：补裁决COMMIT后、交付认领前、认领持久化至实际writer前的撤销/取消/Run终态/换代窗口；分别记录先失效零发送、先实际提交保留已交事实。裁决前及交付前强刷失败关闭，历史变化退出协调区重刷；认领后崩溃/传输/确认持久化失败unknown不自动重发。
- CL-013/016：补原生请求ID/toolCallId/messageId不同命名空间及数字1/字符串1不同标量类型；跨session/原生会话/Run/instance/generation碰撞、冲突重复/迟到和缺关联失败关闭。缺rawInput但已有可信同域关联不一律拒绝；不等待未来工具事件自动放行，也不照抄探针白名单。Web、时间戳、ActorContext、数据库decided不能单独发行授权。
- CL-031：补client_gate→owner交付协调区→短SQLite事务统一锁序；强刷/网络不得在SQLite事务等待，强刷在最终区外、版本变化释放后重刷。实际writer与认领同gate/协调区、异步排队不得绕过最终复核；审计脱敏及撤销不停止后台保存仍保留。
- CL-036：补单次v2 offer→selected v1按v1 agentInfo/agentCapabilities解码；错误ID/未知或超offer版本/错误selected形状明确失败，不重initialize、不按厂商猜、不业务重发；selected1不意味着支持v2。

修改前后42个稳定ID/标题逐一相同，仅追加对应步骤与预期，没有放宽原结果；OpenSpec strict与git diff --check实际退出0。最终session-contract SHA-256 `1CB0A4E67D6B8475B37EE64E1F9F21023046F14A8AD3B0F8EC9009F1EEAA6D7A`，tests SHA-256 `2920790D47736EF11667C4430A7A4EAE42AC8C162C0463344A2B2B971671865A`。新增断言全部产品not-run；W3R-001–003必须由原reviewer复核，本tester不代关或宣布WP3门禁解除。
## WP3/WP4 §9 Web投影编码：独立用例补充

先读取§9候选（合同C51B9AB5…），发现snapshot只有items但泛称各投影不得省略liveItems的编码歧义，已明确报告主执行，未通过发明字段或降低尾部预期掩盖。主执行补成三种投影各自的精确承载规则后重新读取，本tester未发现该处仍有冲突；原reviewer另行复核设计，本报告不代关其问题。

补充范围仅tests.md现有CL-005/006/024/027/031/042：

- 短list按实际UTF-8帧预算选最多32项、长字段detailsRef、列表cursor失效/范围；未经实际证实的程序/适配器版本null、能力unverified，不因注册虚报verified，证据绑定固定程序/配置摘要。
- content.read的sessionId:null只读节点签发登记/能力元数据，不读session正文、不上传input、不接受任意路径/伪ref；错类型/范围拒绝。actor/认证代数/快照/60分钟租约及逐块gate保持，原对象/帧/队列/限流不放宽；details不带凭据/恢复参数。
- projection_manifest只含允许字段、无嵌套、每对象有界；同turn/ordinal的连续items片段按顺序合并为一轮，逻辑页选中集不变。缺part/SHA不符/超预算明确错误，不显示半份完整投影；未完整接收校验前不替换当前完整内容。
- snapshot.items合并所选提交条目及当前Run全部live，同item取更高版本且包含新未提交item，无独立liveItems；baseline.turns保存提交投影，liveItems保存当前Run全部live后安装合并；history只在所选轮次items合并捕获时live，旧页不插当前Run、无liveItems、冻结后不拼新输出。新补断言区分三者，不将同一字段名强加给全部投影。
- 同item/version换contentRef先验证SHA及原始字节；同正文可换引用，异正文完整性错误，未完成校验不替换，只有committedVersion推进不算正文更新/正常保存提示。
- input为至少一块的UTF-8 JSON prompt结构，支持块保留原始字段，不支持显式unsupported_capability不压文本；原对象/摘要随事务保留。subscribe成功result→baseline→events顺序；精确unsubscribe只停止该连接会话后续入队，不取消Run/后台保存、不改devices编码。

最终合同SHA-256 `CBF1BFE389CA3824884918C0BA9147B117C472359D396A7A132F150204B7D071`；tests SHA-256 `1CEB400EC05147BAEDD1DBF5E02DF371A8B61684FE85A0AF9AB3E3A1A6DFB717`。42稳定ID/标题逐一保持，OpenSpec strict与git diff --check实际退出0。此节仅用例维护，新投影/网关/真实Web断言全部未执行，不签实现或最终产品通过。
## §10 登记/生命周期/通用ledger：独立用例补充

平台包先完成BLOCKED报告后读取主D新增§10，并核对§9的catalog_command_result null读取例外已同步。本tester未发现该版条款仍与现有通用ledger、保存失败或原生恢复语义冲突；按既有动作补实施断言，不新增Web动作或改变产品规则。

仅补CL-004/005/010/011/017/020/022/031：所有change变更动作共用actor/request及100000上限，登记目标无假session、通用typed结果不借input Run模型；登记定义/Command/audit/nodeRev原子、无报告unverified且不执行任意探测；create受理creating、一次owner认领CAS提交原生绑定才loaded，失败/unknown不重试；cancel保留前台互斥，release强刷/关闭/引用回收未确认不released；resume只原binding，旧资源终结须owner证据且原Command unknown保留；Command/预期状态/bindingVersion迟到回报不盖新状态。

新增catalog_command_result断言仅允许harness.register/workspace.register的受限结果以sessionId:null读取，绑定当前actor/代数/确切Command/60分钟租约/逐块gate，正文只动作目标状态及已提交ID/revision，不含定义参数或秘密。其他actor、会话Command、正文或任意文件不在此例外，仍需真实sessionId/会话范围。

修改前后42稳定ID/标题完全相同，OpenSpec strict及git diff --check实际退出0。合同SHA-256 `828A97CB8C2F2765CD5D116C2777D53A456CA7C5775E6B2DB5C57FC28DC19738`；tests SHA-256 `9D75A416F00960944DFDDC1C03BDD8DA4AAF4C70540DA7801DB6E221584EED9D`。此处所有catalog/生命周期产品断言尚未执行，不签编译/实现通过、不代关reviewer的W3R008，不消除平台PLAT-T001阻断。
§10后续覆盖复核：按原reviewer要求在CL-004明确create/cancel/release/resume各动作的accepted后、一次owner claim后、原生结果未持久化前真实节点终止/重启窗口，核Command/lifecycle/binding/approval来源及派发计数；旧owner claim回报拒绝、unknown不重派、不替代新ID。保持42标题不变，strict/diff实际退出0；tests新SHA-256 `5FD9D9975EB1CB0C29694339489705C1ECA474AE6E3D5FC9DAE8C9759B3275F7`。仅定义补充，产品not-run，交原reviewer复核门槛。