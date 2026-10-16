# Tasks

## 1. 落账时序改为三笔（WP1）

- [x] 1.1 受理提交只落幂等记录（会话标识为空、不写会话行）；成功后在同一次提交里写会话行 + 回填会话标识 + 终态；失败零会话行——判据：`cargo test -p core` 新增用例断言「受理后无会话行」与「失败后无会话行」
- [x] 1.2 会话行落账前到达的事件在 broker 内有界排队，落账后按序写库再广播，超限走既有的「超限结束该 Agent」失败关闭口径——判据：`cargo test -p core` 中分别断言正常排队、顺序正确与超限失败关闭三态
- [x] 1.3 崩溃窗口的新形状（受理已落盘、无会话行）写进恢复路径——判据：`cargo test -p core` 中断言该行恢复后终结为不确定且没有会话行

## 2. 终态判据与命令级日志（WP2）

- [x] 2.1 两个面统一为「可证明 ⇒ `failed` / 说不清 ⇒ `uncertain`」，Node Link 侧换掉「有持久记录就 uncertain」的替身，`failed` 复用 `internal.unavailable`——判据：`cargo test -p server` 新增用例对两类失败各断言一次终态类别
- [x] 2.2 命令级结构化日志：受理与终态各一条，含主体标识、命令名、终态类别与成因类别，不含路径与凭据——判据：`cargo test -p server` 断言字段集合，并手工观察一次失败路径的实际日志行

## 3. 权威文档写回（WP3）

- [x] 3.1 `docs/CORE_PORTS_AND_STORAGE.md` §6 第 16/20/22 条按新时序改写（含「恢复走有会话分支」不再成立这一条）——判据：`npm run check` 的 `check:drift` 与 `check:docs` 通过
- [x] 3.2 `docs/SYNC_PROTOCOL.md` §11.2/§11.5、`docs/NODE_LINK_PROTOCOL.md` 相应小节与 `docs/SECURITY_DESIGN.md` §14 与实现口径一致——判据：`npm run check` 通过

## 4. 协议级端到端（WP4）

- [x] 4.1 用例覆盖：失败零会话行、`accepted` → 终态 `failed`、同 `requestId` 重发返回首次终态、崩溃窗口恢复结 `uncertain`——判据：`cargo test -p app` 中四条断言各自可独立失败
- [x] 4.2 用例覆盖：窗口期事件不丢不重且顺序正确、队列超限走失败关闭——判据：`cargo test -p app` 中对事件序列与超限行为各断言一次

## 5. 合入与真机验收

- [x] 5.1 按 plan 的 Merge Order 串行合入 WP1→WP3→WP2→WP4，每次合入后跑该序的再验证——判据：每次 `git apply --check` 通过且随后的检查全绿
- [x] 5.2 主检出跑 PV1（`cargo test -p core -p server -p app`）、PV2（`npm run check`）、PV3（`cargo clippy --workspace --all-targets -- -D warnings`）——判据：三条命令全绿
- [x] 5.3 执行 plan 的 Main E2E：真实 Daemon + 真实页面 + 一个会失败的 Agent，观察到受理 → 终态失败、库里零会话行、日志含主体与成因——判据：三重观察证据同时成立
