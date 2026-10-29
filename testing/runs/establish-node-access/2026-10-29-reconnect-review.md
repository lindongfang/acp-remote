# 自动恢复管理连接：独立小范围检视

日期：2026-10-29。当前结论：PASS（仅 NA-RC-001/002 修复语义及本次小范围静态检视/单元回归）；两项均 Closed，未发现新增确认阻断。真实浏览器自动恢复与页面生命周期集成尚未由本 reviewer 执行。本报告不重新判定 establish-node-access 整体验收。

## 修复复查（23:30）

### NA-RC-001：Closed

- `management/reconnect.ts` 保存 `nextAttemptAt`，`cancel()` 只撤销 timer，`resume()` 按剩余 deadline 挂单一 timer；deadline 到达才调用连接回调。`main.ts:98` 的 online/pageshow 统一进入 resume；`:104` 的实际连接入口仍检查 pageActive、主动断开、恢复资格、现有连接及已批准身份。因此重复 online 不提前连接，pagehide 后恢复不会丢失原限流截止时间。
- 新增测试在 60 秒限流窗口第 10 秒 cancel/resume，并重复 resume，断言只有一个 timer，第 59999 毫秒尚未调用，第 60000 毫秒仅调用一次。独立运行通过。
- 精确语义：成功 reset 清除历史；终端错误的 schedule 同样调用 reset，但页面随即设置 automaticReconnectAllowed=false，故 online/pageshow 不会借由已清空的 deadline 复活该身份。主动断开也有独立布尔栅栏。

### NA-RC-002：Closed

- `management/client.ts:504` 保留首次本地失败；`:468` 将有效原因放进 `ManagementClose.error`，远端 4410/4401 优先于本地失败。`main.ts:506`、`:862` 使用 close.error，而非仅按本地 4400 重建错误；本地 request_timeout 继续归为暂时错误，真正 ManagementProtocolError 仍停止恢复。
- 新增 `management.test.ts:503` 完成真实客户端签名认证流程后，以 fake socket/fake timer 触发 node.info 请求超时，验证 close code 为 4400、close.error 为 request_timeout 且 command 仅发送一次。独立运行通过。
- 代码未增加管理命令重发：自动恢复仍仅新建连接、重新证明身份、读取信息/列表及恢复订阅。该测试实际使用 node.info，不是 device.revoke；对撤销不重发的结论来自共同请求实现与恢复入口的静态复核，没有把它写成真实撤销故障测试通过。

### 验证及剩余限制

- reviewer 独立执行 `npm.cmd --prefix clients/web run check` 和 `npm.cmd --prefix clients/web run test -- --reporter=dot`，均退出 0；7 文件、38 测试通过。
- 新回归分别覆盖调度器 deadline 与连接层错误传递；main.ts 的实际 online/pagehide/pageshow、手动断开、远端身份拒绝交错时序未由这两个测试直接运行。已静态检查对应资格栅栏和 generation 检查，未发现新的确认缺陷；真实浏览器结果需单列证据，当前不推断通过。
- 本次复查没有改源码、主报告、review.md 或清单。下文保留原问题及原字节，作为修复历史，不代表当前仍 Open。

当前复查字节：

```text
33644C4EA9DD9160B709D41C67BD8BC6274B27F9AA7F57C0C372356E8FB6D230  clients/web/src/management/reconnect.ts
D95041EC35CA679B9F7564240540DC38F3F1C6C4E1FAD1B1D8A0A394954D629F  clients/web/src/main.ts
8F6501E2E242385303EEA5EE0497D4F89AD0B7BA65B434F9EED75810DF7C5BE8  clients/web/src/management/client.ts
6423002304B5BD953FD648DDC918306F748864ECE0FB36127A0C854D92EBFAFF  clients/web/tests/reconnect.test.ts
9F199F3B0B3BE7FF28FDE3E9C51A6E69B3ED73FBD5A534E540535026F7767445  clients/web/tests/management.test.ts
```

access-contract 和 client-trust 增量/长期规格摘要与初审一致。

## 范围与依据

只读检视 reconnect.ts、main.ts、reconnect.test.ts，并追读 management/client.ts 的超时、断开和 close 回调。依据当前 access-contract §4 自动恢复条款、client-trust 增量和长期规格“身份必须经过验证”，以及用户确认的 2/4/8/16/30 秒退避、限流等待 60 秒、终端错误停止、页面离开/主动断开取消、原身份复用和不重发命令要求。

已读取当前 AGENTS、OpenSpec 配置、reviewer 职责和 tests.md。当前末例 NA-033 仅要求用户主观理解判断；本检视不要求恢复旧人工安全签署或旧用例编号。已知用户流程/规格编辑按授权保留。

## 初审问题（历史，当前两项均已关闭）

### NA-RC-001 — Medium：online/BFcache 恢复绕过退避和限流窗口

- 位置：`clients/web/src/main.ts:92`、`:98`、`:462`；`clients/web/src/management/reconnect.ts:16`。
- 依据：失败后 schedule 设置等待计时器，但 management 已为 null。online 直接调用 resumeManagement；pageshow(persisted) 也调用同一入口。resumeManagement 仅检查允许恢复、身份和连接状态，没有检查重试最早时间。随后 openManagement 会取消现有计时器并立即连接。
- 复现：节点以 4429/4409 拒绝，或初始化请求返回 rate_limited，已安排 60 秒等待；在 1 秒后浏览器产生 online 事件，立即建立新 WSS。pagehide 取消计时器后，在 60 秒内 BFcache pageshow 恢复也会立即连接。普通 2/4/8 秒退避同样可以被事件提前绕过。
- 影响：违反限流至少等待一个固定窗口的合同；网络抖动或反复页面恢复可使重连更密集。当前单计时器机制本身不会阻止事件直接发起连接。
- 建议：由统一恢复调度器保存最早重试时间，并让 online/pageshow 与 timer 共用该入口；pagehide 可取消 timer，但保留限流截止时间，恢复后仅安排剩余等待。增加页面集成回归：限流后 online、pagehide/pageshow 不提前创建 socket；到期仅创建一个连接；终端错误和主动断开后不创建连接。

### NA-RC-002 — Medium：已认证请求超时会被本地 4400 覆盖，停止自动恢复

- 位置：`clients/web/src/main.ts:582`、`:611`、`:862`；`clients/web/src/management/client.ts:390`、`:466`、`:494`；`clients/web/src/management/reconnect.ts:56`。
- 依据：request timeout 被调度器定义为可恢复。但已认证连接的 device.list 请求超时后，refreshDevices.catch 仅显示 dashboard 错误，并未把 request_timeout 交给恢复器；client 的超时路径调用 #failProtocol，使用本地协议关闭码 4400。对端正常回应关闭握手时，onClose 收到 4400，main 将它重新包装为 connection_closed/4400，恢复器按终端协议错误取消恢复。
- 复现：完成认证、订阅并显示列表后，发起一次刷新，让响应超过 15 秒；对端正常响应客户端 4400 close。观察页面进入停止状态，无退避计时器，后续 online/BFcache 也因 automaticReconnectAllowed=false 无法恢复。若此时网络直接断开产生 1006 则可能恢复，不能以该路径通过覆盖正常 close 握手路径。
- 影响：合法身份在临时请求超时后永久停止本页自动恢复，违背用户的临时断线自动恢复要求。撤销请求超时也存在相同的原始错误丢失路径；修复不得自动重发该命令。
- 建议：连接层保留导致关闭的本地原因并传给页面，区分暂时超时和真正协议校验失败；页面统一消费该原因安排恢复。恢复只重新认证、读取列表/订阅，不重发 device.revoke。增加真实 client + 页面协调的 fake socket/fake timer 回归，覆盖 list/revoke 超时后对端回显 4400、恰一次重连、撤销命令不重发，以及真正协议错误仍停止。

## 已验证与限制

- 独立执行 `npm.cmd --prefix clients/web run check` 通过；`npm.cmd --prefix clients/web run test -- --reporter=dot` 通过，7 文件、36 测试。
- 上述两项是源码控制流可确认的问题；本 reviewer 未执行浏览器故障复现。真实浏览器验证仍由当前执行者进行，其尚未交付的结果未计为本报告通过证据。
- reconnect 单元测试验证 2/4/8/16/30 秒、单 timer、cancel/reset、部分终端错误和 60 秒限流分类，但未执行 main.ts 的 online/pageshow/请求超时协调。测试标题中的页面离开/主动断开实际上仅直接调用 cancel，不能证明页面事件集成通过。
- 正向边界：每次 openManagement 新建连接并保留原 privateKey/node binding；connect 仍执行新挑战和身份证明；未发现自动重发 device.revoke 或自动替换私钥的新增路径。generation 校验能拒绝多数旧连接异步结果，已认证 4410 会禁用恢复并递增 generation。
- 未声称完整证明所有终端错误竞态安全；特别是页面事件、未完成请求与 close 回调的组合应随两项修复补充集成覆盖。当前没有新增已确认的私钥替换或权限绕过问题。

## 本次审阅字节 SHA-256

```text
6138609715C6F11EB191D1BC3021D7CC90F7F7AC440216467B117451A25129D4  clients/web/src/management/reconnect.ts
145CD74585C2E6A097FDD8BAC581ED1F57132BE2F185C4ED77FEC027D63D61BA  clients/web/src/main.ts
8E936661EEB9B212C81E9D50FFD1D7C4C332335F311267ABF2BF6A203F17074E  clients/web/src/management/client.ts
19DC8CFFEB9525A7622E4AC5C805EA677FD27664997145D2BB52C849523B37CB  clients/web/tests/reconnect.test.ts
C1B6E004CF4429F3B5355ED97F147231BA8FEC95DCE836251D034E1F7BC524F6  openspec/changes/establish-node-access/access-contract.md
3DC1E8CDDFC6479EC3598B2FA9754D30C57246534F920E83B7411EB2C6421843  openspec/changes/establish-node-access/specs/client-trust/spec.md
6D0BD3FDED297B10965677F5666F3C8C6521C74964D27508123E11DDD01420C4  openspec/specs/client-trust/spec.md
6E841079F89E7D5C689EDA6A246C722D24DA06C6A4C50794CB325CDEFCCA0A59  openspec/changes/establish-node-access/tests.md
```

初审仅新增本报告，未修改源码、主 reviewer 报告、review.md 或清单。当前修复关闭依据见顶部复查节；结论不自动扩展到后续修改。
