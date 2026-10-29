# establish-node-access 独立存储故障验收

## NA-REV-P01 修复交接（2026-10-29 21:26 +08:00）

**状态：修复已实现并自验，待原 reviewer 独立复查；不由本 tester 关闭。** 原 reviewer 报告 `2026-10-29-independent-reviewer.md` 中 Medium/Open 保持不动。本次仅修改持久探针 main.rs/README 和本报告，运行产物在专用 .tmp；没有产品源码、锁文件或既有清单变更。NA-007/NA-027 整例仍均为 PARTIAL，剩余矩阵不因辅助资产修复而消失。

修复内容：

1. 新建 `data-run` 后，**在 rcgen 生成任何密钥前**，调用 app public `platform::protect_private_directory`。独立 Windows Get-Acl 断言 owner 为当前 SID、DACL 受保护、恰一条显式 Allow、仅当前 SID FullControl、目录 ACE 带 ObjectInherit/ContainerInherit。权限检查失败则不生成 secret。TLS 私钥写入已经受限的根目录，随后额外调用 public `protect_private_file` 并再次断言文件 DACL 受保护且仅 owner FullControl。路径与 SID 经环境变量传递给固定 PowerShell 代码，不插入命令字符串，不输出私钥内容。
2. 移除覆盖整个 run 的外层 180 秒 timeout，改为仅取消业务体。真实 runtime 的 JoinHandle 和外部 SQLite pool 由预算外的资源所有者持有。业务成功、失败、超时都先显式 pool.close，再经 owner IPC stop（最多 20 秒）、等待 runtime join（最多 25 秒）；如 join 超时则 abort 并 await，关闭未确认时保留受限目录、非零退出并明确路径，不能继续假报删除成功。
3. stop/join 成功后实际重绑监听端口、验证原管道不可访问、重新 acquire 并显式 unlock 目录锁，最后删除仅本轮拥有的目录。删除错误带目录路径并非零退出。初始化在 runtime 成功启动之前失败时没有本轮已接管的活动节点，清理本轮目录。强杀不可能保证执行用户态清理；本修复以预先收紧的 DACL 限制残留材料访问，不声称强杀清理通过。强杀场景未实测；既有目录拒绝覆盖。

最终源码构建：`cargo build --manifest-path testing/probes/establish-node-access-storage/Cargo.toml --offline --locked`，退出 0。CARGO_TARGET_DIR 仍为 `.tmp/establish-node-access-storage-acceptance/target`；正常用户上下文顺序运行同一二进制的 normal、failure、timeout 三模式，实际结果：

| 模式 | 注入位置/行为 | 断言与退出 | 清理实测 |
|---|---|---|---|
| normal | 原 SQLite 批准/撤销故障和容量矩阵 | 58 PASS，0 FAIL，退出 0，PROBE COMPLETE | pool close、stop/join、端口/管道/锁释放、data-run 不存在 |
| failure | 真实节点和数据库连接建立后返回指定错误 | 9 条权限/清理 PASS；预期退出 1；错误 `P01 injected failure after live node and database acquisition` | 同上；非零结果未被吞掉 |
| timeout | 真实节点和数据库连接建立后业务 pending；同一 timeout 逻辑使用 20ms 预算 | 9 条权限/清理 PASS；预期退出 1；错误 `P01 injected/body timeout` | 同上；未采用 drop 即声称节点已关闭 |

normal 的 58 条包含历史 52 条全部重跑，新增 2 条 DACL、1 条显式 pool close、3 条端口/管道/目录锁断言。failure/timeout 是辅助资源生命周期检查，不增加原产品用例的通过数量。20ms 与正常 180 秒使用相同超时分支，只缩短故障验证等待；没有实际等待 180 秒。此次没有执行关闭失败/删除失败/OS 强杀注入，不把保留目录错误分支的代码检查宣称成实测。

新增资产摘要（替代下方历史摘要作为当前探针版本；Cargo.toml/lock 未变）：

```text
8BBDE7F5E93511A953B37CA209A0C50C533B3661FF738C43C405D026FEDA92AF  testing/probes/establish-node-access-storage/Cargo.toml
78C72125BAE6D05A8EE1D914CC07106618C0BE802243FA462FF0350B02E02680  testing/probes/establish-node-access-storage/Cargo.lock
EDF8B5DCCDCC240D1AFAAD2F45B94064DC08380C702D285A059392FEE9231848  testing/probes/establish-node-access-storage/src/main.rs
09C31439A020A4440E9E763E76C2EC3CB7BE9C970F5A5D3C347944CB3D4D70F8  testing/probes/establish-node-access-storage/README.md
60A4FDDCC7F44B6329D9B4FC19F0FAABB2AFEAD2C888653D81DB5331BD64E26A  .tmp/establish-node-access-storage-acceptance/p01-normal.log
081DA457074465AEBE9ABA6A5B87FBBF00101D9750BA138E7510185924C89DA2  .tmp/establish-node-access-storage-acceptance/p01-failure.log
AFFAF679C959971C4E6B9BF570060346CE1F2763492DF66CF94580006B11F2D7  .tmp/establish-node-access-storage-acceptance/p01-timeout.log
```

21:26:58 +08:00 再核原 71/22 项清单，0 mismatch。复查请求：请原 reviewer 复核秘密生成前的目录权限顺序、取消范围和 stop/join 所有权、三模式独立重跑及摘要，再决定是否关闭 NA-REV-P01。本会话可见工具未列出原 reviewer 的可寻址会话，未向不相关会话发送材料，也未冒充其复查结果。复查可直接读取本报告与持久资产；当前记录是“待原 reviewer”，不是 Closed。下文保留第一次交付及适配失败的历史证据；末尾追加修复版逐断言输出。

## 以下为修复前交付的历史记录

## 结论与范围

2026-10-29，独立 tester 并行子任务。**NA-007 PARTIAL；NA-027 PARTIAL。** 持久探针最终运行退出码 0，实际执行 52 条 PASS 断言（含协议关联、前提及清理断言，不能解读为 52 个独立业务场景），输出 PROBE COMPLETE。未重跑开发单测充作新增矩阵。没有人工验收、独立供应链/安全代签或完整 L5/L6 通过声明。

交付资产：`testing/probes/establish-node-access-storage/{Cargo.toml,Cargo.lock,src/main.rs,README.md}`。运行说明在该目录 README。构建 target、运行日志与生成数据均限定在 `.tmp/establish-node-access-storage-acceptance/`。持久目录只有上述四文件，没有 data/key/cert/log/target。未修改 app、既有用例、清单、其他文档或报告；不提交、不推送。

## 固定版本与环境

- 产品清单 `2026-10-29-snapshot.sha256`：71 项，SHA256 `6B1D981658CE1ACA98D390B3C56650908924B0A7486A55A87836DEEC74200BB9`。
- 配置清单 `2026-10-29-config-snapshot.sha256`：22 项，SHA256 `13AD9DDE14AFC5235217CBA748B8EAB830162F56625FF4183BAB386172DFB3F1`。
- 开始及最终运行后逐文件核验，两清单全部匹配、0 mismatch。仓库原有未提交变更及未跟踪 app/clients/testing 等保持原状。新增探针摘要独立列出，不扩充 71 项产品源码清单。
- Windows NT 10.0.26300.0 x64；rustc 1.98.1 (48a229cea 2026-09-01)，cargo 1.98.1 (797e8a9bc 2026-08-05)。最终运行约 21:15:48 +08:00，耗时约 1.23 秒。
- 读取 AGENTS.md、tester 职责、OpenSpec 配置、access-contract.md、tests.md、当前设计与 README，并自行检查 app 公共 API、Storage/TrustStore/PairingService/IPC/NodeRuntime 及已有故障单测。
- 独立 Rust 工程 path 依赖本地 app，仅使用 public API；真实 SQLite、当前用户 DPAPI、owner named pipe、直接 TLS。非浏览器 Rust 客户端完成配对 claim。TLS 使用每轮新生成的 localhost 证书，客户端仅信任该证书，无跳过验证，无系统信任库安装。
- `cargo build --offline --locked`，不联网、不新增生产依赖。独立锁文件以 app 锁文件为起点生成；原 app 锁文件的非根 package name/version 均在探针锁文件中保留（差异数 0）。新工程根包变化不是产品版本变化。未执行独立漏洞数据库/许可证审计。
- 本任务用户未要求且本探针未建立另一 Windows SID；当前用户只用于操作本轮新建数据，不声称完成跨 SID 隔离。另一 tester 的浏览器/跨 SID 工作不计入本报告。

## 方法与真实/模拟边界

未使用 `cfg(test)`、私有构造器、源码 include、unsafe 内存访问或产品内部测试钩子。`TrustStore::new`、`Storage::trust_store`、`fail_next_commit_for_test` 不对外开放；绕过这一限制的方法是启动真实 NodeRuntime，经真实本机协议发送指定 requestId 的撤销请求。批准先经公开配对 HTTP 端点完成真实 P-256/JWS claim，再经 public PairingSession 提交批准。

两种 SQLite 故障：

1. 审计插入前触发 `RAISE(ABORT)`，使已经执行的业务写入所在事务失败。
2. 审计插入后触发向专用表写不存在的父键；外键声明 `DEFERRABLE INITIALLY DEFERRED`。控制事务先证明 INSERT 成功，**COMMIT 实际返回 SQLite 错误 787**，且审计回滚。相同约束分别用于真实批准和撤销路径：此前 SQL 可执行，实际提交因未满足延迟外键而失败。不是开发单测中“提交前直接返回错误”的替代模拟。产品外部按合同只返回 storage_unavailable，不暴露 SQLite 内部错误。

回滚快照比较包括所有 clients 的 ID/status/auth_generation/revoked_at、node_state.revision、security_audit 总数及 management_commands 总数。批准故障发生时 clients 为空且前后不变，证明无半信任、公钥记录或审计残留；不额外声称已验证浏览器恢复和认证拒绝。撤销失败后两条夹具客户端仍 trusted、generation=0、revoked_at=NULL，revision/audit/result 不变。

容量夹具直接向隔离库插入 9998 条未来过期的结果，保留一条真实撤销结果，达到 9999；经真实请求进入 10000。夹具 actor 跨多个值，验证全库容量而非单 actor 容量。夹具行并非 9998 次真实协议受理；仅用于建立容量前提。client 夹具使用有效 P-256 公钥，未冒充真实配对创建。过期夹具直接修改指定行 expires_at 为当前时间前一秒，不修改操作系统时钟。

## 逐项结果

| 断言子集 | 实际证据 | 状态 |
|---|---|---|
| 延迟约束控制 | INSERT 成功；COMMIT 错误 787；审计计数回到 0 | PASS |
| NA-007 提交前语句失败 | 真实 claim 到 pending_confirmation，批准明确报 storage_unavailable；四类持久数据快照不变 | PASS（子集） |
| NA-007 提交中失败 | 延迟外键在实际 COMMIT 拒绝批准；同样明确错误且无半信任/审计/revision 残留 | PASS（子集） |
| NA-027 撤销语句/COMMIT 失败 | 两种故障均明确 storage_unavailable；状态/代数/撤销时间/revision/审计/结果回滚 | PASS（子集） |
| 故障移除重试 | 原 requestId=200 可成功撤销 client 1，未被失败结果污染 | PASS |
| 原请求重放/载荷冲突 | 相同 requestId 和载荷返回完整原响应、不增审计/revision；不同 clientId 返回 request_conflict | PASS |
| RFC8785 摘要 | 对合同封闭字符串字段对象手工构造规范化 UTF-8 字节，独立 SHA256 与实际存储 digest 相同 | PASS（仅当前合法封闭载荷） |
| 24 小时写入期限 | 实际 expires_at 与观测墙钟+86400 相差不超过 5 秒 | PASS（存储期限子断言） |
| 只读不膨胀 | 5 轮真实 owner device.list 与 status 后，结果/审计/状态/revision 不变 | PASS（非 WSS node.info） |
| 9999→10000 边界 | 新 requestId 撤销已撤销 ID 可受理，恰新增一个审计且不改 revision，结果到 10000 | PASS |
| 满额拒绝与不驱逐 | 新 revoke 被 storage_unavailable 拒绝；快照不变；真实旧请求仍可重放，载荷冲突仍有效 | PASS |
| 过期槽回收 | 单个夹具过期后一个新请求成功，结果仍恰 10000，未过期真实结果保留 | PASS（过期夹具） |
| 过期 ID 重用 | 真实旧 requestId 行设为过期后可改为另一 clientId 受理；原身份仍 revoked，代数不增加；重复撤销另一身份亦不重复增加代数 | PASS（过期夹具） |
| 关闭与清理 | owner stop 成功、runtime join 成功、仅本轮 data-run 删除；最终不存在该目录 | PASS |

最终库观察值：audit=4、clients=2（均 revoked，generation=1）、management_commands=10000、revision=2。夹具初建 clients 不递增 revision；该最终值只衡量本探针两次真实首次撤销，不能用作正常配对 revision 的验收证据。

## 原用例完整性与剩余缺口

| 原用例 | 整例状态 | 尚未实际执行 |
|---|---|---|
| NA-007 | PARTIAL | 提交成功后响应丢失、CLI 同时取消/关闭的并发线性化、取消提示 device revoke、浏览器持原 key 以 clientId=null 恢复，以及未知/撤销统一认证拒绝。成功批准原子提交/重启恢复也未在本探针验收。 |
| NA-027 | PARTIAL | 真实 Web A→B 管理撤销、B 重连/管理拒绝、真实或可控时钟跨 24 小时及精确到期边界、实际磁盘满；WSS node.info 不写幂等结果、订阅/逐帧审计不膨胀，及重启后重放。 |

上述缺口各子项均 NOT-RUN，不能把部分 PASS 提升为整例 PASS。当前无产品故障断言 FAIL；适配失败记录见下节，不隐去也不算产品通过。

最小后续测试入口建议（**本任务未修改或扩展产品 API**）：

- STORAGE-GAP-01：TrustClock 虽是 public trait，TrustStore 构造和 clock 注入不对外开放，NodeRuntime 也没有注入 clock 的入口。准确验证 24 小时前一秒/到期瞬间，需要测试专用启动装配器向真实 TrustStore 注入时钟；保留正常生产构造路径。修改数据库 expires_at 只能证明过期分支，不能证明时钟语义。
- STORAGE-GAP-02：本轮未建立 SQLITE_FULL 故障，延迟外键不等于磁盘满。后续可在探针拥有的容量受限卷/虚拟磁盘运行，或建立仅测试可用的 SQLite VFS/连接级 max_page_count 入口，并确认实际返回 SQLITE_FULL 后验证同事务回滚。勿对用户磁盘填满或改写已有数据库。
- STORAGE-GAP-03：提交后响应丢失及取消的确定性并发，需要测试专用“COMMIT 已完成、响应未发送”屏障，或协议断流代理与独立已提交观察器。不能用固定 sleep/单纯网络断开臆测触及该窗口。公开配对 API 已可运行，故不将整个 NA-007 写为“无入口”。

## 实际执行与适配修正

1. `.tmp` 独立工程首次 offline 编译成功，沙箱内运行在 DPAPI CryptProtectData 返回 OS error 2，业务断言未运行。数据清理完成。该次不是 PASS；随后正常用户上下文实际使用 DPAPI，没有改用明文 protector。
2. 正常用户上下文第 1 次运行：延迟约束控制的 3 个断言通过，之后 TLS 连接返回 **Winsock 10049 AddrNotAvailable**，退出 1。原因是探针传 `127.0.0.1:0`，NodeRuntime.listen_addr() 返回配置中的端口 0，而非系统实际分配的监听端口。该次只算适配失败，不算完整故障探针 PASS。owner stop、runtime join、data-run 清理均成功。
3. 修正仅在探针：先用 TcpListener 分配空闲回环端口，取得实际地址、释放后给节点启动。该短暂释放窗口若遭竞争将明确启动失败，未改产品 listen_addr 行为。随后逐步补容量边界并强化批准错误码必须是 storage_unavailable，最终版本跑到 PROBE COMPLETE。
4. 用户扩展写范围后，复制必要源码到持久探针目录，把 app path 改为 `../../../app`，保持 Cargo.lock；独立 target 仍在 `.tmp`。对持久目录源码重新 `cargo build --manifest-path testing/probes/establish-node-access-storage/Cargo.toml --offline --locked`，退出 0；再运行该二进制，52 条断言通过、退出 0。下节保存的是这次最终日志。
5. 最后清单核验 0 mismatch，`git diff --check` 通过（工作区已有 CRLF 提示不等于差异错误）。本次未运行整仓开发测试；没有产品变更，不把其他 tester 的结果混入。

## 额外验证资产 SHA256

独立于既有源码/配置清单，路径均相对仓库根目录：

```text
8BBDE7F5E93511A953B37CA209A0C50C533B3661FF738C43C405D026FEDA92AF  testing/probes/establish-node-access-storage/Cargo.toml
78C72125BAE6D05A8EE1D914CC07106618C0BE802243FA462FF0350B02E02680  testing/probes/establish-node-access-storage/Cargo.lock
83A86CB4700A365BB60826D8EF9C1DC2015FA3EE0A65F9DD6630899C8B43A5F5  testing/probes/establish-node-access-storage/src/main.rs
EF5328ACE4C22E69F334F9D9AE3AE598263177C04981BAB68D5F97220DEB11CD  testing/probes/establish-node-access-storage/README.md
94E475C0817E458F1C720658EB123F25F4F09601FB9B623C9836AE019C9FA458  .tmp/establish-node-access-storage-acceptance/persistent-run.log
```

日志摘要按本机 PowerShell 实际落盘字节计算。为避免临时日志丢失导致证据不可读，下节逐行保留文本；它与日志文件编码不同，不要求本文中的文本块具有相同哈希。

## 最终逐断言原始输出

```text
PASS control: deferred trigger statement succeeds
PASS control: actual COMMIT foreign-key error 787
PASS control: failed COMMIT rolled back audit
PASS TLS pair HTTP 200
PASS TLS pair HTTP 200
PASS pair pending_confirmation
PASS NA007 precommit statement failure
PASS NA007 precommit statement failure: clients/revision/audit/results unchanged
PASS TLS pair HTTP 200
PASS TLS pair HTTP 200
PASS pair pending_confirmation
PASS NA007 actual COMMIT failure
PASS NA007 actual COMMIT failure: clients/revision/audit/results unchanged
PASS IPC response version/request correlation
PASS NA027 statement: storage_unavailable, no success
PASS NA027 statement: complete rollback
PASS IPC response version/request correlation
PASS NA027 commit: storage_unavailable, no success
PASS NA027 commit: complete rollback
PASS IPC response version/request correlation
PASS NA027 retry failed request succeeds after fault removal
PASS IPC response version/request correlation
PASS NA027 same request original response
PASS NA027 same request no extra audit/revision
PASS IPC response version/request correlation
PASS NA027 same request different payload conflict
PASS NA027 digest matches independent closed RFC8785 bytes
PASS NA027 expiry stored at now+86400 (+/-5 seconds)
PASS NA027 list/status no per-query result or audit
PASS NA027 9999 results boundary fixture
PASS IPC response version/request correlation
PASS NA027 9999 admits new request for already revoked ID
PASS NA027 repeat revoke new request one audit, unchanged revision
PASS NA027 exactly 10000 retained results
PASS IPC response version/request correlation
PASS NA027 full budget rejects new revoke
PASS NA027 full budget no eviction/state/audit changes
PASS IPC response version/request correlation
PASS NA027 full budget preserves genuine replay
PASS IPC response version/request correlation
PASS NA027 full budget preserves payload conflict
PASS IPC response version/request correlation
PASS NA027 expired slot admits one new revoke
PASS NA027 expired slot pruning keeps count 10000
PASS NA027 unexpired genuine result not evicted
PASS IPC response version/request correlation
PASS NA027 expired request changed payload accepted
PASS NA027 expired ID does not revive first identity
PASS NA027 repeated revoke does not increment generation
FINAL_STATE {"audit":4,"clients":[["AAAAAAAAAAAAAAAAAAAAAQ","revoked",1,1793279748],["AAAAAAAAAAAAAAAAAAAAAg","revoked",1,1793279748]],"results":10000,"revision":2}
PASS cleanup: owner stop succeeds
PASS cleanup: runtime joins successfully
PASS cleanup: isolated generated keys/cert/database removed
PROBE COMPLETE
```

## NA-REV-P01 修复版逐断言输出


### normal

```text
PASS P01 root protected owner-only inheritable FullControl DACL before secrets
PASS P01 TLS private key protected owner-only FullControl DACL
PASS control: deferred trigger statement succeeds
PASS control: actual COMMIT foreign-key error 787
PASS control: failed COMMIT rolled back audit
PASS TLS pair HTTP 200
PASS TLS pair HTTP 200
PASS pair pending_confirmation
PASS NA007 precommit statement failure
PASS NA007 precommit statement failure: clients/revision/audit/results unchanged
PASS TLS pair HTTP 200
PASS TLS pair HTTP 200
PASS pair pending_confirmation
PASS NA007 actual COMMIT failure
PASS NA007 actual COMMIT failure: clients/revision/audit/results unchanged
PASS IPC response version/request correlation
PASS NA027 statement: storage_unavailable, no success
PASS NA027 statement: complete rollback
PASS IPC response version/request correlation
PASS NA027 commit: storage_unavailable, no success
PASS NA027 commit: complete rollback
PASS IPC response version/request correlation
PASS NA027 retry failed request succeeds after fault removal
PASS IPC response version/request correlation
PASS NA027 same request original response
PASS NA027 same request no extra audit/revision
PASS IPC response version/request correlation
PASS NA027 same request different payload conflict
PASS NA027 digest matches independent closed RFC8785 bytes
PASS NA027 expiry stored at now+86400 (+/-5 seconds)
PASS NA027 list/status no per-query result or audit
PASS NA027 9999 results boundary fixture
PASS IPC response version/request correlation
PASS NA027 9999 admits new request for already revoked ID
PASS NA027 repeat revoke new request one audit, unchanged revision
PASS NA027 exactly 10000 retained results
PASS IPC response version/request correlation
PASS NA027 full budget rejects new revoke
PASS NA027 full budget no eviction/state/audit changes
PASS IPC response version/request correlation
PASS NA027 full budget preserves genuine replay
PASS IPC response version/request correlation
PASS NA027 full budget preserves payload conflict
PASS IPC response version/request correlation
PASS NA027 expired slot admits one new revoke
PASS NA027 expired slot pruning keeps count 10000
PASS NA027 unexpired genuine result not evicted
PASS IPC response version/request correlation
PASS NA027 expired request changed payload accepted
PASS NA027 expired ID does not revive first identity
PASS NA027 repeated revoke does not increment generation
FINAL_STATE {"audit":4,"clients":[["AAAAAAAAAAAAAAAAAAAAAQ","revoked",1,1793280412],["AAAAAAAAAAAAAAAAAAAAAg","revoked",1,1793280412]],"results":10000,"revision":2}
PASS P01 cleanup: external SQLite pool explicitly closed
PASS cleanup: owner stop succeeds
PASS cleanup: runtime joins successfully
PASS P01 cleanup: listener released
PASS P01 cleanup: owner pipe no longer available
PASS P01 cleanup: data-directory lock reacquired and explicitly released
PASS cleanup: isolated generated keys/cert/database removed
PROBE COMPLETE
```

### failure

```text
PASS P01 root protected owner-only inheritable FullControl DACL before secrets
PASS P01 TLS private key protected owner-only FullControl DACL
PASS P01 cleanup: external SQLite pool explicitly closed
PASS cleanup: owner stop succeeds
PASS cleanup: runtime joins successfully
PASS P01 cleanup: listener released
PASS P01 cleanup: owner pipe no longer available
PASS P01 cleanup: data-directory lock reacquired and explicitly released
PASS cleanup: isolated generated keys/cert/database removed
storage-acceptance.exe : Error: "P01 injected failure after live node and database acquisition"
所在位置 行:2 字符: 53
+ ... imeout')) { & ./target/debug/storage-acceptance.exe $mode *> "p01-$mo ...
+                 ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
    + CategoryInfo          : NotSpecified: (Error: "P01 inj...se acquisition":String) [], RemoteException
    + FullyQualifiedErrorId : NativeCommandError
 
```

### timeout

```text
PASS P01 root protected owner-only inheritable FullControl DACL before secrets
PASS P01 TLS private key protected owner-only FullControl DACL
PASS P01 cleanup: external SQLite pool explicitly closed
PASS cleanup: owner stop succeeds
PASS cleanup: runtime joins successfully
PASS P01 cleanup: listener released
PASS P01 cleanup: owner pipe no longer available
PASS P01 cleanup: data-directory lock reacquired and explicitly released
PASS cleanup: isolated generated keys/cert/database removed
storage-acceptance.exe : Error: "P01 injected/body timeout"
所在位置 行:2 字符: 53
+ ... imeout')) { & ./target/debug/storage-acceptance.exe $mode *> "p01-$mo ...
+                 ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
    + CategoryInfo          : NotSpecified: (Error: "P01 injected/body timeout":String) [], RemoteException
    + FullyQualifiedErrorId : NativeCommandError
 
```
