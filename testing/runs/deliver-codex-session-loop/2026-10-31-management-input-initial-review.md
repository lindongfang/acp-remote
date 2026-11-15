# 管理输入冻结候选独立代码检视

2026-10-31，reviewer `/root/baseline_reviewer`。只读外部候选及主区合同，未修改、集成产品，未读取用户数据库/凭据或启动 Harness。

结论：**当前冻结候选不通过交付门禁，暂不合入。** 有两个独立实现问题：请求到期未约束保留正文的寿命，以及最后异步检查后遗漏共享管理故障检查。二者均属于已经授权合同内的实现修正，不需要用户重新选择产品规则。修改后必须另行冻结，保留原失败并由原提出者及独立 tester 复核。

## 固定输入与审查范围

- 合同 `management-command-contract.md` SHA256 `8686d7792f388e70ae7d48ff54fee1a1e5240432798f1bf6c8845fdd05b8bd99`。
- 独立 22 组 M 定义 SHA256 `009f48b045c047613fa17d7b04ab0e0877667c880b9d34b7313cdfb5be23dadb`；既有 `management-input-planning-review.md` 仅批准外部候选准备。
- 候选根 `../deliver-codex-session-loop-management-candidate/`，已实际读取 `HANDOFF.frozen.md`、manifest、patch 与源码；manifest SHA256 `a590e0e8d3ad57a6c659d41010db1c2b0406e9fef2c5806aa794312d97122a61`，patch SHA256 `c1ec6a1a0e0caeb81e303be326ab55a9a4139cf0a5930e851dacc5b2b397c6aa`，均独立计算相符。
- manifest 的 11 个源码摘要逐一计算，全部相符。主要应用源 `workspace/app/src/application/product_commands.rs` SHA256 `39b4b28f777c4f12949f8bbaefe5645054efcf16e06fa6d0a1a3bdca66abd247`；SQL 源 `workspace/app/src/session_storage/product_commands.rs` SHA256 `651746b7e8ff7ee0015cc4bc86db424199ce89e720b2ecc8f7ed3475616fc5ef`。
- 本报告审查 session.input 内层编排、可信端口、SQLite 元数据事务与一次资格。没有批准真实认证发行者、系统时钟装配、真实内容校验/writer、SQL/WSS 主区接线、最终 reconciliation 或恢复 ready。

## 未关闭问题

### MGT-CODE-001 — P2：ticket 保留正文的寿命未绑定请求 expiresAt

位置：候选 `application/product_commands.rs:926–928`；关联 `InputCell.deadline` 声明 363、初始化 498，以及 `prepare_input` 预留 1 小时和 `MemoryInputs::expire`。

`submit` 完成真实验签后，`request_deadline(...)` 只收紧本次调用的局部等待截止；`InputCell.deadline` 此后从未修改。通过 `prepare_input` 建立的对象仍以创建后 1 小时到期。合法旧 token 可以在自身 expiresAt 前数十毫秒受理；调用方保留所得 ticket、不 claim，就会在请求到期后继续保留正文，supervisor 的周期清理只检查原 cell deadline。

这违反合同 §3“期限上限不超过其请求 expiresAt”和临时对象到期清理要求；后续 claim/dispatch 拒绝过期不能替代已经保留正文的主动到期清理。修正须将已验证请求的期限绑定到节点拥有的输入对象，且只能收紧不能续期，保留 ticket/permit 不延长寿命；清理自身引用及预算，不删除命令/Run/审计事实、不补发。

必要独立断言：有效默认 24h token 接近 expiresAt 或合法短期限 token，上传对象原寿命更长，受理后保留 ticket（另覆盖 permit）并让请求及单调期限到期；运行监督清理后正文预算清零，之后原票不获得传输资格，命令责任保持。墙钟向前导致更早到期的策略也须明确并按可信时间执行。

已读取 request_tester 的 `management-targeted-initial.log` 和独立测试源码：该用例在原冻结副本合法 token 的 exp 前 30ms 受理，随后推进可信时钟并实际等 160ms，100ms supervisor 已有执行机会，实测 retained 为 `(55,1)`，预期 `(0,0)`，失败。不是 reviewer 自测；原日志和失败应保留。

### MGT-CODE-002 — P1：最后 await 期间发生共享故障仍可能首次写入

位置：候选 `application/product_commands.rs:1111–1124`、首次 transport 调用 1166；关联 SQL `ProductStore::final_fence` 仅在进入事务前 `require_healthy()`。

`final_fence` 成功提交后，应用还异步调用 `require_current` 和 `refresh_owner`。这些等待中另一目标可以发生必要管理提交失败/未知并使相同 `FaultSignal` 进入 sticky fault。等待恢复后，本调用只再校验 actor、owner、时间和请求期限，没有再检查共享故障，仍会调用 `transport.write_once`。因此“故障前持有 permit”可在故障已经发生后取得首次原生写入，不符合合同 §4/5 的故障关闭及零新原生调用要求。

修正须在最后异步资格检查之后、首次可能交字节之前重新检查共享管理故障；与一次消费、取消和迟到队列规则保持一致。检查到故障时不可调用 transport，按可靠零发送事实保留命令责任；无法确认结果记录时保持故障，不能恢复 ready。不能仅在 claim 或 SQL final_fence 的入口检查，也不能仅把故障写进日志。

必要独立断言：claim 已提交且 final_fence 已提交；在最后 `require_current` 或 `refresh_owner` 暂停，令共享 store 通过另一目标故障/显式可信故障通知进入 fault，再恢复。transport 调用/首次写计数必须为 0，返回拒绝，正文清理、资格失效、故障保持。已有启动故障及 dispatch 开始前故障测试不能替代这个交错。

已读取同一独立初始日志及源码：最终 refresh 钩子暂停、共享 store fault、恢复后 dispatch 返回 `Ok`，独立“必须拒绝”断言失败。初始日志的断言先于 writer 计数断言，故这里不把尚未输出的计数称为日志已经证明的值；源码显示恢复路径确实调用了计数 writer。待 tester 补充明确计数证据及原始清单。

## 已核查的实现边界

- `VerifiedProductRequest` 内移 domain 且保留只读 identity，adapter re-export；应用依赖自己的 `ProductRepository`，SQL 实现该端口，没有 SQL 引用 JWS adapter。`CurrentActor`、`CommandOwnership`、ticket/permit/receipt 构造字段不向外公开。可信 `InputAuthority` 和内容/时间/签名端口仍属于装配信任边界，替身不证明真实发行者已实现。
- SQL 每事务取得 detached 自有连接；未完成事务 Drop 保持故障，连接不回池。COMMIT 未明确成功不返回新 ticket/permit；测试故障入口区分真实延迟外键 COMMIT 失败与真实 COMMIT 成功但响应未知。超时及 Drop 不以查询事实自行复活资格。
- checkpoint 在 BEGIN 后读实际 DB floor 并采样，CAS 同 epoch/旧 floor 且要求 rows=1；过期检查位于去重前，可靠过期拒绝的嵌套结果使必要 floor 可提交。原 schema/旧正文路径未被新应用调用。
- 新受理原子写 command、Run、4 个固定 slot；专用 claim 接受自己的 WorkToken/request/run/awaiting_insertion，原生自发 active/unknown 会阻止自身输入并保留真实活动，不等待 idle 补发。
- 首次 writer 前 SQL 已提交；writer 受 2 秒及总截止/请求期限约束，Drop 取消发送仍是可信 transport 必须实现的端口责任。错误路径在保存 unknown 前释放 actor/owner；这不能替代真实 writer 的取消后零潜伏发送验收。
- audit 使用全 NULL 的一次 UPDATE 填槽，重复同结果不改时间，冲突拒绝；unknown 不覆写 completed。startup 保守标 unknown、不伪填 dispatch、不释放旧资源；发现遗留工作/资源保持故障，reconciliation 始终未发生，没有 reset-ready 入口。
- `Storage` 保留并 clone 同一 ProductStore，重复 accessor 不清故障。生产默认 100000 audit 边界的算术证明与合法路径至多 40000 槽有区分，不将测试缩限当默认自然累积通过。

## 保留限制与交付要求

本轮未自行执行候选开发 27 测试或完整独立 M 矩阵，不把 coder 的绿日志代签独立验收。独立 tester 的当前两项失败只关闭“未动态观察”疑点，不关闭问题本身；剩余 M 分支仍须其正式报告。

内存 `reserve` 当前按 length + caller-supplied extra 收费。真实网关接线前须明确 extra 包含哪些结构索引/分块引用/借出保留成本，以及容器自有固定元数据的计费边界；当前不能宣称 8×16MiB 正文同时保留等于包含全部 allocator/元数据的 128MiB heap 硬界限。这是接线范围说明，不把未固定的内部小开销另造用户产品裁决或作为本轮第三项阻断。

修复候选只交新冻结清单；不得覆盖此 a590/c1ec/39b4 原快照、失败日志或降低断言。原提出者复核两项后还需独立 M 验收及主区实际差异/完整检查，方可决定限定合入。完整 CL-004/018/075/076/079–084、L5/L6、真实 Harness/Windows 树、安全/供应链仍未签；用户暂不外发 Web 依赖元数据，npm audit 未运行。
