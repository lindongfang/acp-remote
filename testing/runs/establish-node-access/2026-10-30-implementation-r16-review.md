# R16跨页面身份协调独立检视

日期：2026-10-30。结论：**BLOCKED：NA-R16-001 / Medium / Open**。initialize与双rotation串行修复在限定路径有效，但普通save仍可把另一页面的新身份覆盖回被拒旧身份，不能关闭多页面身份一致性问题。原tester NA-E2E001失败及复测由其单独记录，本报告不改历史FAIL。

## 固定版本与实际执行

- R16清单SHA256独立核对 `FCA2669CFC561CC159C498F19262D9B893A0E590F4FEA8D5EF3169059F74D8E3`。对 `.tmp/frozen-implementation-r16-20261030` 逐条验配，**83/83匹配**，未用实施中的工作区字节代替。
- 用户普通exe已复制到reviewer专有 `.tmp/reviewer-implementation-r4-target/r16-bin/acp-remote.exe`，复制后摘要为 `3EBF8E44380DF74082A588E5506CB2F3DA91A3B2DA8E94265F5837E1DD96709B`；本轮没有运行该exe冒称完成真实浏览器复测。
- 独立执行 `node clients/web/node_modules/vitest/vitest.mjs run --root .tmp/frozen-implementation-r16-20261030/clients/web`：**7文件49项通过，exit0**。现有3项跨vault单测真实运行，不将其当作覆盖所有写入者。
- 以Vite middleware/SSR加载冻结副本真实IdentityVault、PairingSession、PairingApi，用Node真实非导出P256 CryptoKey、受控共享repository和串行LockManager适配器执行下述复现，无监听服务/浏览器/用户DB。结束关闭Vite。输出仅布尔值，未保存私钥/proof。

## NA-R16-001：旧页面普通save覆盖新key

位置：`clients/web/src/pairing/identity.ts:128–143,212–221`；`clients/web/src/pairing/session.ts:117–132`（其96/173/194其他save亦属写入者）；`clients/web/src/main.ts:560–570`。

initialize与replaceRejectedIdentity取得同origin WebLock，但save不取得该锁、不重新读取当前record比较公钥。真实session仍能根据自己保留的旧record调用save；selfCheck只验证该旧record内部私钥/公钥对应，不能证明它还是共享存储的当前身份。main的managementGeneration只是单页面变量，另一个page旋转不会自动改变本页generation。

### 独立复现（实际通过旧写回路径）

1. 同一受控repository、同一个串行LockManager上创建vault A/B，真实initialize得到Kold；A持有合法pending_confirmation record（真实私钥、公钥及fixture节点绑定），以此创建真实PairingSession。
2. B调用真实replaceRejectedIdentity(old)，得到Knew且Knew!=Kold，仓库此时是Knew。
3. A的迟到认证完成流程调用真实 `PairingSession.acceptAuthenticated(validClientId)`。没有网络mock授权结论，这一步模拟“先前真实连接认证消息在另一页轮换后才被页面处理”的受控时序；该方法实际调用IdentityVault.save。
4. B重新initialize读取仓库，结果为Kold，新Knew丢失。得到 `replacementWasDifferent=true, staleAuthenticatedSaveAccepted=true, currentKeyIsOld=true, replacementLost=true`。

原始汇总 `.tmp/reviewer-implementation-r4-target/r16-stale-save-result.json` SHA256 `71AEE83A9B3A9ACD856B7235339EC52D30815279A068D5935110FD45928A3D6F`（仅布尔结果）。这是实际产品模块/ES256 key路径复现，不是测试自己模拟比较逻辑；repository/锁为明确适配器，未声称已在两个真实Chromium tabs复现。原WebLocks request串行语义在适配器中成立，缺陷发生在完全不进入该锁的save，不依赖模拟锁违反平台语义。

### 影响和最小闭环

有效replacement可被已失效页面回滚；之后旧拒绝再次触发rotation，导致身份丢失/重复生成或再次不一致。没有证据证明服务器撤销记录被复活或失效key获得授权，不升级描述为服务端认证绕过。

最小修复需把所有身份写入口放在同一origin协调边界，并在锁内读取/验证当前身份，拒绝基于不同publicKey的迟到record覆盖。**仅给save加锁仍不够**：晚到save会排队后合法获得锁，再覆盖Knew；需要当前身份比较/条件写入。若要支持跨身份恢复，必须显式区分新建、替换和更新权限，不将普通save变成无条件upsert。reviewer不指定额外按钮或产品流程。

必要回归：A持旧session、B完成rotation后，A迟到acceptAuthenticated/其他普通状态save不能更改Knew；两initialize/两rotation/late旧拒绝仍应复用同Knew；排队pagehide无写入；当前有效record的正常save可继续。若发生变化时向旧页面明确报过时身份，应保证main停止旧session并重新读取，而非自己再次无条件重建。原tester需真实多tab复测，不以这些单测代签其E2E。

## 其余代码检视

replaceRejectedIdentity在同锁下initialize并selfCheck当前record，只有公钥等于rejected旧key才delete+initialize；另一page已换key就返回有效replacement。等待锁后的#initialize先throwIfAborted，read/generate/save前既有AbortSignal检查保留；main传被拒record走单次替换，verified4401/4410前置边界没有扩大。当前目标Chromium的navigator.locks可用性由tester实证；缺少该API时仍走旧repository行为，**不由本报告给无锁平台跨页一致性保证**，也不凭fallback存在新增未承诺平台阻断。

pagehide/页内identityWork继续保护本页任务，不足以解决跨页旧写入，正是本问题范围。delete以后再initialize被取消可能暂时无record，是本轮未提交新身份的状态，不把它误报为替换既有不同key。

SC003/debug-embed、R9/R14生产修复未受影响，保持各自关闭/限定通过。本轮仅新增本报告，未修改产品/旧失败报告/manifest/plan/tasks，未运行或操作用户30447、正常浏览器或用户数据，不提交推送归档。
