# R17条件保存修复与慢writer独立检视

日期：2026-10-30。结论：**PASS（本包限定范围）；NA-R16-001 / Medium / Closed**。原reviewer已重跑原失败路径并验证新身份不被覆盖。原tester NA-E2E001仍由其真实多tab复测单独签署，本报告不改其历史FAIL或自动签署整体可归档。

## 固定版本

独立计算R17清单SHA256 `5074890C209A702335F13042A26DBA64278EFD7A7B0A70A4E42E2F698BA3E26F`。对 `.tmp/frozen-implementation-r17-20261030`逐条校验**83/83匹配**。README、平台文档及plan后续元数据修改不纳入旧83项结论；本轮继续使用冻结源，不因这些修改重跑无关产品或误报源漂移。

ordinary binary先复制至reviewer自有 `.tmp/reviewer-implementation-r4-target/r17-bin/acp-remote.exe`，复制后SHA256 `38AA9D02F334E3C81FDC900ED4F8943BB78339650981BC0046A609381D03E6C2`。没有直接运行主方target。本轮独立动态为下述Web模块/测试和cfg(test)真实WSS，不冒称普通exe真实多tab执行。

完整开发日志 `.tmp/implementation-r17-check.log` SHA256 `E26A925EC419C2D1E0E4068AAA9899698EB1D3F6DC9B8560DAF668385704E7E6`，已读143库+4向量/Web50及构建检查，不称reviewer重跑全量。

## NA-R16-001关闭依据

位置：`clients/web/src/pairing/identity.ts:128–144,213–243`；`main.ts:304,335,368,593,943–958`。

普通save现在也进入与initialize/replace相同origin WebLock，在锁内load/parse并核对当前publicKey+origin，自检当前与incoming真实CryptoKey后才save。记录缺失/不同身份抛identity_changed，普通更新不能无条件upsert。删除改为private #reset，仅原子replace内部调用；该路径在同锁下比较被拒旧key，不删除另一页有效新key。锁覆盖读→资格比较→异步自检→写，serializing本身与条件更新同时成立，修复的不是仅把旧写入排队。

main的submitPair/retry/poll/openManagement捕获identity_changed后中止旧controller、取消reconnect、断开连接、增加本页generation、清旧session/currentRecord；仅pageActive且未主动断开时boot重新读取，不再reset。boot的initialize仍有排队后AbortSignal检查，等待锁不会绕过已有页面取消。没有新增按钮/公网API/平台保证。这里是源码生命周期审查，不冒称已完成真实pagehide/main多tab动态验收。

### 原复现独立重跑

使用Vite middleware/SSR从**R17冻结副本**加载真实IdentityVault、PairingSession、PairingApi；Node真实非导出P256 CryptoKey、受控共享repository与串行LockManager（只适配存储/锁，不自行判断身份）。原同序列：A持旧pending session→B replace为Knew→A迟到acceptAuthenticated。结果：

- 迟到真实PairingSession.acceptAuthenticated抛identity_changed；
- 普通旧record save也抛identity_changed；
- 再initialize仍是Knew，没有回滚Kold；
- Knew当前身份正常save成功；
- 将自有repository设为无记录后，普通save仍抛identity_changed，不能重建/upsert。

实际exit0。汇总 `.tmp/reviewer-implementation-r4-target/r17-stale-save-result.json` SHA256 `5AD2429D024C1C55237FB5B2BAD665A0F2FC86189E11A6738F10396620A91991`，只含六个true布尔断言，不保存密钥/proof。Vite无监听，finally关闭。复现层与R16原失败相同，R16会回写旧key，R17拒绝，故原ID关闭；不是把49→50数量变化当修复证据。

另独立 `node clients/web/node_modules/vitest/vitest.mjs run --root .tmp/frozen-implementation-r17-20261030/clients/web`：**7文件50项通过、exit0**，含并发boot、双rotation/late rejection、排队取消和迟到session保存回归。缺少navigator.locks仍是既有fallback，不对无锁平台授予跨页一致性保证；当前Chromium目标真实WebLocks能力由tester验证。

## 慢writer预算补验

从R17 frozen app，reviewer专有target独立执行：

```text
cargo test --manifest-path .tmp/frozen-implementation-r17-20261030/app/Cargo.toml --target-dir .tmp/reviewer-implementation-r4-target/r10-build --locked --offline --lib paused_real_writers_close_at_the_byte_or_frame_queue_budget -- --nocapture
1 passed; 0 failed; 142 filtered out; exit0
```

1测试含两个真实TLS/WSS场景：writer在实际出站前暂停；字节分支通过真实device.list大页响应、默认命令窗口，在少于256帧时达到1MiB；帧分支node.info小响应达到256帧、仍小于1MiB。检查服务端实际占用+拒绝帧大小，释放writer后读取真实Close4429，正常runtime.shutdown结束。

frame-first为明确**cfg(test)每帧重置CommandWindow适配器**，避免120命令/分钟先关连接遮蔽256帧路径；不声称普通客户端能在真实120额度下同窗口发送257条成功命令。byte-first保留默认命令窗口。其原子bool默认false、入口/统计均cfg(test)，普通budget及无环境开关不变；120/121额度另有组件/真实入口证据。只取snapshot常量或暂停OS socket读不能证明这个队列边界，本测试实际调用正常解析/认证/命令执行/Outbound失败和Close发送路径，补验适用。

## 预算tester最新报告边界复核

已读 `2026-10-30-budget-acceptance.md` 最新追加，读取时SHA256 `902C4650946604444F9D06A4A10E92F9A36B7340C6B537FB2398862E4BEE9AE2`。开头和旧“未覆盖”段保留历史，不能忽略末尾已经执行结果。

- 真实300秒pair流报告为R14 ordinary binary、真实owner IPC/HTTPS和系统时间，无虚拟时钟。299003ms仍无事件、299035ms lookup回显原expiresAt；300048ms expired、300049ms EOF，之后旧码401，信任列表前后摘要相同。读取原始 `pair-lifetime-real.json`，passed=true/clientExited=true。300048ms包含调度/传输/轮询，不说纳秒精确到期，也不把约300秒终结重新列为“尚未执行”。本reviewer没有再等待300秒，采纳的是独立tester相同负责运行时范围的真实证据。
- R17 slowwriter tester为独立自有副本，仅node.rs追加stderr观测，明确不是原普通exe内含测试开关。其报告字节场景1046252+16602>1048576、63帧；frame场景83782字节/256帧；两者实际socket4429。frame CommandWindow适配和FakeProtector/预置DB都如实声明。数字归tester执行，本reviewer独立重跑原冻结测试断言，不冒称自己的stderr数值。
- 原始r17-slow-writer-cleanup.json存在，记录冻结node hash、Webdist复制匹配、进程/监听/目录清理检查；tester报告runtime.shutdown成功、自有任务结束。后续总体汇总应以这些追加结果收缩预算缺口，保留旧IPC FAIL历史及R14修复版本，不混成一次全版本E2E。

## 结论与残余范围

未发现本包新需修问题。SC003、RV001/002、CON/LIFE/I01和R14限定修复结论保持；NA002供应链/密码库选择与锁文件未改，可按R15报告范围复用。本轮不重复秘密扫描或称旧扫描含后来元数据新增；最终版本清单另由主方界定。

剩余工作是原tester真实多tab NA-E2E001与最新全包E2E，不由本reviewer单测/模块受控复现代签；NA008/027/028/029及跨SID等其他具体必要断言按各最新独立证据逐项汇总，不以旧阶段统计替代。用户当前033人工理解确认不重开，未恢复旧034或旧字段门禁。

本轮仅新增本报告，所有构建/probe在reviewer专有目录；不改source/规格/旧报告/manifest/plan/tasks，未操作用户30447、日常浏览器或真实用户数据，不提交推送归档。
