# NA003 / NA004 独立定向验收（2026-10-30）

结论：按当前 tests.md 的有限步骤和预期，NA-003、NA-004 均 PASS。本报告不代签 reviewer，不把挑战适配器测试描述为真实网络或浏览器授权。r5a 原报告保持历史结论；本次新增两例后，沿用其余逐例结论的阶段合计为 **12 PASS / 21 PARTIAL / 0 NOT-RUN**，仍不具备完整待归档条件。NA009 尚待 r6 固定摘要后的独立验收，不在这里升级。

## 版本范围

仓库 D:/Project/acp-remote。r5a 清单摘要 96F7B25DA1B7ABF3C5F65D069723CA87B095800F08967720AADBD4FC621975EC 的历史79项校验不重新包装为当前全仓校验。执行时其他角色已获准修改 identity.rs 的测试辅助路径；本次固定并前后核对下面负责字节，7项 JWS/前端资产均无变化，challenge.rs 与 r5a 项一致。当前编译的库发现125项测试，定向运行不声称完整125项全部通过。

| 文件 | SHA256 |
|---|---|
| app/src/jws.rs | 1D062B1BA3A1C0962CC6C16DC5B86E60A63FB388B530619EAD803E766343656A |
| app/src/claims.rs | 12A54E6CC9932363FC97F6944E6C13AAFA7F1725CF89BDCDA5A1480DDF3AB29D |
| app/src/strict_json.rs | A20B389A58975834069CBE4BE152122EE69A49187964465C5898A6E0F723803F |
| app/src/challenge.rs | 6A42D8B560DD9E7DE76E15E02D93767134DF3AD39C71BD9AC063DF00B672BA21 |
| clients/web/src/protocol/jws.ts | 5BF15B561A0AF4DFC39CDD18FC4ACA4C5F0615041A28D4522F2CD641A3DAB5B5 |
| clients/web/src/management/protocol.ts | FF6775B0B19C1DEC017D68889F7FDE975BF8B24E22AAA7E5665890AC4F592EC3 |
| clients/web/tests/fixtures/jws-v1.json | B8C3E161480C7E163A360F40B6CAE9D0F5C2EF1C90DB11FB8EB3FB160CCE6CD2 |
| clients/web/tests/browser/jws-browser.ts | 7B38EBCCE6E32C2558A4901DA99F15C5301B956A70A83A83F9B667113698F434 |

## 实际命令及边界

独立 CARGO_TARGET_DIR 为 .tmp/approval-acceptance-20261030/probe-target，未占主方 target。以下全部实际 exit0：

- cargo run --manifest-path testing/probes/establish-node-access-challenge-contract/Cargo.toml --locked --offline：22场景通过。先审查现有探针，随后独立执行，不以开发报告代签。
- cargo test --manifest-path app/Cargo.toml --lib --locked --offline challenge::tests::：14 passed，111 filtered。
- cargo run --manifest-path testing/probes/establish-node-access-jws-independent/Cargo.toml --locked --offline：独立重生成真实 DER、有效压缩点、normalize_s、已签名未知字段以及原始字节控制，同时执行 Rust 边界断言。
- 上述独立 probe 分别以 browser-proof.txt、browser-pretty-proof.txt 为参数：两次均由真实 claims::verify_auth_client 验证本轮新生成的 Chromium 签名成功。
- cargo test --manifest-path app/Cargo.toml --test jws_vectors --locked --offline：4 passed。
- cargo test --manifest-path app/Cargo.toml --lib --locked --offline jws::tests::：4 passed。
- ego-browser 执行自有 jws-open.mjs / jws-matrix.mjs / jws-extra.mjs / jws-raw.mjs：均 exit0。复用同一自有 TaskSpace9，未接管已 finish 的6/7。

Windows Chrome/156.0.0.0，isSecureContext=true。Vite仅提供当前源码测试页，地址 http://127.0.0.1:4178；这是 WebCrypto 的本机安全上下文测试，不声明受信远程 TLS/Tailscale 测试。主机证明时间检查临时控制 Date.now 并 finally 恢复；没有替代 WebCrypto 的签名/验签实现。编译有硬链接缓存失败后自动复制的警告，无编译或断言失败。

## NA-003：PASS

按原有限变异分类逐项对应：

| 原断言 | 本轮实际证据 |
|---|---|
| 固定原字节与双向签验 | Chrome固定页7项通过，包含固定Rust/WebCrypto/high-S；新Chrome签名由Rust真实claims验签。Rust固定向量4项通过。 |
| SEC1长度/曲线点 | Chrome拒绝33/64长度、错误前缀、离曲线点及真实有效压缩点；Rust在签名有效的clientPublicKey字段中拒绝有效压缩点、截短点、错误前缀和离曲线点。泛用密码学helper是否支持压缩点不等于协议wire允许。 |
| DER/P1363、r/s | Chrome拒绝真实DER及伪70字节DER；r和s分别0/n/n+1六项拒绝。Rust独立probe同样执行六个标量边界及真实DER拒绝。 |
| high-S/low-S | Chrome与Rust均接受固定high-S和密码库normalize_s生成的low-S。 |
| base64url/header | Chrome拒绝padding、+、/、非零尾位、换行；拒绝未知header、jku、crit、alg=none、跨用途typ。Rust现有定向测试覆盖非规范编码、错误段数、重复header、未知header和用途。 |
| 重复键/非法UTF8/Unicode | Chrome实际拒绝顶层和嵌套重复键、C3 28非法UTF8、未配对D800；Rust定向测试拒绝重复字段、非法UTF8。 |
| 未知字段/安全整数 | 签名有效未知host字段由真实Web协议拒绝，签名有效未知client字段由Rust真实claims拒绝；Chrome拒绝1.0、1e3、9007199254740992；Rust独立probe拒绝已签名iat/exp超安全整数范围。 |
| 不重序列化 | 独立Rust用带空白、交换header字段顺序及pretty payload的原始两段ASCII签名，Rust与Chrome均成功验签；Chrome再对pretty payload签名，Rust真实claims成功。并有payload变异导致签名拒绝的控制。 |

Chrome矩阵25项及额外13项全部通过；13项中“未知client payload由Rust拒绝”明确是Rust断言交接，不能算Chrome独立执行该客户端验证器。另有固定页7项和原始字节控制。全部使用公开合成测试key，不读用户身份；并非只有随机互签。就当前NA003列出的变异类别，无剩余必需步骤。

## NA-004：PASS

外部探针使用普通产品 ChallengeRegistry 和真实ES256签名。只有 ChallengeClock/ChallengeRandom 为显式适配器，不证明操作系统随机质量。

- 13项认证字段/用途变异：nodeId、origin、connectionId、clientId、另一个key、clientNonce、challengeId、nonce、iat、exp、v、nodePublicKey、typ；每次拒绝后原证明再次提交也拒绝，验证失败消费。
- auth在14/15秒和墙钟正常/回拨四组合验证边界及重放；pair claim在29/30秒且回拨验证边界及重复拒绝。回拨不延长期限。
- claim/status/auth三条签发入口分别注入随机源失败，均 RandomUnavailable；恢复随机源后可正常签发，无降级随机算法。
- 14项产品定向测试补足requestId/claim绑定变异、重取使旧挑战失效、跨用途、跨连接身份及隔离：包括 reissue_invalidates_old_challenge_and_cross_purpose_is_rejected、pair_claim_rejects_binding_mutations_without_consuming_the_challenge、proof_for_another_connection_cannot_clear_that_challenge、failed_auth_attempt_clears_only_its_own_connection_challenge。全部实际执行通过。

本例级别L3，不把适配器边界强行升级为网络/浏览器闭环，也不附加原步骤没有要求的无穷字段组合。

## 持久证据与清理

原始证据根 .tmp/independent-jws-20261030；probe源码在 testing/probes/establish-node-access-jws-independent 及 testing/probes/establish-node-access-independent-20261030/jws-*.mjs。所有target、运行结果均在.tmp，无身份数据库交付。

| 证据 | SHA256 |
|---|---|
| matrix.json | 89E2F15379B1E4CDF6A994644998392EDB12A262B30867559E9A27BB9D2096DC |
| extra-browser.json | 08957F91A47EFDDB9EA089B4D8A0E48CCB92DFE8958D88D82AABC23DC4C7F51E |
| raw-bytes.json | 6290579113AD913C964D014A14A1B0EDD29F3A2572C5A6E745F9A6871CF7053D |
| challenge-probe.log | 11A586E2743F697BA48B53AD2441B7EDF3D7160790A92E76A5F78C0C83D9B737 |
| challenge-unit.log | D1713E39452C8E16C29443D9383004F3A693321AF9B9B97250B4254F8167640C |

自有ego空间9已finish({keep:[]})；核对命令行归属后终止Vite PID43104完整树（子PID43600），4178监听为0。本轮没有启动节点或创建产品pipe/数据锁。非提权CIM检查曾因权限拒绝（工具exit1），随后显式提权完成核对和清理exit0；不是产品拒绝测试。用户30447仍由原PID31228监听，未操作。无新增确认产品问题。
