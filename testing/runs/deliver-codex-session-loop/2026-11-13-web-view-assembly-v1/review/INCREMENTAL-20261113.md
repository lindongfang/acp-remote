# Web 内容接线增量交付复查

日期：2026-11-13。只读复查当前 Web 三个产品文件及相关用例；未运行测试、check、build、浏览器、PE 或网络，没有修改产品、测试预期、Git index 或先前报告。该结论仅针对本批三个问题，不是完整 Web/WP4 验收。

## 输入身份

| 文件 | SHA-256 |
|---|---|
| `clients/web/src/main.ts` | `7aac61c99b2a91a67950a5afacbeeb423d427a4601035606f804ed76c554f92d` |
| `clients/web/src/management/session-view-display.ts` | `dd9415fdc783ba209f1cf7aba1869da7a73e51ff9f5f8f718cff9922536b740d` |
| `clients/web/src/management/session-view-status.ts` | `cc424edb61cdb9a38180d4c5799e92eba0ab784915898b09e4bdb32e75008553` |

## 结论

**三项代码检查通过，独立真实 DOM/browser 验收仍未完成。** 当前代码已处理先前提出的 `stream_rebound` credit/detach、打开失败清理、队列正文迟到 descriptor 竞态。未发现需要在这三个点继续阻止后续验证的问题。

- `stream_rebound` 被当作 Module-only 状态。`readSessionViewStatus` 对存在的已知身份字段校验 ID、对存在的 `readEpoch` 接受 null 或有效 ID，不要求可选字段必填；`SessionViewDisplay.accept` 仅在完整且可解析的 status packet 后先归还该 packet 的 credit，再 suspend/seal 并 detach 原 stream。没有从通知创建新的连接绑定。解析失败进入 catch，seal/detach，但不 credit 失败 packet。该顺序符合“credit 表示已消费 packet 的流控”且不宣称 WebSocket 自动重绑定。
- `openSharedSessionView` 在新 stream 的 display 构造、挂载或 initial credit 任一环节失败时，release 新 stream 并 dispose candidate。只有 candidate 已登记为当前 owner 时才回滚全局 owner；原 display 仅在 candidate 成功启动后才 suspend/release。由此避免构造/启动失败泄漏新远端 stream 或提前丢失原 owner。
- `openQueueBody` 的 descriptor 仅在 management generation、原 connection、当前 inspected target 和 body attempt 四者均匹配时登记及发 initial credit。换 target 会经 `resetSessionInputUi` 增加 attempt；乱序同 target open 也仅接受最新 attempt。失效 descriptor 只通过发起它的原 connection 尝试 detach，不会污染或 credit 新读取。

## 未验证范围

相关 `session-view-main.test.ts` 通过 strip-type 从 `main.ts` 抽取生产函数，能覆盖同代 open 乱序、target 更换、重连、构造/挂载/initial-credit 失败及旧 close 迟到等分支；它不是真实主页面 DOM。当前未读取独立 browser 执行结果，因此仍需 WV-15 的实际 main DOM/browser 验证：活跃输入焦点与 selection、raw inspector 展开状态、已选原文状态，以及 timeline host 在 dashboard 刷新/重连时的稳定性。不要把开发报告中的组件或 TypeScript 通过记为该项通过。

本报告也不关闭 WSS/Harness、完整 WP4、CL-030/031/123 验收。
