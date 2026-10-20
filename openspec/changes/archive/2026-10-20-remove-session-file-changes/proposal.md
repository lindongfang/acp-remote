# Proposal

## Why

用户要求“去掉会话变更”，并已确认只移除文件栏的该来源。会话事件提供的差异不保证完整的修改前后内容，单独的会话变更清单容易被误认为可读取本会话专属的完整文件版本；保留目录浏览与 Git 变更即可满足当前查看需求。

## What Changes

- 文件栏只保留“全部文件”和“Git 变更”，移除“会话变更”入口及其专属历次记录查看。
- 首次进入默认“全部文件”，旧的会话变更选择回退到“全部文件”，不更名为“目录”。
- Git 变更明确表示整个会话工作目录的未提交修改，可能包含人工操作和其他会话的修改；非 Git 目录明确说明不可用，仍可浏览全部文件。
- 保留对话时间线的工具调用、文件差异和既有历史记录，不删除后端文件事件、ACP 能力或历史查询。
- 同步原型、交互文档和回归用例；不修改协议、数据库、安全边界、依赖或远程文件读取能力。

## Capabilities

### New Capabilities

无。

### Modified Capabilities

- `pwa-web-client`：文件栏来源、默认项、旧选择回退、结构化条目与文件栏联动、时间线差异保留。

## Impact

- 客户端：`clients/app/src/domain/session-files.ts`、`features/session-files.ts`、`features/client-store.ts`、文件栏与文件查看组件、对话页接线及对应测试与 E2E。
- 设计资产：`prototypes/acp-remote-pwa.html`、原型说明、`docs/FRONTEND_DESIGN.md`、`docs/INITIAL_DESIGN.md`、讨论记录与证据。
- 成功判据：宽窄屏均只有两个来源；首次进入及旧选择均落到全部文件；非仓库可浏览；Git 来源不宣称会话独占；工具差异和历史恢复不回归；打开/关闭文件仍保留草稿、滚动与展开状态。
- 与在途 `session-file-browser` 的“三来源”和会话变更默认项相冲突。本变更是用户后续修订，实施与规范同步顺序必须先旧后新；不重写旧变更的历史验收证据。`conversation-area-complete` 的时间线文件改动要求继续有效。
- 本轮仅创建规划产物，不实施、不提交或发布。
