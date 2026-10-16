# ui-asset-hosting Specification

## Purpose
定义 Daemon 同源托管前端静态资源的行为：根目录入口与单页回退、与协议端点的解析优先级、内容类型与安全响应头、未知路径的明确拒绝，以及前端构建版本与 Daemon 的可检查关联。它让 PWA 能在与 Daemon 相同的来源下加载，从而使同源 WebSocket 连接与设备身份绑定成立。

## Requirements

### Requirement: 根目录入口与单页回退

服务端 SHALL 在根目录提供前端静态资源：根路径返回该构建的入口文档；对任意**非协议**路径，若构建产物中不存在同名文件，SHALL 返回同一入口文档（单页应用回退），使前端路由（如配对页、主机页、目录详情、会话页）可直接通过地址栏访问与刷新。构建产物中存在的文件 SHALL 按其路径原样返回。

#### Scenario: 根路径返回入口

- **WHEN** 浏览器请求根路径
- **THEN** 返回前端构建的入口文档，内容类型为 HTML

#### Scenario: 前端路由深链可刷新

- **WHEN** 浏览器直接请求一个前端路由路径（构建产物中没有该文件）
- **THEN** 返回入口文档而不是 404，页面加载后由前端路由接管

#### Scenario: 构建产物按路径返回

- **WHEN** 浏览器请求一个构建产物中存在的资源路径
- **THEN** 返回该资源本身，内容类型与该文件类型相符

### Requirement: 协议端点优先于静态回退

静态回退 MUST NOT 吞掉任何已注册的协议端点：Sync 与 Node Link 的 WebSocket 端点、两者的配对 HTTP 端点 SHALL 始终由各自的协议处理逻辑接管，MUST NOT 因静态回退而返回入口文档或资源。未注册的协议族路径（例如 Sync 配对端点下的其它子路径，或以非 POST 方式请求配对端点）MUST 按接入层既有的错误语义拒绝，MUST NOT 被静态回退悄悄接管。

#### Scenario: 协议端点不被回退吞掉

- **WHEN** 客户端向 Sync 或 Node Link 的 WebSocket 端点发起升级，或向任一配对端点发起 POST
- **THEN** 请求由对应协议处理，返回的结果 MUST NOT 是入口文档

#### Scenario: 未注册的协议子路径不被回退接管

- **WHEN** 客户端请求协议族下未注册的子路径，或以非 POST 方式请求配对端点
- **THEN** 请求按接入层既有语义被拒绝，MUST NOT 返回前端入口文档

### Requirement: 内容类型与安全响应头

服务端 SHALL 为返回的入口文档与资源设置正确的内容类型；MUST NOT 以通用二进制类型搪塞，也 MUST NOT 让浏览器因类型不符而拒绝执行脚本或样式。静态响应 SHALL 携带内容安全策略，其 SHALL 至少满足前端设计文档规定的基线（默认源与脚本源限同源、连接源限同源、禁止对象嵌入、禁止基地址改写、禁止被框架嵌套）。CSP MUST 由服务端在响应头中下发，MUST NOT 只依赖文档内的元标记。

#### Scenario: 资源带正确类型

- **WHEN** 浏览器请求构建产物中的脚本、样式或图片
- **THEN** 响应内容类型与该类型相符，浏览器正常加载

#### Scenario: 入口文档带 CSP 响应头

- **WHEN** 浏览器请求根路径或任一前端路由
- **THEN** 响应头含内容安全策略，其取值不宽于前端设计文档的基线；MUST NOT 出现允许任意来源脚本或允许连接任意外部来源的指令

### Requirement: 未知路径仍显式拒绝

对既非已注册协议端点、也不属于静态回退范围的请求，服务端 SHALL 返回明确的未找到状态。回退 MUST NOT 成为「任何路径都返回 200 」的兜底，也 MUST NOT 让拼错的协议路径看起来像成功加载的页面。

#### Scenario: 不存在的资源被拒绝

- **WHEN** 浏览器请求一个既有协议端点也不在构建产物中的路径
- **THEN** 在单页回退语义下返回入口文档；对明确不属于回退范围的请求（例如要求非 GET 方法）返回明确的拒绝状态，不得一律 200

#### Scenario: 请求方法不匹配

- **WHEN** 客户端对静态资源路径发起非读取类方法
- **THEN** 请求被明确拒绝，MUST NOT 返回入口文档冒充成功

### Requirement: 前端构建版本与 Daemon 可检查关联

Daemon SHALL 能回答它所托管的前端构建版本，且该版本 MUST 与当前托管的资源出自同一次构建。该关联 SHALL 可通过一个机器可读的入口查询。构建版本缺失或与资源不一致时，SHALL 能被检查出来而不是静默通过。

#### Scenario: 版本可查询且与资源同源

- **WHEN** 运维或客户端查询 Daemon 携带的前端构建版本
- **THEN** 得到该版本标识，且它与实际托管资源出自同一次前端构建

#### Scenario: 版本缺失时不静默通过

- **WHEN** 托管的前端资源没有可识别的构建版本信息
- **THEN** 该情况可被检查出来（查询给出明确缺失语义或构建期即失败），MUST NOT 悄悄返回一个编造的版本

### Requirement: 同源托管下前端样式在既定 CSP 内生效

接入层为静态面固定的 CSP（当前 `default-src 'self'; script-src 'self'; connect-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'`）既是安全约束，也是前端的运行前提：前端 SHALL 在该 CSP 下呈现完整样式，MUST NOT 依赖被该策略拦下的内联样式（`<style>` 元素与 `style` 属性）。样式形态的改变 SHALL 在前端侧完成；把 CSP 放宽（例如加入 `style-src 'unsafe-inline'`）MUST NOT 被当作规避手段。

#### Scenario: 真实 CSP 下样式真的生效

- **WHEN** 浏览器从 Daemon 同源托管的来源加载页面（响应带上述 CSP）
- **THEN** 关键样式 SHALL 真的生效，而不是只有 `<style>` 元素存在：断言 MUST 能在「样式被 CSP 拦下」时变红（例如断言某元素的计算样式等于预期值）

#### Scenario: 不放宽 CSP 换绿

- **WHEN** 样式在既定 CSP 下不生效
- **THEN** 修正 MUST 落在前端侧，CSP 的取值 MUST NOT 被放宽；若确实需要改 CSP，必须先按 `AGENTS.md` §2 请求确认并同步安全设计文档
