# R18共享replacement调用方恢复独立检视

日期：2026-10-30。结论：**PASS（本包late replacement调用方分流范围）**。未发现本次需修新问题。R17的NA-R16-001条件保存关闭保持；原tester NA-E2E001及真实浏览器补验由其分别签署，不以本适配器代关，不宣布整个change ready。

## 固定版本与范围

R18清单自身SHA256 `614413C0243680E6E7E0DF3C98141F0D568DDCAA042209EB9C996A99881E5B8A` 独立核对。对 `.tmp/frozen-implementation-r18-20261030` **83/83逐条匹配**；普通单包exe摘要独立计算为 `328D7CEBCA708A300D48AA673204E4E52F1131E6E05951F85DA4601DA7AA2800`。本轮不启动该exe冒称真实E2E。

负责变更main.ts `862BB37D63C9B1CBC0A739333944E06FED77FBB4C60A618537E92668EB28F928`、pairing.test.ts `A3C452FE7E5A54F9D47BA2EE4ECB91894A58DC9B376788AF3283CD679CD41A1E`。README/docs/plan后续元数据不放进旧清单验配结论；没有因此重跑所有产品。用户完整check143库+4/Web50和构建为开发证据，本reviewer实际执行如下。

## 源码判断

`main.ts:984–997`：原子replace返回后仍先检查signal/pageActive。unbound保持原直接新配对表单；其余状态先解除identityResetting，再boot读取共享record并按正常状态分流，不再次reset或产生另一个key。解除标志使openManagement不因本页尚在reset而拒绝进入；boot重新initialize在相同WebLock下读取，另一页再次更新时采用当时真实状态，不强制使用旧返回副本。PendingConfirmation走既有pollApproval，Approved/Consumed走既有管理认证。没有改verified4401/4410节点校验边界，无新按钮或公网流程。

R17普通save同锁并验证currentkey/origin、防upsert、private reset未变。此调用方缺口不说明R17模块修复无效；也不把这个较晚共享状态的分支通过泛化为所有多tab故障都已执行。

## 实际main有界适配器

资产：`.tmp/reviewer-implementation-r4-target/r18-main-adapter.mjs`，SHA256 `818000FF0CEF388BC27230FE97CBA69C9476E25D7929DD55E95A10F22001897D`；结果 `r18-main-result.json`，SHA256 `C2DDD20A0A19476C67B7CEE10FF2BB01BAA101ACF14B7EC4DA349EF4CA8A8964`。实际执行exit0。

适配方法：从R17/R18各冻结副本通过Vite SSR加载**真实main.ts**；仅禁用自动顶层boot启动并追加测试状态入口，不替换resetLocalIdentity/boot/openManagement/pollApproval函数体。DOM为明确的内存适配器；管理connect与状态challenge传输停在pending Promise并计数，无网络、无浏览器、无认证成功声明。IdentityVault/PairingSession使用真实代码、Node非导出WebCrypto P256 key、共享存储/串行锁适配器。初始化一次旧key、一次真实rotation后，真实save生成对应新身份状态，随后由旧rejected record调用实际main reset路径。

| 独立场景 | 实际观察 |
|---|---|
| R17 consumed负对照 | 仍显示form，管理connect未开始；key/ID未丢。复现原调用方分流缺陷，而非新的存储覆盖失败。 |
| R18 unbound | form存在，未开始管理或poll；保留原新配对流程。 |
| R18 pending_confirmation | 无form，真实pollApproval触发一次challenge适配入口；未开始管理。 |
| R18 approved | 无form，真实openManagement创建一次管理连接并进入connect适配入口。 |
| R18 consumed | 无form，创建一次管理连接；原clientId与key保持。 |
| R18 replace返回前pagehide | 使用实际注册的pagehide处理器；之后无form、无poll/管理连接，pageActive=false，新key仍在。 |

所有R18正场景删除计数都为**1（先前那一次rotation）**，reset调用方未再删；结束identityResetting=false。重读并签验后新key/clientId与当前共享record相同。适配器创建的未决网络Promise无后台socket/计时器；各Vite server finally关闭，未接管日常浏览器。

另独立执行冻结副本新增断言所在原测试：

```text
node clients/web/node_modules/vitest/vitest.mjs run tests/pairing.test.ts --root .tmp/frozen-implementation-r18-20261030/clients/web -t 'rotates a rejected key'
1 passed; 11 skipped; exit0
```

该1项含late旧reject遇到已consumed replacement仍保留key、status、ID及deletecount=1，不把11 skipped或已有Web50开发汇报算成reviewer本轮全跑。PendingClaim等其他既有boot状态分支未在此适配器额外构造，不声称本轮覆盖所有StoredPairingStatus；R18未改变这些分支定义。

## 交接结论

R18修复可采纳，原NA-R16-001/R15 SC003、R9/R14结论保持。原tester只需针对这个晚到consumed分支使用R18产物完成其定向真实补验，既有R17同字节正常链路/模块结果可按明确范围复用，不要求无限版本重跑全链。整体tester最新33例总表与剩余跨SID等明确必要断言另行汇总，reviewer不代签。

本轮新增本报告和自有适配器/结果，维护active review索引；未修改产品/冻结源/规格/旧报告/manifest/plan/tasks，未操作用户30447或日常浏览器，不提交推送归档。
