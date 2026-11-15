# 六项独立问题的修订位置（待原提出者复核）

原独立planning-issues.md `df0ce48e0c63cc3c3b7fd3e537d8edbf4c5fde848eb9bd8659f58e2567293570`、83定义及v1作者包完整保留。下表只交工程修订，六项均**不由作者自行Closed**；当前编码前门槛pending，执行0。

| 原问题 | 本版精确位置/签名 | 修订内容与仍须独立核对 |
| --- | --- | --- |
| SV-PLAN001 | A1 NativeRuntimeHandle::start_shared_views / start_with_ports；A2 ViewReadService::open/next_snapshot/next_delta/wait_change/release；A5 NativeReadCoordinator::new；A8各getter；R2-3错误表 | 给全P/L/G/N/A/C bounds、合法服务工厂/实际Handle/首await监督；opaque descriptor/cursor/address/status/close只读访问，不反造grant。必须由tester核全部正调用与错误/关闭分支可达。 |
| SV-PLAN002 | B2 Prepared::abort / Attachment::enqueue / into_original；B3 drive_next/start_ready_chunk/finish_flush；B4 receive_credit+commit_delivered；B5责任表；B7实际本地module传输 | cursor仅在原真实Delivered+同socket有效credit交汇提交；无public bytes/bool Delivered，原物在未消费阶段归还，已进入sink不伪返Prepared；Drop/取消不丢稳定监督槽。local与WSS收据类型不同，不以L3代真网络。 |
| SV-PLAN003 | B1 SocketAuthAttempt::authenticate→AuthenticatedSocketReceipt::install；CurrentViewAuthority的client_binding/with_current/wait_client_change；A7真实local auth fixture | 真实actor没有socket字段；只能原实际upgrade/ConnectionLease/challenge同握手签context，不能actor+字符串工厂。当前qualification来自真实policy+TrustStore COMMIT，socket/nonSocket issuer明确分开。 |
| SV-PLAN004 | B3同SendRegistry最后实际start_send/三类seal；B4 Started/Flushing pending_ack及成功flush原子提交；B6真实hooks | source/view/stream先seal→零这次start_send；已开始固定块可完成且不能撤回。sourceSeal O(1)flag、不等actor/网络/worker、不阻ND ack。合法earlyCredit在start_send成功后暂存；失败/Unknown不commit，未来伪ack仍拒绝。 |
| SV-PLAN005 | C1 VerifiedCreateObservation/VerifiedCreatedTarget/CreateWatchReceipt/TargetBindReceipt；A3 register_create/resolve_create及route_version/cut barrier；C3 created_from_lease | NewSession真实variant确为Response::NewSession(SessionId)；create前无SID/key/resource，用同actualscope/typedRPC CreateOp登记；真实响应后精确SID回填source暂存。不同create/未知归属不广播、不猜最近Run。 |
| SV-PLAN006 | R2-1固定ND6+receipt overlay；C2完整NativeViewProtocol；C3真实SourceSlot bridge；A6封闭实际ACP source fixture/A7真实配对认证；C5各层入口 | 当前依赖不再v4/live5；P保NativeControlPort全链、三repo与同Frame/Probe/Ticket/Scope等式。新增端口明确尚须实现，不声称已存在。Source fixture从真实spawn/Reservation/Received/IoOwner产生观察身份，ModuleFixture不是假Loaded/ResourceEpoch。L4实际ND/SQL与L5/WSS/Web/L6不消失。 |

A*=read-and-source-api.md；B*=send-and-credit-api.md；C*=native-assembly.md；R2*=engineering-contract.md。case-mapping.md/json逐83原ID给具体方法、观察值、原steps/expected SHA，未删除或调低原级别。

SV034的缓存分支精化：spec允许source释放/换代失效旧缓存，不强制永久保留旧view；旧cursor/prepared始终拒绝。source仍活但刷新失败、raw仍有时新open明示Stale；若已释放/已清旧raw则按原生重读或CurrentOnly限制，不能承诺补回。已向原tester发此最小oracle核对，未把沉默写成同意，原v1 oracle未改；reviewer/tester在v2应分别核“旧内容尚保留/已按生命周期失效”。这不是作者自行减免源安全或产品行为。

本版还明确首missing sequence原子记录/通知、projection failure与raw completeness分离、random/counter失败、现有配对/认证具体调用、ViewShutdownReport真实join/原Handle跨线程Drop和大raw cleanup执行位置。真实OOM/无法安全抵达私有MAX仍静态/未执行，不添加生产state setter。设计文本和hash检查不是编译或动态通过。
