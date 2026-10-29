> **当前独立结论更新（R20 跨 SID 收口）**：产品源码、安全与供应链检视 PASS；NA012 最后真实不同 SID pair/stop 缺口现已由原 platform tester 完成，reviewer 独立核读正式报告、17/17证据摘要及清理/复用链后接受 **NA012 PASS，最后环境阻塞 Closed**。详见 [cross-sid-final-review](../../../testing/runs/establish-node-access/2026-10-30-cross-sid-final-review.md) 与 [原 tester 正式报告](../../../testing/runs/establish-node-access/2026-10-30-cross-sid-r20-acceptance.md)。下文“当前仍NA012 BLOCKED”等为此前检查点，保留历史，由本段取代。唯一33例总表由 primary tester 合并；plan/tasks/docs 与新 ready metadata 由主方同步后再有限读回，本索引不代签其尚未完成动作，不执行归档/提交/推送。初次 watchdog=true 与后续两次 absent 分开保留，不追溯改写初次事实。
# 独立方案与用例检视

当前索引更新：**2026-10-30，R20普通CLI修复；backend/Web复用R19/R18未变范围**。**产品源码及负责独立检视范围PASS；NA007-CANCEL-001已由原提出tester在R20真实TTY复测Closed并由reviewer核读接受。整体仅NA012另一真实SID剩余命令环境/证据BLOCKED，不是ready或待归档批准。** 本文件不维护或代签tester最新33例总表。

最新收口结论：[reviewer-final-status](../../../testing/runs/establish-node-access/2026-10-30-reviewer-final-status.md)，原问题关闭依据为[原提出者R20真实TTY复测](../../../testing/runs/establish-node-access/2026-10-30-approval-cancel-r20-retest.md)。此前[reviewer-closeout-checkpoint](../../../testing/runs/establish-node-access/2026-10-30-reviewer-closeout-checkpoint.md)中NA007未关闭是历史状态。当前唯一逐例结果引用[tester统一表](../../../testing/runs/establish-node-access/2026-10-30-current-independent-status.md)（须其维护者接入此新Closed），正式Web证据合并见[web-evidence-merge](../../../testing/runs/establish-node-access/2026-10-30-web-evidence-merge.md)。本页不复制重签总表，旧阶段数量不再用于当前判断。

### 当前结论的层次（R20续接核对）

| 范围 | 独立结论 |
|---|---|
| reviewer负责的产品源码、安全/供应链及已执行定向复查 | **PASS（限定范围）**，未发现本轮新的需修源码问题；R20中断恢复的源码/helper通过不等于原TTY FAIL已被动态关闭。 |
| NA007原问题闭环 | **原提出tester在R20真实TTY复测Closed，reviewer接受**。control/committed/precommit/同key恢复原始证据30项摘要独立匹配；旧R17失败原文保留，不将同一旧报告Open覆盖最新复测结论。整例总表仍由原tester归并。 |
| NA012剩余平台资源 | **环境BLOCKED**：另一真实SID pair/stop等未覆盖原命令，不能代验。SMB remote-reject已满足有限原文，不加另一机器门槛。 |
| 整体交付 | **BLOCKED，非ready**；当前仅NA012资源/必要证据未满足。不要求用户重复已完成的技术验收，最终metadata同步不等于NA012已通过。 |

本次再次读回R20 source scan结果为实际`[]`，摘要仍为`37517E5F3DC66819F61F5A7BB8ACE1921282415F10551D2DEFA5C3EB0985B570`；此前独立重扫exit0证据保持，不为本次索引文字重复扫描或全suite。原NA007报告本次读回摘要为`ED608964CF78CB61E8FC6C7C314F7FA00F9EA47943EFEEFDDEB5A44241E85358`，仅作为此检查点，不预判其正在执行的新复测结果。

后续R20原复测已落盘并完成：新报告SHA256 `477D8CD47F8FA662B950FD39EA65C82B896CF372D588E08F2E1E31A877D1A2AB`，30个证据文件独立重算全匹配，四结果标志全true。上段“不预判”是前一检查点；当前按新Closed，不删除原FAIL。4.4长期规格/用例/编号定义检查已实际exit0，仍需统一表与plan纳入新Closed并固定最终metadata清单，见reviewer-final-status；不要求NA012先PASS才能准确同步其环境BLOCKED。

最终metadata清单应在原NA007报告、统一表及本索引最后一次结果更新后另行固定，再交reviewer有界核对路径/条数/摘要与结论一致性；当前不需要提前冻结一份马上失效的清单。它须与R20非元数据80项/冻结83项的职责分开，不把新plan/tasks/review或README/docs、追加报告写成旧摘要已覆盖，也不把清单自身纳入其条目造成自引用。

**历史阅读边界：下列设计表、71+22项快照、`4 PASS / 25 PARTIAL / 5 NOT-RUN`、旧NA-033/034及下载受阻等，均为2026-10-29当时记录。** 其中“当前”“最终”“待验收”等词只指该历史阶段，不能当作R20现状。历史发现、证据、统计与编号原文保留；2026-10-30的闭环及剩余范围见文末“当前R20检视闭环索引与整合条件”。

## 历史设计与阶段状态（2026-10-29，以下原文保留）

状态：本快照方案与用例设计检视通过，NA-SEC-01–08及NA-TEST-01均由独立reviewer复查关闭；不构成本轮实施授权或产品验收通过。审查者为独立 reviewer，不继承 coder 对话；只读产品设计，仅维护本报告。审查范围为本 change proposal、specs、design、access-contract、tests、plan、tasks，以及 AGENTS/config/reviewer 职责。独立 tester 已在当前固定版本完成 NA-032 受信 TLS/Tailscale/Chromium A/B 正常闭环；整例仍为 `4 PASS / 25 PARTIAL / 5 NOT-RUN`，不能据此宣称完整产品验收通过。

## 历史设计问题闭环（2026-10-29）

| ID | 摘要与复核依据 | 状态 |
| --- | --- | --- |
| NA-SEC-01 | 合同§1明确可信TLS与本机终端为引导，短码不独立充当身份锚，既有节点公钥禁止静默替换；移动端已依用户决定移出本期。 | 设计关闭；真实TLS/替换拒绝待验收 |
| NA-SEC-02 | 合同§2–4明确lookup/challenge传递requestId、每用途typ、配对和WSS分别闭合字段集合、私钥持有证明及新鲜一次性挑战；02-A/B已复查。 | 设计关闭；固定向量及互操作待验收 |
| NA-SEC-03 | 合同§6定义owner SID、DACL、token模拟/revert、远程管道拒绝、独占锁、DPAPI及初始化恢复。 | 设计关闭；Windows平台效果待验收 |
| NA-SEC-04 | 合同§4将COMMIT、代数更新与队列失效纳入目标身份同一串行临界区，定义实际发送边界与在途字节限制。 | 设计关闭；调度竞态待验收 |
| NA-SEC-05 | 合同§6及NA-031将即时自检与真实跨关闭持久性分开，不承诺识别所有私密模式；丢失身份明确重新配对。 | 设计关闭；真实浏览器待验收 |
| NA-SEC-06 | 合同§3禁止有效/撤销公钥重复登记，重新配对必须新key，撤销记录不自动清理。 | 设计关闭；反复登记待验收 |
| NA-SEC-07 | 合同§3/5/6区分300秒配对流与30秒短IPC；分页限制32条、游标60秒、revision失效；revoke幂等结果24小时/10000条，不驱逐未过期项。07-A/B设计已复查。 | 设计关闭；分页用例已补NA-029，见NA-TEST-01 |
| NA-SEC-08 | node-access规格明确网络与OS认证本机控制边界，并有不同用户拒绝场景。 | 设计关闭；真实隔离待验收 |
| NA-TEST-01 | 初次缺合同§4分页/游标及通知合并测试；独立tester已补NA-029的>64条、32条分页、跨身份/节点/篡改/到期、revision变化及只推revision，reviewer实际读回步骤/预期。 | 用例设计关闭；产品未执行 |

## 历史方案与任务核对（2026-10-29）

两份spec的Requirement/Scenario均能在tests找到对应引用及步骤，正常、失败、并发、取消、重启与身份损坏已显式涉及。新增分页行为的用例覆盖缺口已复查关闭，不以文件存在冒充完整覆盖。plan包含并行评估、隔离写范围、共享路径串行合入、适用检查建立任务、固定版本与独立角色；tasks覆盖WP1–WP6且逐项有完成判据。设计评审当时产品工程入口尚不存在；后续实现虽已建立入口，仍不能把入口存在当作产品验收。尚未审签依赖维护/漏洞、真实Windows与浏览器、密码学互操作、人工产品验收或任何产品能力。

## 历史固定实现快照（2026-10-29）

实现、规格、合同、用例、Web/Rust 工程、验证资产和 Windows 实施说明的固定清单见：

- `testing/runs/establish-node-access/2026-10-29-snapshot.sha256`
- 清单 SHA-256：`6B1D981658CE1ACA98D390B3C56650908924B0A7486A55A87836DEEC74200BB9`
- 覆盖 71 个未忽略交付文件。排除 `.tmp/`、`target/`、`node_modules/`、`dist/` 等生成/私人资料，以及 `plan.md`、`tasks.md`、本报告、独立 reviewer 报告和清单自身这些会随执行状态维护的文件。
- 配置补充清单：`testing/runs/establish-node-access/2026-10-29-config-snapshot.sha256`
- 配置清单 SHA-256：`13AD9DDE14AFC5235217CBA748B8EAB830162F56625FF4183BAB386172DFB3F1`
- 配置清单覆盖 22 个根级配置、流程规则和产品设计文件；与 71 项源码清单无重叠，两份清单合并后共同界定 93 个固定文件。

复核命令：

```powershell
$manifest = Get-Content testing/runs/establish-node-access/2026-10-29-snapshot.sha256
$manifest | ForEach-Object {
  $hash, $path = $_ -split '  ', 2
  $actual = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash
  if ($actual -ne $hash) { throw "hash mismatch: $path" }
}
```

独立 reviewer 必须在同一清单上复核实现 diff、安全边界和验证结果；本报告的设计问题关闭结论不能替代本次实现检视。任一交付文件变化都要重新生成清单并由 tester/reviewer 明确是否重验。

## 独立交付检视摘要（2026-10-29）

- 报告：`testing/runs/establish-node-access/2026-10-29-independent-reviewer.md`
- 清单：71 项源码清单逐文件校验 `71/71 OK`，自身 SHA-256 为 `6B1D981658CE1ACA98D390B3C56650908924B0A7486A55A87836DEEC74200BB9`；22 项配置清单逐文件校验 `22/22 OK`，自身 SHA-256 为 `13AD9DDE14AFC5235217CBA748B8EAB830162F56625FF4183BAB386172DFB3F1`。两份无重叠，合并为 93 个唯一文件。
- 历史：前轮 70 项源码清单摘要为 `9FD12D64713627666AE0BC9D8AB7BABCF356EC2DF408AE30F409450BD8436965`；前轮 Open 结论保留在独立 reviewer 报告中，不与当前 71 项快照混用。
- `NA-REV-001`：Closed。订阅后立即检查当前 stop/failure 状态，提前状态不再丢失。
- `NA-REV-002`：Closed。stop 响应写错误会传播到最终 shutdown 错误；客户端断开不再被报告为正常关闭成功。
- `NA-REV-003`：Closed。全部响应结果被收集，仅共同 deadline 中止未完成投递；资源清理结果只发布一次，投递错误不翻转 watch。
- `NA-REV-004`：Closed。类型化 `Capacity` 映射为 `4429`，`Protocol/Utf8` 映射为 `4400`；32769/32770/65536 字节真实 WebSocket 回归通过。
- `NA-REV-D01`：Closed。合同对依赖、构建入口和验证入口的陈旧描述已纠正。
- 配置范围：`NA-REV-S01` Closed；两个摘要共同覆盖源码和根级配置，但仍不得据此声称 CI、依赖安全或产品行为通过。
- 测试边界：reviewer 独立通过 003/004 定向回归、4 项 JWS 向量、fmt、Clippy、OpenSpec strict、Web 类型检查和 31 项 Web 测试；reviewer 受限进程全量 Rust 为 `115/118`，3 个 DPAPI 失败属于环境限制，不解释为代码缺陷。
- 最终 tester 证据：报告 SHA-256 `074180AE7C49E66D5F49731BE9C70BAC502A27082B0C9E40E5BF3654D411BC7F`，绑定当前 `71 + 22` 摘要，在真实 `faraday\jack` 下完成 `118/118 + 4/4`、fmt/check/Clippy/build、精确回归、Web/OpenSpec/npm audit、真实节点生命周期、pipe 写失败、跨 SID 基础拒绝和受信 L6。
- `NA-REV-T01` Closed：最终报告继续采用“完整用例全部必要断言通过”定义 PASS，逐例统计 `4 PASS / 25 PARTIAL / 5 NOT-RUN` 与正文一致。
- `NA-REV-B01` Open（已收缩）：`4 PASS / 25 PARTIAL / 5 NOT-RUN`；PASS 为 `NA-001/017/019/032`，NOT-RUN 为 `NA-010/020/030/033/034`。存储报告补齐 NA-007/027 的故障子断言，但 NA-007 仍缺批准成功后响应丢失/取消/浏览器恢复，NA-027 仍缺可控 24 小时、实际 `SQLITE_FULL` 和完整 WSS 审计组合。
- `NA-REV-B03` Open（已收缩）：真实跨 SID 的 data DACL、敏感文件、owner pipe 和 DPAPI 基础拒绝已实测；远程 named pipe、伪造 SID/服务端 token 校验失败、异用户实际 `device pair`/`daemon stop` 及敏感错误回显仍未闭合。
- 其余阻断：`NA-REV-B02`、`B04`、`B05`。`B04` 的正常 L6 子范围已关闭：NA-032 PASS，受信 TLS/Tailscale、独立 Edge A/B、重启持久性与完整清理已接受；仍仅剩批准响应丢失和 NA-031 私密模式/写入失败/坏记录/清站点/丢失提示/新 origin 等异常场景。`B02` 为 NA-033/034 人工签署，`B05` 为 Rust advisory/Gitleaks/许可证与维护状态。
- `NA-REV-N01` Open（Low，非当前配置阻断）：`127.0.0.1:0` 被配置接受并实际绑定，但 `NodeRuntime::listen_addr()` 返回 `:0`；正式配置使用显式非零端口，需解冻后修复或拒绝该输入。
- `NA-REV-P01` Closed（Medium，仅探针资产）：修订版在生成 secret 前收紧 `data-run` DACL，私钥再受文件级 owner-only DACL；failure/timeout 在取消业务体后显式关闭 SQLite pool、stop/join，资源释放断言通过后才删除目录，未确认关闭时保留受保护目录并非零返回。DB connect/close 不在业务 timeout 内，stop/join/delete 故障与 OS 强杀仍未实测，作为残余风险保留。
- 供应链扫描：Gitleaks 8.30.1 与 `cargo-audit` 本轮均因下载超时未取得可执行结果，不能写成扫描通过或用户拒绝。
- 结论：`BLOCKED`，不可进入待归档阶段，但当前版本可作为用户人工接棒的交付基线；人工 fixture 端口 `30447` 和 `.tmp/establish-node-access-human-20261029` 保留。tester 报告追加或修订后必须以新哈希重新评估，不能用测试数量、存储子断言或主要断言代替完整用例验收。

## 当前R20检视闭环索引与整合条件（2026-10-30）

### 源码、产品及元数据范围

- 最新R20：`testing/runs/establish-node-access/2026-10-30-implementation-r20.sha256`，SHA256 `73727BCF617F3F96BCC24C4CBB7B38BA044A56856C6CB4D628869832C8B368A4`；reviewer对冻结副本83/83匹配，普通binary复制后摘要 `561A6024E983DD5239C09D1388CA5D256625F690EE9345E64B9654BC43AAFBE3`已核对。CLI decision后等待和owner list恢复提示为普通运行时变更，须原tester采用R20产物，不能用R19 binary代验。
- 新静态资产副本 `.tmp/current-source-scan-r20-20261030` 的80项非plan/tasks/review文件与R20摘要独立匹配；713文件副本由reviewer用官方Gitleaks独立重扫0命中、exit0。三份metadata与本次新增索引/检查点另行固定，不能称已包含在旧83/713项中；细节见收口检查点。
- 前序R19测试包：`testing/runs/establish-node-access/2026-10-30-implementation-r19.sha256`，SHA256 `298EA007D4E1270E15D37DE93FC680A76A1121D452ECF2B96AD8F2E60B1B32C1`；reviewer对冻结副本83/83匹配，普通binary摘要 `377C780CB3FFEBB9936BA131DC46955A2EF3E455134DA56DE1E49805786C38A7`已核对。仅新增测试seam/回归及当时元数据，运行时/Web保持R18；最终新勾选/本索引增补另由metadata清单界定。
- 前序R18清单：`testing/runs/establish-node-access/2026-10-30-implementation-r18.sha256`，SHA256 `614413C0243680E6E7E0DF3C98141F0D568DDCAA042209EB9C996A99881E5B8A`；reviewer对R18冻结副本83/83匹配，普通binary摘要 `328D7CEBCA708A300D48AA673204E4E52F1131E6E05951F85DA4601DA7AA2800`已核对。R18晚到共享replacement调用方分支定向检视通过，R17未变范围按下列报告复用。
- 固定源码清单：`testing/runs/establish-node-access/2026-10-30-implementation-r17.sha256`，SHA256 `5074890C209A702335F13042A26DBA64278EFD7A7B0A70A4E42E2F698BA3E26F`；reviewer对 `.tmp/frozen-implementation-r17-20261030` 独立核对83/83。它不声称覆盖后来修改的README、平台说明、plan新检查点或本review元数据。
- 普通R17产品：SHA256 `38AA9D02F334E3C81FDC900ED4F8943BB78339650981BC0046A609381D03E6C2`，reviewer复制到自有目录后核对；generated Web dist另列，不伪称是83项原清单内容。
- 完整143库+4向量/Web50、类型/fmt/Clippy/build为已读开发日志；reviewer独立执行范围为各下列报告所列定向回归、真实产品资源闭环、Web50和受控模块复现。两者不混记，不以数量代替需求断言。
- 当前用例为NA-001–033；旧NA-033安全汇总已删除，旧NA-034映射当前NA-033用户理解确认，不能按同数字搬运旧状态。当前033用户已确认，不恢复旧人工签署阻断。R13只验证三个中文字段/定义及引用关系，不是行为验收。

### 独立闭环与范围索引

所有路径均为仓库内报告；旧Open/FAIL保留在原报告中，下列明确的新版本关闭结论优先，不把历史阶段统计搬成当前总表。

| 范围 / 稳定ID | 当前限定结论与证据 |
|---|---|
| 早期shutdown/WS修复 | NA-REV-001–004、D01、S01、T01、P01保留原关闭历史；对应当时固定版本和适配限制见2026-10-29独立reviewer报告。不把沙箱DPAPI失败写成产品永久缺口。 |
| 应用审批、挑战与资源 | I01已关闭；真实actor/共享trust与gate、零派发、RAII挑战scope/Weak/正常stop边界，见[implementation-r4-review](../../../testing/runs/establish-node-access/2026-10-30-implementation-r4-review.md)及其追加。无后续Harness真实审批issuer声明。 |
| CON001 / LIFE001 | 已在r5a独立关闭：真实reader受控输入不再压成y/yes，单线程Drop后不yield即拒绝policy操作；见同r4-review文末r5a复查。真实TTY由tester独立证据归属，不冒称受控事件等于TTY。 |
| NA009、挑战probe | [R6 review](../../../testing/runs/establish-node-access/2026-10-30-implementation-r6-review.md)：真实DPAPI子进程中止、精确临时长度、marker及重开稳定性13项，pairing12项和challenge22场景独立通过。不要求硬件断电。 |
| 存储故障与保留预算 | [R7 review](../../../testing/runs/establish-node-access/2026-10-30-implementation-r7-review.md)真实SQLite pager FULL/13回滚恢复通过，非宿主盘满；[R11/R12 review](../../../testing/runs/establish-node-access/2026-10-30-implementation-r12-review.md)满10000与86399/86400秒联动独立通过。与旧probe改expires_at和FK COMMIT失败范围分开。 |
| 平台五模拟失败 | [R8 review](../../../testing/runs/establish-node-access/2026-10-30-implementation-r8-review.md)：真实owner pipe、五线程局部模拟错误、真实revert及后续验证通过；不保证真实Win32永久Revert失败安全恢复。 |
| NA-RV-001 / 002 | [R9 review](../../../testing/runs/establish-node-access/2026-10-30-implementation-r9-review.md)原提出reviewer关闭：受理/普通结果/缓存入队与writer gate回归、首次订阅保留新revision实际通过；[revocation-gap-review](../../../testing/runs/establish-node-access/2026-10-30-revocation-gap-review.md)保留历史及其余有限G项，不能把已关闭ID继续当当前Open。 |
| R19 writer先赢 | [R19 review](../../../testing/runs/establish-node-access/2026-10-30-implementation-r19-review.md)定向独立PASS：真实writer已持actor gate，owner revoke待决且SQL仍trusted；释放后受保护sink.send成功计数+1，撤销完成，真实WSS合法result先于4410。补齐此前G01发送先赢子范围，不直接代签NA008整例。 |
| 限流/queue permit | [R10 review](../../../testing/runs/establish-node-access/2026-10-30-implementation-r10-review.md)隔离重建83项后HTTP9/management7通过，桶满排除global429遮蔽、permit回收有效。文字勘误：帧预算256，128是全节点管理连接数，见NA002报告。 |
| NA-022-001修复 | [R12 review](../../../testing/runs/establish-node-access/2026-10-30-implementation-r12-review.md)：五终态claim缓存、自然到期和批准/consumed最大留存15项独立通过；原tester HTTP失败报告保留，由其新版本真实复测另行闭环，不由reviewer改旧FAIL。 |
| 定义资产 | [R13 review](../../../testing/runs/establish-node-access/2026-10-30-verification-r13-review.md)：83项匹配，33例/28场景/2能力正控制与两项错定义负控制通过；不证明语义充分、执行或产品结果。 |
| IPC32/33及native背压 | [R14 review](../../../testing/runs/establish-node-access/2026-10-30-implementation-r14-review.md)实际32 active、第33拒绝及native254/code231恢复/stop取消独立通过；原NA-BUDGET-IPC001由预算tester用ordinary binary复测关闭，原FAIL和放弃的native33方案保留。 |
| NA002 / SC001–003 | [NA002总检视](../../../testing/runs/establish-node-access/2026-10-30-na002-security-supplychain-review.md)及[R15最终结论](../../../testing/runs/establish-node-access/2026-10-30-implementation-r15-review.md)：供应链/密码库/合同源码限定范围PASS。SC001精确allowlist、SC002依赖说明已闭；SC003已由原reviewer从自有源码编译普通产品，在自有dist隐藏/变更后index及JS/CSS仍一致，真实TLS节点stop/清理闭环。官方Rust扫描、npm审计、Gitleaks、真实commitlint范围可追溯，不再沿用“下载受阻未扫描”的历史结论，也不要求笼统完整法律鉴证。 |
| NA-R16-001 | [R16失败](../../../testing/runs/establish-node-access/2026-10-30-implementation-r16-review.md)保留；[R17 review](../../../testing/runs/establish-node-access/2026-10-30-implementation-r17-review.md)已由原reviewer关闭：迟到session/普通save均identity_changed，Knew不被覆盖；当前key正常save、缺失记录不upsert。真实多tabNA-E2E001已由原tester正式包复查Closed，依据见web-evidence-merge，不由reviewer改旧FAIL。 |
| R18 late replacement调用方 | [R18 review](../../../testing/runs/establish-node-access/2026-10-30-implementation-r18-review.md)限定PASS：真实main适配器R17 consumed负对照复现；R18 unbound保留配对、pending恢复轮询、approved/consumed进入管理，pagehide不继续，key/ID保持且无再次删除。DOM/传输为明确适配器，不代签真实浏览器E2E。 |
| NA007-CANCEL-001 / R20 | 原[TTY失败报告](../../../testing/runs/establish-node-access/2026-10-30-approval-cancel-boundary-acceptance.md)保留；[R20 review](../../../testing/runs/establish-node-access/2026-10-30-implementation-r20-review.md)源码/helper限定PASS之后，[原提出者真实TTY复测](../../../testing/runs/establish-node-access/2026-10-30-approval-cancel-r20-retest.md)已Closed。COMMIT后未交付事件时单次Ctrl-C先诚实提示，再经真实owner list确认同指纹ID/revoke命令；precommit未匹配只报未确认，control无信任，恢复关联通过。reviewer核读30项证据摘要，不代称自己重跑TTY。 |
| 最新预算子项 | [budget-acceptance](../../../testing/runs/establish-node-access/2026-10-30-budget-acceptance.md)末节含真实300秒pair流、32/33修复、真实128连接及R17慢writer两分支。reviewer亦独立执行R17慢writer；帧先满有明确cfg(test)command窗口适配，不冒称普通120额度下257命令通过；不能继续沿用报告旧“未运行”段。 |
| SMB remote-reject | [SMB范围review](../../../testing/runs/establish-node-access/2026-10-30-platform-smb-scope-review.md)：同owner、本机SMB loopback同UNC路径正控制双向成功、产品ACCESS_DENIED、本地ready和清理，满足当前NA012远程管道实效子断言。不额外要求另一机器，不冒充跨SID；旧平台脚注14为历史。 |
| 最新正式Web/管理合并 | 原tester[web-evidence-merge](../../../testing/runs/establish-node-access/2026-10-30-web-evidence-merge.md)采纳NA013/026/031/032正式同包链和异常子项；[统一表](../../../testing/runs/establish-node-access/2026-10-30-current-independent-status.md)亦已采纳NA008/027/028/029最新完整有限证据。旧G项、旧Open和旧阶段统计不覆盖其后真实执行；各report独立角色和适配边界保持。 |

### 仍待整合的具体条件（不是tester33例新总表）

1. 原tester已形成33例统一表并完成正式Web证据合并，NA-E2E001真实多tab、R18 late-consumed、NA013/026/031/032及管理各项按其最新报告采纳；NA007原提出者R20真实复测也已Closed。当前应由统一表/plan维护者接入此新Closed；不把旧“待执行”或旧FAIL累计当现状，不用本页索引数量重签整例总表。NA012环境仍阻断整体通过。
2. NA012另一真实SID的device pair/daemon stop等未执行原五命令子断言仍按对应报告保留；同owner SMB不能代验SID。NA008/027/028/029原有限G项已由原tester最新证据收敛采纳，不继续泛列为Open，不机械照抄旧4/25/5、全部调度跨积或已补的24h/FULL缺口。
3. **R18调用方源码及实际main适配器已定向PASS，原tester真实late-consumed补验亦已由最新合并记录采纳**。它不推翻R17条件保存/多tab基础修复，不要求重跑无限版本全链；整体验收仍由独立执行报告汇总。
4. 最终产品binary、源码固定清单与后续README/平台说明/plan/review等元数据清单须区分；旧清单不覆盖新增元数据节。复用依据按实际负责字节、平台前置及断言范围记录；普通运行时变更由tester采用对应新产物，无关文档/测试修改不使所有历史真实证据一律失效。
5. 新FAIL由独立提出者在修复版本复核；原FAIL/Open和历史编号映射保留。未验证项不能写通过；当前不宣布ready、不批准待归档，也不自动归档。

6. NA007-CANCEL-001已在普通R20产物、有界透明proxy/真实TTY的已提交/未提交/control分支复测Closed，不再列未执行；reviewer的helper错误/多页/5秒预算与原tester的快速真实查询/信任恢复证据分开。4.4定义/路径/编号同步子项已核对，剩余统一表/plan更新及最终metadata摘要核对；完成准确同步也不等于环境阻塞的NA012通过。

本次只维护review.md元数据，不改产品/规格/预期/plan/tasks或tester总表；没有为文档更新重跑全套suite，没有提交、推送或归档。
