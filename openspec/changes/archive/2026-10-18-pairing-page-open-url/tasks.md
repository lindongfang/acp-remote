# Tasks

## 1. WP1 配对页前端实现

- [x] 1.1 在 `pairing-model.ts` 建立新的 `PairingPhase`（含 `reading`、`idle`，删除 `awaiting_qr`）与 `PairingPresentation`（progress/failure/info）；删除 `GUIDANCE`、`canRestart`、`awaitingDesktopConfirmation`、`isPairingInFlight`。完成判据：类型检查通过；`pairing-model.test.ts` 不再引用被删字段。
- [x] 1.2 实现固定呈现映射：无载荷、无法识别、类型不匹配、来源不符、非安全上下文、请求失败、被拒绝、已过期、已被使用九类各自有固定标题/说明/警示；`data-pairing-error` 仍带错误码。完成判据：单测逐类断言标题与「不含载荷秘密」。
- [x] 1.3 `applyPairingFailure` 按错误码把 `pairing.expired` 落到 `expired`、`pairing.consumed`/`pairing.already_claimed` 落到 `consumed`，其余按显式 reason 或错误码分类；`kind_mismatch` 由调用点显式传入。完成判据：单测覆盖 claim 的过期/已用/验签失败与类型不匹配，且不发后续轮询。
- [x] 1.4 在 `composition.ts` 的 sync 与 node-link QR 解析里把「已过期」改为抛 `PairingQrInvalidError("pairing.expired", ...)`；`publicErrorForQrInput` 保留该码。完成判据：`composition.test.ts` 对过期载荷断言 `pairing.expired` 且不发出 claim。
- [x] 1.5 `ClientStore.startPairingFromAddress` 成为唯一入口：无片段时提交 `applyNoPayload(kind)`（`idle` + 固定无载荷呈现），有片段时进入 claim；删除 `submitPairingQrText`。完成判据：单测覆盖「无载荷不发请求」「有片段自动 claim」，并断言公开状态不含 `pairingSecret`。
- [x] 1.6 在 `ClientStore` 中按 `expiresAt` 安排一次到期定时器（使用注入的 `wiring.schedule`，`remaining = max(0, expiresAt - now)`）；到期时取消轮询、丢弃会话、落到 `expired`；`#endPairing` 同时取消到期定时器。完成判据：单测让定时器触发后断言阶段 `expired`、后续不再安排 poll、无已配对记录。
- [x] 1.7 重写 `PairingPanel.tsx`：只接收 `model`；按 `presentation.kind` 渲染 progress/failure/info，去掉输入框、提交按钮、重新扫码、页头与第二个状态徽标；保留 `data-route`、`data-pairing-phase`、`data-pairing-host`、`data-host-address`、`data-pairing-device-id`、`data-pairing-error` 锚点并新增 `data-pairing-countdown`。完成判据：typecheck 通过；组件里不再引用 `onSubmit`/`onRestart`/`isPairingInFlight`。
- [x] 1.8 实现居中转圈与圆环内倒计时：`Animated`/RNW 兼容的旋转环、`m:ss` 文本、每秒按 `expiresAtMs` 重算；progress 且 `countdown` 为真时显示，归零后由 store 迁移，页面不自行改状态。完成判据：typecheck 通过；e2e 能读到 `data-pairing-countdown` 的递减文本。
- [x] 1.9 失败页按固定呈现渲染标题、说明、警示与 glyph，不渲染后端 `message`；无载荷、类型不匹配、来源不符、非安全上下文、请求失败、被拒绝、已过期、已被使用各自可区分。完成判据：`composition.test.ts` 渲染真实 `PairingPanel` 的断言改为读固定文案，且不出现旧「重新扫码」按钮。
- [x] 1.10 重写 `app/pair.tsx` 的页面主体：删除页头与 `back-to-dirs`；只渲染 `PairingPanel`；保留 `startPairingFromAddress` 的挂载读取；新增 approved 后 1.6 秒 `router.replace("/")` 的 effect 与清理函数。完成判据：typecheck 通过；e2e 观察到自动落点 `/`（目录页）。
- [x] 1.11 更新 `app/node-link/pair.tsx` 的文件头与行为说明：复用同一主体、不跳转、不显示倒计时；无载荷时走节点专用文案。完成判据：代码与注释一致，e2e NL-1/NL-2 通过。
- [x] 1.12 确认颜色全部来自 `useTheme()` 的当前调色板，深浅两套样式各建一次；不新增页内主题按钮。完成判据：代码审查确认无硬编码颜色、无主题切换控件。
- [x] 1.13 更新 `pairing-model.test.ts` 与 `composition.test.ts`：用 `wiring.fragment`/`stubAddressBar` + `startPairingFromAddress` 驱动，不再调用 `submitPairingQrText`；保留并加强伪造 hostProof/公钥的负向用例。完成判据：`cd clients/app && npm run typecheck && npm run test` 通过。
- [x] 1.14 删除输入框时代遗留的注释与文档串（`canRestart`、输入框禁用、重新扫码）并同步文件头。完成判据：`grep` 不到 `submitPairingQrText`、`canRestart`、`isPairingInFlight`、`awaiting_qr` 的引用。

## 2. WP2 浏览器 e2e

- [x] 2.1 改写 PW-1：用 `globalThis.location.hash = "#data=…"` 打开真实配对地址，断言自动 claim、pending、主机地址、倒计时存在、approved、约 1.6 秒后落在 `/`（目录页）、连接在线；删除输入/提交断言。完成判据：`node e2e/run-e2e.mjs` 中 PW-1 通过。
- [x] 2.2 改写 PW-2 为无载荷用例：直接打开 `/pair`，断言 `idle`、无载荷标题、无输入框、无请求。完成判据：PW-2 通过且 `recordingFetch.urls.length === 0`。
- [x] 2.3 更新 PW-3/PW-4 为片段驱动，保留「验签失败/公钥逐字节不一致 → 明确失败、停止轮询、不落盘、不建连接」的判别力。完成判据：两条通过。
- [x] 2.4 更新 NL-1/NL-2/NL-3：NL-1 删除输入框断言、保留对端信息与零请求；NL-2/NL-3 用片段喂错内容/环境事实，断言专用失败文案与零请求。完成判据：三条通过。
- [x] 2.5 新增倒计时归零用例：`expiresAt` 设为约 3.5 秒后，假主机始终回 `pending_confirmation`，断言页面进入「地址已过期」且之后不再有 status 请求。完成判据：用例通过，且等待窗口覆盖至少一个轮询周期。
- [x] 2.6 改写 UI-SHELL-3：断言配对页渲染进共享外壳、无全局顶栏、无 `[data-route-header="pair"]`、无 `back-to-dirs`；差分切到根路径后顶栏出现且页头锚点始终不存在。完成判据：UI-SHELL 三条通过。
- [x] 2.7 更新 `support.tsx`/`fixtures.tsx` 的注释与必要的场景参数（如片段注入），删除对 `submitPairingQrText` 与输入框锚点的描述。完成判据：`npm run typecheck:e2e` 与 e2e 全绿。

## 3. WP3 文档与缺口清单

- [x] 3.1 `docs/FRONTEND_DESIGN.md`：改 §4.1 与 §9 的配对页约束（地址唯一入口、无页内输入/页头/返回、真实倒计时、自动进目录、失败固定文案、主题跟随），新增本次修订记录。完成判据：文档中不再有「配对页提供粘贴入口」「配对页呈现自己的头部」的现行口径。
- [x] 3.2 `prototypes/IMPLEMENTATION-GAPS.md` §2.4 与 §5：把「已批准后自动进入目录」「页面入口形态」「5 分钟倒计时」三条从缺口改为已对齐，并注明无载荷/类型不匹配的专门呈现。完成判据：§2.4 不再有「剩一处行为缺口」的旧结论。
- [x] 3.3 `README.md` 与 `prototypes/README.md` 中与配对页现状冲突的句子改为地址唯一入口与自动进目录。完成判据：`grep` 不到「配对页有二维…粘贴」「粘贴入口」的现行实现描述。
- [x] 3.4 核对 `openspec/changes/pairing-page-open-url/` 的 proposal/spec/design/plan/tasks 与最终实现一致，必要时同步 `plan.md` 的工作包结果。完成判据：`npx openspec validate pairing-page-open-url --strict` 通过。

## 4. 合入与验证

- [x] 4.1 在目录身份迁移（PR #154）之上合入 WP1/WP2/WP3；完成判据：前端 typecheck、`typecheck:e2e`、vitest 与 `expo export` 全绿。**结果**：`cd clients/app && npm run check` 通过。
- [x] 4.2 跑 PV1：`npm run verify` 全绿（合同门禁 + Rust fmt/clippy/test）。
- [x] 4.3 跑 PV3：`node e2e/run-e2e.mjs` = **75/75**，PW-1…PW-5、NL-1…NL-3、UI-SHELL-3 全部通过。
- [x] 4.4 跑 PV4 的两条后端过期用例：`expired_pairing_is_rejected_with_410_and_writes_nothing` = 1 passed；`status_reports_the_five_business_states_with_200` = 2 passed。
- [ ] 4.5 推送分支与创建 PR；PR 检查全绿。完成判据：六个必需检查通过。

## 5. 基线依赖

本变更的前端基线由已合并的 PR #154（`restore-directory-identity-baseline`）提供：目录身份已是 `directoryId` + `path`，`clients/app` 的 6 条目录镜像失败与 Rust 格式/clippy 漂移已经解决。本变更只在其上替换配对页形态。
