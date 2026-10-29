# NA002 独立供应链、密码库与合同总检视

日期：2026-10-30。结论：**BLOCKED：NA-SC-003（Medium，debug交付未冻结Web资源）**。除此问题，下述限定供应链与合同源码检视未发现新的需修阻断。不是把“未做完整法律审计”当门禁，不要求无穷协议组合，也不代签动态用例全部通过。

## 版本及方法

R12清单 `6D7E0EC6CC1FD9DD6AC0F07719841AF1B0736F81EB265B7E5C0ACDB3DE6E409B` 的83项已逐一核对；R12冻结副本用于独立pairing15项及R11容量时钟1项。R13只改验证资产，另有独立报告及83项校验，不影响R12运行时结论。没有把整个不断追加报告的工作区称为同一源码版本。

Cargo.lock `28639ACDBA0A6C445FCA88F9191E3E1484DEECA21E63F0421BD2E2DFD9AB6B5A`、根npm lock `5BCD905695CF50530B1A237A8DCCFDB5AF89C20D5EA8DF829FB31D70F0F83EA9`、Web lock `69D1E791BD5A41E585626BD234BC071677900D9A451DB8A4DABD9961038B415B` 均与已扫描版本相同。配置 `.gitleaks.toml` 为 `8302661F460B8BF65B329B6678B73F371AC34773882C46638BCAE360A2C58C18`，SC001/002原关闭保持。

本轮 `cargo metadata --offline --locked --filter-platform x86_64-pc-windows-msvc` 成功，解析238个目标相关包、33个根直接依赖（含3个dev和windows-sys），无缺失license字段。全平台metadata第一次缺离线zerocopy-derive未完成，随后只限定实际Windows x64交付目标；不冒称所有目标可构建。原始metadata保留 `.tmp/reviewer-implementation-r4-target/dependency-metadata.json`，SHA256 `130A2219BFDA1039AE4D8E87AFBEFAE5F4C11DB0B502547EFEF736865E4E4488`。261为整个锁文件包数，不与目标解析238混写。

## NA-SC-003 / Medium / Open：普通交付debug二进制依赖外部Web构建目录

位置：`app/Cargo.toml:31`（rust-embed无debug-embed）、`app/src/pairing_http.rs:50–52,279–289`、`app/build.rs:11–20`、根package.json的build:rust。当前交付路径是debug/acp-remote.exe，build:rust调用普通cargo build，未限定release。合同§1要求同包静态入口、§6单应用二进制；依赖说明也称编译嵌入。

锁定rust-embed8.13.0及其impl源码明确：debug_assertions且无debug-embed时生成运行时read_file_from_fs；只有release或debug-embed才使用编译期内容。build.rs只在构建时检查index存在，不能改变该运行时模式。故同一个已经编译的debug程序会随着原构建Web文件变化/丢失而改变响应；复制exe并不保证完整Web入口。不是密码算法问题，也不是新增允许攻击同用户进程的威胁模型，而是现交付方式和静态资源冻结承诺不一致。

独立最小复现：以reviewer target已锁定的真实rust-embed8.13.0和相同默认feature、同普通debug宏配置编译自有测试exe，引用reviewer自有index.html。第一次输出fixture-v1；不重编译只改该文件后输出fixture-v2；仅移走该自有文件后输出MISSING。产物在 `.tmp/reviewer-implementation-r4-target/embed-behavior`。这是实际依赖/宏配置行为，不是启动用户节点复现404，不声称已经运行产品迁移故障。上游[官方维护页](https://pyrossh.dev/repos/rust-embed)也说明debug默认从文件系统读取；本轮同时读锁定8.13.0源，不以官方页8.12标题当产品版本。

最小修复建议：启用rust-embed的debug-embed以使普通交付也固定资源，或将正式交付明确限定到实际验证的release构建并更新命令/证据。若选择前者会改变feature解析，需新固定版本及相应构建。闭环：用复制到隔离目录的真实产品产物，构建树不可见/不含dist时仍能取index和所引assets；保持原构建资源之后变化不改变已生成exe资源。不得改用户真实dist或用户部署资源，可从自有构建树完成。无须重复所有JWS/DPAPI测试。

## 密码库与维护、许可、平台结论

维护审查采用可追溯的上游归属/发布包metadata/维护与安全渠道、真实锁定版本和RustSec未维护公告，**不把“必须每月发布”或“必须最新版本”发明成门禁**。已读官方项目当前页面/README及关键版本资料；全量直接依赖的上游URL、版本、许可证见文末由实际metadata生成表。小型稳定库发布间隔不自动等于弃维护；扫描未报unmaintained，不等于保证未来维护。没有看到本次选择必须替换的弃维护/许可证冲突证据。

| 分类 | 独立结论及上游依据 |
|---|---|
| 签名/哈希/清零 | P2560.14.0由RustCrypto维护，Apache-2.0 OR MIT；[官方Cargo](https://raw.githubusercontent.com/RustCrypto/elliptic-curves/master/p256/Cargo.toml)可核对算法与MSRV。SHA2和zeroize同RustCrypto，subtle为dalek BSD-3-Clause。[zeroize官方资料](https://raw.githubusercontent.com/RustCrypto/utils/master/zeroize/Cargo.toml)、[subtle资料](https://raw.githubusercontent.com/dalek-cryptography/subtle/main/Cargo.toml)。代码调用库Signer/Verifier、SHA256及系统随机；未自写曲线或伪随机算法。直接sha2为0.10.9，P256链还解析0.11.1，不误当锁文件冲突。 |
| TLS | rustls0.23.46显式关闭默认provider并选ring0.17.14，tokio-rustls同ring；[锁定版本官方README](https://raw.githubusercontent.com/rustls/rustls/v/0.23.46/README.md)支持此选择和TLS1.2/1.3。不同于当前main0.24的provider接口，不能拿新主分支API说当前配置失效。ring许可证为Apache-2.0 **AND** ISC，需同时保留有关声明，不误写成二选一。[ring官方源](https://github.com/briansmith/ring)。未宣称FIPS或密码实现形式证明。 |
| OS随机/Windows | getrandom0.3.4取OS源，错误不降级；[官方目标说明](https://raw.githubusercontent.com/rust-random/getrandom/master/README.md)有Windows支持。windows-sys0.61.2来自[Microsoft windows-rs](https://github.com/microsoft/windows-rs)，cfg(windows)；真实Windows编译及既有owner/DPAPI证据补平台结论，不扩到Linux/macOS。 |
| 服务/协议 | Tokio、Axum、Bytes、Tracing、Hyper-util为对应官方生态项目，MIT；[Tokio维护渠道](https://github.com/tokio-rs/tokio)、[tungstenite](https://github.com/snapview/tungstenite-rs)等包许可证及feature与锁一致。tungstenite直接类型依赖与axum同版本，未引入另一解析器。 |
| SQLite/文件 | SQLx0.8.6仅sqlite/runtime-tokio，bundled SQLite；[官方SQLx](https://github.com/launchbadge/sqlx)是发布/安全维护来源。[fs4官方](https://github.com/al8n/fs4)说明其fs2分叉来源；不能把fs2的历史结论直接套给fs4。实际库/节点测试、SQLITE_FULL/FK提交失败均有独立证据。 |
| 编解码/错误/CLI | Serde/serde_json、toml、base64、hex、pem、url、anyhow/thiserror、clap、mime_guess的发布metadata来源清楚，MIT/Apache等宽松许可证。未知键/重复键/规范化/Origin仍由产品封装和合同测试实现，库选型本身不保证这些边界。[Serde](https://github.com/serde-rs/serde)、[clap](https://github.com/clap-rs/clap)、[base64](https://github.com/marshallpierce/rust-base64)、[mime_guess](https://github.com/abonander/mime_guess)上游可核对维护渠道。 |
| Web工具 | 产品浏览器无运行时第三方包；TypeScript7.0.2 Apache-2.0、Vite8.3.4 MIT、Vitest5.0.5 MIT。已读本地实际package与[Microsoft TS](https://github.com/microsoft/TypeScript)、[Vite](https://github.com/vitejs/vite)、[Vitest](https://github.com/vitest-dev/vitest)官方资料。当前Node22.20.0满足Vite>=22.12、Vitest^22.12；构建不等于Windows外所有平台验证。 |

所有目标解析license表达式已枚举，包括MIT/Apache、BSD、ISC、Unicode-3.0、Zlib、Unlicense可选项、BSL-1.0可选项；没有发现必须按GPL/AGPL等分发本产品源码的选择。ring的AND、多组件NOTICE/Unicode版权等保留义务不能忽略；本change尚未发布安装包，不凭“未做法务全面鉴证”阻断实现验收，也不声称已审核未来发行包的每份notice。原文档zeroize写1.9.0、锁实际1.9.1，以及部分“扫描待执行”旧句应在metadata整合时纠正，锁文件为准，不因此重开已完成扫描。

## 静态和真实工具证据

| 必要项目 | 当前可采纳证据/限制 |
|---|---|
| Rust公告 | 同锁文件，官方cargo-audit0.22.2对RustSec固定提交ee1a49a2769a24a596afc1c20cb33748d2769586的1386公告、261包实扫，0漏洞、warnings={}；已读原始cargo-audit-offline.json。来源和ZIP校验沿用独立供应链报告，不重装。未查yanked、未证明零日/实时最新公告；不能再写工具未取得。 |
| npm审计 | 原独立tester真实网络根125/Web81依赖审计均0，当前两锁文件摘要不变可复用；本轮未冒称再次联网audit。根开发工具不能替代Rust产品公告。 |
| 秘密扫描 | SC001精确AND规则已独立130正反控制/26规则隔离、历史331 commits0命中。主方522副本0只绑定其快照。本轮另独立复制git cached+others+exclude-standard可见当前590文件（包含当时新增源码/报告），官方Gitleaks8.30.1扫描约7.32MB、0命中、exit0；结果 `.tmp/reviewer-implementation-r4-target/na002-current-result.json` SHA256 `37517E5F3DC66819F61F5A7BB8ACE1921282415F10551D2DEFA5C3EB0985B570`。不含被忽略临时真key和未来修改。 |
| 真提交lint | 本轮实际 `npm.cmd run lint:commits -- --from 5777067e2ed17503cd8aa3f042015ba11a9cb152 --to 7a6e9ec4bdf4b41d9408b39cb3669313e52f41b2 --verbose`，0问题0警告exit0；两端均真实commit，只覆盖现有一个重建基线提交，未提交源码没有虚构commit接受记录。PowerShell npm.ps1初次策略拒绝后改npm.cmd成功，不记作最终未执行。 |
| check/build | r12原始check已读，140库+4向量/Web46及fmt/clippy，开发完整日志可按固定版本复用；reviewer本轮只独立定向执行15+1。不得声称远端CI已跑。CI有Windows产品/npm audit、Gitleaks固定action版本和真实commit范围，未配cargo-audit但本地实际执行补足，不虚构CI作业。 |
| Chromium/JWS | 已读独立 `2026-10-30-jws-challenge-independent-acceptance.md`，真实自有Chromium+Rust往返、原始bytes/DER/标量/载荷封闭和challenge实际执行，负责字节与本次相同可复用。reviewer此前独立Rust/Node复跑也已记录，不冒称其是reviewer自己Chromium执行。 |

## 合同§1–7整体映射（源码结论和动态范围分开）

| 合同 | 实现/证据及当前判定 |
|---|---|
| §1信任根/部署 | config/tls/pairing_http/management_ws检查HTTPS canonicalOrigin、Host/Origin、直接TLS和响应安全头；不自动装根、不信XFF授权。Web节点pin错误不自动覆盖已在R01/R03关闭。**同包资源在当前debug交付受NA-SC-003阻断**；TLS名称/信任属已有真实浏览器证据，不用静态代码代签。 |
| §2编码/签名 | jws/claims/strict_json与Web jws/protocol用真实P256/WebCrypto，六typ隔离，签原始段、严格base64url/SEC1/P1363及闭合字段；NA003/004独立证据可按未变bytes采纳。未自行实现算法，产品自写严格解析不是新密码原语。 |
| §3配对 | pairing/challenge/http/IPC/main；只owner批准，CSPRNG短码+挑战，PendingConfirmation缓存边界由R12修复，终态/自然到期/保留max边界已独立15项；tester原NA022失败保留待其HTTP复测，不能用reviewer单测修改旧HTTP结果。 |
| §4管理/撤销 | application_policy真实actor产生；TrustStore DB/generation检查、same actor gate受理/入队、目标revoke/写socket门，R9 RV001/002独立关闭；无新公网action，无管理身份替代运行时审批。NA028多连接/自撤/通知及完整分页剩余仅按revocation-gap-review具体G项，不写无限组合。 |
| §5预算/错误 | HTTP/管理固定窗口、1024桶不驱逐、帧字节permit由R10独立9+7验证；TLS/IPC/关闭错误fail-closed、deadline reserve已读。实际慢连接/默认关闭时限的动态证据需按执行报告范围，不用常量检查替代。 |
| §6平台/存储 | windows.rs显式owner DACL、token impersonation/revert、当前用户DPAPI；identity先密文文件再meta，损坏不替换；R6真实DPAPI子进程13项、R7真pagerFULL、R8五模拟失败均独立执行。R11满10000/24h联动已补；真实另一SID五命令/远程pipe等必要缺口仍在对应NA012，不因总体源码检视通过删除。Web不可导出IndexedDB identity由已有真实浏览器存储证据支持，丢失仅条件提示撤旧身份，不假检测。 |
| §7验证分界 | 独立reports保留真实/适配/开发界限，未执行不写PASS。R13新结构checker只证明33例/28场景定义一致，负向控制独立通过，不能代表充分性或执行通过。不恢复旧12字段/旧034LX。 |

## 可执行闭环与不新增门禁

reviewer文字勘误：此前r10-review把帧预算在一处写为128，应为源码/合同的**256帧**；128是全节点管理连接上限。本轮已重新核对MAX_OUTBOUND_FRAMES=256；原测试通过实际常量迭代，结果不受文字错误影响，旧报告保持历史原文。

本包唯一新需修问题为NA-SC-003，按上述真实产物资源隔离试验闭环。没有以“全面法律审计”“所有传递库每个版本最新”“所有宿主盘故障”“真实断电”新增阻断。上游信息随时间变，未来发布应按当时公告重审，不把本次0漏洞写永久保证。

R13 metadata整合后的最新报告/扫描数量不在590旧副本中自动出现，应在最终交付汇总标明实际字节范围；不要求每次仅报告文字追加就重跑所有产品测试。下一任何runtime修复须新固定版本并由tester采用相应binary复测。用户当前33人工理解已确认，不恢复旧人工安全签字门禁。仅新增本报告及本轮授权报告，不修改产品/冻结资产/旧失败报告、索引或plan/tasks，不提交推送归档。

## 实际直接依赖发布元数据

下表从本轮目标解析根节点的33个直接依赖生成（不混入同名传递版本）；license/repository来自已缓存的registry发布包manifest，经Cargo.lock绑定。表示来源/许可审查，不声称逐个上游最新提交均已审计。

| 包 | 锁定版本 | 许可证 | 上游维护渠道 |
|---|---|---|---|
| anyhow | 1.0.104 | MIT OR Apache-2.0 | [upstream](https://github.com/dtolnay/anyhow) |
| axum | 0.8.9 | MIT | [upstream](https://github.com/tokio-rs/axum) |
| base64 | 0.22.1 | MIT OR Apache-2.0 | [upstream](https://github.com/marshallpierce/rust-base64) |
| bytes | 1.12.1 | MIT | [upstream](https://github.com/tokio-rs/bytes) |
| clap | 4.6.7 | MIT OR Apache-2.0 | [upstream](https://github.com/clap-rs/clap) |
| fs4 | 1.1.0 | MIT OR Apache-2.0 | [upstream](https://github.com/al8n/fs4) |
| futures-util | 0.3.34 | MIT OR Apache-2.0 | [upstream](https://github.com/rust-lang/futures-rs) |
| getrandom | 0.3.4 | MIT OR Apache-2.0 | [upstream](https://github.com/rust-random/getrandom) |
| hex | 0.4.3 | MIT OR Apache-2.0 | [upstream](https://github.com/KokaKiwi/rust-hex) |
| hyper-util | 0.1.21 | MIT | [upstream](https://github.com/hyperium/hyper-util) |
| mime_guess | 2.0.5 | MIT | [upstream](https://github.com/abonander/mime_guess) |
| p256 | 0.14.0 | Apache-2.0 OR MIT | [upstream](https://github.com/RustCrypto/elliptic-curves) |
| pem | 3.0.6 | MIT | [upstream](https://github.com/jcreekmore/pem-rs.git) |
| rcgen | 0.14.10 | MIT OR Apache-2.0 | [upstream](https://github.com/rustls/rcgen) |
| rust-embed | 8.13.0 | MIT | [upstream](https://pyrossh.dev/repos/rust-embed) |
| rustls | 0.23.46 | Apache-2.0 OR ISC OR MIT | [upstream](https://github.com/rustls/rustls) |
| rustls-pki-types | 1.15.1 | MIT OR Apache-2.0 | [upstream](https://github.com/rustls/pki-types) |
| serde | 1.0.229 | MIT OR Apache-2.0 | [upstream](https://github.com/serde-rs/serde) |
| serde_json | 1.0.154 | MIT OR Apache-2.0 | [upstream](https://github.com/serde-rs/json) |
| sha2 | 0.10.9 | MIT OR Apache-2.0 | [upstream](https://github.com/RustCrypto/hashes) |
| sqlx | 0.8.6 | MIT OR Apache-2.0 | [upstream](https://github.com/launchbadge/sqlx) |
| subtle | 2.6.1 | BSD-3-Clause | [upstream](https://github.com/dalek-cryptography/subtle) |
| thiserror | 2.0.21 | MIT OR Apache-2.0 | [upstream](https://github.com/dtolnay/thiserror) |
| tokio | 1.54.1 | MIT | [upstream](https://github.com/tokio-rs/tokio) |
| tokio-rustls | 0.26.6 | MIT OR Apache-2.0 | [upstream](https://github.com/rustls/tokio-rustls) |
| tokio-tungstenite | 0.29.0 | MIT | [upstream](https://github.com/snapview/tokio-tungstenite) |
| toml | 1.1.8+spec-1.1.0 | MIT OR Apache-2.0 | [upstream](https://github.com/toml-rs/toml) |
| tower | 0.5.3 | MIT | [upstream](https://github.com/tower-rs/tower) |
| tracing | 0.1.44 | MIT | [upstream](https://github.com/tokio-rs/tracing) |
| tungstenite | 0.29.0 | MIT OR Apache-2.0 | [upstream](https://github.com/snapview/tungstenite-rs) |
| url | 2.5.8 | MIT OR Apache-2.0 | [upstream](https://github.com/servo/rust-url) |
| windows-sys | 0.61.2 | MIT OR Apache-2.0 | [upstream](https://github.com/microsoft/windows-rs) |
| zeroize | 1.9.1 | Apache-2.0 OR MIT | [upstream](https://github.com/RustCrypto/utils) |
