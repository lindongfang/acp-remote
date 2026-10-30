# Web隔离准备阶段

日期：2026-10-30。工作区 `C:/Users/jack/.codex/worktrees/codex-session-web/acp-remote`。组件尚未挂入main页面，未与节点或真实Harness联调，不签WP4完成。

初版14文件经静态独立检视发现WEB-R001异步安装回退、WEB-R002慢详情下载耗尽列表游标、WEB-R003同批条目身份冲突三项问题。旧清单和日志保留，不将其74项开发测试通过当成正确性证明。

修复版20文件清单位于工作区 `clients/web/.tmp/wp4-web-prepare/r001-r003/handoff.json`，原字节SHA256 `48EC154E5A735E563A80AD04CF3AB0AD987CAFBC8C23F3B9163BC0A4C354DC13`。提交瞬间复核view代际/安装序号/提交及live版本、基于最新映射合并；先枚举短列表后读取详情，设明确有界缓存；同批身份索引和原始字节检查同版本内容。开发类型检查、84项测试、构建及差异检查exit0。

原reviewer已核新20文件前后摘要并独立执行3个回归文件10/10通过，关闭三项问题，详见 `reviewer-web-preparation.md`。回归中的SessionPanel使用视图替身，不是实际DOM/浏览器验收。协议分块/帧深度/单调版本、投影manifest、广告审批和未知交付组件均待最终组合后真实复验；不得据此把真实Web会话链路标通过。

自动审批曾拒绝在准备阶段挂载main.ts/styles.css，理由是超出WP3冻结前隔离准备范围；操作未执行且未重试。两文件保持原基线。最终WP4在WP3冻结交接后再按计划接管主页面并进行真实浏览器验证，不绕过阶段门槛。
