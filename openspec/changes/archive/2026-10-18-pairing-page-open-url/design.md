# Design

## Context

配对页当前由 `app/pair.tsx` 的 `PairingRouteBody` + `src/components/PairingPanel.tsx` + `src/features/pairing-model.ts` + `ClientStore` 的配对用例组成。二维码内容从地址片段进入，由组合根的 `takeQrTextFromFragment` 读取并立即 `history.replaceState` 清除；载荷只活在 `ClientStore` 的私有字段里，claim/status 由组合根的 `HttpPairingTransport` 发出。`/node-link/pair` 复用同一份页面主体，只把期望类型声明为 `node-link`。

两份原型把配对页改成「打开地址即配对」：居中转圈、圆环内显示 5 分钟倒计时、没有输入框、没有页内返回、通过后 1.6 秒进目录、失败页只显示原因。实现仍保留粘贴输入与自建页头，`approved` 停在原地，失败页渲染后端 `message` 并提供「重新扫码」。

后端已经具备本次需要的事实：二维码载荷与状态响应都带 `expiresAt`；claim 越过有效期返回 410 `pairing.expired`；status 按当前时刻即时判定过期并返回 200 + `expired`；CLI 本地也按 `expiresAt` 退出。因此本次不改后端行为与 wire。

## Goals / Non-Goals

**Goals:**

- 让 sync 与 node-link 两个配对入口都符合原型的「地址唯一入口 + 单目的状态页」形态。
- 用载荷里的真实 `expiresAt` 呈现倒计时，并在归零时立即停止轮询、进入过期页。
- 审批通过后自动进目录；失败页使用固定文案、无按钮。
- 让「无载荷」「载荷类型不匹配」「已批准但连接装配失败」三个原型未画到的分支有确定、可测试的呈现。
- 配对页完整复用当前主题调色板，但不提供页内主题切换。
- 用既有后端用例证明「超过 5 分钟必须失败」，不改后端生产代码。

**Non-Goals:**

- 不加入摄像头扫码/二维码解码依赖；不改变二维码地址形态。
- 不改变 Sync/Node Link 协议、错误码词表、后端状态机或存储。
- 不实现移动原型独有的底部导航、push 页或 56px 会话行。
- 不改变主机与连接面板的信息/动作集合；不新增「重新配对」按钮。
- 不把失败页做成可重试表单；页面内没有回到可输入状态的路径。

## Decisions

### D1：地址片段是唯一入口，移除页内输入与页内返回

页面只调用 `store.startPairingFromAddress(kind)`，由组合根读取并清除片段，再进入 claim/轮询。删除 `submitPairingQrText`、输入框、提交按钮与「重新扫码」按钮；删除 `app/pair.tsx` 自建的页头与 `back-to-dirs`。

理由：原型明确「打开地址即配对」；保留输入框会让页面同时承担入口与状态两种角色，并让「失败后只能由电脑重新发起」的恢复路径变成两套。直接访问 `/pair`（无片段）不是错误输入，而是一个明确的「没有配对信息」状态。

代价：片段被清除后刷新页面无法恢复这次配对，必须由电脑重新生成地址。这正是秘密「读完即清」的必然结果；本次把它显式呈现在无载荷页上，而不是用输入框兜底。

替代方案：保留输入框但默认隐藏（只在无载荷时显示）。否决原因：它仍让页面拥有第二种入口，且与「失败页无按钮」的形态冲突；产品已确认 URL 唯一入口。

### D2：`PairingPageModel` 改为「状态 + 固定呈现」

模型保留 `phase`（`reading | idle | claiming | pending_confirmation | approved | rejected | expired | consumed | node_link_scanned | failed`）与公开的主机/设备信息，新增：

- `kind: PairingKind`：决定无载荷与 node-link 文案。
- `expiresAtMs: number | null`：载荷的绝对截止时间；node-link 与无载荷为 `null`。
- `failure: PairingFailureKind | null`：`no_payload | unrecognized | kind_mismatch | origin_mismatch | insecure_context | request_failed`。
- `presentation`：`progress | failure | info` 三支的固定标题/说明/提示/警示/色调/是否显示倒计时。

`PairingPanel` 只渲染 `presentation` 与公开信息，不再读 `error.message`、不再分支 `phase` 决定文案。错误码仍保留在 `error.code` 与 `data-pairing-error` 上，供测试与诊断。

`modelFor(phase, overrides)` 是唯一的组装点；`GUIDANCE`、`canRestart`、`awaitingDesktopConfirmation`、`isPairingInFlight` 随输入框一起删除。`reading` 是初始阶段：页面挂载后由 `startPairingFromAddress` 把它推进到 `claiming`（有片段）或 `idle`（无片段），避免首帧把「还没读地址」误呈现成「没有配对信息」。

理由：固定文案是产品要求，且避免后端 `message` 直接铺到页面；把呈现放进模型让 e2e 与单测都能断言最终文案，不依赖组件内部条件。

### D3：过期由 `ClientStore` 定时，页面只负责显示

- 读到 sync 载荷后，`ClientStore` 用注入的 `wiring.schedule(remaining, task)` 安排一次到期回调；`remaining = max(0, Date.parse(qr.expiresAt) - this.#now())`。
- 回调触发时：取消轮询、丢弃含秘密的会话、把阶段推进到 `expired`（固定 S3 呈现）。
- `#endPairing()` 同时取消轮询与到期定时器；已批准、终态、claim/poll 失败都走它。
- 页面每秒重算 `max(0, expiresAtMs - now)` 并显示 `m:ss`；不负责状态迁移。
- 组合根的 QR 解析在 `expiresAt <= now` 时抛 `PairingQrInvalidError("pairing.expired", …)`；`applyPairingFailure` 依据该码落到 `expired`，因此打开即过期不会发 claim。

理由：状态迁移与秘密生命周期属于 store，显示属于页面。用绝对截止时间而不是页面打开时刻，避免「二维码已生成两分钟」时显示错误的 5:00。服务端仍是权威：即使客户端时钟偏差导致本地提前/延后，下一次 status 响应也会给出真实状态。

替代方案：固定 300 秒倒计时。否决：与载荷的 `expiresAt` 不一致，且电脑可以用 `expiresInMs` 收窄窗口。

### D4：审批通过后自动 `replace` 到目录页

页面在 `phase === "approved" && kind === "sync"` 时启动一个 1.6 秒定时器，到期调用 `router.replace("/")`；清理函数在阶段变化或卸载时取消。node-link 入口不进入 `approved`，因此不跳转。

不等连接 online：目录页已有「尚未收到第一份快照」的加载态，连接状态由共享外壳的设备胶囊表达。若 `onPaired` 失败，store 仍保持 `approved` 并记录收口错误；页面照常跳转，目录页显示断开/重连中。这样不会出现「已配对记录已落盘，但页面说配对未完成」的矛盾。

替代方案：等 online 再跳。否决：慢网下会把用户卡在配对页；原型也明确是短暂过渡后进目录。

### D5：配对页不绘制任何页内外壳，但复用当前主题

`PairingRouteBody` 只渲染居中舞台；不渲染页头、返回按钮、主题按钮。`PairingPanel` 的所有颜色来自 `useTheme()` 的当前调色板，深浅两套样式在模块加载时各建一次（与现有组件一致）。共享外壳继续在配对路径隐藏全局顶栏。

理由：桌面原型没有顶栏；移动原型的标题栏承担的是「这是哪一页」与主题切换，而不是返回。实现是一套响应式页面，保留自建栏会造出第二套外壳形态。代价是配对页内不能切主题；主题可在其它页面切换，且配对页会立即跟随当前取值。这是对移动原型的一处有意偏离，登记在 `proposal.md`。

### D6：失败类别 → 固定呈现的映射

| 来源 | `phase` / `failure` | 标题 |
|---|---|---|
| 无片段 | `idle` / `no_payload` | 这个地址里没有配对信息 |
| 结构非法 / 协议不符 | `failed` / `unrecognized` | 无法识别这个地址 |
| 载荷类型与入口不符 | `failed` / `kind_mismatch` | 这不是本入口的配对地址 |
| 来源不一致（`auth.origin_mismatch` / `auth.host_mismatch`） | `failed` / `origin_mismatch` | 这个地址属于另一台电脑 |
| 非安全上下文（`auth.insecure_context`） | `failed` / `insecure_context` | 需要 HTTPS 才能配对 |
| 网络 / 本地存储 / 未知 | `failed` / `request_failed` | 配对请求未能完成 |
| 主机拒绝 | `rejected` | 审批未通过 |
| `pairing.expired` 或倒计时归零 | `expired` | 配对地址已过期 |
| `pairing.consumed` / `pairing.already_claimed` | `consumed` | 这个配对地址已经被使用 |
| node-link 内容已读 | `node_link_scanned` | 已读到 node-link 配对内容（info） |

`applyPairingFailure(current, error, reason?)` 先按错误码决定 `expired`/`consumed`/`failed`，再用显式 `reason` 或错误码分类出 `failure`。claim 与 poll 共用同一分类；`kind_mismatch` 与「输入不可解析」由调用点显式传 reason，避免把 `protocol.schema_invalid` 混成同一类。

失败页不显示主机卡片与设备标识，不提供任何按钮；`data-pairing-phase` 与 `data-pairing-error` 保留。

### D7：后端零行为变更，只补过期验收证据

不改 `crates/server/src/sync/pairing.rs` 的生产代码、schema、错误码或 CLI。既有用例已经覆盖：

- `expired_pairing_is_rejected_with_410_and_writes_nothing`：claim 越过 `expiresAt` → 410，零写入。
- `status_reports_the_five_business_states_with_200`：claimed 后越过窗口 → status 200 + `expired`，且不带 device/host。
- CLI 的 `await_claim` 在本地按 `expiresAt` 退出。

主检出验证时点名跑这两条用例，并在变更证据里写明「后端行为未变、过期判定已由既有用例证明」。前端另加一条缩小窗口的浏览器 e2e：`expiresAt` 设为约 2 秒后，页面在归零时进入过期页并停止轮询。

### D8：类型不匹配与无载荷是两种不同的失败

- `kind_mismatch`：地址里的载荷类型与路由声明不一致。它仍必须读载荷、校验环境事实，然后在发任何请求之前失败；文案说明「这不是本入口的配对地址」。
- `no_payload`：地址里根本没有可读内容（或片段已被清除）。它不是载荷错误，不发请求，也不尝试任何解析。

两者都用同一套失败布局，但标题/说明不同，且 `data-pairing-error` 只在 `kind_mismatch`/结构非法等有错误码时出现。

## Risks / Trade-offs

- **刷新即失去配对会话**：片段读完即清，刷新后只能重新生成地址。缓解：无载荷页明确说明原因与下一步；这是安全设计的要求，不用输入框兜底。
- **客户端时钟偏差**：倒计时基于本机时钟。服务端仍是权威；偏差只会让页面提前/延后停止轮询，下一次 status 会纠正。若客户端时钟严重超前，可能在服务端仍有效时提前显示过期；这比显示一个必然错误的固定 5:00 更可诊断。
- **自动跳转与连接状态脱节**：通过后不等 online，用户可能先看到目录页的加载/断开态。缓解：设备胶囊是连接状态的唯一表达；目录页已有加载态，且重连会自动进行。
- **失败页无按钮**：用户不能在页面内重试。缓解：所有失败文案都指向「电脑重新生成地址」；这是原型与产品确认的恢复路径。
- **主题切换缺席**：配对页内无法切主题。缓解：配对页通常只出现几秒；主题在其它页面切换后立即生效。这是对移动原型的有意偏离。
- **`reading` 首帧**：页面挂载到片段读取之间有一个极短过渡。缓解：用与「配对中」一致的进度呈现，不把它渲染成无载荷失败。

## Migration Plan

无数据迁移、无部署形态变化、无 wire 变更。回滚只需回退前端代码与文档；后端无需回滚。旧客户端若仍保留粘贴入口，与本变更的协议行为完全兼容（仍走同一 claim/status），因此不需要兼容分支。
