# Tasks

## 1. 解码边界修正（WP1）

- [x] 1.1 在 `clients/app/src/sync-client/wire.ts` 把 `EVENT_ORIGIN_KINDS` 补齐为五值（增 `node`），并把该表的类型从 `Record<string, true>` 改为 `Record<EventOriginKind, true>`，使运行期词表由 `clients/app/src/protocol/event.ts` 的类型钉住；判据：`cd clients/app && npm run typecheck` 通过，且报告里给出判别力核对——临时删掉表中的 `node` 一项会编译失败（该临时改动不留痕）
- [x] 1.2 在解码边界加 `origin` 的 node 配对规则：来源类型为 `node` 时要求身份字段 `nodeId` 存在且为 UUID、`deviceId` 为空，并按 `missing_field`（键缺席）与 `field_type`（值为空）两种理由分别拒收；其余四种来源类型**不新增任何**身份字段必填性；判据：`cd clients/app && npm run test` 中既有 `wire.test.ts` 全绿，且报告逐条给出四种非 node 形状（含 `{kind: "device", deviceId: null}` 与不带 `nodeId` 键的冻结形状）仍被受理的核对结论
- [x] 1.3 把 `error` 从「认证后必须有连接标识与连接序号」的集合中移出，改为「两者同时出现或同时缺席」：同时出现时按既有 UUID / `decimalString` 形状校验，只出现一个时按 `envelope_violation` 拒收；`control.ping`/`control.pong` 保持在认证后集合不动；判据：既有 `wire.test.ts` 全绿，且报告给出四种组合（都缺席 / 都出现 / 只带标识 / 只带序号）各自的实测结论

## 2. 固定样例门禁与连接级回归（WP2）

- [x] 2.1 新建 `clients/app/src/sync-client/wire-fixtures.test.ts`：从 `clients/app/src/protocol/manifest.ts`（**不经** `protocol` 的 barrel）读仓库根 `fixtures/sync/v1/manifest.json`，断言其中 `valid === true` 且 `schema` 以 `message.schema.json` 结尾的每条样例的磁盘原文都被解码边界受理；判据：`cd clients/app && npm run check` 通过，且报告写明本轮样例条数与被覆盖的消息类型数（当前为 101 条 / 18 个类型）
- [x] 2.2 在同一用例文件里对两个 node 负例做定点断言：`invalid/event-origin-node-missing-node-id.json` 的拒绝原因为 `missing_field`、`invalid/event-origin-node-without-node-id.json` 的拒绝原因为 `field_type`；判据：用例通过，且报告给出「把 1.2 的配对规则删掉后这两条断言变红」的判别力核对（临时改动不留痕）
- [x] 2.3 在用例文件里显式声明**不**做「所有非法样例都必须被拒收」的门禁，并说明理由：多数 `invalid` 样例违反的是视图值域，而解码边界的既定口径只判信封与判别字段（值域归 `schemas/sync/v1/**` 与降级路径）；判据：该说明以注释形式落在用例文件里，且检视时确认没有出现全量非法样例断言
- [x] 2.4 新建 `clients/app/src/sync-client/frame-acceptance.test.ts`：用 `clients/app/src/sync-client/testing/harness.ts` 的假 socket 与认证前置，投递一条 `origin.kind = "node"` 的合法事件，断言它被提交进账本并触发 ACK（游标推进到该事件之后），且其后的下一条事件同样被呈现；判据：`cd clients/app && npm run check` 通过，且报告给出「把词表改回四值后该用例变红」的判别力核对（临时改动不留痕）

## 3. 浏览器级用例（WP3）

- [x] 3.1 在 `clients/app/e2e/page/fixtures.tsx` 的 `wireEvent` 增加可选的 `origin` 入参（缺省值保持今天的 `{ kind: "agent", deviceId: null }`），使既有检查集的取值逐字不变；判据：`cd clients/app && npm run check` 的 `typecheck:e2e` 通过，且 `node e2e/run-e2e.mjs` 的既有检查集条数与通过状态不变
- [x] 3.2 新建 `clients/app/e2e/page/checks/frame-acceptance.ts`：假主机在真实连接上投递一条 `origin.kind = "node"`（含 `nodeId`）的合法事件，断言三件事——页面呈现该事件、假主机收到覆盖该事件 `globalSequence` 的 `sync.ack`、该事件之后的下一条事件同样被呈现；判据：`cd clients/app && node e2e/run-e2e.mjs` 全绿且输出含新检查集（断言的是可观察行为，不是启动成功）
- [x] 3.3 把新检查集注册进 `clients/app/e2e/page/index.tsx` 的 `ALL_CHECKS`（位置按该文件既有的顺序约定），并在检查集文件头写明判别力来源；判据：注册后 `node e2e/run-e2e.mjs` 报告的新条数等于旧条数加新增条数，且报告给出「把 1.1/1.2 的修正临时回退后该检查变红」的实测输出（回退不留痕）

## 4. 过期文档回正（WP4）

- [x] 4.1 修 `crates/server/src/sync/mod.rs` 的模块文档：已落地切片列表补 `event`（`project`/`frame`/`deliver` 的实际落地范围），删除「尚未落地：事件投递腿」整段，并把「过渡形态」段落改成与 `crates/server/src/sync/subscribe/mod.rs` 今天的自述一致（事件投递只走 `UseCases::sync_replay`，`Broker::replay` 的直连只剩保留窗口探测一处，不再写「任务 1.6 在途」）；判据：报告给出「改前表述 → 代码/文件证据 → 改后表述」三列，每条声明都能在 `crates/server/src/sync/` 找到对应实现，且 `cargo fmt --all -- --check` 通过
- [x] 4.2 修 `docs/FRONTEND_DESIGN.md` 的三处过期口径（§4.1 的未闭合边界①、§9 的 `[实现现状]`、§9 上方修订记录行），统一到「真实 Daemon 的浏览器级端到端验收已于 2026-10-16 执行」这一事实，并以 `openspec/changes/archive/2026-10-16-frontend-sync-wiring/plan.md` 的「真机验收记录」为证据源（含其中两项当批修掉的缺陷与第 7 项的后续闭合）；判据：报告给出每处「改前 → 证据 → 改后」，且 `npm run check` 的 `check:docs` 通过
- [x] 4.3 核对 `README.md` 无需改动（它与真机验收记录一致），并确认本次**没有**改到 `docs/FRONTEND_DESIGN.md` §9 第 11 条的前置条件（proposal 已把它列为非目标，留给后续变更）；判据：`git diff` 的改动清单里不出现 §9 第 11 条那一段，报告写明核对结论

## 5. 合入与主检出验证

- [x] 5.1 按 plan 的 Merge Order 串行 `git apply --check` → `git apply`，每次合入后跑该行登记的 Re-verify；判据：四条 patch 全部合入且每次 Re-verify 的输出留存
- [x] 5.2 在主检出跑 Project Verify PV1–PV6（`cd clients/app && npm run check`、`cd clients/app && node e2e/run-e2e.mjs`、`npm run check`、`cargo test --locked --workspace --all-features`、`cargo clippy` + `cargo fmt --check`、`npm run verify`）；判据：全部通过，且验收记录写明新门禁的样例条数、浏览器检查的总条数与新检查集的实测输出

## Workflow follow-up

- 合入并跑过 Project Verify 后，按 `docs/WORKFLOW.md` §6.2 走远端落地：为这批 patch 开分支、推远端、`gh pr create --fill`、等六个必需检查（含 `前端检查（clients/app）`）全绿后 `gh pr merge --squash`；提交信息按 `commitlint.config.mjs` 手写（scope 用 `frontend`，文档改动同批）。
- 归档前确认 specs 增量已同步到 `openspec/specs/`，且 `npm run check`（含 `check:openspec`）全绿。
