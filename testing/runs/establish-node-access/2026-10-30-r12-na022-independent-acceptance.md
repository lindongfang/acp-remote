# R12：NA022原红灯复验、有限留存边界及NA027补充

**NA-022-001：Closed，由原独立tester复验关闭。NA022：PASS。** 原真实HTTP两个红灯在新固定binary上均变为401 pairing_invalid，原9行全部通过；另外补完批准响应丢弃后留存内重取及自然过期、max留存边界。不是使用R9 binary或只照抄开发自评关闭。

NA027新增10000条/精确24小时子断言独立通过，整例仍PARTIAL。按上一份本tester阶段表，仅NA022由FAIL转PASS，累计 **15 PASS / 18 PARTIAL / 0 NOT-RUN / 0本tester未闭FAIL**；尚未合并其他并行tester新报告，仍不可归档，不代签reviewer。

## 固定目标与真实/适配器边界

执行前、执行后独立核对 `.tmp/frozen-implementation-r12-20261030/2026-10-30-implementation-r12.sha256`：自身SHA256 **6D7E0EC6CC1FD9DD6AC0F07719841AF1B0736F81EB265B7E5C0ACDB3DE6E409B**，**83/83、0 missing/mismatch**。不把额外生成Web dist描述为83原始清单条目。

冻结普通binary SHA256 **92D9D759D4D6EFE5806338FB1E3DF0FA996CCA10F4766D9E6CC4E14484A2DD44**，核对后复制到 `.tmp/independent-na022-r12-20261030/bin/acp-remote.exe`，执行后相同。真实节点由该副本的 `--config … daemon start` 启动，不运行主target exe、不使用旧版NodeRuntime替代新binary。

协议probe可复用的IPC/JWS客户端helper也链接冻结副本app：自有版本化源码复制到本轮 `.tmp/.../probe-build`，仅其生成Cargo.toml的app依赖指向冻结app绝对路径。独立CARGO_TARGET_DIR为 `.tmp/approval-acceptance-20261030/probe-target`。没有修改冻结源文件、锁文件或现行产品源码。

Windows真实jack上下文显式require_escalated。HTTP实验用真实SQLite、DPAPI、owner named pipe、HTTPS，localhost测试证书被显式加入客户端trust roots，未忽略证书校验。固定端口30452，专属data/cert/key，未使用用户30447。没有新浏览器空间，不将本轮HTTP probe冒称真实Chromium。

时间边界测试使用明确TestClock，真实配对注册表/真实ES256/SQLite；不声称实际墙钟等了359秒或24小时，也不替换其真实事务/验签逻辑。

## 实际命令与退出码

真实HTTP原探针：

```
$env:CARGO_TARGET_DIR='D:/Project/acp-remote/.tmp/approval-acceptance-20261030/probe-target'
$env:NA022_RUNTIME_ROOT='D:/Project/acp-remote/.tmp/independent-na022-r12-20261030'
$env:NA022_BINARY='D:/Project/acp-remote/.tmp/independent-na022-r12-20261030/bin/acp-remote.exe'
cargo run --manifest-path .tmp/independent-na022-r12-20261030/probe-build/Cargo.toml --locked --offline
```

exit0：`COMPLETE old-material rows=9 assertion_failures=0`。随后同入口设置NA022_APPROVED_ONLY=1执行批准结果交付丢弃分支，最终exit0，2个旧lookup/claim拒绝断言及留存内恢复通过。保存两次结果文件，未覆盖原9行证据。

以下命令均从冻结app运行，均exit0，每项 **1 passed / 139 filtered**，不存在零测试冒充通过：

```
cargo test --manifest-path .tmp/frozen-implementation-r12-20261030/app/Cargo.toml --lib --locked --offline TEST_NAME -- --exact --nocapture
```

| TEST_NAME | 实测耗时/范围 |
|---|---|
| pairing::tests::claim_retries_stop_at_every_terminal_state | 0.23秒；5终态及pending原proof幂等控制 |
| pairing::tests::natural_expiry_rejects_old_material_and_fresh_status_without_trust | 0.06秒；299/300边界及无信任 |
| pairing::tests::approved_and_consumed_results_follow_the_exact_maximum_retention_deadline | 0.60秒；0/240/299批准与300/300/359留存 |
| trust::tests::full_revoke_budget_preserves_results_until_the_exact_clock_deadline | 0.07秒；10000结果与86399/86400联动 |

本轮没有重新执行主方全check；用户提供的140库+4向量/Web46全check不写成本tester独立运行数量。

## NA022有限步骤全部对应

| 原步骤/预期 | 实际覆盖与结论 |
|---|---|
| 取消后旧码/claim/status证明 | 新binary真实HTTPS：3项均401 pairing_invalid；新挑战仅绑定key可读cancelled、clientId=null，hostProof真实验签。PASS |
| 拒绝后旧码/claim/status证明 | 新binary真实HTTPS：3项均401 pairing_invalid；owner reject事件已读到；新绑定证明可读rejected、clientId=null。PASS |
| 同data进程重启后旧材料 | 原binary子进程正常owner stop退出，同data新进程启动；3项均401，新status challenge也401；owner pipe名称相同。PASS |
| 自然过期 | 冻结cfg(test)在299秒取得新status proof，300秒旧lookup/claim/该新鲜proof及新status challenge均PairingInvalid，DB无该key信任；不是用已超30秒proof失败冒充会话过期。PASS |
| 各合法终态旧claim拒绝 | pending同proof幂等控制相同hostProof；cancelled/rejected/expired/approved/consumed五终态后同proof均PairingInvalid。PASS |
| 批准响应丢弃后留存内重取 | 新binary真实HTTP：owner批准完成；批准后新status挑战，实际取得HTTP200、验签确认approved及原clientId，随后在probe交付边界丢弃结果；等待后用另一个新挑战/证明重新status，HTTP200、验签、同原ID。PASS |
| max两个分支及相等 | 批准发生t=0/240/299，真实签名status expiresAt分别对应300/300/359。PASS |
| consumed仍可取结果，仅绑定key可看 | 三分支均验证其他key不能取得状态挑战；consumed后在retention-1用新proof成功，原稳定ID。PASS |
| exact deadline与之后 | retention-1再签一份仍新鲜proof，到exact deadline拒绝；deadline及deadline+1新status challenge均拒绝。PASS |
| 清临时材料不撤销信任 | 各边界后真实SQLite查询原key仍对应原稳定ID；没有用重新批准或新key掩盖身份替换。PASS |

“丢响应”是明确受控的调用结果丢弃，已收到并验签后不向后续模拟消费者交付，不声称物理网卡丢包。旧独立报告 `2026-10-30-response-loss-acceptance.md` 仍提供真实Chromium生产页保持key/clientId=null及重启恢复证据；该旧binary浏览器记录不冒称R12页面重跑。本轮新增的R12真实HTTP留存内恢复填补了仅有重启恢复的缺口。

追加分支首轮夹具错误：它误用批准前status挑战，批准清理挑战后HTTP不是200，自有断言exit1。该结果不计新产品失败；改为批准后取得新挑战后一次有界重跑exit0。保留 `approved-response-loss.log` 原失败日志及 `approved-response-loss-final.log` 成功日志，不隐去失败命令。未改产品或降低预期。

原NA-022-001的问题事实为“取消/拒绝后原claim仍200”；本轮在同类真实入口明确改为401且终态查询保持正常，因此关闭该ID。历史红灯报告原样保留。

## NA027精确24小时+满10000：子项PASS

独立审查并执行冻结测试，未仅采纳开发文字：先真实revoke产生结果，再填入9999条尚未过期合成记录，到86399秒重放原结果成功、同request不同载荷冲突、新request容量满拒绝、数量仍10000；到86400精确边界原槽过期，同request可处理第二目标，结果可重放、数量仍10000，两客户端仍revoked。不是直接UPDATE原行expiresAt制造过期。

结合已独立完成的SQLiteFull真实错误码13回滚/容量恢复同request重试、原独立存储事务失败和10000容量证据，本项预算/过期联动缺口收缩。**NA027仍PARTIAL**：本tester已采纳证据中，尚缺真实WSS连续list/info不逐次落幂等结果、审计不逐帧膨胀的计数关联，以及该故障矩阵下B重连/管理拒绝与最终状态的完整对应。可用既有SQLite快照+真实管理WS补，不要求用户人工验收或额外无穷故障组合。

NA007真实CLI提交后取消的device revoke提示仍未执行，状态不升级；本报告不把cfg(test)取消不回滚信任代替该真实终端提示。

## 清理与持久证据

四组自有节点均owner stop/join；每组检查确切named pipe路径不存在后删除本组data、DPAPI identity、SQLite和测试cert/key。最终cancelled/rejected/restart/approved四目录全部不存在，专属binary进程0，30451/30452监听0。用户30447原PID31228仍监听，未操作。cfg(test)自有temp实际为空后删除。未操作日常browser、旧tester空间或其他临时目录。

保留非敏感probe-build源码、binary副本及脱敏结果；运行根 `.tmp/independent-na022-r12-20261030`。probe源码为 `testing/probes/establish-node-access-na022-independent`，NA022_APPROVED_ONLY追加分支不改变原3组默认入口。

| 证据文件 | SHA256 |
|---|---|
| protocol.log | 6B64149CD4937BC72D2B90B5B4B5776FF16E171AEFC2872BB34B71A9D8A1E5E3 |
| results.json | 94F8CF906DD80808132EBE202CACF73FE0B66340F5D1626AD5E7B025D563CE4F |
| approved-response-loss-final.log | 80CD373B97289E98365FB0C7802A4E49CFD761ADDFD7B127CBB8572313C6A6E3 |
| approved-results.json | E2D29423B579A400D5037272DEFAC76860CB5A5A12B41235C62E76E1745EA5BF |
| test-results.json | 624BEED96892F38E586527BEE4331C83CA315579772F787E4E0074A3B771D98A |

冻结83项与binary后验仍匹配；未修改产品、规格、清单、plan/tasks或历史报告，无提交、推送、归档。
