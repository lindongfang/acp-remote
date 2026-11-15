# revision2 对83独立分支的逐项工程映射

只读原独立定义，83ID/steps/expected不改；每行原步骤/预期SHA保存在case-mapping.json。下面是作者接口可达性说明，不是tester预期改写、执行或SV-PLAN关闭。A*=read-and-source-api，B*=send-and-credit-api，C*=native-assembly，R2*=engineering-contract。接口均候选尚未实现，L4/5/6仍按原定义真实依赖。

| ID | 原级别 | 精确合同位置 | 具体方法/真实构造 | 可观察值/范围 |
|---|---|---|---|---|
| SV-001 | L3、L4 | A1/A3/C3 | start_shared_views;install_source;retain_view_observation;capture_received | SourceInspection frontiers;真实Lease/Received;同receipt_order不同sequence |
| SV-002 | L3、L4 | A3/A6 | capture_received;reobserve_from | 两个actual Connection.scope;首missing_seq;当前target记录数 |
| SV-003 | L3、L4 | A3/C1/C5 | bind_known;source_bound;AcpViewModuleFixture::resume_existing | 同key route_version/source身份;不同来源隔离 |
| SV-004 | L4 | C1/C2/C3 | register_create;reserve_view_create;created_from_lease;resolve_create | 原NewSession typedRPC;无SID预watch;response_seq及exactSID回填 |
| SV-005 | L3、L4 | A3/C1 | bind_known;resolve_create;open | retained/processed cut与route_version;earlyraw一次投影 |
| SV-006 | L3 | A3/A6 | capture_received;bind_known | 无session/未bound source暂存;另一target无泄漏 |
| SV-007 | L3、L4 | A2/A3/C1 | descriptor;bind_known;resolve_create;unbind | Scope/ResourceStamp/ReadEpoch/ViewEpoch/platform anchor各自来源 |
| SV-008 | L4 | C3/B3 | on_lease;P::acknowledge;seal_source | 原ND apply/gap/ack及真实writer;无Lease所有权交view |
| SV-009 | L3、L4 | A3/C3 | source_terminal;P::next | 原terminal无sequence;最后retained cut及worker真实drain |
| SV-010 | L3 | A1/A3/A5/A6 | 合法fixture facade;公开getters;compile-fail | 先正imports/build，再private ctor/Clone/Deserialize/From攻击逐原因 |
| SV-011 | L3 | A3/A4/B7 | capture_received;next_snapshot;ModuleViewTransfer::next_record | 重组原raw bytes/未知数值词法;无Debug canary |
| SV-012 | L3 | A4/B7 | RetainedRaw::clone;next_record;真实cleanup join | 20000深度/叶数完整/flat drop;非法原wire无Received |
| SV-013 | L3 | A2/B7 | next_snapshot;RecordAddress;next_record | 32MiB两侧精确长度/散列;单record跨块 |
| SV-014 | L3、L4 | A1/A3/B7 | open_transfer;credit;next_record | 16MiB/128MiB之外记录全部可读;无quota驱逐 |
| SV-015 | L3 | A3/A4/B7 | capture_received;next_record;TargetInspection | 4097/32769完整条目、raw与原顺序 |
| SV-016 | L3、L4 | A1/A2/A6 | open;descriptor;release;probe | 65读/65纯模块target与版本refs;不外推ND主资源容量 |
| SV-017 | L3、L5 | A4/B2/B7 | metadata;address;next_record | 长message/tool ID和kind只raw refs;WSS实际分支另验 |
| SV-018 | L3、L5 | A4/C1 | NativeSessionKey::from_config;bind_known | 既有session/RPC限制与输出ID无预算分离 |
| SV-019 | L3 | A3/A4 | capture_received;TargetInspection | 4097工具无LRU;anchor原first_position/version refs |
| SV-020 | L3 | A4 | version-specific reducer;TargetInspection | 先update/full原位同toolID,后文字位置不动 |
| SV-021 | L3 | A4 | message reducer;TargetInspection | v1无ID每chunk;同可靠messageID仅原版语义关联 |
| SV-022 | L3 | A4 | raw identity reducer;TargetInspection | namespace/decoded UTF8精确;Unicode不正规化 |
| SV-023 | L3 | A4/R2-3 | tool_patch规则纯转换;descriptor | omitted/null/_meta/rawInput/content等明确表;raw保全 |
| SV-024 | L3、L5 | A4/B6 | descriptor;next_record | UnknownStructured raw/关系限制;不伪轮次Run |
| SV-025 | L4 | R2-1/R2-4/C4 | start_shared_views;shutdown | L4真实SQL触达及旧表canary摘要;无正文恢复/删除 |
| SV-026 | L3、L5 | A2/A3 | open;next_snapshot | 取cut/barrier/H;无新输出仍有原文 |
| SV-027 | L3 | A2/A3 | open;SourceInspection;AfterCaptureEnqueue | Notify先注册再核cut;actual Received非peer.write |
| SV-028 | L3 | A2/B4 | snapshot_cursor;next_snapshot;commit_delivered | H固定版本;>H日志另存 |
| SV-029 | L3 | A2/B2/B4 | next_snapshot;RecordAddress;finish_flush | 跨chunk同不可变record/连续offset;epoch变stale |
| SV-030 | L3 | A2/B4 | next_delta;commit_delivered;SnapshotEnd | SnapshotEnd经真Delivered+credit才转Tail(H) |
| SV-031 | L3 | A2/A3 | wait_change;descriptor | 通知订阅/重核顺序;source失败/换代唤醒 |
| SV-032 | L3 | A2/B5 | open;wait_change;release | consumer取消不seal source/取消原生;其它reader继续 |
| SV-033 | L3、L5 | A2/B5 | open;release;close_stream | 跨reader不同H;最后reader不清source raw |
| SV-034 | L3、L4 | A2/B3/C1 | unbind;seal_source;seal_view;next_snapshot | 旧cursor/prepared拒绝;新open按实际retained/stale或原生重读 |
| SV-035 | L4、L5 | A2/B1/B2 | client_binding;enqueue;receive_credit | 真实SocketLifetime/actor binding；同actor异socket拒绝 |
| SV-036 | L3 | A2/B4 | RecordAddress getters;next_snapshot;credit parser | checked offset/counter;无安全MAX入口静态/未执行 |
| SV-037 | L3、L5 | A2/A5 | NativeReadControl::request;descriptor | CurrentOnly限制醒目;无replay不封其它能力 |
| SV-038 | L4、L5 | A1/A2/A5 | open;request;acquire_read_binding | 各ReadReason真实校验/重读;cache Revalidating/Stale分轴 |
| SV-039 | L4 | A5/C3 | NativeReadCoordinator::new;request;reserve_replay | 同target一个在途actual RPC;desired_refresh一次后继 |
| SV-040 | L4 | A5/C2 | acquire_read_binding;reserve_view_replay | 同完整BoundSetup/MCP/额外根/opaque配置;Unknown不空覆盖 |
| SV-041 | L4 | A5/C3 | replay_completed_from_lease;ReadCompletionProof getters | 原Replay RequestKind/typedRPC/scope/epoch匹配;非裸JSON |
| SV-042 | L3、L4 | A2/A5/C3 | descriptor;observe | Written/Resumed/idle/SnapshotEnd/H不mintComplete |
| SV-043 | L4 | A5/C3/R2-3 | observe;replay_completed_from_lease | 真实早Replayed+后flushErr;完成proof不等Written |
| SV-044 | L3、L4 | A3/A5 | observe;processed_frontier;worker barrier | proof之前raw未processed不Complete |
| SV-045 | L3、L4 | R2-3/R2-4/A3 | capture_received;SourceInspection;descriptor | 首缺seq sticky且按完成boundary前后范围分别 |
| SV-046 | L4 | A5/C3/R2-3 | observe;source_terminal;descriptor | Complete前Incomplete/后Stale保原proof;原Ticket责任 |
| SV-047 | L4 | A3/A5/C3 | ReadCompletionProof getters;replay_completed_from_lease | typed number/string/旧scope/rpc/readEpoch拒绝覆盖 |
| SV-048 | L3、L4 | A2/A4/A5 | request;open;descriptor | 无跨load稳定ID重建;Live/replay窗口不猜无损合流 |
| SV-049 | L4 | A5/C2 | reserve_replay;enqueue_replay;observe | 独立write deadline/At或Unbounded原映射;不重发 |
| SV-050 | L4 | A5/C2 | ReadEnqueueError::into_original;cancel_reserved;ReadCancelError::into_original | 原R/O/Ticket保监督；Reserved实际ProbeZero才Zero |
| SV-051 | L4、L5 | A2/A5/B5/C4 | release;close_stream;observe | 不session/cancel/不造Run；read可能写留native原owner |
| SV-052 | L3、L4 | A3/R2-4 | capture_received;BeforeWorkerDetach | 仅短handoff metadata锁;projection锁外;无有界正文channel |
| SV-053 | L4 | C3/C5/B3 | on_lease;P::acknowledge;seal_source | L4实际ND/queue/SQL/control writer；view不加gate |
| SV-054 | L3、L4 | R2-3/R2-4/A3 | capture_received;SourceInspection;open | try_reserve fault预分配首缺seq/通知；其它source独立 |
| SV-055 | L3 | A4/R2-3 | descriptor;next_record | raw保全ProjectionUnavailable，未知合法PartialKnownStructure |
| SV-056 | L3、L4 | A2/B2/B5 | next_snapshot;enqueue;into_original | chunk/outbound失败只本read，别reader还可读 |
| SV-057 | L3、L4 | A1/R2-4 | shutdown;ViewShutdownReport getters | 同稳定slot并发/cancel waiter/截止不丢join责任 |
| SV-058 | L3、L4 | R2-4/A1 | SharedViewOwner Drop;shutdown | 原Handle跨无runtime线程;不panic/假join；cleanup实际任务 |
| SV-059 | L4 | B3/C4/R2-4 | seal_source;unbind;shutdown | view seal不等慢send/projection/析构；ND实际关闭继续 |
| SV-060 | L4 | R2-3/C4/C5 | on_lease;shutdown;NativeReadControl | L4真实必要SQL/附加审计/展示职责分离；actor旧受理不续租 |
| SV-061 | L4、L5 | A7/B1 | SocketAuthAttempt::authenticate;with_current | 真实challenge/proof/lease/Trusted/current actor；不placeholder |
| SV-062 | L4、L5 | A2/A7/B1 | with_current;next_snapshot | copy前撤销COMMIT零正文；B独立合法读取 |
| SV-063 | L4、L5 | B2/B3/B6 | enqueue;drive_next;start_ready_chunk | queued后撤销先赢；同实际gate丢块/close |
| SV-064 | L4、L5 | B3/B6 | start_ready_chunk;poll_flush;finish_flush | actor gate护已开始固定块/原15s；下一块拒绝 |
| SV-065 | L4、L5 | B3/B4/B6 | seal_source;seal_view;close_stream;start_ready_chunk | source/view/stream最后同锁围栏；已begin不可撤回分列 |
| SV-066 | L5 | B1/B4/C5 | SocketAuthAttempt;view.open DTO;receive_credit | 真实WSS保32KiB/depth8/120command；未认证binary拒绝 |
| SV-067 | L3、L5 | B4/B7 | 72字节codec;RecordAddress getters | magic/reserved/length/flags/type/empty/offset完整矩阵 |
| SV-068 | L3、L5 | A4/B4/B7 | next_record;真实Web scanner | 跨UTF8/escape/数值；u64 BigInt；超长ID rawrefs |
| SV-069 | L3、L5 | B2/B4/B7 | receive_credit;CreditReceipt;StreamInspection | 初始/重复/未来/错socket credit；permit最多1 |
| SV-070 | L4、L5 | B3/B4/B6 | start_ready_chunk;receive_credit;finish_flush | 实际start_send后earlycredit Hold，真flush后原子commit；失败不提交 |
| SV-071 | L5 | B4/C5 | receive_credit;view.open DTO | >120Data/Credit不计command；真实管理计数不重置 |
| SV-072 | L5 | B2/B5/C5 | enqueue;drive_next;shutdown | 1MiB/256实际outbound背压，node raw/control不受限 |
| SV-073 | L3、L5 | A2/B1/B2/B5 | release;receive_detach;close_stream | 仅本stream；new socket新open/current auth |
| SV-074 | L4、L5 | B3/B4/B5 | finish_flush;receive_credit;close_stream | actual失败/Unknown零commit；保原物/不自动重发native |
| SV-075 | L3、L5 | A1/A2/B2/B6 | metadata/getters;Debug/Display边界 | 原raw/R/Prepared无公开正文getter；仅实际已交付fixture bytes可取 |
| SV-076 | L5 | R2-2/C5 | 实际Web renderer/DOM测试 | textContent/HTML禁执行/unknown占位；零管理请求 |
| SV-077 | L5 | A4/B1/C5 | 实际Web renderer/真实审批owner | 控制字符显露；unknown人工原optionID不默认禁选或允许 |
| SV-078 | L5 | R2-2/C5 | 实际Web资源占位/独立资源入口 | 无自动fetch/代理/凭据/越根；未有入口Unsupported |
| SV-079 | L3、L5 | A4/B4/C5 | 实际Web scanner/平台Status codec | raw迭代词法；非整体JSON.parse/stringify；平台decimal strings |
| SV-080 | L5 | A2/A5/B1/C5 | open;descriptor;release;实际Web状态 | source来源/缓存生命周期/CurrentOnly与Complete明显分轴 |
| SV-081 | L5 | A1/B1/C3/C5 | 真实Storage native工厂→start_shared_views→socket服务 | L5完整source/queue/SQL/WSS真实签名链；不替身代签 |
| SV-082 | L6 | C5 | 实际Harness/Windows/Web专用来源全链 | 真实可达与未执行分列；不代CL123执行闭环 |
| SV-083 | L1、L2、L4、L5 | R2-5/C5 | 实际README/package/check/build/alltarget/frontend | 新集成前后hash/数量/命令及cleanup；旧306不外推 |
