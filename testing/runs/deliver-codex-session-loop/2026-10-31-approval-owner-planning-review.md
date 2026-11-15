# 审批 owner 候选只读预审

2026-10-31，reviewer `/root/baseline_reviewer`。输入 `approval-owner-contract.md` SHA256 `9d0cc70c5447c6c161d41296b43d4241e5bca572a91f8f15990308f0bfd47820`；实际读取现行 permission-approvals/session-execution/management-audit spec、implementation-contract §8/8A 与 schema3 的 product_approvals/product_deliveries 实际 DDL。未修改或编码产品。

结论：**方向符合已确认产品规则，但当前只是预审，不允许据此编码或接线。** 尚无必须由用户裁决的新产品冲突。以下是可在已授权范围内补齐的工程状态机和验收定义，不应再次询问已经确认的无 Run 审批、原决定保留、安全响应或管理故障规则。

## 与规格一致的方向

- 当前真实 Connection/RPC/原广告是来源，无 Run、无工具主题以及后台工作均不凭最近 Run 借权；idle、Run 完成、load 返回不能批量失效审批。
- 原用户唯一决定、实际 selected/cancelled、write/flush 事实和原生执行结果分别表达；不把安全 cancelled 当第二个人工裁决，不永久保存完整审批或工具正文。
- 新 metadata-only delivery attempt 能弥补 schema3 仅有交付 state 无法明确“原 allow、实际 cancelled、传输未知”的表达缺口。采用加表迁移属于落实现有可追溯规则的工程选择，不是必须新增用户产品授权，也不表示只有这一种布局可行。当前 SCHEMA_VERSION 保持 3 正确；布局/验收冻结前不得修改。
- 决定和危险交付必须必要提交确认、当前 actor/资源/请求及原广告复核；可能发送后不重发。存储故障保留协议安全收敛路径，与允许派发资格分开。

## 编码前工程问题

### AP-PLAN-001 — P2，open：唯一 attempt 与最终实际响应的冻结/改选边界未定义

候选 §3 规定每审批只有一个 attempt、记录实际 response_kind；§5 又规定 claim 时创建 attempt，随后最终验证若请求失效则抑制已保存 selected 并按协议安全 cancelled。目前没有明确 selected attempt 已提交后、可靠零发送的最后检查失败时，如何使同一 attempt 表达实际 cancelled，以及如何保证可能已经写入后不会改写响应事实或再发第二个响应。

需要冻结具体状态机：什么阶段字段代表计划、什么阶段冻结实际响应；若允许零发送前转换，列出 CAS 前态、同一 claim/owner 证明、option_digest/reason 变化及投影同事务条件；可能交字节后禁止转换为另一个实际响应，不把 unknown 变成新发送机会。无实际待答复 RPC 的 suppressed 与“cancelled 已尝试但未知”也必须可区分，不能写一条 claimed selected 后实际 cancelled、再靠 unknown 猜测。

必要测试：原 allow 提交后、claim 前失效；claim selected 提交后/第一字节前失效；第一字节可能写后取消或故障；claim COMMIT 成功但响应丢失。逐项检查原决定不变、attempt 的实际 kind/reason/option、投影及至多一次 writer。两 pool 与重复回调不能转换出第二次发送。

### AP-PLAN-002 — P2，open：管理故障时安全 cancelled 的资格与记录失败路径未定义

§3/4 允许观察或决定 COMMIT 失败/未知时安全 cancelled，§5 的通用 claim 却要求创建持久 attempt/dispatch 槽并明确提交后才给一次 permit。观察未提交时甚至没有 approval 持久行，attempt 的 FK 不可能成功。不能套用危险 permit 使安全取消无限等待存储，也不能把故障当成跳过真实连接/RPC/已答复检查的通用绕过。

需要单列节点协议 owner 的安全响应资格及状态：由真实待答复 RPC/连接和运行/资源代际发行，只能 cancelled，不能任意 selected；对持久观察缺失、决定/claim 提交未知、attempt 已存在、从未尝试与可能发送分别处理。无持久行或写不成功时保存有界内存事实并明确记录未确认，不伪报可靠登记/决定/跨重启可恢复结果；资格消费或 Drop 后迟到队列不得发送。保存结果不依赖原 actor 仍可信，但远程故障控制仍须符合 spec 的当前操作者/目标校验，内部协议收敛与远程命令不能混为一个 API。

必要测试：观察失败无 approval 行；决定回滚与真实 COMMIT 成功未知；claim 回滚与真实成功未知；安全响应自身记录失败；原 actor 撤销、源失效、已答复和可能 selected 写入后收到取消。记录不可用时仍能在绝对预算内安全结束有效待答复 RPC，所有路径零 allow、至多一次实际响应；已可能发送时不补第二 cancelled。

## 其余必须冻结的细节及测试输入

- schema4 需单独完整 DDL：STRICT、各字段范围/规范编码、selected/cancelled 与 option/reason/状态交叉约束、approval/claim 及原决定的关联与唯一性、suppressed 的有无 attempt 语义、不可覆写真实响应证据。仅在描述中写 canonical128bit 不等于 SQLite 已有规范校验。
- 明确 1/2/3→4 精确顺序和旧版本识别；尤其 schema3 的 request_epoch/floor、已存在 approvals/deliveries/slot 状态完整保留，不能 bootstrap 重置。R002 缺身份拒绝、R003 完整 WAL 读取及允许辅助文件的限定边界继续适用。安排必要写点、双版本、真实 deferred-FK COMMIT 失败/成功未知的独立迁移测试。
- 固定 SDK/官方来源下的 v1/v2 请求取消/会话取消范围和所需响应矩阵；候选目前只要求后续定义，尚未提供实际矩阵。分别列未知主体、未知选项、加载/恢复、无 Run、前台/后台不区分、Run 终态、前台 idle、连接失效与请求明确取消，不把前台结束推成全部审批失效。
- 精确定义产品命令到期与原生请求仍有效的交错：到期不能新交付旧决定，也不把固定 24h 产品请求期限说成原生审批寿命。后续只读事实、安全响应或继续等待须由可靠协议状态决定，不能自动再接受第二个人工决定。
- 内存账本需冻结根/连接/目标/单请求布局及共享 lease、候选双份、借出 writer、原解析帧并存的峰值收费；64 引用不能只数 registry 而遗漏旧显示租约。超预算实际请求安全结束的路径仍需 AP-PLAN-002 的真实来源及一次资格。
- 补独立阶段用例后再审门禁；应包括 schema4、无 Run 真实协议替身、相反选择两 pool、决定/claim/最终检查共享故障、请求到期/撤销/资源失效、writer 2 秒取消/迟到队列、记录失败不重发及新旧正文隔离。新的管理 MGT-CODE-001/002 回归原则也必须适用于此 owner，不能复制原候选缺口。

本预审不证明任何审批运行能力、schema4 迁移、真实 actor/source 发行者、持久恢复或 reconciliation 已实现。独立定义尚未到齐，所有新执行均 not-run；AP §8A 纯协议投影通过不能代签本阶段。无用户数据、进程或依赖更改。

## 两项定义修订复核

2026-10-31追加，实际完整复读并计算候选新SHA256：`0882a1bc1da0815df1a2c1a97970b76d327ff33716b7c10a034afd0fae14a857`。原9d0c版本及其发现保留，上文open为原快照状态；**原提出者在此将AP-PLAN-001、AP-PLAN-002关闭为“定义已修正”，不表示实现或验收通过。**

- **AP-PLAN-001：resolved（定义）。** claim仅冻结planned，actual由同一真实RPC的唯一writer消费边界及节点回执提供。最终检查抑制selected时必须可靠证明尚未进入首次可能传输，丢弃危险permit并转安全响应，不新增attempt/人工决定。planned selected和actual cancelled可同时如实表达；actual不能由planned猜填。writer已选择实际种类后不可改种类，可能selected写后禁止补发cancelled。这样原决定、认领意图及实际响应不再相互覆盖。
- **AP-PLAN-002：resolved（定义）。** 当前协议owner独占且有界的ReplyCell明确Pending→Consumed(actual-kind)→Written/Unknown，selected/cancelled共享原子单次消费，Consumed先于第一字节，之后不得回Pending。selected还需持久危险permit；安全cancelled只具有真实连接/RPC/raw绑定的安全响应资格，不等待数据库成功。无approval持久行、写失败或提交未知时保留有界回执/资源责任、报告管理未确认并关闭危险入口；恢复只补可证明事实、不发送，进程退出丢回执保留未知缺口。没有借故障放开allow或跨连接复用。

本次修订符合现有产品规则，不需用户重裁。**阶段编码门禁仍未通过**：schema4完整DDL/迁移及故障点、固定v1/v2取消范围矩阵、全部保留对象/租约/回执的预算账本、独立子定义与公开接口仍待冻结和检视。上述必要失败/取消/并发用例完整保留；所有新产品执行仍not-run。管理输入原冻结版MGT-CODE-001/002不受此次文档关闭影响，仍待另版产品修复与独立复验。
