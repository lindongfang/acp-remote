# establish-node-access 独立供应链补充验收

收尾记录（2026-10-30）：收到主方通知，配置 `8302661f…` 已纳入 r5a 的 79 项清单，SC-001 原 reviewer 正在独立复查，SC-002 文档及 r5a 构建检查由主方处理。这些是交接信息，不作为本会话新增扫描或复核结论；本报告的当前文件扫描只覆盖下述实际复制的 444 文件版本，不扩展到 r5a，也未再次扫描主方工作目录。本次收尾未发现新的扫描命中、未新增例外、未修改配置，复算配置 SHA-256 仍为 `8302661f460b8bf65b329b6678b73f371ac34773882c46638bcae360a2c58c18`，不自签 SC-001 关闭。

已核对删除目标绝对路径均位于本次专属临时根目录后，清理 `allowlist-probe/`、两份 `controls*` 合成输入目录及重复的 `current-sc001-fixed/` 中间副本。保留首轮 `current/`、最终已扫描 `current-sc001-final/` 冻结副本、对应摘要清单、脱敏原始报告、控制汇总、官方工具和固定公告库供尚在进行的 reviewer 复核；未清理用户证书/密钥、主方 r5-target 或其他验收目录。没有本次仍运行的扫描任务。

## SC-001 修复交付（2026-10-30 追加，等待独立 reviewer）

用户在首轮报告后明确授权本会话修改 `.gitleaks.toml` 和本报告；SC-002 由主方单独处理。本会话因此转为 SC-001 配置修复者，下面为修复、自测和扫描交付证据，**不由本会话签署 SC-001 关闭**。后文“首轮独立检查”保留原始发现和当时未修改配置的事实，不能作为最新配置状态。

修复配置 SHA-256：`8302661f460b8bf65b329b6678b73f371ac34773882c46638bcae360a2c58c18`。所有 11 项 allowlist 均设 `condition="AND"`、单个 `targetRules`、`regexTarget="secret"`、`^完整值$` 与 `^完整仓库相对路径$`；共 26 个规则/值/路径组合。原 9 项的公开材料继续保留但按已核实的真实值和真实路径重组，收窄文档 UUID 例外到实际一个 UUID，展开 transcript 的有限文件名，不使用路径前缀、后缀或目录通配。当前新增两份已验签固定 JWT，以及当前 WebSocket 测试使用的 RFC 6455 公开 nonce。默认规则和原 keystore 自定义规则保留，无 commit 豁免、stopword 或忽略整个源码目录。

官方 Gitleaks [v8.30.1 README](https://github.com/gitleaks/gitleaks/blob/v8.30.1/README.md) 经 ego-browser 实际读取，确认 `targetRules` 只把例外分配给指定规则；正反控制还实际验证其效果。使用 secret 完整匹配，避免把同一行中另一个秘密随公开向量一起放行。目录扫描从副本根目录运行 `gitleaks dir .`，使锚定路径与 CI Git 扫描一致。

### 历史命中的逐项核实

不是只依赖旧 allowlist 注释：使用 `git show COMMIT:PATH` 在进程内读取原 blob，从扫描 Match 重建候选值，计算候选摘要并解码；日志只输出路径、行、提交、摘要和合成特征。移除所有 allowlist 的诊断扫描实际得到 30 项；全部 30 项均核实，避免把旧例外中的未知项继续带入新配置。

首轮收窄后出现的 10 项，按首轮表格顺序逐项对应如下。提交完整值、路径和行号仍见首轮表格；此处序列使用十六进制表示，不打印原 token：

| 项 | 原命中位置 | 独立来源与合成特征 | 本次处置 |
| --- | --- | --- | --- |
| H01 | c1193a… / pairing-panel-render.test.ts:98 | 错误响应脱敏测试；解码为 16 字节 00–0F，与同测试 details 中固定值一致 | 该完整值、该路径、generic-api-key |
| H02 | c1193a… / pairing_fixtures.rs:468 | Rust JSON 合同测试，example.test；32 字节 00–1F | 该完整值、该路径、generic-api-key |
| H03 | c1193a… / unknown-field.json:3 | invalid fixture，附加 pairingCode 字段；32 字节 01–20 | 该完整值、该路径、generic-api-key |
| H04 | c1193a… / pairing-qr-lookup-request.json:3 | valid lookup fixture；32 字节 01–20，与 H03 一致 | 该完整值、该路径、generic-api-key |
| H05 | c1193a… / vectors.json:12 | canonical 固定向量 qrToken；32 字节 00–1F | 该完整值、该路径、generic-api-key |
| H06 | c1193a… / vectors.json:128 | 指纹不匹配负例中的 hostKeyFingerprint；32 字节 00–1F，**不是**正确公钥摘要 | 仅该固定负例值、该路径、generic-api-key |
| H07 | 681256… / pairing-lookup-response.json:7 | example.test 固定响应 fixture；32 字节 00–1F | 该完整值、该路径、generic-api-key |
| H08 | 1b62d8… / pairing-kinds.ts:64 | e2e 固定 PAIRING_SECRET，与测试节点示例一起构造；32 字节 00–1F | 该完整值、该路径、generic-api-key |
| H09 | 22d9dd… / pairing-first-run.ts:68 | e2e 常量，注释明确与 composition.test.ts 共用；32 字节 00–1F | 该完整值、该路径、generic-api-key |
| H10 | 22d9dd… / pairing-model.test.ts:47 | 单测 QR_SECRET，测试断言公开状态不包含它；32 字节 00–1F | 该完整值、该路径、generic-api-key |

旧注释把 H03/H04 写为 00–1F，但实际为 01–20；旧指纹例外只覆盖正确公钥摘要，未覆盖 H06 故意损坏的指纹。新描述按真实候选值重算，不沿用这些错误解释。

其余 20 项也核实为上述相同固定序列、31 字节 01–1F 长度负例、00–0F 脱敏测试值、ASCII 50–6F / A0–BF transcript 合成序列、公开 accessNodeId UUID，或公钥 SHA-256 指纹；正确指纹以历史 vectors.json 的 canonical.hostPublicKey 的 SEC1 原始字节重算一致。完整逐项结果位于临时 `proven-history-values.json`（只包含已证明公开的值）和脱敏 `history-review.json`；构造脚本 `classify-history.mjs` 对任何不能证明的值直接失败，未设置“其余全部允许”分支。

当前两份 JWT 再次以 Node crypto 验签，并确认私钥为 [08;32] 合成值，仅完整 compact 字符串在 `clients/web/tests/fixtures/jws-v1.json` 对 jwt 规则获豁免，没有忽略该文件或 jwt 规则。修复后首次当前扫描还检出 `app/src/node.rs:1590`，核实为 `management_websocket_rejects_invalid_origin_host_and_protocols` 的 `#[tokio::test]` 中 RFC 6455 §1.3 公开握手 nonce，值与原历史 RFC 示例一致，才增加该**具体路径**的同值/同规则例外；未修改 node.rs。

### 正反控制（最终配置）

最终 `controls-sc001.mjs` 在新建 UUID 隔离目录内逐个测试全部 26 个值/路径组合，共 130 项检查全部符合预期：

1. 精确规则+完整值+完整允许路径：目标规则 0 命中。
2. 同一路径替换为随机生成的合成标记（JWT 则保留头/载荷，随机化签名段）：目标规则重新命中。
3. 同一个公开值放在不允许的路径：目标规则重新命中。
4. 原路径添加目录前缀：目标规则重新命中。
5. 原路径添加文件后缀：目标规则重新命中。

另以临时配置新增 `sc001-unrelated-canary` 规则，在全部允许路径匹配同一批公开值：得到 26 项 canary 命中，证明例外不影响其他规则。相关控制输入均为公开值或本次随机合成数据，不是运行凭据；输出 `--redact=100`。当前 JWT 两项包含在这套正反控制内。

最终控制进程退出 0，`pairs=26`、`checks=130`、`allPairsPassed=true`、`unrelatedRuleHits=26`；证据 `sc001-controls-summary.json` SHA-256：`4ffed5897bfd8e7492b05b4df338a308235cb2b77f3955f2571170e625b1dee5`。增加 node.rs 路径后的第一次重跑复用了旧控制目录，残留的 suffix 文件导致计数失败；这是控制脚本隔离缺陷，未修改配置以绕过它，改为每次生成全新 UUID 目录后完整重跑成功。该失败不作为通过证据。

### 修复后扫描快照与 Rust 结果复用

当前源码快照于北京时间 2026-10-30 01:15:35 扫描结束；HEAD 仍为 `7a6e9ec4bdf4b41d9408b39cb3669313e52f41b2`。重新按 cached+others+exclude-standard 枚举并复制 444 个文件，其中 app 31、clients 26；包含全部枚举到的未跟踪产品文件，排除 .tmp 实际私钥/证书、target、node_modules。副本为 `current-sc001-final/`，逐文件 SHA-256 清单为 `current-sc001-manifest.json`，摘要 `af29f54b95ceac584965fb32c4e8f8cc3f26cf30444754104f6210663346b28a`。扫描期间 444 个源文件逐一复算无变化；不声称该快照覆盖并行 r5 后续尚未冻结的修改。

| 执行 | 实际结果 | 证据 |
| --- | --- | --- |
| 副本根目录 `gitleaks dir . --config <repo/.gitleaks.toml> --redact=100 --timeout 45` | 约 6.54 MB，0 命中，exit 0 | current-sc001-fixed.json / current-sc001-summary.json |
| 原仓库 `gitleaks git . --log-opts=--all --config .gitleaks.toml --redact=100 --timeout 30` | 331 commits scanned，约 54.32 MB，0 命中，exit 0 | history-sc001-final.json |
| 修复后 app/Cargo.lock 摘要复核 | `28639acdba0a6c445fca88f9191e3e1484deeca21e63f0421bd2e2dfd9ab6b5a`，与首轮相同 | current-sc001-summary.json |

两份空命中 JSON 的 SHA-256 均为 `37517e5f3dc66819f61f5a7bb8ace1921282415f10551d2defa5c3eb0985b570`。保留脱敏原始报告，无秘密打印。本报告的追加交付文字在快照扫描后写入，不属于该已冻结扫描摘要；产品源码和最终配置扫描时已在副本中。Rust 复用首轮 cargo-audit 0.22.2 对相同锁文件、同官方 `ee1a49a2769a24a596afc1c20cb33748d2769586` 快照、1386 条公告的真实执行结果：261 包、0 漏洞、无 warnings；本轮未重跑，未扩大到 yanked 检查、实时最新公告或包内容/编译产物完整性。

### 交付权限与尚待独立复核

SC-001 状态为“修复及自测交付，待原独立 reviewer 复核关闭”。SC-002 的依赖文档由主方单独修复，本会话不修改或代签。后续 r5 冻结应对照当前清单确认变化；产品文件变化时重扫最终当前快照，配置变化时同时重跑正反控制和历史扫描。没有重写历史、修改真正凭据、提交、推送或归档。

交付前再次对照清单：`.gitleaks.toml` 和 Cargo.lock 摘要保持上述值；并行主方已继续修改 `app/src/node.rs` 与 platform-acceptance.md，本报告也追加了交付文字。因此本轮 444 文件的零命中只绑定 01:15:35 已冻结副本，**不代签随后 r5 文件内容**。按用户要求不等待或干预主方，最终 r5 应重新生成当前副本扫描；历史扫描及本配置的全部正反控制仍绑定本报告配置摘要。范围内 `git diff --check` 退出 0，仅提示既有 LF/CRLF 转换。

## 首轮独立检查（以下保留首轮执行事实）

执行日期：2026-10-30（Asia/Shanghai）。执行者为独立验收会话，未继承开发对话。用户限定本次仅补齐扫描工具、供应链执行证据及审查，不提交、推送或归档。最终仅新增本报告；工具、下载、扫描副本与脱敏结果全部位于 `.tmp/independent-supplychain-20261030/`。未修改源码、规格、现有报告、锁文件或仓库配置。

## 结论与用例边界

| 范围 | 实际结果 | 结论 |
| --- | --- | --- |
| NA-002 Rust 公告漏洞扫描 | cargo-audit 0.22.2，261 个锁定包，官方公告快照含 1386 条公告；漏洞 0，warnings 为空，退出 0 | 固定快照范围完成；未查询 yanked（撤回发布）状态 |
| NA-002 当前文件秘密扫描 | Gitleaks 8.30.1，433 个文件，约 6.39 MB，退出 1，2 个 jwt 命中 | 两项均经独立核对为固定合成 JWS 测试向量；扫描器原始状态仍为命中，未新增忽略项 |
| NA-002 本地 Git 历史扫描 | 仓库现有配置、`--log-opts=--all`，331 commits scanned，约 54.32 MB，退出 0 | 有执行证据，但受 SC-001 过宽允许清单影响，不能作为完整通过依据 |
| NA-002 允许清单收窄诊断 | 仅临时配置给所有 9 个实际 allowlist 加 AND，历史重扫退出 1，10 项命中 | 证明原零命中不足；新增命中尚未逐项完成真实性处置，不能宣称泄密，也不能全部认定假阳性 |
| NA-002 锁文件追溯 | Rust registry 包均有 checksum；两份 npm lock 均有 integrity、license，resolved 均为官方 npm registry | 结构核对完成，不替代许可证法律审查、实际包内容验证或逐项上游维护核验 |
| NA-014 运行时秘密保护 | 本次不发送认证请求、不操作主方真实 TTY/节点或用户身份 | 未执行；静态扫描不代表运行时响应、日志、storage、网络保密验收通过 |

本次不签署 NA-002 整例通过，也不签署整体待归档验收通过。SC-001、SC-002 尚需闭环；完整官方维护状态及全部传递许可证兼容性仍未逐项联网核验。用户转述的 npm registry 审计 0 作为交接背景，本会话未重跑，不冒充独立执行结果。静态构建、真实 Chromium JWS、提交词表、运行时安全及真实节点闭环均不在本侧任务的通过声明内。

## 输入、基线和范围

已读取仓库 AGENTS.md、openspec/config.yaml、openspec/role/tester.md、当前 change tests.md（重点 NA-002/014）、app/DEPENDENCIES.md、app/Cargo.toml、根与 Web package.json、三份主锁文件、.gitleaks.toml、.github/workflows/ci.yml。配置中的“当前无产品实现”背景不能代替实际文件事实，本次按已存在的 app、clients/web 与当前测试要求审查。

HEAD：`7a6e9ec4bdf4b41d9408b39cb3669313e52f41b2`。开始时已有大量未提交修改，app/、clients/、testing/ 等尚未跟踪；没有改写索引。`git rev-list --all --count` 为 365；Gitleaks 自报实际扫描 331 commits，两者不混为同一个计数，未声称覆盖远端尚未取回的历史。

当前副本由 `git ls-files --cached --others --exclude-standard` 枚举，复制实际存在的文件，共 433 个，并逐文件计算 SHA-256；包括未跟踪的产品源码、测试和文档。副本不包含 .git、被忽略的构建输出、node_modules、.tmp 下真实证书/私钥和本机运行数据；未把 app 或 clients 源码整体 ignore。Git 历史由原仓库读取，与当前副本分开扫描。后续并行工作若改动文件，需依据清单重判证据有效性。

持久基线摘要（SHA-256）：

| 文件 | 摘要 |
| --- | --- |
| app/Cargo.lock | 28639acdba0a6c445fca88f9191e3e1484deeca21e63f0421bd2e2dfd9ab6b5a |
| clients/web/package-lock.json | 69d1e791bd5a41e585626bd234bc071677900d9a451db8a4dabd9961038b415b |
| package-lock.json | 5bcd905695cf50530b1a237a8dccfdb5af89c20d5ea8df829fb31d70f0f83ea9 |
| .gitleaks.toml | c473773f48be9e31ab47daf40372f4771185410e888db9f08d4852f25341520e |
| .github/workflows/ci.yml | f546052bb6b5d6213a2657ef68fdbc59fd43a0d3b3e719fe5cef9544fc7ca5be |
| current-manifest.sha256（临时目录内） | 39dedf8b1cdda03a3120a013b0bcabda0011cdf6e60ee935b47a8f17addca4a3 |

## 工具来源、网络诊断与校验

使用 Windows PowerShell，Gitleaks 8.30.1、cargo-audit 0.22.2。已读取 ego-browser SKILL.md，并实际执行 `ego-browser skill`；通过独立 Agent TaskSpace 5 获取官方发布，不选择或修改普通用户 profile。

用户提供的主方诊断为 `curl -4 -I --connect-timeout 5 --max-time 10 https://api.github.com` 返回 Schannel `CRYPT_E_REVOCATION_OFFLINE (0x80092013)`：离线吊销检查失败，不是 HTTP 404，也不等于根证书不受信。本会话两次使用 `curl.exe -4 --ssl-revoke-best-effort -L --connect-timeout 5 --max-time 25 --fail` 请求 api.github.com 的 Gitleaks release 与 RustSec latest；均立即退出 6：`Could not resolve host: api.github.com`。本会话这两次失败发生于 DNS 阶段，不能据此确认吊销检查已恢复。

`--ssl-revoke-best-effort` 仅用于上述工具来源请求，保留证书链和主机名验证；从未使用 `-k/--insecure`，未更改产品 TLS 配置。未重复 cargo install，也未争用 package cache。交接中先前离线安装缺 backtrace、官方 ZIP 零字节超时为用户提供的背景，不记作本会话重做证据。

浏览器下载遇到一次 Gitleaks bootstrap 连接失败、一次 cargo-audit 资产折叠导致不可点击、一次下载 canceled；在同一 TaskSpace 有界重试后成功。下载事件等待 15–20 秒，后续重试 saveAs 采用 25 秒上限。官方来源和下载前取得、下载后实算一致的 SHA-256：

| 工具 | 官方来源 | SHA-256 |
| --- | --- | --- |
| Gitleaks Windows x64 ZIP | [v8.30.1](https://github.com/gitleaks/gitleaks/releases/tag/v8.30.1)，先下载同 release 的 gitleaks_8.30.1_checksums.txt | d29144deff3a68aa93ced33dddf84b7fdc26070add4aa0f4513094c8332afc4e |
| cargo-audit Windows MSVC ZIP | [cargo-audit/v0.22.2](https://github.com/rustsec/rustsec/releases/tag/cargo-audit%2Fv0.22.2)，先读取对应资产官方页面的 sha256 digest | 0a7316540862c13d954f648917ceacca593747baed6eec180fafa590be2710ab |

仅校验匹配后解压并运行版本命令。校验基于官方 HTTPS 同源摘要，未声称完成独立签名/构建溯源认证。两个扫描器均为上游官方发行二进制，没有使用第三方重打包工具。

## cargo-audit 实际执行

首次使用独立 `--db` 目录在线拉取，进程设 25 秒上限，实际提前失败：`couldn't fetch advisory database` / IO error / GitHub `info/refs?service=git-upload-pack` 请求失败。没有把空数据库结果当通过。

随后通过浏览器打开 [RustSec 官方公告库](https://github.com/RustSec/advisory-db)，观察到当前提交 `ee1a49a2769a24a596afc1c20cb33748d2769586`，下载该固定提交官方 ZIP 并解压。ZIP SHA-256 为 `72e94e11d190837bf8fd822faba5597bc0c27caaad1a55f28df489141cf04804`，这是本地记录摘要；上游未提供该源码归档的独立 checksum，不冒充已核验官方归档摘要。归档来源由官方 HTTPS 固定提交 URL 约束。

实际扫描命令（路径均在指定临时目录）：

```text
cargo-audit.exe audit --db <advisory-snapshot/advisory-db-ee1a49a...> --no-fetch --stale --no-yanked --file <current/app/Cargo.lock> --json
```

退出 0。报告：advisory-count=1386、dependency-count=261、vulnerabilities.found=false、count=0、list=[]、warnings={}，ignore=[]，不限制 OS/架构。数据库使用源码快照，没有伪造本地 Git 提交；报告 last-commit/last-updated 为 null，真实来源提交由本报告补足。`--stale` 允许离线读取，不能据此声称扫描时自动联网确认新鲜度；`--no-yanked` 明确跳过 registry 撤回状态查询。

结果保存在 `cargo-audit-offline.json`，SHA-256：`c46fb48f07c6a68dbb6bf25164758c64fa4592bc9f72583e24921e52f83fdf74`。该扫描完成了固定公告快照对锁文件的漏洞比对，未证明依赖没有未知漏洞、供应链投毒或不安全使用。

## Gitleaks 执行与独立核实

```text
gitleaks.exe dir <current> --config .gitleaks.toml --redact=100 --no-banner --no-color --timeout 45 --report-format json --report-path <current-gitleaks.json>
gitleaks.exe git . --log-opts=--all --config .gitleaks.toml --redact=100 --no-banner --no-color --timeout 45 --report-format json --report-path <history-gitleaks.json>
```

输出全程使用 100% 脱敏，没有打印命中秘密。当前两项均在 `clients/web/tests/fixtures/jws-v1.json` 的第 20、24 行，规则 jwt，分别为 rust/webcrypto 固定 compact 签名。独立 Node crypto 核对：fixture 私钥标量为固定重复字节的合成值，派生公钥与 fixture 一致，两份 compact 签名均可由该公钥验签；载荷使用 example.test 与固定过期时间。因而两处是公开测试向量，非本次运行凭据。本次未新增 allowlist、未篡改扫描退出码，不把误报核实混为扫描器零命中。

结果摘要：current-gitleaks.json SHA-256 `eeebcd457775d6a9b00d7ef2279df923f5bff5286942cd0ab4fcbceb405714ac`；history-gitleaks.json SHA-256 `37517e5f3dc66819f61f5a7bb8ace1921282415f10551d2defa5c3eb0985b570`。

### SC-001：允许清单范围不符合“确切值且确切文件”的注释（P2，未关闭）

版本：上述 .gitleaks.toml 摘要。9 个实际 `[[allowlists]]`（首项第 64 行，其余 84/90/96/102/113/119/125/136 行）同时配置 paths 和 regexes，却没有 `condition = "AND"`。行为实证：把原本命中 2 项的同一 JWS fixture 复制到隔离的 `crates/server/src/transport/net/test_client.rs`，原配置扫描该副本为 0 bytes / no leaks / exit 0；在同目录树增加内容完全相同的 control.json 后，只有 control.json 两项命中，exit 1。测试文件内容不含该条 RFC 示例允许值，仍被路径单独豁免。

这证明实际路径匹配足以跳过整个文件，配置注释声称的“值+文件”交集没有实现；同时不能依赖这些允许项的正则只作用于列出路径。未把仓库源码整体忽略，也未改原配置。

仅在临时副本中对 9 项增加 AND，保持其余规则不变，运行同样全历史扫描：331 commits、54.32 MB、exit 1、10 个 generic-api-key 命中。首次临时配置生成误替换注释导致 TOML 错误，已改为只替换行首表头后重跑；只有后者算执行证据。

收窄后命中位置（仅列元数据，尚未逐项裁定）：

| 提交 | 文件与行 |
| --- | --- |
| c1193a0795a7e238bdb7f9365832559ae43e88dd | clients/app/src/components/pairing-panel-render.test.ts:98；crates/sync-protocol/tests/pairing_fixtures.rs:468；fixtures/sync/v1/invalid/pairing-qr-lookup-request-unknown-field.json:3；fixtures/sync/v1/valid/pairing-qr-lookup-request.json:3；fixtures/sync/v1/device-qr/vectors.json:12、128 |
| 681256a6872a18d59ea5772318618450b53de2fe | fixtures/sync/v1/valid/pairing-lookup-response.json:7 |
| 1b62d87d1b219faadb2acdbe767f09939e4a8f83 | clients/app/e2e/page/checks/pairing-kinds.ts:64 |
| 22d9dd25068471c9e4df5ec9745a7e6582ee3f29 | clients/app/e2e/page/checks/pairing-first-run.ts:68；clients/app/src/features/pairing-model.test.ts:47 |

复核建议：由有配置修改权限的工作包修复交集语义，逐项核实上述历史命中并维护精确例外后，独立重扫。不能只加 AND 就宣称历史已通过，也不能未经核实扩大 allowlist。本侧任务按写范围保持问题开放。

### SC-002：依赖审查文档遗漏新增直接依赖（P2，未关闭）

`app/Cargo.toml:27` 直接引入 hyper-util 0.1.21（http1/server/service/tokio）；锁文件同版本，但 app/DEPENDENCIES.md 的直接依赖表没有这一项，因此必要性、许可证、平台和维护风险记录不完整。tokio、rcgen 的 manifest 下限不同于锁定版本本身不构成错误：Cargo 使用兼容范围，锁文件实际为 tokio 1.54.1、rcgen 0.14.10，与现有文档一致。需要补齐 hyper-util 的可追溯审查，不能用“Tokio 生态”概括代签。

## 锁文件与 CI 核对

独立脚本 `review.mjs` / `lock-review.json`：Cargo.lock 共 261 包，260 个 registry 包均有 64 位十六进制 checksum，没有 Git/路径等非 registry 外部来源；剩余本地包为产品。根 npm lock v3 含 125 个非根条目，Web lock v3 含 81 个，均有 integrity、license，无非官方 npm resolved URL。这只校验元数据存在和来源格式，未重新下载所有包与 checksum 比对。

密码库实际锁定 p256 0.14.0、rustls 0.23.46、ring 0.17.14；manifest 将 rustls/tokio-rustls 设为 default-features=false 且启用 ring，sqlx 为 default-features=false 且 sqlite/runtime-tokio，windows-sys 位于 cfg(windows)。与依赖文档的主要平台/密码库选择一致；不是对密码学实现正确性的替代验收。

CI secrets job 使用 fetch-depth:0、固定 Gitleaks action commit、GITLEAKS_VERSION=8.30.1 与 .gitleaks.toml；product/checks 配置 npm audit，Windows product 配置锁定安装和 npm run check。当前 CI 未见 cargo-audit 作业；本次本地独立执行补充 Rust 公告扫描，不声称已有远端 CI 通过。CI checkout 不包含尚未跟踪的产品代码，故本次当前副本扫描必不可少。

结束执行 `git diff --check` 退出 0，仅有已有文件的 LF/CRLF 提示。保留临时工具、固定数据库、清单和脱敏报告供主方复核；没有删除私人数据、停止主方节点或占用其 TTY。工具取得与扫描执行已完成，本报告不要求用户手工测试。
