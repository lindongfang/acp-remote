# WP2 2.3 审批内层独立交付检视

日期：2026-10-30。**PASS（仅冻结14文件的领域/应用端口/SQLite内层）；未发现该范围需阻断的问题。** 不签 task2.3 最终产品完成，不签真实审批发行者、client_gate、原生 writer、Web、Windows资源或 CL-030/031。独立 reviewer，只读产品，未继承 coder 对话；开发说明只作范围说明，结论来自源码、实际差异、摘要、独立重跑及日志核读。未改 plan/任务/产品。

## 固定范围和证据

候选根：`C:/Users/jack/.codex/worktrees/codex-session-wp2/acp-remote`；对照根：`D:/Project/acp-remote`（冻结前 WP2 2.2）。清单 `.tmp/wp2-2.3-logs/frozen-files.json` SHA256=`88A094ABC6682B3F7CA70AEB640725928C36B09A7C6489EFBD9BF68ED24B07AA`。

开始和结束逐文件核验候选 **14/14** 与清单相同；结束时清单全部非null mainSha256亦与D主产品一致，错配0。实际 no-index 产品差异为14文件、2496新增/15删除，与清单范围一致，未发现隐藏安全模块/依赖/索引修改。差异检查无空白错误，no-index因存在差异返回1，另有既有LF/CRLF提示；不将该1冒称检查exit0。冻结清单保存完整文件摘要，本报告不重复第二套清单。

实际重算 verification.json 指向的 owner/clippy/fmt/all/history 五日志摘要 **5/5一致**。读取日志正文：owner前置真实确认创建对象owner与进程用户一致；Clippy完成；fmt日志为空且verification记录exit0；全目标测试正文为200库+4协议向量通过，history日志34通过。fmt退出码来自已有执行记录，本reviewer未重跑fmt；未把开发全绿直接代替独立功能结论。

独立实际运行同一已编译库testbin（真实owner提权执行，未重编译/修改候选）：

- `.tmp/wp2-target/debug/deps/acp_remote-266e97892ace37dd.exe session_storage::approvals::tests:: --test-threads=1`：exit0，10通过/0失败，2.05秒。
- 同exe `history --test-threads=1`：exit0，34通过/0失败，9.15秒，包含 HIST-R001 广播唤醒、强刷等待已有在途保存、预算/失败停止、实际SQLite故障及崩溃尾部测试。

二进制 SHA256=`A1814871EA9C4DC725C3107B830F3CC43DDD4F83054D82DDC2AF056FB7168803`。源码与该已有编译物的关联依据为冻结后all-candidate实际编译/运行日志；本reviewer没有独立重建全目标。上述44项是开发纯测试的独立重跑，不是正式tester验收或真实Harness证据。测试使用PID/计数命名专用临时SQLite根并在Drop按路径前缀清理，没有读取用户数据库/原生会话/凭据。

## 检视结果

- **裁决及幂等事务**：decide使用BEGIN IMMEDIATE，在当前actor检查、共享actor/request命名空间探测、容量限制后，原子插入Command、pending CAS、选择/操作者关联及元数据审计；重复返回同一关联，异action/摘要冲突。input受理也新增action复核，防止跨动作复用请求ID。双池相反裁决竞争、实际DDL/审计/延迟外键COMMIT失败等开发断言经独立重跑通过。
- **交付事实分离**：claim仅允许decided/not_sent、原操作者与代数、当前Run/原生绑定/连接代际，持久化唯一claim无回到not_sent的路径；重复claim拒绝。report保留WriteCompleted与source_current_at_report，状态不把传输写入当对端执行。传输/确认落库失败不能清除决定或生成新claim；失效/重启将未决认领unknown，未发送suppressed，不自动重发。晚报告即使source已失效仍可保留原报告而不虚报当时来源当前；实际writer边界由WP3负责。
- **原执行失效**：连接代际单调绑定，旧/关闭代际不可复活；run绑定固定session/turn/native session/instance/代际，取消请求使审批失效但Run保持cancelling前台互斥，可靠终态才更新状态。连接退出使活跃输入unknown、会话interrupted；重启恢复原子失效审批来源，审批裁决Command不被误改成输入未知派发。重复请求仍保留决定查询事实。
- **强刷检查点**：prepare在最终fence之外强刷；checkpoint私有字段记录scope、observation_revision与Arc owner身份，拒绝dirty/inflight/fault/stopped/closed；已接收新版本推进revision，重新attach不能复用旧owner checkpoint。commit/claim再次检查，测试覆盖强刷失败、prepare后脏数据及决定/claim零新增。内部API明确调用者持gate→source owner，这个前提尚未由WP2类型或真实锁强制；它是WP3必须满足的合同，而不是已实现安全证明。
- **typed来源与结构保留**：NativeRequestId区分JSON-RPC字符串与安全整数，同值跨request/tool类型不混；tool ID是ACP字符串标识独立类型。工具索引绑定平台session/turn、原生session、instance/代际，在scope校验基础上查找；哈希仅索引，命中仍比原文字段，多个匹配/冲突失败。原始请求、scope、选项和协议字段保存，未知请求/作用范围/选项、缺工具关联记录unsupported，不默认允许或等待未来事件自动授权。当前内部protocol_version字符串不直接充当Web wire整数，WP4必须显式投影转换并拒绝不合法值。
- **审计与正常Rust路径**：security_audit/approval_audit_links仅平台ID、代数、固定动作/结果和时间，未将原生敏感ID/工具正文/选项名称拼入错误或审计。正常新增审批路径未发现unwrap/expect/panic；测试中的expect属于断言。错误类型有语义，SQLite细节被收敛。新增层无外部进程/后台任务所有权；历史owner保持现有关闭/取消路径，34项回归通过。
- **schema**：本次扩展仍属未发布schema2布局，schema1迁移按严格结构新增；旧中间schema2布局不匹配即拒绝，没有清库或重建。原identity/clients/撤销管理表未改变。不得将此接受解释为未来已发布schema2可以无版本升级改布局；正式交付后的新增布局需另行迁移设计。

## 问题与限制

本冻结范围未发现需创建AP-R阻断项的问题。此结论不是“无需后续审查”：真正私有发行者、actor/source fence、gate到实际writer的同区提交、撤销/取消窗口、对端版本策略及真实工具执行均未实现/未验收。本reviewer未从内部ActorContext或数据库decided推导运行授权。WP3接线若绕过这些边界需独立报问题，不能援引本PASS豁免。

主D权威合同在本次结束的SHA256=`CBF1BFE389CA3824884918C0BA9147B117C472359D396A7A132F150204B7D071`；§9 live承载澄清另在reviewer-wp3-readiness追加设计PASS，tester补断言待核读。实际14文件集成前应逐摘要保真，集成后重跑受影响真实检查；最终tester/reviewer仍须固定同一集成版本独立验收，不由本报告代签。
