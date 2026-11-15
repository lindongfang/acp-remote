# ACPI v4 候选独立只读复审

日期：2026-11-08  
**判定：BLOCK Root 合入与真实 WSS 编码。** v4 四份候选相互引用和主要验收定义已基本闭合；剩余阻断是 Root 当前 OpenSpec/仓库流程要求的设计资产未纳入写集，以及对测试用例接收与必要观测 seam 的编码前决定尚未作出。

## 已复核文件与摘要

- `WRITESET-v4.md` — SHA-256 `147B70D2E6E2FBC85C51F68841EAD47A349FC49BFB239F2816FA11B6D65EC437`
- `spec-candidate-v4.md` — SHA-256 `E450C05ED0FE24D87A31264137441A3EF597CD0FB9E40234B13A162EAC39976A`
- `tests-candidate-v4.md` — SHA-256 `88C97333952D738F60D1FC44626D4755184E0FE61A2A7B08E962EAFC8AB6C773`
- `CONTRACT-v4.md` — SHA-256 `6BD506F56EC4B5AE7BA318E75C004C96463D9B692CF42F8998ECE6BA6B502C69`

未修改 Root、spec、产品源码或 Git index；未运行构建或动态测试，未建立源码副本或 target。

## 交叉绑定

四个当前 v4 文件的**路径和责任交叉绑定一致**：`CONTRACT-v4.md` 把 spec 候选和完整测试定义分别指向 `spec-candidate-v4.md` 与 `tests-candidate-v4.md`；`WRITESET-v4.md` 规定新增 spec 内容取自前者、tests.md 追加后者全文，并将 proposal 变更严格限制为一条能力登记；测试文件声明其权威行为来源为新 capability spec。v4 对 v3 findings 的引用 SHA 也与已复审的 v3 独立报告一致。

这些关系是文件名绑定，未在四份文档内记录各 v4 文件的内容哈希。此报告将上述四个输入 SHA 固定为本次审查对象；任一文件变更后，本次结论不自动覆盖新版本。执行合入前应按这些摘要确认实际复制内容一致。

## v3 findings 与 R01–R08

- **proposal 写集冲突：已修复。** v4 明确允许且只允许一行 `management-input-upload` capability 登记，并保护其他 proposal 内容。
- **spec/test 分散：已修复。** tests 候选独立包含 R01–R08 全文，不再从 C 盘 v2 映射补位。Root tests.md 当前尚无这八例，v4 写集要求追加全集，范围一致。
- **R06 stop 先后：已修复为可执行时序模型。** 先真实发送错误 ACPI 帧并在 `BeforeInputDispatch` 暂停，随后触发 Node stop，等待 stop 可观察后释放 barrier；以处理前 stop 复核、dispatch 计数零和唯一 1001 验收。它明确不向已关闭 socket 再发帧，也不将同时 ready 的 select 分支顺序当作证明。格式错误先处理的反向用例分别验 4400。
- **R06 cancellation：定义已足以指导测试 seam，但仍需实现后证明。** 明确取消在 owner 调用前、调用返回后/结果 enqueue 前，以及 accept_data 返回后/ACK enqueue 前暂停；区分 peer Close 和 Node ConnectionStop，说明 pending future drop、owner seal、receipt registered/join 的观察次序。新 barrier 不得跨业务锁停住。若实现无法令这些边界稳定可观察，相关 case 必须受阻，不能用计数零冒充 join。
- **R07 背压与 FIFO：静态可执行。** queued slot 与 writer 已取出的 in-flight frame 分开计数，ACK in-flight 仍占 32 byte permit；256 queued 加 1 in-flight 后由下一条真实幂等 Data 尝试触发 frame saturation。byte saturation 采用 `MAX_OUTBOUND_BYTES - 31` 的原 semaphore permit 注入，清楚标成受控资源故障而非自然慢网事实。与 spec 的 256 frame、1 MiB、32-byte 预算和单 writer 语义一致。
- **R08 revoke：静态可执行。** revoke 发生在 ACK 尚未通过 writer actor gate 时，预计 4410/无 ACK；反向顺序让已 gate-authorized send 完成后再 commit，不在持 gate 时等待 revoke，避免自锁。与 spec 的撤销线性顺序一致。
- **R01/R02/R03/R04/R05：覆盖匹配。** service bundle/control-only、四命令和真实 Binary ACK；幂等末块重试/冲突；preparing/ready/failed；release 幂等及双连接隔离；ACPI/ACPV 分类、格式边界、32 KiB 上限均有独立操作和观察结果。

以上判断针对“若列明 test-only seam 被按合同实现，测试定义可操作”的静态可执行性，不代表 fixture 当前已含这些 seam或用例已执行。候选五叶约束本身允许这些 seam；实现后仍须独立 tester 和真实 TLS/WSS 运行结果。

## spec 与 tests 一致性 / 权威合同

未发现 v4 新 capability 与 Root `session-execution` 或 `harness-management` 的直接行为冲突：upload/ACK/finish 被明确限定为接收和准备事实，不等于队列受理、派发或 Harness 成功；受理后节点接管等队列语义仍由现有规格负责。R01–R08 均能映射到 v4 spec 的 requirement/scenario，测试对 `body_unavailable`、`product_not_ready`、4400/4410/4429/1001、释放后 status、ACK FIFO 与 permit 回收的预期与 spec 相符。current Root wire envelope 已使用 `result`/`error`，与 v4 JSON envelope 一致；ID 和十进制字符串规则也有现存严格解析实现依据。

需保留两点非阻断验收注意：

1. v4 错误表包含 `allocation_failed` 和 `necessary_storage_unavailable`，R01–R08 没有确定注入这两类业务错误的场景；若该映射要求被作为路由验收证明，应增加最小确定性错误注入，或明确这些映射只由 codec/owner 层合同覆盖、WSS 不要求独立故障注入。
2. `WRITESET-v4.md` 将 Root 合入限定为 proposal/spec/tests 三项，排除了 change `design.md`；而 Root 的 OpenSpec config 要求 design artifact，仓库 AGENTS 规定变更产品行为/接口时同步维护设计、规格和验证资产。当前 `design.md` 未检出 ACPI 路由、ownership、writer/backpressure 或关闭决策。v4 合同内容不能自动替代 Root change 的 design artifact。因此按当前授权范围，不能把 spec/test 增量视作已具备实现所需的完整、同步设计依据。

## 放行状态与未闭合项

**BLOCK 当前写集合并及编码。** v4 技术候选已显著闭合前两轮阻断，R01–R08 和 R06 时序定义本身可接受；但写集仍缺 `design.md` 的最小同步更新，违反当前 OpenSpec/仓库维护要求。需要将负责 ACPI 路由设计的最小增量纳入经过审查的写集（或明确其内容已在另一权威 design 中，给出可核验路径），并更新候选内容摘要后再复审写集。另需由独立 tester 对 R01–R08 测试定义作接收，尤其确认新 barrier/passive observer 的观测不会改变生产语义；这不是作者自述可替代的独立接收。

在这些项目闭合前，不能据 v4 进行 Root 合入或授权真实 WSS 接线编码。此次报告本身不代表独立 tester 接收、动态验收或产品实现。
