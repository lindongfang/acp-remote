# implementation-r4 独立交付检视

> 最新修复复查见文末“r5a 最终定向复查”；下文 r4 的 Open/BLOCKED 保留历史事实，不代表这些代码问题仍开放。

日期：2026-10-30。结论：**BLOCKED**。I01 的挑战清理修复已独立验证；审批门关键集成与定向回归通过。发现两项需修问题：Console 输入事件被有损转换，以及直接 Drop 节点未同步关闭公开 policy 的 ready 状态。真实 TTY 的 NA-PROTO-001 仍待 tester 独立复查，本报告不代签整包或整体 change 通过。

## 固定版本与写入边界

- 清单：`testing/runs/establish-node-access/2026-10-30-implementation-r4.sha256`；自身 SHA-256 独立计算为 `A1A7F70D36CD1EA7BDA79A25E1F6FFA864487EE7CE046818AB963144761AEA53`。**79 个唯一条目，79/79 匹配**；结束前再次核对仍 0 mismatch。
- 相对 closeout：新增 access_policy.rs、platform/console.rs；修改 Cargo Console feature、lib、main、management_ws、node、platform/mod、pairing 及 proposal/design。未将源码实施中的摘要或主方全绿直接视为当前独立验收。
- 用户授权本包按 spec 实施，当前无新增人工签署要求。reviewer 只写本报告；构建和临时探针二进制限定 `.tmp/reviewer-implementation-r4-target`，未改产品/规格/plan/tasks、设计报告、历史清单或其他报告，未提交/推送/归档。

## NA-R4-CON-001：Medium / Open — 有损控制台解码可把其他输入变成批准词

位置：`app/src/platform/console.rs:34-44`；`app/src/main.rs:261-267`、`:270-304`。

新 reader 对每个 KEY_EVENT 仅读取一个 UnicodeChar，没有处理 wRepeatCount；UTF-16 高低代理项各自传给 char::from_u32 后变成 None。上层将 None 当作没有字符，保留此前 answer，最后 trim/lowercase 后只检查 y/yes。它不区分非文本焦点/键释放事件和无法表示的文本字符。

已执行受控事件复现：Rust 探针直接通过 `#[path]` 编译当前实际 console.rs，只用内存 Windows API 替身提供 INPUT_RECORD 的相关字段，保留真实 reader 控制流。探针编译、执行退出 0，观察为：

```text
输入 KEY_EVENT: UnicodeChar='y', wRepeatCount=3；随后 Enter
actual_reader="y\r"; normalized="y"

输入 KEY_EVENT: 'y'；U+D83D；U+DE00；Enter
actual_reader="y\r"; normalized="y"
```

因此 reader 已确认丢失重复次数及非 BMP 文本，输出与单独输入 y 相同；按当前 main 的确定性批准表达式会被归为批准。测试没有模拟真实 Console 内核、真实粘贴或完整 TTY，本报告不声称真实终端已复现该批准。这里的风险也不是远程绕过本机确认，而是本机输入被静默压缩为另一条批准决定。

最小修复建议：事件表示应保留文本重复次数；对不能正确表达的文本/UTF-16 序列，至少使当前决定失败关闭，不能静默丢弃后仍批准。可以完整处理 UTF-16 与重复键，也可以在仅接受 ASCII y/yes 的确认边界把不支持的文本标为无效并拒绝；仍可忽略纯键释放/窗口/鼠标等非文本事件。不要靠忽略整类文本来“修复”。

关闭条件：对真实 reader/行累积逻辑补受控事件回归，确认普通 y/yes、默认 Enter、n、退格正常；重复 y、y 后非 BMP 输入、孤立代理项和过长输入不能被丢字后批准。由 tester 在真实 TTY 复查 Ctrl-C 不需额外 Enter 即返回，且普通批准/拒绝输入不回归。不能只以 Cargo/Clippy 通过关闭本问题。

## NA-R4-LIFE-001：Medium / Open — NodeRuntime Drop 后 policy 仍存在接受新挑战的窗口

位置：`app/src/node.rs:401-413` 的 Drop；`app/src/access_policy.rs:149-163`、require_ready；`app/src/management_ws.rs:131-133`。

正常 shutdown 会先 control.request_stop，再收敛资源，新的 policy ready 校验对此正确。但 Drop 只调用 management.begin_shutdown 并 abort 网络/IPC 任务；begin_shutdown 只关闭连接注册表，不更新共享 IpcControl 的 Ready 状态。异步任务取消不在 abort 调用点立即保证析构；它们暂时仍可强持有 PairingService。外部 ApplicationPolicy 因此可能在 NodeRuntime 已 drop 后仍 upgrade Weak 并签发挑战。

已通过真实产品 API 动态复现（非纯状态模型）：

1. reviewer 自有单线程 Tokio runtime 中启动隔离 NodeRuntime，使用真实 TLS 证书、SQLite、当前用户 DACL/named pipe；仅 SecretProtector 是明确的 fixture，非真实 DPAPI，不触及用户身份。
2. 保存 application_policy() 返回的外部 Arc，直接 drop(node)。
3. 在给取消任务下一次调度前调用 issue_authentication_challenge；实际返回 Ok：

```text
DROP_RUNTIME_NEW_CHALLENGE_ACCEPTED=true
```

探针随后丢弃新 lease/policy，等待取消任务调度并删除已核对属于 reviewer 独立 target 的受保护临时根目录，进程退出 0；未使用用户 30447 节点。该结果证明的是 Drop 退场路径窗口，不是正常 owner stop 失败，也未证明运行时能力可派发（当前审批仍拒绝）。

最小修复建议：NodeRuntime 的 Drop 最先将同一 control 标为停止，再执行连接关闭/abort；保持操作幂等，不能在 Drop 中尝试异步阻塞 join。正常 shutdown 的有界收敛保持现状。若其他退场路径暴露 policy，也应复用同一停止事实。

关闭条件：在单线程执行器下持有外部 policy/with_clock 句柄和 lease，drop NodeRuntime 后不先 yield 就调用 issue/auth/require_current_actor，必须 NodeUnavailable；随后允许任务收敛，确认资源释放。已有正常 stop 下外部句柄存活回归仍应通过。

## I01 及应用策略集成复核

**NA-APR-I01：Closed（当前 lease 清理范围）**。

- AuthenticationChallenge 没有外部可写字段或 Clone；connection_id 由系统随机源内部生成，不接受调用方指定 scope；Drop 只清自己的 Auth scope。lease 和 policy 都弱引用 PairingService，with_clock clone 同一 TrustStore/control，不强持有节点身份私钥。WSS 原 AuthChallengeCleanup 保留。
- 独立产品 API 探针生成两个不同 lease，签署真实 AuthProof：丢弃第一个后原 proof 返回 Proof 错误；第二个 scope 的有效证明仍成功消费，随后因客户端未登记被真实 Store 返回 InvalidActor；再次提交第二个 proof 返回 Proof 错误。分别验证立即清理、作用域不互伤及一次消费，不把“未知身份拒绝”单独误当挑战清理成功。
- 同一探针在持有外部 policy/lease 时正常 owner stop 和 wait_for_stop 均完成，之后新 challenge 返回 NodeUnavailable；再丢弃旧 lease 不阻塞。它验证 Weak 与正常停止组合；直接 Drop 的例外已另记 LIFE-001，不混为 I01 未修。

应用策略关键检查：

- NodeRuntime 实际装配服务，WSS 实际复用 authenticate/require_current_actor；不是测试自己模拟资格或无人调用的 helper。actor 从 PairingService 真实一次性 proof 与 Store 产生，字段对外不可构造，绑定节点/ID/key/generation。
- runtime request 取得与撤销共用的 client_gate，再校验真实当前资格；require_current_actor 不重复取该锁，WSS writer 已持锁时亦不发生新增递归锁。没有发现该包引入重入死锁。
- Missing、expires_at <= now、future 但无 issuer 均在 dispatcher 前返回错误；with_clock 不提供授权来源。未新增公网 action 或页面，现有三个管理 action 没有被错误要求运行时批准。
- 保留本地 NodeIdentity 的强引用只在原节点、PairingService/管理任务；policy/lease 仅 Weak。SqlitePool clone 本身不是泄漏证据，正常 Storage.close 会关闭共享池；已通过持句柄正常停止验证。异常 Drop 的 Ready 生命周期需修，不能由 Weak 结构正确替代。

## Console 取消修复的正向观察及限制

- 不再使用阻塞 stdin 工作线程；read_approval_line 每轮 sleep 20ms，外层保留 Ctrl-C select；没有 process::exit、SetConsoleMode 或关闭借来的标准输入 handle。GetNumber 后只请求读取一个记录，避免逐行读取等待 Enter。
- GetNumber/Read 的“非阻塞”以当前进程独占读输入为前提；若同一个 console 有另一个 reader，在检查与消费间可发生竞争。未复现该条件，本报告只保留假设边界，不新增已确认阻断；不能把 count/read 两个调用描述成 OS 原子非阻塞 API。
- 未执行真实 TTY、ConPTY 的 Ctrl-C/拒绝/退格/粘贴验证；NA-PROTO-001 由原 tester 对固定 r4 真实终端复查。不得因这里已删除 stdin 线程或全量单测通过，就把它自动标为独立关闭。

## 独立检查与证据

| 项目 | 本轮结果与边界 |
| --- | --- |
| r4 清单 | 独立重算 79/79，前后匹配 |
| cargo fmt --all --check | reviewer 独立通过 |
| cargo clippy --all-targets --locked --offline -- -D warnings | reviewer 独立 target 通过 |
| cargo build --lib --locked --offline | reviewer 独立 target 通过，用于链接只读资源探针 |
| runtime_approval_gate_is_independent_of_real_management_authentication | reviewer 独立定向 1/1 通过；真实 proof/Store、Missing/Expired/Unverified、spy 正对照、零派发、重新认证、撤销与外部句柄下正常 stop；SecretProtector 是测试 fixture |
| 额外 lease/Drop 产品 API 探针 | 清理/隔离/重放/正常 stop 断言通过，Drop 后新挑战接受问题已复现；退出 0 并清理自有运行目录 |
| Console 实际 reader 的受控事件探针 | 两组输入均输出 y/Enter，记录 CON-001；Windows API 为内存替身，不是 TTY |
| git diff --check | 通过，仅既有换行提示 |

定向节点测试首次在受限进程中因自有 data-directory lock 权限拒绝失败；随后经批准在真实 Windows 用户上下文重跑通过。未改 fixture 绕过 DACL，也未将初次失败称为产品回归。额外真实节点资源探针同样在获准上下文中运行。

主方原始 `.tmp/implementation-r4-check.log` 已读取，SHA-256 `75EB74C1BD9C81F6F04360C99C256E145E80A9D8DAE9E579513E800EAD2356C6`，记录库 121、Web 46、JWS 4 全量通过；这是主方实际执行证据，与 reviewer 上述独立执行分开，不用于填补尚未执行的真实 TTY 或其他整个 change 的断言。Web 产品未改，本轮未为该 Rust 包重复运行浏览器全矩阵。

独立探针均在命令内存中编译，生成物仅在 reviewer target；临时源码没有进入冻结快照。模拟 Console 布局只包含 reader 访问字段，编译产生的 unnecessary-unsafe 警告属于替身布局，不是产品 Clippy 结果。资源探针的 fixture protector 仅用于隔离测试，不宣称 DPAPI 验收。

## 关闭与后续复查范围

当前包 BLOCKED：CON-001、LIFE-001 需修；NA-PROTO-001 真实 TTY 独立结论和 NA-010/030 tester 证据尚待形成。I01 可以关闭，不能一并关闭整个 change 的预算/故障/供应链等历史门禁。没有新增用户人工签署要求。

修复后冻结新摘要，原 reviewer 优先重跑 Console 事件解析、Drop/正常 stop、lease 与审批门定向回归；真实 TTY 交原 tester。仅涉及本包的证据重验，不因本报告扩大公网协议或假造未来审批成功路径。

## 附加 JWS 证据检视与有界独立补验

本节应用户追加请求核对主方同日浏览器 JWS 证据，不扩大到全部协议组合，也不改变上述 Console/Drop 问题或整个 change 的验收状态。再次核对 r4 仍 **79/79 匹配**；产品源码未改。

### 主方真实/模拟边界

- `.tmp/jws-matrix-20261030.json` 实际记录 userAgent 为 **Chrome/156.0.0.0**，不是 157；secureContext=true。执行入口源码 `-01/-02` 显示从 localhost Vite 4175 切到 4176。localhost 安全上下文足以运行真实 WebCrypto，这不是本包受信 TLS/Tailscale 部署验收，不能据该字段声称 HTTPS 证书路径通过。
- `jws-browser.ts` 导入真实产品 jws 实现和公开 fixture，实际调用 WebCrypto import/sign/verify。现有页面 7 项结果均 true：固定 Rust/WebCrypto/high-S 签名、浏览器新签名、自签自验与三项篡改/编码拒绝；这是主方浏览器开发执行，reviewer 只读核对该事实，未重新打开 Chromium 或接管已结束 Space6。
- `-03.mjs` 在真实页面动态 import `/src/protocol/jws.ts`，对同一公开 fixture 构造受控变异，逐项要求真实 JwsError；若应拒而接受或报错类型不对，脚本会抛错而非添加 passed=true。结果完整计数为 25 条、0 条 false，与脚本循环数量对应。
- 25 项主要是严格解析/字节编码与无效 key 验证，不是 25 种线上认证攻击已执行。unknown/JKU/crit/none/cross-purpose 是 envelope 类型拒绝；没有测试这些地址的真实加载或所有证明用途/连接组合。
- `der_encoding` 具体是 70 字节零数组，验证错误签名长度被拒，不能扩写成真实 ASN.1 DER 结构变异矩阵；`compressed_key` 是 33 字节零数组，证明该长度被拒，不等于测试了合法压缩曲线点。off_curve 为前缀 04 加 64 个零，确实走无效曲线点拒绝。浮点/指数项使用 1.0/1e3 原始 JSON 文本，保留了编码变异。
- 新签名使用已交付公开 fixture 的私有 JWK，在 WebCrypto 以 extractable=false 导入。该 key 是公开测试材料，不是用户实际身份；保存的 compact proof 不应作为生产凭据使用。脚本只把签名 proof 另存 .tmp，结果 JSON 不复制 JWK 私钥。

### reviewer 独立执行

1. 在 reviewer 独立 r4 target 内，用内存 Rust 探针链接已核对的实际产品库，读取 `.tmp/jws-browser-generated-20261030.txt` 并调用 `claims::verify_auth_client`。**通过，退出 0**；完整反序列化 AuthProof 等于公开 fixture.payload，compact 的原始 headerSegment/payloadSegment 也逐字相同。没有输出 proof 或密钥正文。
2. 读取相同 `-03.mjs` 的 25 个负向输入/断言段，用现有 Vite 内存转译实际 `src/protocol/jws.ts`，在 **Node/WebCrypto** 重跑。**25/25 通过，退出 0**；名称、错误码及结果与浏览器 JSON 精确相同。此独立补验不冒称为 reviewer 的 Chromium 执行。
3. 对浏览器页 7 项结果计数、全 true 及原脚本断言方式完成独立静态核对；没有把主方开发角色改写为独立 tester。

Rust 此处验证签名、用途与 payload/schema；`verify_auth_client` 不是当前 live challenge registry 的授权接口。公开 fixture 的旧时间字段不意味着已在真实连接通过新鲜性、一次消费或管理认证，不能据该 probe 声称重放/过期检查通过。

### 证据摘要与结论

```text
89E2F15379B1E4CDF6A994644998392EDB12A262B30867559E9A27BB9D2096DC  .tmp/jws-matrix-20261030.json
74C9B27B64C9522B69D5DD4BBB7B032A260F3C883E4DC5CA35D5D49FEAD626A9  .tmp/jws-matrix-20261030-03.mjs
A7D989711FAD618FA31E3E8E6FB44F723C77683B79BB484711A63AD461A2E244  .tmp/jws-browser-generated-20261030.txt
```

可接受“主方真实 Chromium 固定向量/25 项变异开发证据 + reviewer 对同一浏览器签名的 Rust 独立回验 + 相同负向集合 Node/WebCrypto 独立重跑”这些子范围。未发现该证据集中新的确认产品失败。它收缩密码学互操作证据缺口，但没有补齐单独 r/s 零值/全部越界、有效 DER 结构、payload 闭合字段等全部原必要矩阵，也没有独立 tester 的整例重判，故 **不将 NA-003 升级为整例 PASS**。

脚本包含 finish 自有空间，主方报告已停 Vite；reviewer 此次观察 4175/4176 无 LISTENING。未访问用户日常浏览器、30447 或其身份；没有为补验重新启动浏览器/Vite/节点。

## r5a 最终定向复查（2026-10-30）

本轮结论：**PASS（CON001/LIFE001/SC001/SC002 修复限定范围）**；整个 change 验收仍 **INCOMPLETE**，此节不签署可归档。只更新本报告，未修改产品、规格、其他报告、清单、plan/tasks 或用户资源。I01 原关闭结论保持。

### 最终版本

独立计算 `2026-10-30-implementation-r5a.sha256` 自身 SHA-256 为 `96F7B25DA1B7ABF3C5F65D069723CA87B095800F08967720AADBD4FC621975EC`，79 项全部匹配。`.gitleaks.toml` 为 `8302661F460B8BF65B329B6678B73F371AC34773882C46638BCAE360A2C58C18`。候选 r5-product 曾核对 78/78，但随后测试常量减法改为 saturating_sub；不将候选清单视为最终版本或全量通过证据。

主方全量日志 `.tmp/implementation-r5a-check.log` 摘要 `D854A419B2751DF35CE550B0C39EC76AB2E4BD3C927E69DDD8AF1D0255F9F75B`，实际含 Web46、Rust124、向量4通过；这是开发方执行，非 reviewer 全量重跑。交付 exe 摘要独立核对为 `2DC0AFA04B55376797587136048F5EFBE7D665FF7EE69525DA248FA4D73305F2`。

### 原问题关闭依据

- **NA-R4-CON-001 / Medium / Closed**：`app/src/platform/console.rs:10–28,68` 保留 repeat_count，将不能表示的 UTF-16 和非零文本的非法 repeat=0 转为 U+FFFD；`app/src/main.rs:276` 遍历重复次数。独立编译真实 reader 源码，仅替换 Windows API 输入为受控事件；重复 y、y+成对代理、y+孤立代理、repeat0 分别输出 `yyy`、`y��`、`y�`、`�`，均不会变为 y/yes。最终源码对应 reader 再次执行通过。此为真实解码路径的受控传输复现，不声称真实 TTY 键入或完整 CLI 动态验收；非文本0、key-up仍忽略，未新增线程。
- **NA-R4-LIFE-001 / Medium / Closed**：`app/src/node.rs:403` Drop 首步同步 request_stop，再关闭管理入口、abort任务。对最终 r5a 在 reviewer 专属 target 独立重新编译并执行 `dropping_node_closes_policy_before_aborted_tasks_can_run`，1/1通过。默认单线程 Tokio；真实 NodeRuntime、SQLite、管道、真实 proof 形成 actor，Drop 后不 yield，issue/auth/current 均为 NodeUnavailable。候选复跑首次沙箱目录锁 AccessDenied，真实用户运行通过；最终 r5a 同样真实用户通过。使用测试 SecretProtector，不据此声明 DPAPI 新验收；没有触碰30447节点。
- **SC-001 / Medium / Closed**：11项 allowlist 均为 AND、指定规则、secret 完整值和完整路径锚定，无目录放行。读取配置和控制脚本，并独立重算历史 Git blob 的30项公开材料来源（固定序列、公钥指纹、公开 UUID），全部分类通过；当前JWT此前已独立验签为合成 fixture。重新运行26组合/130项正反控制及26项无关规则 canary，最终全部通过；结果摘要 `4FFED5897BFD8E7492B05B4DF338A308235CB2B77F3955F2571170E625B1DEE5`。首次原脚本随机 changed 标记在 UUID 路径未触发默认规则，不能算通过；首次固定00..31替代又恰好命中其他已有精确例外，亦不算通过。最终使用不在例外集合中的固定FF..E0合成字节，所有 changed/outside/prefix/suffix 均命中，精确允许项不命中。这是控制数据修正，不改产品配置；未发现新的配置漏洞。
- **SC-002 / Medium / Closed**：`app/DEPENDENCIES.md:28` 已明确 hyper-util 0.1.21 必要性、MIT、Windows验证范围、维护来源和连接任务所有权；与 manifest 的 http1/server/service/tokio 及锁文件版本一致。关闭文档遗漏，不扩展为全部上游维护/许可证联网认证。

### 最终扫描与复用边界

使用已校验官方 Gitleaks8.30.1 二进制，ZIP 本地摘要与供应链来源记录一致 `D29144DEFF3A68AA93CED33DDDF84B7FDC26070ADD4AA0F4513094C8332AFC4E`。独立从 git cached+others+exclude-standard 枚举、复制当前446文件到 reviewer target，自副本根目录扫描：约6.58MB、0命中、exit0。独立本地 `git --all` 历史扫描：331 commits、约54.32MB、0命中、exit0。两份原始空结果摘要均为 `37517E5F3DC66819F61F5A7BB8ACE1921282415F10551D2DEFA5C3EB0985B570`。证据在 `.tmp/reviewer-implementation-r4-target/sc001/`；不覆盖被忽略的临时真实私钥、远端未取回历史或运行时秘密泄漏。

Rust公告扫描复用独立供应链报告：相同 Cargo.lock 摘要 `28639ACDBA0A6C445FCA88F9191E3E1484DEECA21E63F0421BD2E2DFD9AB6B5A`，官方固定公告提交、261包/1386公告、0漏洞；官方ZIP摘要已读。该项不是下载仍失败：工具已成功取得并实扫；但未查yanked、未证明实时最新公告或包内容无投毒。没有重新安装或扩大联网检视。

### 未代签范围及验收约束

本节仅关闭上述四个问题。独立 tester 的真实 TTY/Ctrl-C、NA010/030审批 probe 及当前33例最终逐断言结果仍需由对应独立报告汇总；不以旧4/25/5等统计代替当前结论，也不因这四项关闭而自动宣布整包通过。

新增默认 shutdown 故障测试确实使用 DPAPI、owner stop、Storage.close barrier 和默认15秒预算（14秒清理+1秒响应预留），检查错误与 Failed 而非成功；本轮未独立重跑该14秒用例。主方 `<17秒`容差不是严格15秒响应上限证明，需以真实计时与生产期限路径解释；原 `>=15` 红灯属于断言预算误设，不记产品故障或声称实际等满15秒。剩余阻断必须逐条对应当前 Steps/Expected：NA009要求阶段进程中止，不新增真实断电或无穷组合；未变平台字节可按明确断言复用历史SID/DPAPI证据，五命令、远程pipe等实际必要缺口不得隐去。用户已通过当前NA033，不恢复旧人工签署门禁。

## NA009 最小测试 seam 方案检视（r5a 后设计，不是实施验收）

结论：**PASS（设计限定范围）**。当前 tests.md:97–101 明确要求密文临时写入、刷新、更名、meta事务阶段中止进程；identity.rs:201–213 现有中止点在 sync_all 完成后，确实不能替代写入中和刷新前。新增仅 cfg(test) 的 partial-key-write 与 before-key-flush 两点合理，不新增硬件断电门禁；非test原 write_all.and_then(sync_all)、DACL与错误清理顺序应原样保留，无生产环境开关或新API。

实施必要断言：使用专属新目录和真实当前用户DPAPI子进程；stdout明确命中对应点且先flush，再验证非零中止，不能把启动失败/普通panic当命中。partial点应在成功写入严格非空前缀后、后半段写入前；flush前点在完整write_all后、sync_all前。两次write错误仍须走原错误处理，不能中止在未成功写入处。父进程检查正式key不存在、meta记录不存在（不要求SQLite数据库文件不存在）、本轮唯一非空临时密文保留；进一步确认partial点前缀长度与实际临时文件长度相符，且小于完整密文长度，避免仅“非空”掩盖seam位置错误。长度可作为非秘密元数据，不输出密文或私钥。

重启应按首次初始化成功，随后再次reopen比较node_id和完整公钥一致；不能把临时前缀当正式身份恢复。其依据是此时从未有正式key/meta提交，不要求与被中止进程的未提交私钥相同。不得为测试手动清空目录或预删临时文件；保留标记文件等原数据验证未被清空。对已有正式身份路径，继续复用/执行同公钥key-only补meta、meta-only/不符/未知schema拒绝及身份不变断言，不把本次新目录恢复结果扩展为覆盖这些旧身份场景。现有afterflush、更名后及meta提交前后点继续保留。

边界：中止进程可验证应用崩溃一致性，不证明实际介质持久性/断电；before-flush并不证明OS尚未后台刷新。两个新点本身不足以签署NA009全例，需与当前其余必要断言证据合并。本次未修改源码或执行尚未实施的新回归；r5a既有关闭结论不变，等待tester收尾期间不改产品。

## NA003 新增编码与载荷边界证据复核（r5a 产品未改）

结论：**PASS（本批子断言）**，交由 tester 合并判断 NA003，不自动关闭整例。读取 `.tmp/jws-extra-20261030/{Cargo.toml,Cargo.lock,src/main.rs}`、生成JSON、两份browser脚本及结果。生成JSON SHA256 `CBED0299298DB35D4A6370E4942572F134CD9C85F5C3C6B1D685F80AD80EDDFF`；浏览器结果 `08957F91A47EFDDB9EA089B4D8A0E48CCB92DFE8958D88D82AABC23DC4C7F51E`；02脚本 `ACE7BAEAF135B9119DABA3B87A0B6D35076E55F925ACC8942DEE4DF23CADBE8E`。

已独立将该Rust生成器经stdin编译、链接reviewer target中的真实r5a库，仅改输出到reviewer专有文件，执行exit0。真实Signature.to_der、有效压缩SEC1、normalize_s均来自P256库；合成[08;32]/[07;32]公私钥对应fixture断言通过；unknown客户端载荷由正确同key签名后交真实claims::verify_auth_client拒绝。不是零数组伪DER/伪压缩点；未输出签名、私钥内容或使用用户身份。

又独立通过Vite仅middleware/SSR加载真实jws.ts与management/protocol.ts，无监听端口，按原脚本执行Node WebCrypto：low/high-S接受、真实DER/压缩点拒绝、r/s分别0/n/n+1拒绝、合法hostProof正对照与正确key签名unknown host payload拒绝，12项实际执行结果与浏览器JSON相符，server finally关闭。第13项浏览器条目仅检查Rust生成的boolean，不是浏览器自己调用Rust；其真实Rust执行本轮已另行独立补验，不能把13项均描述为浏览器协议执行。

开发浏览器证据为Chrome156、localhost4176 secureContext。hostProof使用fixture时间注入，finally恢复Date.now；正对照和unknown字段负例使用同一时钟/合法签名，支持载荷封闭性结论，不代表线上challenge注册、实时过期、网络认证、TLS/Tailscale或重放验收。reviewer没有接管已finish的Space7、没有重开浏览器，不冒充独立Chromium执行。此前空数组样本的覆盖限制在这些具体DER/压缩点及标量边界上已得到补充；不扩展为全部协议组合或NA003整例通过。未发现本批新增产品缺陷。
