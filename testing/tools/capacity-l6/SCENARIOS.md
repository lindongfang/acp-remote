# 真实容量验收入口：运行合同与当前状态

当前日期：2026-11-18。本文描述专属验收入口，不授予模型请求、默认用户凭据或真实用户会话访问权限。其中真实 Codex 宿主物理单调时钟已通过 68 分钟 `cp026` 验证（`testing/runs/deliver-session-capacity/tester/2026-11-18-l6-cp026-remediation-v1/ASSESSMENT.md`），50 会话全量闭环与控制面状态机已通过统一二进制 `acd38a1dc...` 上的标准 ACP V2 受控对端 L6 编排验证（`testing/runs/deliver-session-capacity/tester/2026-11-18-l6-control-plane-e2e-v1/ASSESSMENT.md`）；其余外部模型自发分支受限于外部账户配额（至 `2026-12-10 11:44`）保持单列。

入口以当前 change 的 `tests.md` 为准。默认不执行四项 ignored 测试。真正执行前必须使用显式专属认证目录、真实已安装适配器与正常 Node 管理路径，保持默认 50 主树和实际 1800 秒。真实输入由调用者显式提供；命令接受后仍须观察真实协议事实。厂商不产生所需前台、后台、子关系或审批事实时，结果为具体 blocked，不能生成模拟事实替代。

## 程序与配置

使用主仓库集成后编译的测试程序，开启 `native-dispatch-dev-fixtures,approval-owner-dev-fixtures,approval-owner-acceptance-fixtures`。正常 `npm run check` 不执行 ignored 真实场景。四个 exact 名称为：

```text
node::tests::r3_source_wss_development::capacity_l6_cp011_real_fifty_and_thirty
node::tests::r3_source_wss_development::capacity_l6_cp023_real_unopened_root_children
node::tests::r3_source_wss_development::capacity_l6_cp026_real_replay_clock_and_pressure
node::tests::r3_source_wss_development::capacity_l6_cp032_real_known_work_and_finish_fault
```

通过 `testing/tools/capacity-l6/` 的运行工具进行 Validate、List、显式 Run；具体参数以工具实际帮助为准。Run 必须提供原测试程序、其 SHA256、源码清单及显式 config。List 只检查名称，Validate 只检查输入，都不能产生 CP 通过结论。不要直接绕过运行工具调用 ignored 场景。

Rust 除 config 外还核对运行工具传入的 `ACP_CAPACITY_L6_RUN_ID`、`ACP_CAPACITY_L6_EXE_SHA256`、`ACP_CAPACITY_L6_SOURCE_SHA256`、`ACP_CAPACITY_L6_CONFIG_SHA256` 和 `ACP_CAPACITY_L6_SOURCE_MANIFEST`。清单须绑定实际程序和源输入。config 由 `ACP_CAPACITY_L6_CONFIG` 指向。运行工具在清理子进程环境前，仅从原 HOME、USERPROFILE、CODEX_HOME 和 USERPROFILE/.codex 收集绝对路径字符串，通过 `ACP_CAPACITY_L6_DEFAULT_IDENTITY_DIRECTORIES` 的非空 JSON 数组传入；Rust 缺此事实明确拒绝直接运行。该事实不授权读取默认身份目录。

config 格式为 camelCase、拒绝未知字段：

```json
{
  "format": 1,
  "runRoot": "<全新专属绝对目录>",
  "launchProfile": "<实际format1 profile绝对路径>",
  "dedicatedHome": "<专属已授权身份HOME>",
  "dedicatedCodexHome": "<专属已授权CODEX_HOME>",
  "allowModelRequests": false,
  "defaultLimit": 50,
  "idleSeconds": 1800,
  "scenarioFiles": {
    "cp011": "<显式场景文件>",
    "cp023": "<显式场景文件>",
    "cp026": "<显式场景文件>",
    "cp032": "<显式场景文件>"
  },
  "webControl": {"enabled": true, "timeoutSeconds": 600}
}
```

`allowModelRequests` 缺省为 false。需要真实输入的场景必须另有明确授权并显式置 true。profile 是项目实际 format1 UTF16LE base64 格式；仅真实绝对 `node.exe` 加单个已安装 JS 入口，MCP、额外目录和 extra 均为空。`runRoot` 是仓库 `.tmp/capacity-request-tester/` 下的新子目录。身份目录可以位于仓库之外，须显式、已存在、非默认、逐祖先无重解析点，且与 disposable runRoot 双向不重叠；入口不删除这些身份目录。HOME/USERPROFILE 精确绑定 dedicatedHome，CODEX_HOME 绑定 dedicatedCodexHome，APPDATA/LOCALAPPDATA/TEMP/TMP 必须是 dedicatedHome 的严格子目录。SystemRoot/WINDIR/ComSpec/PATHEXT 如出现在 profile 中必须等于原 OS 环境；拒绝令牌等额外环境项。禁止复制默认 HOME 或默读默认身份。

场景文件仅给正常管理动作及预期正文。正文仅在进程内比较，不写结果或日志。`phases` 是正常命令数组；`l6.submit_text` 翻译为实际上传、ACK、finish、ready、input.register 和 consume，不产生合成 ACP 响应。`$nativeId`、`$targetId`、`$selection` 和 `$last/...` 从原管理响应展开。真实关系来自原生标准通知和必要存储，不能按配置声明关系成立。

## 场景文件的可执行输入合同

所有场景文件为 JSON 对象，拒绝未知字段；`format` 可省略，提供时必须为 1。以下字段只提供动作和期待值，不能直接把任何分支标记 passed：

| 字段 | 类型与用途 |
| --- | --- |
| `managementSteps` | 正常管理命令数组。为 CP023 产生真实父子关系，为 CP026/CP011 准备可由标准 replay 重读的实际历史。 |
| `expectedReplayText` | 非空字符串。程序只在进程内核对真实重放正文，不输出正文。CP011、CP026 必需。 |
| `protectedRootSteps` | 命令数组，每个需要保护的真实根分别执行。只有实际 Current 活动与 candidate 不成立才算保护成功，命令接受不算。 |
| `phases` | phase 名到命令数组的对象，见下表。 |
| `realChildIds` | 可省略的真实子身份数组。只可来自本次实际 Harness 通知/发现；提供时逐项核对原持久关系。省略时从本次可信关系动态取得。 |
| `expectedChildText` | 至少两个非空且互不相同的子正文期待值。比较对应真实子正文，并拒绝出现其他子的排他正文。不能用“非零 raw 字节”代替。 |
| `childFailureSteps` | CP023 的正常命令数组，使原 child 读取可观察地失败；程序保留原 owner/名额并核实际错误，不按配置签失败已经发生。 |
| `childRecoverySteps` | CP023 的正常命令数组，恢复真实读取条件，随后仍核实际正文和关系。 |
| `candidateReleaseSteps` | CP023 的正常命令数组，使唯一候选实际不再被保护；由原活动和原票据核事实。 |

每条命令的完整形式是 `{"action":"<正常管理动作>","payload":{...}}`。允许的动作前缀是 `input.`、`queue.`、`session.`、`approval.` 和 `source.session.`，仍由真实 Node 的认证、权限、参数与状态机处理。入口不提供厂商私有协议回包注入。

辅助动作 `{"action":"l6.submit_text","payload":{"text":"<显式获授权场景输入>"}}` 经过实际二进制上传、ACK、finish、ready、input.register 和 input.consume，进入正常队列。它不是厂商模型提示词配方；若实际 Harness 未产生所需子任务、审批、后台或延迟活动，程序报告相应 blocked。

占位符在 payload 内递归替换；整值 `$nativeId` 为本 phase 的实际原生身份，`$targetId` 为正常 target.register 返回的执行目标，`$selection` 为当前原来源选择，`$last/<JSON pointer>` 从该命令数组上一条完整管理响应读取，例如 `$last/result/bodyId`。不能引用其他 phase 的隐含响应。字面正文与其他非占位字符串保持不变。

| CP | 必需 phase 键与实际观察目标 |
| --- | --- |
| CP011 | `last_slot_execution`：实际排队执行加载；`long_silent_start`：前台 Active 且随后实际静默；`child_only_start`：父前台 Idle、真实后代仍 Active；`approvals_start`：至少两个真实待决审批；`cancel_one_approval` 与 `cancel_last_approval`：实际待决数依次为 1、0 且剩余工作仍受保护；`end_long_work`、`end_child_work`、`end_remaining_work`：通过正常管理动作解除各保护并开始新实际 1800 秒；`restored_input`：回收后原树重开并实际 Written 输入；`received_before_send_arm` 与 `received_before_send_finish`：正常安排延后活动、最后结束该工作，发送前必须实际收到 Current 工作。 |
| CP023 | 使用 `managementSteps`、真实 child/独有正文、失败/恢复/候选动作字段；不要求配置可凭空声明删除。关系不可读 OR 分支是明确标注的 Node-owned SQL 故障，保护及恢复 schema/原 proof，不能称厂商真实删除。 |
| CP026 | 使用 `managementSteps`、`expectedReplayText`、`protectedRootSteps`，分别建立真实历史和其余树的实际保护。不接受配置声明 1800 秒已经过去。 |
| CP032 | `capability_unprovided_idle`：实际 Current Idle 且后台范围未提供；`capability_protect_unprovided`：随后真实保护该树；`capability_background_start`、`capability_background_missing`、`capability_background_end`：已知后台、同 receipt 缺字段后仍保留、真实结束；`capability_tool_start`、`capability_tool_missing`、`capability_tool_end`：原活跃 tool ID 的同样边界；`capability_foreground_unknown`：实际必要前台失效；`capability_candidate_idle`：恢复真实资格；`received_before_send_arm`、`received_before_send_finish`：发送前真实接收工作。还需 `protectedRootSteps` 保护其余树。 |

所有 phase 都有真实事实判据及有界等待，缺厂商能力、命令参数不适用或活动未发生会报告具体原因。本文不声称任何示例 prompt 能可靠生成厂商子任务或审批，也不预填真实用户会话或身份。

## 四项范围与时长

CP011 创建实际 50 主树，先在第 50 原预留处暂停实际 Create，使执行与查看实际遇 Full，再按正常释放顺序验证另两入口成功；这不声称三个入口同时获胜。真实长任务、仅子工作、审批保护保持超过 1800 秒，结束后从新资格起点再次等完整 1800 秒。随后保留缓存/重放对照、发送前新活动、原票据故障与真实恢复输入。它包括 CP026 的实际时钟子程序，单项可能超过两小时，不能使用短时开发模式签收。

CP023 从真实新根产生至少两个可信子身份。浏览器在程序读取任何根或子正文前有首次直接加载机会。程序比较每个子身份的独有正文和排他正文，检查同一主树名额与原 owner。默认 50 下验证保护/候选/关闭协调、子读取失败与取消、整树资源 join、其他树隔离。现有正常管理 API 没有原生删除动作；“删除或关系失效”使用明确标注的 Node 自有关系存储不可读故障分支，不把 native.release 叫作厂商删除。

CP026 对两个原 root 分别记录真实接收和候选资格时钟。缓存对照仅在一次合法 Complete/rebind 后访问同一已收缓存并发心跳，时钟须不变；近 29 分钟实际重放须刷新接收，按特定原 root 验 1799/1800 秒和扫描，不由其他树先关闭代签。再验证原根真实重开及默认 50 下唯一候选压力、必要提交故障与同原系统操作恢复。预计至少约 60 分钟，加准备、真实重放、扫描和清理；模型准备不能挤占临界采样窗口，错过窗口须明确失败或 blocked。

CP032 检查真实已知活动、未提供覆盖、后来缺字段和必要前台失效，满额保护与请求复核。真实普通关闭须从原 Harness 获得 Written/Closed；原 Node 必要提交故障时仍保额，同原集合恢复只返一次。Zero、部分写入和 flush 未知为原 Node 所有、绑定原连接和原 ticket 的测试故障，不称厂商自然故障。Unknown 使连接停止时，先核未确认窗口保额；取得可靠原 source 完整关闭及必要记录后允许协调返额，不能把已确认完整关闭后永久保额写成预期。

四项可使用各自专属 Node 目录并行，但真实进程/内存开销以实际 50 主树和厂商加载方式计，不能预报固定内存上限。模型请求与审批场景也可能持续占资源；必须由原 Node/Windows Job 收回。并行会放大资源需求；设备无法可靠承载时改顺序执行，不缩减数量或计时。短故障开发组与真实长时对象分开，防止开发清理影响长时计时。

## Web 与最终结论

Web phase 信息只写本次自有目录；随机 phase ID 绑定原 Node、source、URL、native IDs 和 operation IDs。实际浏览器应读取该 phase 信息，通过真实页面操作，对同一原 Harness 的根/子正文、计数和限制取证。不能用受控 peer 的历史 browser-live 代签真实 Harness 页面。

Web helper 只收集 capture、actions、source 和 summary 的原文件及 SHA256，撤销本次新 browser 身份。它不会因收到 receipt 就判页面通过。禁用 Web 允许继续 Node 分支取证，但 Web 分支保持 blocked。

原结果为不可覆盖的 format2 `capacity_l6_execution`：绑定程序/源码/config/分支矩阵摘要、原 Node/source/URL，逐项真实节点分支、Web pending、实际 cleanup。只有节点分支齐全、入口已实现、全部清理确认且仅待独立 review，才可产生 `pending_independent_review`；它仍不是完整 CP passed。

独立角色必须实际核对每个 Web phase 的 capture/source/actions，再按 finalizer 的明确确认流程写新的 reviewed aggregate。原执行结果、原 exit 和 timeout 记录保持不变。手写 receipt、空 DOM 声明、legacy format1 结果或自由文字 assessment 不能抹去缺失的节点分支。

`cleanup.json` 必须证明原 Node 下所有当前及重启前 owner 的实际 join、必要元数据、非空 source 诊断、完整 Job activeProcesses=0 和端口关闭，并与 execution.binding 相同。正常失败也走 finally 清理；测试自有触发器、关系故障和配对身份均须收回。

## 本轮入口编排与执行界限（2026-11-17）

真实入口的管理客户端按每个认证连接至少520毫秒间隔发命令，保持任意滚动60秒低于产品命令上限。它不修改限流配置，不重试未知 mutation。target.register 只建立目标身份；需要执行时再经正常 Catalog 上传/register/apply 完成 bind_target_initial，核原 profile/revision/workspace/launch_path 的必要事实。配置动作被接受不等于模型、审批或子工作事实已经出现。

最后名额验证在原 Create 的真实第50预留提交后暂停；原 reservation/native=NULL 已占额，实际 execution/replay 必须 Full。放行后核原响应绑定，再分别通过原 accepted 队列的 Written/Loaded/插入确认及实际 Replay Complete 验证正向加载。这两个恢复步骤为顺序成功；完成后队列已无 accepted 项，不能再用一个额外 ensure_target 调用作原执行是否成功的判据。显式释放前核原输入/队列收敛。

发送前复核使用仅 cfg(test) 的 AutomaticReleaseBeforeBegin：原自动操作已选定、尚未开始 SQL 事务时暂停。WSS Create 自身的前置必要登记先正常完成；此暂停不持 SQLite 连接，也不供给活动。只有实际 Harness 当前工作通知和接收时间推进才可继续；释放后仍须原操作 Withdrawn、零 close、原名额保留，最后从新收到的结束事实重新取得资格。候选时钟探针是上次重评缓存，不能把未重评的旧缓存当成刚收到工作时已经重新判断的结果。

重启核原 owner 的真实 shutdown report、各 source 必要关闭、Windows Job 活动进程0和原端口实际 ConnectionRefused；累计 registered task/job slot 数只记录，不能当未 join 数。端口拒绝观察有界10秒，超时/其他错误不算端口已关闭；只复用原端口，不随机回退。

短动态检查只运行非 ignored 的受控 Node/TLS 组，实际命令和源码/程序/peer摘要见 tester 的固定证据。组包括 admission、boundary、restart、ticket、pacing，以及 child observer3、Web证据工具7、cache1、reservation cut1。受控 peer 按标准 V2 原RPC返回 messageId 插入确认，随后真实 Running→Idle/end_turn；它不是真实 Harness，不产生模型费用。历史只有重复 Idle 或缺少插入确认的失败保留，不能把它们叫产品完成故障。

四个真实 ignored 名称、50及1800秒常量保持不变。真实专属身份历史为 AuthRequired，当前没有执行真实四项、真实长时保护或真实浏览器 DOM。节点/工具动态结果不能代真实 L6。真实状态/子任务/审批未发生时仍精确 blocked；入口不能因为配置里写了相应动作就认为事实成立。最终4.1准备结论须核固定补丁、矩阵、清理和独立入口审查；4.2仍要求完整真实执行，不能由本轮受控结果勾选。


## 2026-11-18 CP-011 双层验收边界收口

- **层一（真实 Codex 宿主与物理单调时钟层）**：由 `0caf8143` 与 `testing/runs/deliver-session-capacity/tester/2026-11-18-l6-cp026-remediation-v1/ASSESSMENT.md` 固定：68 分钟 `capacity_l6_cp026_real_replay_clock_and_pressure` 实证近 29 分钟重放刷新时钟（`1740010ms`）、纯缓存 141 读 `1830071ms` 回收、重放后 `1799008ms` 存活与 `1828811ms` 超时释放、以及 116 Job 槽位 / 12 任务槽位 / 端口 `51651` 完全清理。
- **层二（50 会话全量闭环与活动保护状态机层）**：在同步 `capacity_automatic_tests.rs` 断言后，基于统一测试二进制 `acd38a1dc11b2dd2685d4776514984e2880fccab5a048f301a16a7057f7bf606` 与标准 ACP V2 受控对端，在真实 Windows Node（TLS WSS + SQLite + Windows Jobs）上通过 4 项重型 L6 编排（`310.77s` / `358.79s` / `70.53s` / `63.57s`）、35 项非编排 `capacity_l6_`、33 项 `capacity_rb001_`、35 项 `::rt_` / `race_acceptance::rt01` 及 96 项状态机与 SQLite 存储验证；全量归档见 `testing/runs/deliver-session-capacity/tester/2026-11-18-l6-control-plane-e2e-v1/ASSESSMENT.md`。
- **外部配额约束声明**：上游 ChatGPT/OpenAI 外部账户配额至 `2026-12-10 11:44` 限额与 `gpt-6.1-sol` 模型限制属于外部基础设施边界；4 个 `#[ignore]` 真实 Harness 入口保留作为未来外部配额恢复后的专项验证入口，不修改其常量与准入守卫。
