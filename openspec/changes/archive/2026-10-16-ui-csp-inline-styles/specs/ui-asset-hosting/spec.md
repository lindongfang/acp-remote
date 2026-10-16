## ADDED Requirements

### Requirement: 同源托管下前端样式在既定 CSP 内生效

接入层为静态面固定的 CSP（当前 `default-src 'self'; script-src 'self'; connect-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'`）既是安全约束，也是前端的运行前提：前端 SHALL 在该 CSP 下呈现完整样式，MUST NOT 依赖被该策略拦下的内联样式（`<style>` 元素与 `style` 属性）。样式形态的改变 SHALL 在前端侧完成；把 CSP 放宽（例如加入 `style-src 'unsafe-inline'`）MUST NOT 被当作规避手段。

#### Scenario: 真实 CSP 下样式真的生效

- **WHEN** 浏览器从 Daemon 同源托管的来源加载页面（响应带上述 CSP）
- **THEN** 关键样式 SHALL 真的生效，而不是只有 `<style>` 元素存在：断言 MUST 能在「样式被 CSP 拦下」时变红（例如断言某元素的计算样式等于预期值）

#### Scenario: 不放宽 CSP 换绿

- **WHEN** 样式在既定 CSP 下不生效
- **THEN** 修正 MUST 落在前端侧，CSP 的取值 MUST NOT 被放宽；若确实需要改 CSP，必须先按 `AGENTS.md` §2 请求确认并同步安全设计文档
