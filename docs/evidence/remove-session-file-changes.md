# `remove-session-file-changes` 验收证据

本文件按层登记本变更的证据：**只写实际执行过的命令与观察结果**；未执行项明确列在最后，不写成通过，
也不以启动成功、截图或计划代替断言。本变更不重写 [`session-file-browser.md`](./session-file-browser.md)
等旧证据；那些文件记录的是当时的三来源状态，属于历史。

证据分层：

1. 组件与假主机用例（客户端行为，`clients/app`）
2. 真实 Daemon 托管的 PWA（真实浏览器 + 真实数据）
3. 原型（桌面原型源码与浏览器检查）
4. 未执行项

## 1. 组件与假主机用例

状态：**已执行（2026-10-20，主检出，Windows 10.0.26300 / Node 22 / Chromium 内核由本地 Edge 提供）**。

实际命令与结果：

- `cd clients/app && npm run check`：`tsc` 与 `tsc --project e2e/tsconfig.json` 均通过；`vitest run` = **81 files / 1257 tests passed**；`expo export --platform web` 成功（`dist` 已生成）。
- `cd clients/app && node e2e/run-e2e.mjs`：**101/101 passed**，覆盖 `SF-1`–`SF-10`、`CP-5`、`CP-7`（时间线文件改动）、`CP-8`（imported 降级）等。
- 本层覆盖的断言：

- 来源收敛：`FileSource` 只接受 `all | git`，首次进入默认 `all`；旧的 `session` 输入值在入口归一为
  `all`，不发起会话变更历史刷新，也不恢复历次记录查看。
- 文件栏专属移除：不存在 `session_changed` / 会话记录归并模型 / 记录选择器；手动刷新只派发当前来源的
  目录或 Git 查询，在途不重复派发，失败保留现有列表。
- 事件与历史保留：当前会话的 `file.changed` 仍触发目录/Git 刷新或待刷新标记；对话首次读取、历史分页与
  去重不受影响；时间线的工具调用、结构化差异、缺失差异说明与 ACP 原文诊断仍可用。
- 界面与宽窄屏：宽屏右栏与窄屏入口都只有「全部文件 / Git 变更」；打开/关闭文件保留草稿、滚动与条目
  展开状态；关闭后来源保持或回到 `all`；迟到响应不覆盖较新文件或重开已关闭查看区。
- Git 范围：文案说明整个会话工作目录的未提交修改可能包含人工与其他会话的修改；干净仓库、非仓库、
  冲突、二进制与截断各有可区分说明；非仓库仍可浏览全部文件。
- 远程门控：imported 会话与未协商能力的连接明确不可用，且不查询本机替代目录。
- 假主机浏览器用例与组件测试文件由本变更 WP1 一并更新，具体用例名与计数以主检出的实际输出为准。

## 2. 真实 Daemon 托管的 PWA

状态：**未执行**。本层需要把主检出构建的客户端产物交给本机真实 Daemon 托管，并用真实浏览器完成配对后操作；本轮只执行了第 1 层的假主机 PWA 与第 3 层的静态原型/本地文件原型，没有启动真实 Daemon，也不把假主机结果替代本层。

待执行的场景（Main E2E）：

- 已认证 PWA 打开真实会话，宽屏与窄屏都只有两个来源，首次进入为「全部文件」。
- 使用临时 Git 仓库与非仓库目录：人工修改在 Git 变更中如实出现，界面说明修改可能来自人工或其他会话；
  非仓库目录中 Git 变更显式不可用，全部文件仍可浏览。
- Agent 产生工具调用与文件差异：文件栏无会话记录清单，时间线仍可展开差异与原文诊断；重新进入恢复历史，
  重复事件不重复呈现。
- 打开文本/图片与 Git 差异、快速切换、关闭及跨宽窄屏保持草稿、滚动与展开；Agent 不停止，迟到响应不重开。
- 手动刷新与文件事件只刷新目录/Git；对话历史继续分页，无会话变更专属刷新。
- imported 会话、离线与旧能力连接的不可用说明，且不访问替代目录。

未执行时不把上述任一条写作通过。

## 3. 原型（桌面原型源码静态核对）

状态：**已执行静态核对与浏览器断言（2026-10-20，主检出）**。

核对方式与观察：

- `grep -n "会话变更\|session_changes\|records\|历次" prototypes/acp-remote-pwa.html`：无匹配。
  该命令只覆盖文案、来源标识与历次记录模式，**不**覆盖脚本里的分支判断，因此分支另行核对：
  - `grep -n '===[[:space:]]*"session"\|"session"[[:space:]]*===' prototypes/acp-remote-pwa.html`：
    修复前命中 `2742: if(source==="session") A.selFile=path;`。该行在 `data-open` 处理器内，当前不可达
    （生产者只有 `data-open="all"` 与 `data-open="git"`），但它仍是已移除来源的脚本分支；且
    `FILE_SOURCE_LABELS` 已无 `session`，一旦恢复任一 `data-open="session"` 入口，文件查看来源标签会渲染为
    `undefined`。本轮已删除该行，删除后同一命令无匹配。
  - `grep -n 'data-open="' prototypes/acp-remote-pwa.html`：只命中 `data-open="all"`（目录条目）与
    `data-open="git"`（Git 变更条目），没有 `data-open="session"`。删除上述分支后，处理器只做
    `A.viewFile={ source:op.dataset.open, path:op.dataset.path }`，即「生产者来源集合」与标签集合
    （`all`/`git`）一致。
  - `selFile` 仍被时间线文件折叠正常使用（默认值、`fold` 的 `sel` 选中类、`[data-file]` 点击），与文件栏
    来源无关；删除该分支不影响时间线。
  - 移动端生成链 `prototypes/acp-remote-pwa-mobile.html` 与 `prototypes/.build/{m-head.html,m-tail.js,reuse.js}`：
    `grep -n 'source==="session"'` 无匹配（本轮未改生成链）。
- 来源集合只有 `var FILE_SOURCE_LABELS = { all:"全部文件", git:"Git 变更" };`；文件栏标签由
  `["all","git"]` 生成，默认状态为 `fileSource:"all"`。
- 切换会话时 `A.fileSource` 重置为 `"all"`；关闭文件只清空 `A.viewFile`，保留当前来源。
- Git 来源新增说明：范围是整个会话工作目录的未提交修改，可能包含人工操作或其他会话的修改。
- 已删除只服务于旧来源的 `.fvsource.session` 样式。
- 用 `new Function` 解析原型内两段脚本（第 9–19 行、第 916–3103 行；删除上述分支后重新执行），
  均无语法错误（`node --check` 不接受 `.html`，因此未用它）。

浏览器断言（`file://` 打开 `prototypes/acp-remote-pwa.html#/chat/s1`，本地 Edge CDP）：

- 宽屏 1400×900：`[data-fsource]` 恰为「全部文件」「Git 变更」，`aria-selected="true"` 只有「全部文件」；
  页面文本不含「会话变更」。
- 从「全部文件」打开一个文件后，查看器来源标签为「全部文件」；关闭后查看器消失，来源仍为「全部文件」。
- 切到窄屏 600×800：来源集合与默认选中不变，仍只有两项且不含「会话变更」。
- 切到「Git 变更」：出现范围提示，明确「整个会话工作目录」与「其他会话的修改」。

`openspec validate remove-session-file-changes --strict`：**通过**。

未执行：移动端生成物 `acp-remote-pwa-mobile.html` 与 `.build/` 本轮未改动，也未做浏览器复验。

## 4. 未执行项

已执行：PV1（`clients/app npm run check`）、PV2（仓库根 `npm run check`）、PV3 的假主机
`node e2e/run-e2e.mjs`（101/101）、PV4（`openspec validate --strict` 与桌面原型浏览器断言）。

仍未执行：

- 真实 Daemon 托管的已认证 PWA Main E2E（建立临时 Git 仓库/非仓库目录、人工修改、真实 Agent
  工具差异、imported/离线/旧能力连接）；本轮没有启动真实 Daemon，不把假主机结果替代该层。
- 移动端生成物与 `.build/` 的浏览器复验（本轮未改生成链，来源集合已一致，Git 范围文案尚未同步）。
- 真实浏览器截图与真机证据。

以上未执行项不写作通过。

## 5. 归档与规范同步顺序

本变更修改的是 `session-file-browser` 引入的「文件栏提供三种来源」等要求；归档/同步时必须**先**
把 `session-file-browser` 的增量合入主规范，**再**应用本变更的 MODIFIED 增量。不得先同步本变更，
也不得让旧增量把主规范重新覆盖为三来源；旧变更的历史验收证据不重写。
