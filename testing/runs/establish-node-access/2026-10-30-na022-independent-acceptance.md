# NA022 真实旧材料验收 / NA012 r8故障子项

结论：**NA022 FAIL：NA-022-001（Medium/Open）**。已完成取消、拒绝、重启三组×lookup/claim/status共9行：7行符合当前预期，2行失败。自然过期及max留存边界未执行，需下述明确clock测试入口。NA012的五个一次性模拟错误点实际通过，但整例仍PARTIAL。

以此前14 PASS / 19 PARTIAL为基线，仅NA022由PARTIAL改为FAIL，阶段为 **14 PASS / 18 PARTIAL / 1 FAIL / 0 NOT-RUN**。不把NA012子项通过升级整例；不修改r6历史报告；不可归档。

## 固定范围及独立性

本轮开始复算r8清单自身SHA256为 **1F0F7A599C6839D53560EBF40C676015852BFE364DA4C7B51B2BF0BB849E9A9E**，清单83项；核对时已有用户明确授权的r9修改，唯一条目差异是app/src/management_ws.rs。没有声称整仓仍等于r8，没有回写清单或把该已知变化当产品缺陷。当前编译库有131测试，未运行全部131项。

本例按用户授权固定pairing负责文件范围；执行前后5项均匹配：

| 文件 | SHA256 |
|---|---|
| app/src/pairing.rs | C73F06D8187272C5E465B37AB3F97C6005BE84001173183EBDF52613D4E35C70 |
| app/src/challenge.rs | 6A42D8B560DD9E7DE76E15E02D93767134DF3AD39C71BD9AC063DF00B672BA21 |
| app/src/claims.rs | 12A54E6CC9932363FC97F6944E6C13AAFA7F1725CF89BDCDA5A1480DDF3AB29D |
| app/src/ipc.rs | E65826C1B56289D45E4832FEB5F2592A28050A702ED52D1FC989E05E1446162A |
| app/src/platform/windows.rs | CD028D68BB4AF7B9927BE17C1A1A76D6DD260063010A88FB40018F617EE69971 |

普通外部probe直接链接app，使用真实NodeRuntime、SQLite、DPAPI、owner named pipe和HTTPS，并非只看源码。未创建浏览器空间，不声称真实浏览器或Tailscale验收。本轮使用专属30452及 `.tmp/independent-na022-20261030`；TLS由probe生成localhost证书并显式加入客户端trust roots，没有ignore证书验证。未运行主方target中的exe。

## NA-022-001：终态仍接受旧claim proof

严重性Medium。依据：当前tests.md NA022要求失效材料经lookup/claim/status不恢复旧配对、未认证统一pairing_invalid；access-contract §3规定已接受proof摘要与结果缓存“直到配对终结”，终态只能由绑定key的有效status证明查询。

复现（两次独立配对，分别取消和拒绝）：

1. owner IPC begin_pairing，新真实公钥经HTTPS lookup、签名claim，断言HTTP200及owner pending_confirmation。
2. 提前取得status挑战、保存由同key签名的status proof，但尚不提交。
3. 取消分支drop自有IPC会话，等待250ms；拒绝分支owner session.reject，实际读取rejected事件。使用新status挑战验证最终状态作为终结成功控制。
4. 在原300秒内，逐字重发原claim proof到 `/api/access/v1/pair/claim`。
5. **实际HTTP200，预期401 pairing_invalid**。两分支均复现。lookup旧码和旧status proof返回401 pairing_invalid，说明不能把结果归因于终结操作未发生。

边界：没有观测到信任建立或终态复活。绑定key用新status挑战仍返回cancelled/rejected、clientId=null；失败事实是终态claim端点仍给成功，而非已经证明权限提升。源码定位供修复参考：PairingService::claim在检查session.state之前对相同requestId及原compact返回缓存hostProof；此定位不是动态复现的替代。

不自行修改预期或产品。建议修复时明确区分仍允许保留查询的approved/consumed和rejected/cancelled/expired的claim重放规则；按现有合同修复后用本probe重跑。

| 失效原因 | 旧码lookup | 原claim proof | 终结前取得的status proof |
|---|---|---|---|
| cancelled | 401 pairing_invalid：PASS | **200：FAIL** | 401 pairing_invalid：PASS |
| rejected | 401 pairing_invalid：PASS | **200：FAIL** | 401 pairing_invalid：PASS |
| 同data正常stop/restart | 401 pairing_invalid：PASS | 401 pairing_invalid：PASS | 401 pairing_invalid：PASS |
| 自然expired | NOT-RUN | NOT-RUN | NOT-RUN |

额外已执行：cancelled/rejected只有绑定key的新status证明成功取回终态且ID为null，hostProof用实际节点公钥验签成功；重启后连新status challenge也401。没有把旧status proof失败错误等同于所有新状态查询禁止。

## 执行入口与退出码

自有源码 `testing/probes/establish-node-access-na022-independent/{Cargo.toml,Cargo.lock,src/main.rs}`；原始运行目录 `.tmp/independent-na022-20261030`。

```
$env:CARGO_TARGET_DIR='D:/Project/acp-remote/.tmp/approval-acceptance-20261030/probe-target'
cargo run --manifest-path testing/probes/establish-node-access-na022-independent/Cargo.toml --locked --offline
```

显式require_escalated真实jack运行。首次自有probe误用private解码helper，编译exit101且未启动节点；改为base64公开库后真实执行exit1，末尾明确 `COMPLETE old-material rows=9 assertion_failures=2`。退出1来自上述两个产品断言，不是超时或fixture错误。全部三组均完成，失败后也正常stop/join、清理，不在首行红灯就遗留资源。

## NA022尚缺的精确有限入口

`PairingService::with_sources`公开但所需TrustStore不能从普通Storage公开取得（trust_store是pub(crate)）；NodeRuntime没有注入pairing clock的公开构造器。因此不能仅靠普通外部crate设置受控pairing时间。未通过改生产可见性、改用户时钟或冒充已跑clock测试绕过。

请主方在cfg(test) Fixture内补以下有限断言，tester再独立运行：

- claimed配对在300秒自然过期，旧lookup/claim/status证明，以及终结前后fresh status行为逐项按合同断言；同时检查不建信任。
- 批准在t=0、240、299秒三个分支，结果expiresAt分别为原300、相等300、延长359，覆盖max两侧及相等。
- 每个分支用新鲜status挑战/证明在retention-1、retention、retention+1查询，不能用已超30秒的旧proof失败冒充留存清理成功。
- retained期间标记consumed仍返回原稳定ID；过界仅删除临时材料，权威DB同key信任和ID不变。未绑定key不获终态。

NA007的真实CLI提交后取消/revoke提示本轮未执行；原响应丢失恢复证据保持已覆盖分支，不随本例升级。

## NA012：r8五个错误点子项PASS，整例PARTIAL

审查并显式非沙箱独立运行：

```
cargo test --manifest-path app/Cargo.toml --lib --locked --offline platform::windows::tests::named_pipe_check_failures_reject_and_restore_the_current_thread -- --exact --nocapture
```

exit0，**1 passed，130 filtered**，不是零目标。真实pipe和真实当前线程token；线程局部一次性错误点分别为ImpersonateNamedPipeClient、OpenThreadToken、GetTokenInformation(size)、GetTokenInformation(TokenUser)、RevertToSelf。每次断言失败关闭、故障点已消耗、线程不再impersonating，下一次合法verify成功且线程仍无impersonation。

**RevertToSelf是显式调用前模拟失败，RAII Drop随后真正调用RevertToSelf成功还原**；没有模拟或证明持续Win32 Revert失败时仍能恢复。pipe/client测试句柄drop释放。

环境校准：按用户最新确认，当前jack双执行环境没有可用的另一SID/admin权限可继续完整跨用户测试。旧真实SID证据仅复用它实际覆盖的DACL/pipe/DPAPI/命令，不能写“当前两SID可用”。NA012仍缺异SID pair/stop、remote pipe等，整例不提升PASS。

## 证据与清理

| 原始证据 | SHA256 |
|---|---|
| protocol-run.log | 1DDB335E545F0BE82CA42494D5825550AFCC9F1FC3537D1B398AAE5310FC0742 |
| results.json | E8BB689F9C79ED3A6807779382DDA1F4CE3485A8418DC45AF02F737227E8C76F |
| pipe-failures.log | C612703A00E0683902DE10616E2090E31CA55189F6ECB02BD25CB4E7C761A569 |

每组owner stop/join后明确检查自己的确切pipe名称不存在，再删除专属data/cert/key目录。最终运行根无任何子目录，probe进程0、30451/30452监听0；用户30447仍原PID31228监听。未操作用户身份/日常浏览器。未修改产品、规格、清单或旧报告；不提交、推送、归档。
