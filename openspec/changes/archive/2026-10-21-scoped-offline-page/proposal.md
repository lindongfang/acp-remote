# Proposal

## Why

当前离线状态在顶栏胶囊、对话页内容来源条、文件栏提示和会话行上各出现一次，同一件事有多套说法；顶栏又只依赖浏览器报告 WebSocket 关闭，网络静默消失时可能长时间显示在线。用户要求把离线状态归一到顶栏，用 1 秒客户端探活让在线/离线及时准确，并用覆盖内容区的连接问题弹窗承载具体错误信息和恢复动作。

## What Changes

- 业务页面只在认证完成后进入；`connecting` / `authenticating` 期间呈现独立认证过渡屏，不显示全局顶栏。
- 顶栏胶囊在认证完成后只表达在线/离线两档：`replaying`、`online`、`incompatible`、`identity_changed`、`replaced` 为在线；`disconnected`、`reconnecting`、`revoked` 为离线。认证前的状态不进入顶栏。
- 新增连接问题弹窗，覆盖内容区、顶栏保持可见；首次认证失败时显示在认证过渡屏中央。弹窗不可手动关闭，包含五个变体：
  - 普通离线：自动重连，无按钮，显示重连次数与下次重试时间；
  - `revoked`：显示「重新配对」，跳转 `/pair`；
  - `incompatible`：显示「重新加载」；
  - `identity_changed`：显示「重新配对」，跳转 `/pair`；
  - `replaced`：显示「重新加载」。
- 弹窗展示人类可读原因、错误码/关闭码与可展开技术详情，不展示密钥、token 或请求体。
- 客户端在认证完成后、页面可见时每 1 秒发送 `control.ping`；只有 nonce 匹配的 `control.pong` 算健康证据；最老未回应 ping 超过 3 秒判离线并进入现有重连。页面隐藏时停止探活且不判离线，恢复可见时立即补发并给约 3 秒宽限。
- 服务端 30 秒心跳与 90 秒静默超时保持不变；本次只增加客户端探活，不改协议 wire。
- 移除对话页内容来源条、会话头与会话行的来源离线标签、文件栏「当前不在线」提示，以及目录页/详情页的内容来源与同步时间展示；设备级离线时输入只禁用、不再显示离线原因文字，保留单个来源离线、附件、提交中、刷新失败等页面可访问时的功能性原因。
- 已配对设备打开 `/pair` 时自动 `replace` 到 `/`。
- 非目标：移动端、Access 多 Owner 切换、标签页共享连接、离线内容缓存、后台常驻、系统通知、服务端心跳改频。

## Capabilities

### New Capabilities

- `connection-health`: 认证后的客户端 1 秒探活、仅 pong 判定、可见性边界与离线转换。
- `connection-problem-overlay`: 认证门、顶栏在线/离线映射、五类连接问题弹窗及恢复动作。

### Modified Capabilities

- `pwa-web-client`: 调整共享外壳的连接状态表达、认证前页面进入规则、同步时间标注范围，以及 imported 来源离线只保留功能性禁用。

## Impact

主要涉及 `clients/app` 的连接层、组合根、feature 状态模型、共享外壳、对话/文件组件与测试；同步更新 `docs/FRONTEND_DESIGN.md`、`docs/SYNC_PROTOCOL.md` §13 与 PWA spec。不改变 Owner 权威、设备认证、命令执行模型、wire schema 或 Rust crate 边界。
