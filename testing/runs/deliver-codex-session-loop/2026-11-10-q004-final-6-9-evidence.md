# Q-004 6.9 开发闭环证据

## 结论

任务 6.9 的当前开发门槛已满足：实际 Windows 子进程、节点 WSS 队列入口、L4
队列 owner/SQLite、实际 Edge DOM 和生产 Web 构建使用同一工作树版本；四种非成功
停止原因可显示，X/Z 保持暂停，普通继续不抢占活动新输入，立即发送仅推进原已选
Y，选择失效或中断未确认时零回退，继续重试复用同一登记和消费身份。

该结论只覆盖 6.9 的开发、组件和受控协议对端检查。浏览器快照是合成的，原生对端
是明确标记的 ACP 开发夹具；它不覆盖真实 Harness，也不代替 6.10 对 CL-117–123
的独立逐 ID 验收。

## 基线

- 分支：`codex/session-foundations-replay-planning`
- HEAD：`65ec7815bb0280f4519989d93a7b4ed3b4b37994`
- 规格：`openspec/changes/deliver-codex-session-loop/specs/session-execution/spec.md`
  SHA-256 `45CB72683BFDCCD91444190DA27D076028969043B0A74FEA499CC9C927BC4287`
- 用例：`openspec/changes/deliver-codex-session-loop/tests.md`
  SHA-256 `1B9086F633785D5A0DAC1BB39AC9A8C3E17397F5A0228F63E79D6B2BB9CE1F53`
- 工作树包含本 change 的既有未提交实现和证据；本记录没有提交、推送或归档。

## 实现与门控

- 队列快照展示和中文停止原因提取到生产
  `clients/web/src/management/session-queue-view.ts`，主页面与浏览器夹具共用同一实现。
- 显式继续身份提取到生产
  `clients/web/src/management/session-resume.ts`；登记和消费失败后保留同一
  `registrationId` 与 `consumeRequestId`，节点确认成功后清除待用身份。
- WSS 编辑/取消/优先发送/继续用例删除异步派发后的瞬时 `priority` 断言，改为检查
  规格要求的持久事实：等待项为 X、Z，X revision 为 2，队列不再暂停，且没有停止
  或选择失败。删除的不是产品能力或持久一致性检查。
- 浏览器夹具直接加载上述生产模块，并在实际 Edge 中点击继续、注入一次传输失败、
  再点击重试；它不是文案副本。

## 结果矩阵

| 判据 | 实际结果 | 证据 |
| --- | --- | --- |
| v1/v2 的 `max_tokens`、`max_turn_requests`、`refusal`、`cancelled` 保留停止原因并暂停三项 | 真实 Windows 子进程 1/1 通过，内部覆盖 8 个版本/原因场景；新输入成功不恢复旧项，显式继续恢复原序 | `2026-11-10-q004-final-6-9-native-resume.stdout.txt`，SHA-256 `8729B089FC31D74195CD51F386BB8A444700AF35FA4BD6DC325A96147A8A5925` |
| 四种原因在 Web 显示具体中文原因且 X/Z 显示暂停 | Edge DOM 4/4 通过 | `2026-11-10-q004-final-6-9-browser-dom-v2/report.json`，SHA-256 `4B794F877029BA4A689785C27E36A015220BA1E17E67BAF1DED778EC095F8979` |
| `selection_changed`、`interrupt_unconfirmed` 明确提示且零回退 | Edge DOM 2/2 通过 | 同上；截图 SHA-256 `BD73D4EE457DC6E132D854EC1113EE9F68462EB35AC85E2A45811CFA55EDA75A` |
| 编辑、取消、优先发送和继续经节点 WSS 生效；X/Z 顺序和 revision 持久正确 | 定向 4/4 通过 | `2026-11-10-q004-final-6-9-node-queue.stdout.txt`，SHA-256 `9F211690B0A8512E838F875B2E581453D2047410912467A6A97AB09467BA5897` |
| 普通继续不抢占活动新输入，三旧项恢复后仍按原序，失败/失效不回退 | L4 共享输入全套 39/39 通过，其中 `cl112` 4 项、`cl115` 2 项直接覆盖 | `2026-11-10-q004-final-6-9-shared-input-acceptance.stdout.txt`，SHA-256 `67C260A01931E1929D0B148D684A76415DDD812CFEF797DDAC52CCEC6075D357` |
| 已选 Y 被编辑、取消或资源换代后暂停且零回退；中断失败同样零回退 | 队列 owner 4/4 通过 | `2026-11-10-q004-final-6-9-priority-owner.stdout.txt`，SHA-256 `5905D03B75843AD25BCCA44929C31532E0BBA4C9CB59BEC25193DD5DEB9326BC` |
| 立即发送优先项完成一次派发；后继成功、失败或取消后 X/Z 仍暂停 | v1/v2 × 三种结果共 6 个真实 Windows 子进程场景，1/1 测试通过 | `2026-11-10-q004-final-6-9-native-priority-v2.stdout.txt`，SHA-256 `FC2CF8C1F1B5AE537417F3DE4B30FA119FE3149C47559E0F4EDB21555F443CC0` |
| 继续第一次传输失败后重试复用同一操作身份，成功只生成一次身份 | Edge 点击 1/1 和单元合同 2/2 通过；Web 定向合计 124/124 | `2026-11-10-q004-final-6-9-browser-dom-v2/report.json`、`2026-11-10-q004-final-6-9-web-protocol-management-v2.stdout.txt`，后者 SHA-256 `9C1EF369AB57870315E036E2F6E2D682F3D966189D6BF2318DC8EA6EC0166865` |
| Web 完整回归、类型检查和生产构建 | 29 文件 667/667；check 与 build exit 0 | `2026-11-10-q004-final-6-9-web-test-v2.stdout.txt` SHA-256 `C59B418327029EAF71F5E8F1035B816FE0CBBC4DF96CF3DACD3B822DBFDEAC27`；check `341F44BBB423176E0CCA38CC70A3A13041F1683434727A55EFF28C5B5569A07F`；build `D9934FBDE39499CFAAF617280265F3C4B983B335E41F2998136AB1FBB3DAB946` |
| 完整测试资源清理 | 原生测试以原 Job/Storage shutdown 收尾；四类目标进程检查为空 | `2026-11-10-q004-final-6-9-process-tree-after-v2.json`，SHA-256 `59BBDF644F69B703652C535F2BF4BA023B7AE9AC4A593D288C6F2D63FBC1D89B` |
| Rust 格式、diff 空白、OpenSpec strict | 均 exit 0；strict 1/1 passed | `2026-11-10-q004-final-6-9-cargo-fmt.stdout.txt`、`2026-11-10-q004-final-6-9-git-diff-check.stdout.txt`、`2026-11-10-q004-final-6-9-openspec-strict.json` |

浏览器报告记录无外部请求、无页面错误，Context、Browser、本地 server 和临时目录均已
关闭。原生测试的输出包含既有编译 warning，但没有 test failure；残余进程文件在
原生停止/恢复和原生优先级测试之后生成。

## 最终实现摘要

- `clients/web/src/main.ts`：
  `07DEC2B0B5534F1195ADB2C0D714036BEF1391B3FD5E8790FDD1DEB2EBCCEB0A`
- `clients/web/src/management/session-protocol.ts`：
  `35F4245DFF783F7060784860BFFF2F49E32679472F4699B8B647B01400D48927`
- `clients/web/src/management/session-queue-view.ts`：
  `06977F7E8B3D4B434BD8793E25C2DBE5C9A4C64E65D2E1146FF6F914548058F4`
- `clients/web/src/management/session-resume.ts`：
  `C139B3EAD04AE92EFA3A99234612FA32DBC559948D3E6BCF171EE6F001B1C61C`
- `app/src/node/session_services/queue_read_development_tests.rs`：
  `FB445EF247900B8E41AEB24156DFFBFD111C1E11485FBCCE95E757277692E7C5`
- `app/src/application/native_dispatch/dev_tests.rs`：
  `B8B1EB9365DA9DB43A98B9807B3A2B5D239C49926B4390630AB0A078F8D17E81`

## 限制与后续

- 浏览器使用生产展示/继续模块和实际 Edge DOM，但队列快照由本地夹具提供；没有用
  该结果代签真实认证节点、真实 Harness 或 CL-123。
- 原生子进程是可重现的受控 ACP 对端，不是 Codex、DeepSeek 或其他真实 Harness。
- 6.10 仍需独立 tester/reviewer 对 CL-117–123 逐 ID 记录通过、失败、受阻或未执行，
  并由真实 Windows、可信 Web、SQLite、Harness/适配器及专用原生来源复核同一版本。
