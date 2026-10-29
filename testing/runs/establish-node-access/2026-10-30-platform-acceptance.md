# Windows 平台独立补验：establish-node-access r4

结论：本轮完成真实 owner 五命令调用、同目录第二节点拒绝、独立 Windows 定向测试与资源清理；未完成本版另一真实 SID 五命令矩阵，NA-012 仍为 PARTIAL。不能据此声明整个 change 已到待归档。不提交、推送、归档，不扩展后续 Harness。

## 范围与版本

- 独立 tester，无 coder 对话继承。只写本报告及 `testing/probes/establish-node-access-platform-20261030/`；运行资源位于 `.tmp/platform-acceptance-20261030/`。未修改产品、spec、既有报告、其他 agent 文件、索引或用户已有修改。
- 已读取 AGENTS、config、tester 角色、当前 tests NA-009/011/012/017、plan、node-access spec、design/access-contract、Windows 文档、历史 independent-tester-regression 及下述原始文件。config 中“尚无实现”是旧 context，实际实现存在；验收行为按当前 spec/tests/access-contract。
- 开始独立验证清单 `testing/runs/establish-node-access/2026-10-30-implementation-r4.sha256`：SHA256 `A1A7F70D36CD1EA7BDA79A25E1F6FFA864487EE7CE046818AB963144761AEA53`；79 项逐文件校验，0 不符。
- 普通产品 binary `.tmp/acceptance-20261030-target/debug/acp-remote.exe`：SHA256 `6F0C92B02DEAC8A6857C3834B2309565A00109ACA49CEDF58AF4986E08F1D302`，匹配。
- 当前 Windows PowerShell 执行 `whoami /user`：默认与 require_escalated 均为 `faraday\jack`，SID `S-1-5-21-1850245178-3063344048-767428509-1002`。旧 sandbox 身份不再是本轮默认身份。未取得另一用户凭据或可用执行令牌；未新建账户、重设密码或操作 sandbox 服务。
- 独立编译 target 为 `.tmp/platform-acceptance-20261030/target`，测试程序 `debug/deps/acp_remote-052b60fa8496c74e.exe`；编译产物不是上述普通产品 binary，二者证据分开。

## 本轮真实 owner 命令

复现入口：仓库根运行 `testing/probes/establish-node-access-platform-20261030/owner.ps1`（拒绝复用已存在运行目录）；pair 必须另用真实交互终端；结束运行 `cleanup.ps1`。脚本是保留的复现源码，不把后续未执行运行视为证据。

配置根仅对现有 `faraday\codexsandboxoffline` 增加读取/遍历权限，产品初始化后的 data 仍只有 `FARADAY\jack:(OI)(CI)(F)`。配置直接引用 `.tmp/establish-node-access-tailscale-20261029/` 已有证书路径，未打印或复制私钥。只监听 `127.0.0.1:30453`，origin 为 `https://faraday.tail74842e.ts.net:30453`；本轮不宣称受信浏览器/Tailscale链路验收。

节点 PID `37536`，nodeId `42733b0fe1037a129ed7525ee25b9d995b546da84944e3de3129fcf633d4dca5`。所有命令为指定普通 binary，加 `--config .tmp/platform-acceptance-20261030/node.toml`。

| 命令 | 实际结果 | 判定范围 |
| --- | --- | --- |
| daemon status | 退出 0，state=ready，PID/nodeId/origin 一致 | owner 管理成功 |
| device pair（TTY） | 显示节点指纹/origin/到期，已建真实配对会话；Ctrl-C 后 `Pairing cancelled before approval.`，退出 0 | owner 配对入口成功及取消；配对码不录入报告，无 Web 批准 |
| device list | 退出 0，空列表 | 无 Web 身份也可查询 |
| device revoke --device-id AAAAAAAAAAAAAAAAAAAAAA | 退出 1，`local IPC failed: not_found` | 格式合法但不存在的客户端，已到达管理查找路径；不是拒绝 owner，也不是已信任设备撤销成功 |
| daemon stop | 退出 0，state=stopped；节点 stdout 有 `ACP Remote node stopped` | 正常 owner stop 成功 |

最初使用临时名称及 UUID 字符串作为 device-id 得到 `invalid_request`，纠正为合同 128 位 base64url ID 后为 `not_found`。这两次输入错误不算 owner 权限失败。当前数据没有 Web 信任，故“owner 成功撤销已有可信设备”仍未补齐，不用未知设备响应代替。

第二份 `node2.toml` 使用同一 data、端口30454；第二次 `daemon start` 退出1，`error: data directory is already in use`。第一实例仍可正常 stop，未删锁或未知 PID 抢占。

## 另一真实 SID 矩阵与历史证据复核

本版 status/pair/list/revoke/stop **全部 NOT-RUN**：两类工具执行均返回 jack。当前令牌 `whoami /priv` 没有可用的另一账户登录/令牌启动入口；只读进程查询需要提升权限，提升后看到 sandbox 服务并不等于取得 sandbox 用户执行上下文。本轮不尝试绕过账户边界，不把 jack 的错误路径标成异 SID 拒绝。配置根虽已允许该现有 SID 读取，但这不是其实际读取或产品拒绝证据。

历史对应账户 SID 为 `S-1-5-21-1850245178-3063344048-767428509-1004`。复核文件：

- `testing/runs/establish-node-access/2026-10-29-independent-tester-regression.md` 的跨真实 SID 节及逐例校准；原报告已明确当时 pair/stop、远程管道及失败注入缺口。
- `.tmp/establish-node-access-independent-tester-regression-20261029-204025/pre-rev003-results.md`：保留跨 SID 文件/pipe 的 `UnauthorizedAccessException / 80070005` 与可读262字节密文 CurrentUser DPAPI 的 `CryptographicException / 80070002` 记录；这是历史执行摘录，并非本轮重新执行的逐命令原始控制台。
- `.tmp/establish-node-access-independent-tester-regression-20261029-r2/owner-data-dacl.log`：旧 node2/data 仅 jack DACL；`cross-sid-copy-acl.log`：旧测试密文副本 ACL 操作成功。
- 同目录 `test-lib.log`：118 项通过，其中 DPAPI 新进程恢复、当前用户范围、named pipe DACL/SID/first-instance 均有实际 test 行。属于旧版独立测试日志，不是 r4 新运行证明。

旧 status/list/revoke 在客户端 data canonicalize 阶段 `os error 5`，旧 pipe 直接连接为 `80070005`；不能把前者说成已观察服务端 token SID 错误响应。历史跨 SID DPAPI 仅作为平台历史事实参考，未证明与 r4 完整同版本；当前整例不继承旧 PASS。没有用 SMB/网络失败冒充 reject_remote_clients 成功；本轮没有执行真实远程 named pipe。

## 本版独立动态定向测试

实际在独立 target 运行 `cargo test --manifest-path app/Cargo.toml --locked --lib FILTER`：

| FILTER | 实际结果与覆盖 |
| --- | --- |
| platform::windows::tests::named_pipe_enforces_owner_dacl_sid_and_first_instance（-- --exact） | 1 passed，120 filtered；真实 pipe 的 DACL 只有 owner、first instance 独占、正确预期 SID 成功、错误预期 SID `S-1-5-21-0-0-0-1000` 拒绝；成功及拒绝之后均断言线程无模拟 token |
| identity::tests::（-- --nocapture） | 12 passed，109 filtered，14.44s；含两个子进程辅助测试，不能算12个独立业务场景；覆盖真实子进程崩溃恢复、key-only、meta-only、格式损坏、key/meta不符、未知schema、有历史但无身份、meta写失败保留key、新进程DPAPI恢复 |
| platform::windows::tests::dpapi_round_trip_uses_current_user_scope（-- --exact） | 1 passed，120 filtered；当前用户 protect/unprotect 成功、错误 entropy 拒绝；不等于跨用户解密拒绝 |

这些是本 tester 实际调用冻结源码的组件测试，未将开发方先前运行代签为独立执行；也不代替普通产品 CLI、异SID或完整 L5。错误预期SID测试不是客户端伪造SID报文；服务端使用真实token检查的成功与拒绝路径已运行，但 ImpersonateNamedPipeClient/OpenThreadToken/GetTokenInformation/RevertToSelf 本身失败没有故障注入入口，本轮未模拟。无 thread token 的断言证明正常与SID拒绝后的恢复，不证明 RevertToSelf 失败时仍能安全运行。

## 当前用例整例判断及剩余断言

| ID | 本轮可给出的整例状态 | 明确剩余 |
| --- | --- | --- |
| NA-009 | PARTIAL | 已跑现有真实崩溃阶段与恢复测试；密文临时写入中途、flush中途的实际终止/断电及逐阶段无替代身份仍缺。不能凭已有命名阶段推断中途故障通过 |
| NA-011 | PARTIAL | 新正常启动、同目录第二节点拒绝、正常stop清理已跑；本轮未逐一执行无效origin/TLS、不可写存储、占用端口及每项修正后重试与错误脱敏，旧版事实不自动提升r4整例 |
| NA-012 | PARTIAL | owner五命令及同目录独占、组件SID拒绝与token恢复已跑；缺另一真实SID五命令（pair须TTY）、直接pipe拒绝、本版真实远程pipe拒绝、Win32校验/模拟/revert失败注入；缺有既存信任时owner无Web身份成功list/revoke；未知PID不抢锁仅代码检查，未独立造未知PID运行断言 |
| NA-017 | PARTIAL | 身份恢复/损坏组件测试及owner DPAPI已跑；本版未完成A/B配对、撤销B、重启A成功/B拒绝、复制数据的不可读key与meta损坏启动及跨SID DPAPI动态拒绝。旧版PASS仅保留为历史，不继承为本轮PASS |

本报告不评价其他 tester 的同版本新增证据；主方可按断言与版本映射合并，但不能以本报告补齐未执行断言。无本轮确认产品缺陷，剩余均为验收覆盖缺口，不等于通过或已发现实现失败。

## 清理与交接

`cleanup.ps1` 实际输出：owned_pipe_before=True；stop_exit=0；owned_process_present=False；owned_pipe_after=False；owned_port_listeners=0；exclusive_lock_reopen=PASS。同一owner以 FileShare.None 重开 `.node.lock` 成功；PID37536消失，30453/30454无监听，自有pipe由存在变为消失。未停止其他节点/管道，未占用30451/30452/30447；未启动浏览器或Harness。

首次清理因自动审批审核连接断开未执行，原样重试经审核成功并完成上述核验；不是产品清理失败，未绕过审核。数据库、DPAPI身份和原证书保留，未删除用户数据库/身份或放宽产品data权限。

交主方持久摘要：r4清单79/79与binary初验匹配；真实owner status/list/pair取消/stop成功、合法未知ID revoke到达not_found、同data第二实例拒绝；独立动态组件测试1+12+1通过；另一SID本版五命令与跨SID DPAPI NOT-RUN，真实远程pipe及Win32失败注入未跑。NA-009/011/012/017本报告均PARTIAL，不能据此勾整例完成或到待归档。

交付前再次逐项验证：79项、0不符；普通binary仍为上述 `6F0C…F1D302`。`git diff --check` 退出0，仅提示已有文件LF/CRLF；新增资产均为未跟踪文件，未写索引。未提交、推送或归档。

## 追加：有界证据有效性复核（只读分析，不是新动态执行）

本节修正上文“一概不继承旧版证据”的过严口径及“断电”措辞。复用应按断言涉及的实现、依赖和环境判断，不要求二进制整体字节相同；当前缺少另一SID执行入口不使已成立且实现未变的历史跨SID断言自动失效。本节不宣称新跑另一SID，也不修改用例门禁；旧整例PASS仍须拆解后使用。

### 源摘要链及边界

实际重新读取并比较以下清单：

| 清单 | 清单自身SHA256 |
| --- | --- |
| 2026-10-29-snapshot.sha256（71项，旧独立回归最终快照） | 6B1D981658CE1ACA98D390B3C56650908924B0A7486A55A87836DEEC74200BB9 |
| 2026-10-30-acceptance-r3.sha256 | 351D086478979DFE2BDF4FE7B47B3B9B46FD67058BB786DCB9CE43877EA263A2 |
| 2026-10-30-implementation-r4.sha256 | A1A7F70D36CD1EA7BDA79A25E1F6FFA864487EE7CE046818AB963144761AEA53 |

以下完整文件摘要在上述三份清单及本次读取的当前工作树一致：

| 文件 | 共同SHA256 |
| --- | --- |
| app/src/platform/windows.rs | DF461BE339E8B18AA43D9A44A3E1A6E82B81397BD92C6200174E99B439A3A5A4 |
| app/src/identity.rs | ED6D57D2A023DF0160220B2FFE9F7F63B4848F1262366624909D62929C375A6F |
| app/src/data_dir.rs | DBD57605D59201DEF4F95AD5F41C9A1F85B6881211644DE539DA15B570F613C5 |
| app/src/ipc.rs | E65826C1B56289D45E4832FEB5F2592A28050A702ED52D1FC989E05E1446162A |
| app/src/config.rs | FDDF58A7F94841152598FD90286B59DEFA746B117D90FE46FE266139D6FD346F |
| app/src/storage.rs | B6532B45DE877CD2D8F15FA79F36AE18A29CB611C06BA31B5A8865EA7723515B |
| app/src/test_support.rs | 98C39D7A413986D5BB9F7365A513EEF979F25E48DCB21B59B7C2B3B3CC3AA244 |
| app/Cargo.lock | 28639ACDBA0A6C445FCA88F9191E3E1484DEECA21E63F0421BD2E2DFD9AB6B5A |

没有找到已冻结的r5命名清单，因此对r5仅陈述“本次当前工作树相关文件与上述摘要一致”，不签署r5整包固定版本验收。当前platform/mod.rs为 `45CCCB01874F16BF883402F356226D5E0AB820633E47DD0B74B85798AFCA5B04`，console.rs为 `6D011E5A617714D8E836F4B9DC159DB33BE897C1EEC5EAEC176E624502AFA717`；mod仍直接从windows导出DpapiProtector及owner检查，从独立console模块导出控制台类型/读取函数。当前main与r4摘要也不同。console/main变化影响真实CLI交互及其退出路径的证据，不能使未改变的DPAPI、文件DACL和直接pipe连接拒绝证据整体作废；亦不能反过来用这些底层证据证明新版pair/stop交互全部通过。

原始材料定位与完整性：旧最终报告SHA256为 `074180AE7C49E66D5F49731BE9C70BAC502A27082B0C9E40E5BF3654D411BC7F`；r2/test-lib.log为 `74936E83AFE65754F8EE16B3B84B67A2427FECE332876820D6A1AA2A5B0A21AC`。pre-rev003-results.md为 `39A99561FF66793DFC65E12810D97EB8067EB6A2E540315CD9FEE5D8CC7879A2`，其自身明确只标识更早70项快照，并警告当时7项变更，不能将其单独绑定到最终71项版本；跨SID断言的主绑定采用最终独立报告记录的71项匹配、实际SID与原生错误，pre-rev003摘录只作一致性旁证。摘录不是新增逐命令原始控制台。本节没有因事后计算摘要就声称证明了历史采集过程。

### 按当前Steps/Expected逐断言复用

| 当前断言 | 旧证据实际做到什么 | 本次有效性判断 |
| --- | --- | --- |
| NA-012异Windows用户不能读取data及身份材料 | 真实1004 SID打开.node.lock/identity.key/node.sqlite3均80070005，data DACL仅1002 SID | 可复用为异用户文件边界；data_dir/windows主体及依赖锁未变，账户SID和Windows平台边界相同。测试目录路径/端口不同不改变这个断言 |
| NA-012异用户daemon status/device list/device revoke拒绝 | 最终独立报告分别记录退出1、canonicalize数据目录os error 5；更早摘录只有status/list，不将摘录扩大 | 可复用这三条“实际CLI被拒”结果及共同owner-data gate断言；拒绝发生在客户端目录访问，不能说服务端token比较已运行。当前完整CLI版本无新跑，仍标“历史复用”，非r5二进制五命令回归 |
| NA-012异用户直接owner-only pipe拒绝 | 直接连接标记status/list/revoke均80070005，协议分派前已拒绝 | 可复用为pipe DACL连接拒绝；其拒绝与command无关，但不能据此说另一SID实际执行过pair/stop命令，更不能声称收到对应服务端错误帧 |
| NA-012服务端token SID双校验、伪造SID/校验后恢复 | 旧named_pipe测试成功与错误预期SID均检验线程不再模拟；本报告又在r4独立执行1项通过 | 可复用并有r4新执行佐证；这是实际token与错误expected SID不匹配，不是伪造网络SID报文，也不补Impersonate/OpenThreadToken/GetTokenInformation/RevertToSelf自身失败注入 |
| NA-012另一SID pair/stop及远程pipe、模拟失败 | 旧最终报告明确这些未执行 | 不可补成通过。当前另一SID执行环境阻断保留；远程SMB连接失败不证明产品reject_remote_clients |
| NA-012 owner无Web身份list/revoke、同目录独占 | 旧owner控制、锁独占，r4有新CLI与未知ID not_found证据 | 底层控制资格与独占可复用/有新佐证；成功撤销已有可信设备不能由未知ID替代。完整操作结果结合其他同版本信任记录判断 |
| NA-017 DPAPI跨用户不能解密 | 另一真实SID对可读262字节密文副本，以准确entropy acp-remote/node-identity/v1调用CurrentUser DPAPI，80070002且无明文 | **可复用该Expected子断言**；不是文件ACL挡住解密，密文副本已可读，DPAPI/identity上下文/用户作用域及锁依赖不变。无需因当前不能新开1004令牌而一概重列缺口；这不证明1004自身DPAPI初始化健康，也不证明r5整例A/B重启 |
| NA-009身份损坏/只有key/meta/不匹配/未知schema及既有crash阶段 | 旧独立日志通过，identity/storage/test_support摘要不变；r4本报告12项又实际通过 | 可复用具体断言，不重复计为缺失；写/flush阶段不足见下节，不以旧报告整例状态代替步骤核对 |

因此，NA-012仍PARTIAL，但缺口应收敛为旧证据从未跑过的真实另一SID pair/stop、远程pipe、失败注入等，以及受console/main变更影响而尚未在最终二进制确认的交互路径；不能再把已有三命令拒绝、文件/pipe DACL和DPAPI跨SID子断言全部列为“无有效证据”。NA-017的跨SID DPAPI Expected现有可复用证据，移出本报告“尚缺”清单；整例A/B及复制数据损坏启动的有效性需按其他相关实现与对应执行记录另行合并，本次不扩展签署。

### NA-009原步骤与crash hooks精确覆盖

当前原文仅要求“在隔离目录的密文临时写入、刷新、更名、meta事务各阶段中止进程”，没有硬件断电要求，也没有要求终止恰好发生在单个Win32系统调用内部。上文“断电”不应成为验收门禁，予以撤回。`test_support::crash_at`会打印阶段标记、flush stdout并调用`std::process::abort()`；父测试确实启动子进程，要求非成功退出且出现对应标记，随后重开storage检查状态。这满足真实进程中止，不是仅抛出可恢复错误。

| 必要阶段 | 实际钩子位置及断言 | 覆盖判断 |
| --- | --- | --- |
| 临时密文写入 | persist_private_key的write_all(ciphertext)后立即串接sync_all；在二者前或之间均无crash钩子 | 尚无“写入尚未完成”或“写完但未刷盘”独立中止观察；不能从rename前钩子推出此阶段已覆盖 |
| 刷新 | identity-before-key-rename位于write_all与sync_all均成功后、drop临时文件前 | 已覆盖刷新完成后的边界；未单独区分刷新前/尚未确认刷新完成的窗口。不要求杀进sync_all内部或模拟硬件丢缓存 |
| 更名 | identity-before-key-rename：正式key/meta均无，遗留identity.tmp-*，重启可完成新身份；identity-after-key-commit：rename成功、meta未写，重启key字节保持不变 | 更名前后已覆盖。前者正式身份尚未提交，允许首次初始化完成，不应误称为替换已有信任根 |
| meta事务 | identity-before-meta-commit位于事务INSERT执行后commit前；identity-after-meta-commit位于commit成功后 | 未提交事务回滚、已提交事务恢复均覆盖；key保持原密文，分别检查meta无/有。不要求在SQLite内部commit指令中间精确杀进程 |

判断：现有钩子覆盖rename及meta必要边界，并覆盖write+flush完成后的合并边界；不足以说写入与刷新两个阶段已经分别中止。NA-009保持PARTIAL的有据缺口是缺少这两个阶段可区分的执行证据，**不是缺硬件断电**；也不附加“每个字节/每个系统调用中途”穷举要求。

建议最小测试专用钩子（未实施、未运行）：将测试路径密文写分两段，在非空前缀写完后设置一个abort/可控栅栏；在write_all完成但sync_all尚未调用前设置第二钩子。保留原刷新完成/rename前钩子，即可区分部分临时密文、完整未刷新临时密文、已刷新待更名三状态。每轮对子进程要求命中标记且确实终止，检查无正式key/meta的半提交状态、历史记录未清空，重启只恢复已提交身份或合法完成首次初始化；对已有key/meta另沿用损坏失败断言。不必为此增加真实硬件断电、驱动级flush中断或产品常态故障开关。用户本次要求仅授权分析，不能当作这些钩子已经存在或测试已通过。

本次仅追加报告，未执行新动态测试、创建账户、改变ACL或修改源码。本节的断言级复用与缺口收敛优先于上文笼统的旧证据失效表述；不会据此将NA-009/012整例提升为PASS。

### 平台证据复用分析收尾

主方可沿用上表有源码SHA256链支持的跨SID子项：私有data/文件访问拒绝、owner-only pipe连接拒绝、旧status/list/revoke实际拒绝，以及可读密文上的CurrentUser DPAPI跨用户解密拒绝。platform/windows.rs、identity.rs、data_dir.rs、ipc.rs负责这些断言的字节保持一致；这属于历史证据按断言复用，不是本轮新增跨SID执行，也不是r5a整包签署。

当前冻结版本仍缺另一真实SID的完整五命令矩阵（daemon status/device pair/device list/device revoke/daemon stop）；其中旧status/list/revoke拒绝子项可复用，但不能因此称本版五命令均已实际执行，旧pair/stop从未补齐。真实远程pipe拒绝、模拟/token检查及revert自身失败注入仍缺，不能用SMB环境失败或正常token恢复测试替代。另一SID当前不可执行是环境阻断，不创建账户、不更改现有账户身份或正常用户ACL。

prefix-write与pre-flush测试专用钩子及真实子进程崩溃矩阵留待下一工作包；按用户指示保持当前r5a评审冻结，本轮不修改产品、测试源码或生产IO路径，不新增常态开关，不追加驱动内部中断或硬件断电验收要求。此平台分析任务至此收尾；NA-009/012剩余断言保留未完成，不以收尾替代验收通过。
