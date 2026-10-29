# establish-node-access 独立最终交付检视

状态：`BLOCKED`

- 审查日期：2026-10-29（Asia/Shanghai）
- 基线 HEAD：`7a6e9ec4bdf4b41d9408b39cb3669313e52f41b2`
- 固定源码清单：`testing/runs/establish-node-access/2026-10-29-snapshot.sha256`
- 源码清单自身 SHA-256：`6B1D981658CE1ACA98D390B3C56650908924B0A7486A55A87836DEEC74200BB9`
- 固定配置清单：`testing/runs/establish-node-access/2026-10-29-config-snapshot.sha256`
- 配置清单自身 SHA-256：`13AD9DDE14AFC5235217CBA748B8EAB830162F56625FF4183BAB386172DFB3F1`
- 审查角色：独立 reviewer；未继承 coder 对话，不以开发自测、旧 tester 报告或结构检查代替产品验收。
- 写入边界：除本报告和 `openspec/changes/establish-node-access/review.md` 的摘要外，未修改清单内文件；未提交、推送或归档。

## 结论

两份固定清单本身完整可核验。当前快照的 `NA-REV-001` 至 `NA-REV-004`、`NA-REV-D01`、`NA-REV-S01` 和 `NA-REV-P01` 均可关闭；最终 tester 报告在当前快照上为 `4 PASS / 25 PARTIAL / 5 NOT-RUN`，`NA-032` 的受信 TLS/Tailscale/Chromium A/B 正常 L6 闭环已通过。仍缺少批准响应丢失、NA-031 异常存储、跨用户完整矩阵、供应链和人工签署，因此不能宣布产品通过，也不能进入待归档阶段。

- `NA-REV-001`：Closed。
- `NA-REV-002`：Closed；断开客户端导致 stop 响应写失败时，shutdown 明确失败，不虚报成功。
- `NA-REV-003`：Closed；响应投递错误不再中止其他 stop 客户端，也不改写已发布的资源清理结果。
- `NA-REV-004`：Closed；类型化传输错误分别映射到 `4429`/`4400`，真实 WebSocket 边界回归通过。
- `NA-REV-D01`：Closed；冻结合同中依赖、构建入口和验证入口的旧描述已纠正。
- `NA-REV-S01`：Closed；重建配置清单与 71 项源码清单无重叠，并共同覆盖 93 个文件。
- `NA-REV-T01`：Closed；最终 tester 报告保持“完整必要断言全部满足才 PASS”，逐例统计为 `4 PASS / 25 PARTIAL / 5 NOT-RUN`，与正文一致。
- `NA-REV-B01`、`NA-REV-B03`：Open 但范围已按 tester 真实证据收缩。
- `NA-REV-B02`、`NA-REV-B04`、`NA-REV-B05`：Open，均为验收阻断。
- `NA-REV-N01`：Open，Low；产品接受 `127.0.0.1:0`，但 `NodeRuntime::listen_addr()` 返回配置值 `:0` 而不是实际绑定端口。当前正式配置使用显式非零端口，故不是当前产品验收阻断，但不能把该现象只归因于探针。
- `NA-REV-P01`：Closed，Medium，仅限辅助探针资产；21:27 修订版已在生成 secret 前保护 `data-run`，TLS 私钥再受文件级 owner-only DACL，并在失败/超时后显式关闭 pool、stop/join、断言资源释放后才删除。数据库 open/close 与 stop/join/delete 故障注入、强杀清理仍属残余未验证风险，但因目录预先受保护且关闭未确认时保留目录并返回错误，不再构成原 P01 阻断。

## 两份固定清单校验

独立重算结果：

- 源码清单自身 SHA-256 为 `6B1D981658CE1ACA98D390B3C56650908924B0A7486A55A87836DEEC74200BB9`，与任务给定值一致；包含 71 行有效条目，71 个路径均唯一，逐文件重算 `71/71 OK`。
- 配置清单自身 SHA-256 为 `13AD9DDE14AFC5235217CBA748B8EAB830162F56625FF4183BAB386172DFB3F1`，与任务给定值一致；包含 22 行有效条目，22 个路径均唯一，逐文件重算 `22/22 OK`。
- 两份清单无重叠，合并后为 93 个唯一路径；均无缺文件、摘要不符、重复路径、坏行或清单自条目。

配置清单显式覆盖 `.github/dependabot.yml`、PR 模板、CI、`.gitignore`、`.nvmrc`、`AGENTS.md`、OpenSpec 配置/角色/模板、根 `package.json`/`package-lock.json`、`README.md`、`rust-toolchain.toml` 和产品设计文档。`docs/HARNESS_CONTROL_PLANE_REDESIGN.md` 是用户正在调整的规格，当前字节摘要 `38332BEDB4707D9D752413A3D3FD38C7246D5FAAB7C00C0A7957E7256423E451` 已进入重建配置清单。独立核对认为其 §7.4 的会话按活动回收、50 个主会话容量等规则属于后续会话资源策略，与本 change 的配对、管理认证、IPC 关闭和设备撤销边界没有直接冲突。

历史记录：前一轮固定源码清单为 70 项、摘要 `9FD12D64713627666AE0BC9D8AB7BABCF356EC2DF408AE30F409450BD8436965`；当时 `NA-REV-003`、`NA-REV-004`、`NA-REV-D01` 为 Open。配置设计文件发生用户编辑后，旧配置摘要 `EF438276DB57D164F01ACA369E13B2AA523AFDBF51E4F916D4C2E450B82FF3E9` 一度不再匹配；用户确认该编辑合法并重建清单后，当前两份清单恢复完整。旧状态和旧摘要保留为审查历史，不与当前字节混用。

## NA-REV-001/002 复查

### NA-REV-001：Closed

- 位置：`app/src/node.rs:221-251`。
- `run()` 和 `wait_for_stop()` 在创建 `watch::Receiver` 后立即调用 `borrow_and_update()` 检查当前值，再进入等待。
- 因此，订阅前已经发布 `true` 的 stop/failure 不会丢失；检查之后、等待之前到达的更新也会使后续 `changed()` 立即返回。
- 回归：`app/src/node.rs:1703` 覆盖提前 stop，`app/src/node.rs:1716` 覆盖提前 failure。
- 结论：此前“订阅前状态丢失”问题关闭。对应测试由开发报告声称通过，但本次没有把它写成独立产品验收。

### NA-REV-002：Closed

- 位置：`app/src/ipc.rs:168`、`app/src/ipc.rs:253-308`、`app/src/node.rs:337-365`。
- stop 响应任务类型为 `JoinSet<io::Result<()>>`，`write_response` 的结果会返回给 `finish_stop_responses`，不再吞掉写入错误。
- 节点在 IPC 任务结束后发布一次资源清理结果，然后等待所有 stop 响应；响应投递错误会进入最终 `shutdown` 的 `Err`，不会走正常成功返回，也不会改写已发布的资源清理结果。
- 真实未连接管道写失败测试位于 `app/src/ipc.rs:1143`。
- 保守语义确认：某个 stop 客户端在写入前断开会使最终 shutdown 报失败。这是诚实报告，不应被优化成没有诊断的成功提示；健康 stop 客户端仍可观察同一资源清理结果。
- 结论：此前“写失败被吞、节点虚报成功”问题关闭。

## 问题闭环与仍开启问题

| ID | 严重性 | 状态 | 文件位置 | 依据与影响 | 复现/修复建议 |
| --- | --- | --- | --- | --- | --- |
| `NA-REV-003` | Medium | Closed | `app/src/ipc.rs:278-308`、`app/src/node.rs:352-365`、`app/src/ipc.rs:1170-1215` | 前轮问题为首个响应失败会 `abort_all()`，且 watch 可能先发布 `Completed` 后翻为 `Failed`。当前实现累计全部响应结果，只有共同 deadline 到期才中止未完成任务；`control.complete()` 在交付前只发布一次资源清理结果，投递错误仅进入最终 `shutdown` 错误。双 stop 测试中一客户端断开、另一客户端仍收到 `stopped`，watch 保持 `Completed`。 | 后续修改需保持“资源清理结果”和“响应投递诊断”分离，并保留双客户端故障回归。 |
| `NA-REV-004` | Medium | Closed | `app/src/management_ws.rs:1142-1150`、`app/Cargo.toml:44`、`app/src/node.rs:1387-1400` | 前轮问题为传输层超限错误统一落入 `1001`。当前通过同一锁定版本的 `tungstenite::Error` downcast，将 `Capacity` 映射为 `4429`、`Protocol`/`Utf8` 映射为 `4400`；依赖树仅解析一个 `tungstenite 0.29.0`。冻结版本上 32769、32770、65536 字节真实 WebSocket 帧回归均通过。 | 该类型化分类依赖 axum 与直接依赖的 tungstenite 版本一致；升级依赖时必须保留锁定检查和这三档真实帧回归。 |
| `NA-REV-D01` | Low | Closed | `openspec/changes/establish-node-access/access-contract.md:108`、`openspec/changes/establish-node-access/access-contract.md:112` | 前轮合同仍写“本轮没有新增依赖”和“产品编译/前端入口不存在”。当前已改为：依赖和锁文件已引入，选择依据在 `app/DEPENDENCIES.md`；Rust/Web 检查入口已建立，但逐例通过仍以固定版本报告为准，构建/lint 不替代产品验收。 | 保持合同只描述已建立能力，不把入口存在写成验收通过。 |
| `NA-REV-S01` | Medium | Closed | `testing/runs/establish-node-access/2026-10-29-config-snapshot.sha256` | 重建配置清单 `22/22 OK`，自身摘要为 `13AD9DDE14AFC5235217CBA748B8EAB830162F56625FF4183BAB386172DFB3F1`；与 71 项源码清单无重叠，合并为 93 个唯一文件。用户确认产品设计文档编辑合法，其中 §7.4 与节点接入无直接冲突。 | 任一文件变化后必须重建对应摘要；两个摘要只固定审查字节，不证明 CI、依赖安全或产品行为通过。 |
| `NA-REV-T01` | Medium | Closed | `testing/runs/establish-node-access/2026-10-29-independent-tester-regression.md` | 初版把“当前环境主要断言”当作 PASS；后续校准已改为“全部必要断言通过才 PASS”。最终版 SHA-256 `074180AE7C49E66D5F49731BE9C70BAC502A27082B0C9E40E5BF3654D411BC7F` 的逐例统计为 `4 PASS / 25 PARTIAL / 5 NOT-RUN`，与表格一致，`NA-006/012/016/018/021/031` 等未完成原断言者保持 PARTIAL。 | 后续追加仍须更新哈希并保持逐例表与统计一致；不得恢复弱 PASS 定义。 |
| `NA-REV-B01` | Blocker | Open（已收缩） | `testing/runs/establish-node-access/2026-10-29-independent-tester-regression.md`、`testing/runs/establish-node-access/2026-10-29-storage-acceptance.md`、`openspec/changes/establish-node-access/tasks.md:28` | 最终 tester 报告在当前 `71 + 22` 摘要上完成 Rust `118/118 + 4/4`、fmt/check/Clippy/build、精确回归、Web/OpenSpec/npm audit、真实节点生命周期、pipe 写失败、跨 SID 基础拒绝和受信 L6。整例统计为 `4 PASS / 25 PARTIAL / 5 NOT-RUN`；PASS 仅 `NA-001/017/019/032`，NOT-RUN 为 `NA-010/020/030/033/034`。存储探针另覆盖真实 COMMIT 失败、幂等预算、过期槽和安全清理；NA-007 仍缺批准成功后响应丢失/CLI 同时取消/浏览器恢复，NA-027 仍缺可控 24 小时、实际 `SQLITE_FULL` 与完整 WSS 审计组合。 | 交付人工接棒时附完整 25 个 PARTIAL/5 个 NOT-RUN 清单；不能用已通过的 4 例、宏观 L6 或 118 个测试替代逐例验收。 |
| `NA-REV-B02` | Blocker | Open | `testing/runs/establish-node-access/2026-10-29-human-acceptance-handoff.md`、`testing/runs/establish-node-access/2026-10-29-trusted-tailscale-development.md:133-134` | NA-033 和 NA-034 要求独立人工安全/依赖检视及配对提示可用性验收，当前没有真实人员签署。 | 由未参与实现的人员按用例独立执行并留下可追溯签署；reviewer 不能冒签。 |
| `NA-REV-B03` | Blocker | Open（已收缩） | `testing/runs/establish-node-access/2026-10-29-independent-tester-regression.md:129-141`、`app/src/platform/windows.rs`、`app/src/ipc.rs` | tester 已在两个真实 SID（`faraday\jack` 与 `faraday\codexsandboxoffline`）下验证 data 目录 DACL、`.node.lock`/`identity.key`/`node.sqlite3` 拒绝、owner pipe 连接拒绝、CLI 路径拒绝及跨 SID DPAPI 解密拒绝。NA-012 仍明确缺远程 named pipe、伪造 SID/服务端 token 校验失败，以及另一用户实际 `device pair`/`daemon stop` 路径；候选 SID 在客户端 data/DACL 阶段即被拒，尚非服务端 SID 错误响应证据。 | 补齐远程 pipe、伪造 SID/模拟失败和另一用户实际命令路径，并保留无敏感错误回显检查；已有基础拒绝证据不得扩大为完整 NA-012 通过。 |
| `NA-REV-B04` | Blocker | Open（L6 正常闭环已关闭） | `testing/runs/establish-node-access/2026-10-29-independent-tester-regression.md:117-219`、`openspec/changes/establish-node-access/tests.md:457-467` | `NA-032` 已在当前快照整例 PASS：真实 Tailscale IP/名称匹配 Let's Encrypt 证书、两个独立 Edge profile、真实 device pair/本机批准、认证/list/订阅、A 撤 B、重启后 A 原身份恢复且 B 以 `4401` 拒绝、owner stop 后 A `1001`，进程/监听/pipe/lock 均释放；`NA-017/019` 亦为 PASS。B04 不再表示“整体 L6 未执行”，仅保留批准提交成功后响应丢失的恢复，以及 `NA-031` 私密模式、写入失败、坏记录 key、清站点数据、key 丢失提示和新 origin 隔离等异常场景。 | 继续补齐上述 NA-007/NA-031 异常子项及证据；不得把 NA-032 正常闭环通过扩写为这些失败/恢复场景也通过。 |
| `NA-REV-B05` | Blocker | Open（已收缩） | `testing/runs/establish-node-access/2026-10-29-independent-tester-regression.md:48-51`、`app/Cargo.lock` | tester 已在真实 `jack` 网络环境完成根目录和 `clients/web` 的 `npm audit --audit-level=high`，均报告 0 漏洞；Rust `cargo-audit`/advisory 扫描仍未完成，依赖许可证和维护状态也需人工验收。Gitleaks 8.30.1 下载和 `cargo-audit` 前后尝试均因下载超时未取得可执行结果，不能写成成功扫描或用户拒绝。 | 在可联网或预置离线资产的环境中重试固定版本 Gitleaks、Rust advisory、许可证和维护状态检查；npm audit 通过只关闭其对应子范围。 |
| `NA-REV-N01` | Low | Open（非当前验收阻断） | `app/src/node.rs:98-107`、`app/src/node.rs:193-200`、`app/src/config.rs:48-57`、`app/src/main.rs:105-109` | `NodeConfig` 接受 `127.0.0.1:0`；`NodeRuntime::start()` 实际绑定由系统分配的端口，却把 `config.listen` 保存为 `self.listen`，因此 `listen_addr()` 和 CLI readiness 输出是 `:0`。存储探针首次真实运行因此连接 `127.0.0.1:0` 得到 Winsock 10049。正式部署配置使用显式非零端口，故当前不阻断该配置路径；但该公开 API/就绪报告对合法 `SocketAddr` 输入不自洽。 | 解冻后优先保存 `listener.local_addr()`（绑定后实际地址）到 runtime；若明确不支持端口 0，则在配置和公共启动边界拒绝并文档化。增加 `127.0.0.1:0` 就绪地址回归。 |
| `NA-REV-P01` | Medium | Closed（仅辅助探针） | `testing/probes/establish-node-access-storage/src/main.rs:175-210`、`testing/probes/establish-node-access-storage/src/main.rs:228-236`、`testing/probes/establish-node-access-storage/src/main.rs:301-336`、`testing/probes/establish-node-access-storage/src/main.rs:338-376`、`app/src/platform/windows.rs:226-243` | 前轮问题为 `data-run`/TLS 私钥处于继承 ACL，异常超时无法保证清理。修订版先创建 `data-run`，在 `rcgen` 生成密钥前调用 `protect_private_directory` 并以独立 `Get-Acl` 断言 owner、受保护 DACL、唯一当前 SID FullControl 和继承标志；私钥写入后再调用 `protect_private_file` 并断言。180 秒预算只包业务体；normal/failure/timeout 均在预算外显式 `db.close()`、owner stop（20 秒）、join（25 秒），join 超时则 abort/await；未确认关闭时保留受保护目录并非零返回。日志独立重算为 58/0、9/0、9/0，`data-run` 与生成的 key/cert/db 均不存在。 | 保持密钥前先保护目录、关闭未确认不删除以及清理结果显式断言的顺序。残余限制：数据库 connect/close 不在业务体 timeout 内；stop/join/delete 故障注入和 OS 强杀未实测，强杀本身不承诺用户态清理。 |

## NA-001–034 与测试资产

- 结构检查独立通过：34 个唯一用例 ID、15 个 Requirement、28 个 Scenario、28 行追踪矩阵；`NA-033`/`NA-034` 仍为人工 LX。
- 该检查只证明 ID、引用和矩阵结构完整，不证明产品行为、安全语义或测试已执行。
- 最终 tester 报告已按“完整用例全部必要断言”重定义 PASS。逐例表为 `4 PASS / 25 PARTIAL / 5 NOT-RUN`，PASS 为 `NA-001/017/019/032`，NOT-RUN 为 `NA-010/020/030/033/034`，与正文统计一致。
- `NA-012` 仍缺远程 named pipe、伪造 SID/服务端 token 校验失败及另一用户实际 `device pair`/`daemon stop`；`NA-016` 仍缺真实 15 秒期限耗尽和必要保存提交失败矩阵。存储探针已补齐强制 COMMIT 和 10000 条幂等预算的独立子断言，但 `NA-007/027` 仍因成功响应丢失、真实 Web/时钟/SQLite full 等缺口为 `PARTIAL`。
- `openspec/changes/establish-node-access/tasks.md` 实际仅勾选 `1.1`、`1.2`、`1.3`、`2.2`；`2.1`、`2.3`–`2.5`、`3.x`、`4.1`–`4.4` 均未完成。不能用计划完成度代替验收。

## 独立 tester 最终报告（含 L6）

- 报告：`testing/runs/establish-node-access/2026-10-29-independent-tester-regression.md`
- 本次审阅当前版本 SHA-256：`074180AE7C49E66D5F49731BE9C70BAC502A27082B0C9E40E5BF3654D411BC7F`；长度 `30346`，修改时间 `2026-10-29 21:33:25.064 +08:00`。
- 报告明确绑定源码 `6B1D981...`（71/71）与配置 `13AD9DDE...`（22/22），运行者为真实 `faraday\jack` 用户；独立执行了 `cargo test --all-targets --locked --offline`（库 118/118、JWS 4/4）、fmt/check/Clippy/build、5 个精确回归、Web 检查/31 项测试/构建、OpenSpec strict、根与 Web npm audit、真实节点 start/stop/restart、真实 pipe 写失败、跨 SID 探针和受信 L6。
- 最终逐例统计为 `4 PASS / 25 PARTIAL / 5 NOT-RUN`；该统计只表示整例完成度，不代表 25 个 PARTIAL 已有失败，也不代表 5 个 NOT-RUN 可以直接豁免。
- 该报告不在源码清单的 71 个条目内；本次按上述文件哈希作为外部测试证据，若 tester 追加或修改报告，必须重新读取并按新哈希评估，不能把当前内容自动延伸到后续版本。
- 证据收缩：B01 不再是“没有当前 tester 证据”，而是“已完成大部分自动化和本地平台探针，但逐例完整验收仍未闭合”；B03 不再是“跨用户完全未执行”，而是“跨 SID 基础拒绝已实测，但完整合同步骤仍未覆盖”。
- L6 证据独立交叉核对：`l6-final-state.json` 显示 A 的 `secureContext=true`、clientId `rDUhhTY_GJ4ryk9e7uR-IQ`，B 的 clientId `n5BBVXjiROzeXaX249l8Sg`，两者 `privateKeyExtractable=false` 且 localStorage/sessionStorage 为空；`close-code-read.json` 显示 A `1001/node stopping/clean=true`、B `4401/authentication failed/clean=true`。截图 `a-final.png` 与 `b-final-revoked.png` 和 JSON 一致；证书 SAN/DNS 为 `faraday.tail74842e.ts.net`，摘要 `12AD6ABE...`。当前复核无 `30448/9228/9229` LISTENING、无匹配 Edge/profile 或 `acp-remote.exe` 进程。
- 复核限制：当前 reviewer 进程无法读取 `tailscaled` 状态（访问被拒绝），故 Tailscale BackendState 依赖 tester 的真实用户 CLI 记录；DNS 当前解析到 `100.119.123.57`，证书与页面 `secureContext` 证据一致。该限制不改变 NA-032 正常闭环的 tester 证据可用性。

## 独立存储探针（21:17 初版，历史）

- 报告：`testing/runs/establish-node-access/2026-10-29-storage-acceptance.md`；SHA-256 `3296003658DF909B1B1A92698518262C3A248AFF2CAB94B46427849CE32096D7`，修改时间 `2026-10-29 21:17:56.247 +08:00`。
- 证据：最终日志 SHA-256 `94E475C0817E458F1C720658EB123F25F4F09601FB9B623C9836AE019C9FA458`；独立重数 52 条 `PASS`、0 条 `FAIL`，存在 `PROBE COMPLETE`。52 条包含协议关联、前提和清理断言，不是 52 个独立业务场景。
- 探针四文件在 71 项源码清单之外，报告已单列摘要：`Cargo.toml` `8BBDE7...`、`Cargo.lock` `78C721...`、`src/main.rs` `83A86C...`、`README.md` `EF5328...`。71/22 清单仍未改变，探针证据不能扩张为完整产品快照。
- 安全控制：`data-run` 已存在时拒绝覆盖；使用当前用户 DPAPI、owner named pipe；TLS 客户端只信任当次生成证书，不加载系统根或跳过校验；总运行 180 秒超时。
- 限制：报告正确区分 SQLite 外键延迟约束 `COMMIT` 错误 787 与真实磁盘满 `SQLITE_FULL`；也正确说明直接修改 `expires_at` 只证明过期分支，不等于真实/可控时钟跨 24 小时。`NA-REV-P01` 记录临时 TLS 私钥目录 ACL 和超时清理风险。
- `NA-007` 已接受子断言：真实 claim 到 `pending_confirmation`，语句失败与延迟外键实际 `COMMIT` 失败均返回 `storage_unavailable`，clients/revision/audit/results 无残留。仍缺成功提交后响应丢失、CLI 同时取消/关闭、取消提示、浏览器持原 key 恢复及未知/撤销统一拒绝。
- `NA-027` 已接受子断言：撤销语句/COMMIT 回滚、相同 requestId 重放、载荷冲突、RFC8785 摘要、24 小时写入期限、9999→10000、满额拒绝不驱逐、过期槽回收和过期 ID 重用。仍缺真实 Web A→B/B 重连、真实或可控时钟跨越 24 小时、实际磁盘满、WSS `node.info` 不写幂等结果、订阅/逐帧审计和重启后重放。
- 结论：`NA-007 PARTIAL`、`NA-027 PARTIAL`；不得将子集通过写成整例通过，B01 仍 Open。

## NA-REV-P01 修复复查（21:27 报告）

- 报告：`testing/runs/establish-node-access/2026-10-29-storage-acceptance.md`；当前 SHA-256 `5945DC48244763D831177BD4317608D774D28B032A14A68538BB6DA610DB6FC7`，修改时间 `2026-10-29 21:27:47.016 +08:00`。旧报告 `32960036...` 和旧探针摘要仅保留为历史，不用于本次关闭。
- 当前探针资产摘要独立核对一致：`Cargo.toml` `8BBDE7F5E93511A953B37CA209A0C50C533B3661FF738C43C405D026FEDA92AF`、`Cargo.lock` `78C72125BAE6D05A8EE1D914CC07106618C0BE802243FA462FF0350B02E02680`、`src/main.rs` `EDF8B5DCCDCC240D1AFAAD2F45B94064DC08380C702D285A059392FEE9231848`、`README.md` `09C31439A020A4440E9E763E76C2EC3CB7BE9C970F5A5D3C347944CB3D4D70F8`。产品 `71/71` 与配置 `22/22` 清单仍完整一致。
- 日志摘要和逐条重数一致：`p01-normal.log` `60A4FDDCC7F44B6329D9B4FC19F0FAABB2AFEAD2C888653D81DB5331BD64E26A` 为 58 PASS/0 FAIL/`PROBE COMPLETE`；`p01-failure.log` `081DA457074465AEBE9ABA6A5B87FBBF00101D9750BA138E7510185924C89DA2` 与 `p01-timeout.log` `AFFAF679C959971C4E6B9BF570060346CE1F2763492DF66CF94580006B11F2D7` 各 9 PASS/0 FAIL。日志本身未内嵌 `$LASTEXITCODE`；但源码在清理完成后以 `result?` 返回注入错误，报告使用“预期退出 1”而未把这些模式计作产品功能 PASS，语义成立。
- 独立静态复查顺序：`main()` 先拒绝既有 `data-run`、创建空目录、调用 `protect_private_directory` 并独立断言，之后才进入 `run()` 生成证书和私钥；私钥落盘后再次 `protect_private_file`。`db.close()` 位于可取消业务体之外；stop、join 和 join 超时 abort/await 均有明确所有者；未确认关闭时 `cleanup_safe=false`，保留目录并返回错误；只有 stop/join 成功且端口、pipe、目录锁断言通过后才 `remove_dir_all`。
- 独立离线编译 `cargo build --manifest-path testing/probes/establish-node-access-storage/Cargo.toml --offline --locked` 通过。复查 `.tmp/establish-node-access-storage-acceptance/` 时 `data-run` 不存在，递归扫描未发现 `server-key.pem`、证书、SQLite 数据库或私钥文件残留。
- 残余风险不重新打开 P01：SQLite `connect_with`/`db.close()` 在 180 秒业务 timeout 之外，理论上可延长运行；stop/join/delete 故障分支和 OS 强杀未实测。目录在 secret 生成前已受 owner-only DACL，且未确认关闭时不会删除目录，因此这些限制已作为证据范围保留，而不是秘密提前暴露或失败路径静默清理成功的反向结论。
- 结论：`NA-REV-P01` Closed；`NA-007`、`NA-027` 仍分别为 `PARTIAL`，B01 仍因完整用例缺口保持 Open。

## 旧证据复用边界

- `2026-10-29-development.md`：113 项旧 Rust 开发记录，不是独立验收。
- `2026-10-29-final-development.md`：30 项 Web 测试、自签证书绕过 Chromium 证书错误，且声明为 development evidence only；其浏览器结果不是受信 TLS 验收。
- `2026-10-29-trusted-tailscale-development.md`：包含 31 项 Web 测试及受信 TLS/Tailscale 开发线索，但明确不替代独立验收；其源码早于后续 shutdown 修复，不能作为当前快照的新鲜浏览器运行。
- `2026-10-29-independent-tester.md`：旧工作区、26 项 Web 测试，明确未运行产品节点、L5/L6 和多数 NA 场景，不能绑定当前 71 项源码清单。
- `2026-10-29-shutdown-regression-development.md`：准确记录 NA-REV-001/002 修复及开发侧 `116 + 4` Rust 测试，但明确 development only，并声明旧 TLS/Tailscale/browser 不是 shutdown 修复后的新鲜运行。本 review 只把其中的说明当作待复核线索，没有把它写成独立通过。
- `2026-10-29-review-regression-development.md`：记录 NA-REV-003/004/D01 修复及开发侧真实 Windows `118 + 4` 通过，但明确 development only。后续新 tester 报告绑定当前摘要并独立记录 `118 + 4`；本 review 接受该 tester 证据，但不把测试数量等同于 34 个用例的完整验收。
- 未发现这些报告直接伪造“独立通过”；主要风险是旧版本证据被错误绑定到新快照，或把开发自测累计数当成验收结论。

## 本轮独立执行

以下检查在隔离构建目录中执行；结果只覆盖所述范围：

| 检查 | 结果 |
| --- | --- |
| 源码 `71` 项与配置 `22` 项摘要及逐文件重算 | 通过，`71/71`、`22/22`，无重叠，合并 93 项 |
| `node testing/suites/verify-suite.mjs` | 通过；34 IDs、15 Requirements、28 Scenarios、28 matrix rows |
| OpenSpec strict validation | 通过 |
| Web TypeScript check | 通过 |
| Web Vitest | 通过；6 files、31 tests |
| `cargo fmt --all --check` | 通过 |
| `cargo clippy --all-targets --locked -- -D warnings` | 通过 |
| `stop_response_` 定向回归 | 通过；2 tests |
| `management_websocket_closes_oversized_transport_frames_with_4429` | 通过；32769/32770/65536 字节真实 WebSocket |
| JWS 固定向量 | 通过；4 tests |
| `cargo test --all-targets --locked`（受限 reviewer 进程） | 未完成：库测试 `115/118` 通过；3 个 DPAPI 用例因 `CryptProtectData` 系统错误 2 失败，不计为整体通过 |

文本秘密扫描未发现真实私钥/证书材料进入清单；发现的内容为测试占位私钥和 `.tmp` 路径引用。该文本扫描不等同于完整的二进制、熵、日志泄漏或供应链秘密审查。

## 未验证项

- 新 tester 已在真实 `faraday\jack` 下记录当前快照 Rust `118 + 4` 通过；reviewer 不重复该测试。reviewer 受限进程中的 3 个 DPAPI 失败仅是环境限制，不构成本代码快照的独立产品结论。
- 批准事务提交成功后响应丢失、CLI 同时取消/关闭以及真实浏览器持原 key 以 `clientId=null` 恢复的完整闭环。
- NA-031 的私密模式、持久化写入失败、已有坏记录 key、清站点数据、key 丢失提示和新 origin 隔离；真实 A 全部 Edge 进程退出后的同 profile 持久性已经通过，不得把该子项缺失误写成整体未执行。
- 第二个 Windows 用户完整 matrix：基础 DACL/文件/pipe/DPAPI 拒绝已由 tester 覆盖，但 owner/异用户全部 5 条命令、伪造 SID、远程 pipe、token 模拟失败/回退和敏感错误回显仍未完整覆盖。
- 并发批准/撤销、撤销栅栏、队列失效、发送边界、关闭期限和任务清理全矩阵；存储探针的语句/COMMIT 失败、幂等、预算和过期槽子断言已记录，但未覆盖真实 Web、实际磁盘满和真实时钟。
- 完整敏感信息、原始协议字段、日志/错误回显和 Windows 平台资源清理审查。
- Rust `cargo-audit`/advisory、Gitleaks 全仓秘密扫描与依赖许可证/维护状态验收；根目录和 Web 的 npm audit 已由 tester 在真实网络下报告 0 漏洞。Gitleaks 8.30.1 和 `cargo-audit` 的本轮下载尝试均超时，不能写成已执行成功。
- NA-033、NA-034 的人工签署。

## 门禁结论

- 已通过范围：两份清单完整性、测试资产结构、OpenSpec 严格校验、Rust fmt/Clippy、Web 类型检查/31 项测试/构建、根与 Web npm audit；`NA-REV-001`–`004`、`NA-REV-D01`、`NA-REV-S01`、`NA-REV-T01`、`NA-REV-P01` 可关闭；`NA-001/017/019/032` 整例 PASS，其中包含受信 TLS/Tailscale、独立 Edge A/B、重启持久性和完整清理；003/004 定向回归、JWS 向量和存储探针 normal 58 条、failure/timeout 各 9 条子断言独立核对通过。
- 阻断范围：`NA-REV-B01`（最终 `4 PASS / 25 PARTIAL / 5 NOT-RUN`）、`NA-REV-B03`（远程 pipe、伪造 SID、异用户实际 pair/stop 等）、`NA-REV-B02`（NA-033/034 人工签署）、`NA-REV-B04`（仅剩批准响应丢失和 NA-031 异常场景）、`NA-REV-B05`（Rust advisory/Gitleaks/许可证）。`NA-REV-N01` 为已接受零端口配置的 Low 就绪报告缺陷，当前非阻断；P01 的 DB connect/close、故障分支和强杀限制为已记录残余风险，不再作为当前探针秘密保护阻断。
- 总状态：`BLOCKED`。
- 待归档结论：不可进入待归档阶段。当前版本已适合交付用户进行人工验收接棒；人工 fixture 端口 `30447` 和 `.tmp/establish-node-access-human-20261029` 保留给用户，至少必须完成 NA-033/NA-034 签署，并按 25 个 PARTIAL/5 个 NOT-RUN 清单处理其余缺口。不能用旧 tester 报告、开发自测或结构门禁替代。
