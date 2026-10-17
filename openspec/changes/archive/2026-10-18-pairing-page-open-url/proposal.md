# Proposal

## Why

**用户原始要求**：

> 参照原型，调整一下配对页，相应的后端原型也需要同步调整。
>
> 后端对接超过 5 分钟失败也要支持。

**现状**：两份 PWA 原型（桌面与移动）的配对页已经改成「打开配对地址即开始配对」——居中转圈、圆环内显示剩余有效期、没有输入框、没有页内返回入口、审批通过后自动进入目录、失败页只显示原因（提交 `873b4d2`）。实现仍停在旧形态：`PairingPanel` 有粘贴输入框与「重新扫码」按钮，`app/pair.tsx` 自建「‹ 目录」页头，`approved` 阶段停在原地不跳转，页面也不显示倒计时。`prototypes/IMPLEMENTATION-GAPS.md` §2.4 已逐条登记这些差距。

后端配对链路（`POST /sync/v1/pairing/claim`、`POST /sync/v1/pairing/status`、二维码生成与 CLI 审批）已经具备原型所需的数据与判定：二维码与状态响应都带 `expiresAt`，claim 越过有效期返回 410，status 在有效期内/外分别返回 `pending_confirmation` 与 `expired`，CLI 本地也按 `expiresAt` 退出。因此本次**不改变后端行为**，只把「超过 5 分钟必须失败」补成可核对的回归证据，并让页面按真实截止时间呈现。

## What Changes

- 配对页的入口收敛为**唯一一条**：浏览器打开带 `#data=` 的配对地址。移除页内粘贴输入框、提交按钮与「重新扫码」入口。
- 移除配对页自建的页头与「‹ 目录」返回入口；全局顶栏仍不呈现。页面不提供页内主题切换，但完整复用当前主题的调色板（跟随系统/白天/黑夜）。
- 配对进行中呈现居中转圈与**真实剩余有效期** `mm:ss`：从载荷的 `expiresAt` 计算，每秒刷新；归零立即停止轮询并切到「地址已过期」。二维码在打开时已经过期，同样直接进过期页，且不发任何请求。
- 审批通过后显示短暂过渡，约 1.6 秒后自动 `replace` 到目录页，不再要求用户操作；不等待连接进入 online。`/node-link/pair` 不是 node-link 配对的参与方，保持只呈现对端信息，不跳转、不显示倒计时。
- 失败态改为固定文案 + 固定形态：按「被拒绝 / 已过期 / 已被使用 / 无法识别 / 来源不符 / 非安全上下文 / 请求失败 / 载荷类型不匹配 / 没有载荷」九类呈现标题、说明与警示；失败页不提供任何按钮。
- 没有配对载荷（直接访问 `/pair` 或刷新后片段已清）时，呈现「这个地址里没有配对信息」并指引用户用电脑重新生成的地址打开。
- 「审批通过但连接装配失败」仍自动进入目录，由目录页的设备胶囊表达断开与自动重连；配对页不新增状态。
- 载荷类型与入口不匹配（sync 载荷进 `/node-link/pair` 或反之）落到专用失败页文案，仍不发任何请求。
- 前端模型去掉 `awaiting_qr` / `canRestart` / 「进行中禁用输入」等输入框时代的字段与分支；新增固定呈现模型、`expiresAtMs`、到期定时器与「无载荷」状态。
- 后端零行为变更；补一条验收证据：越过 `expiresAt` 后 claim 与 status 都失败（复用并点名既有测试），以及前端在缩小的窗口下能按倒计时进入过期页。
- 同步修改 `pwa-web-client` 规格、`FRONTEND_DESIGN.md`、`prototypes/IMPLEMENTATION-GAPS.md` 与 README 的现状描述。

## Capabilities

### New Capabilities

（无）

### Modified Capabilities

- `pwa-web-client`：配对入口形态（地址唯一入口、无页内输入/页头/返回）、真实有效期倒计时、审批通过后自动进目录、失败态的固定呈现与无按钮恢复路径、无载荷与载荷类型不匹配两种专用失败、配对页主题呈现。

## Impact

- **前端代码**：`clients/app/src/features/pairing-model.ts`、`clients/app/src/features/client-store.ts`、`clients/app/src/composition.ts`、`clients/app/src/components/PairingPanel.tsx`、`clients/app/app/pair.tsx`、`clients/app/app/node-link/pair.tsx`。
- **前端测试**：`pairing-model.test.ts`、`composition.test.ts`、`e2e/page/checks/{pairing-first-run,pairing-kinds,ui-shell}.ts` 与 e2e 支撑文件。
- **后端**：无生产代码与 wire/schema 变更。仅把「超过有效期必须失败」纳入主检出验证，复用 `crates/server/src/sync/pairing.rs` 的既有 claim/status 用例。
- **文档**：`docs/FRONTEND_DESIGN.md`、`prototypes/IMPLEMENTATION-GAPS.md`、`README.md`（如有与配对页现状冲突的句子）。
- **用户可见**：配对页只认地址；不再能手工粘贴；刷新或丢失片段后必须由电脑重新生成地址；通过后自动进目录；失败页只给原因。

## 非目标

- 不加入摄像头扫码或二维码解码依赖：本次只调整现有「地址片段」入口的呈现与落点。
- 不改变 Sync 配对协议、`claim`/`status` 的 wire 形状、错误码词表或后端状态机。
- 不改变 `/node-link/pair` 的参与边界：浏览器仍不代替节点 claim/status。
- 不改动主机与连接面板的信息/动作集合（重新配对仍由电脑重新发起地址）。
- 不实现移动原型独有的底部导航形态；本次只在现有响应式应用里对齐配对页。
- 不引入页内主题切换控件；主题三态仍由其它页面的共享外壳切换。

## 硬约束

- 配对秘密仍只从地址片段进入，读取后立即清除；秘密 MUST NOT 进入日志、本地明文存储、地址历史、公开状态或失败文案。
- 页面 MUST NOT 自行发请求或读地址栏：片段读取、claim、status 都在组合根与 `ClientStore`。
- 失败文案 MUST 是固定文案，MUST NOT 直接渲染后端 `message` 或任何载荷内容。
- 越过 `expiresAt` 后，客户端 MUST 停止轮询；服务端仍是过期判定的权威。
- `npm run verify`、`clients/app` 的 `npm run check`、`clients/app` 的浏览器 e2e 必须全绿。

## 成功判据

1. 打开带 `#data=` 的 sync 配对地址：页面立即进入配对中，显示真实剩余有效期；无输入框、无页内返回入口。
2. 剩余有效期归零（或打开时已过期）：页面进入「地址已过期」，不再发送状态查询。
3. 审批通过：页面显示「已通过审批，正在进入目录」，约 1.6 秒后落在目录页；连接状态由目录页设备胶囊表达。
4. 直接访问 `/pair` 或 `/node-link/pair`（无载荷）：呈现「这个地址里没有配对信息」，不发任何请求。
5. 九类失败分支各自呈现固定标题与说明；任何失败页都没有进入目录/重新扫码/页内返回按钮。
6. 后端既有 claim/status 过期用例通过，证明越过 `expiresAt` 后 claim 与 status 都失败。
7. `npm run verify`、`clients/app` 的 `npm run check` 与浏览器 e2e 全绿。
