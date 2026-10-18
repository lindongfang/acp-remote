# ADR-0011：监听绑定与对外身份的配置边界

- 状态：Accepted
- 日期：2026-10-18
- 影响范围：`daemon.port` / `daemon.bind` / `daemon.public_origin`、接入层绑定、配对地址派生、CLI `config origin`
- 关联：[CONFIG_REFERENCE.md](../CONFIG_REFERENCE.md) §1、[SECURITY_DESIGN.md](../SECURITY_DESIGN.md) §7.1、[GLOSSARY.md](../../GLOSSARY.md)

## 背景

原来的 `daemon.listen = "ip:port"` 把两件不同的事压在一个字符串里：**本机在哪个地址收包**（部署与暴露面）和 **外部用什么地址访问**（`public_origin`，设备身份与配对 URL 的锚点）。远程部署（尤其是 Tailscale）让这两个问题同时暴露：

- 用户只想配端口，却必须理解 loopback 与 `0.0.0.0` 的安全差异；
- `public_origin` 要求手写完整 `https://host[:port]`，而实际来源（Tailscale MagicDNS）在别处；
- 默认只监听 loopback 与“远程优先”的产品方向冲突，但无条件改成 `0.0.0.0` 会在 `proxy` 模式下把明文 HTTP 暴露到局域网。

## 决策

1. **拆分绑定键**：`daemon.port`（整数，默认 `8765`）与可选的 `daemon.bind`（单个 IP 字面量）。显式 `bind` 只绑定该地址，不自动补第二个地址；`bind = "::"` 表示只监听 IPv6。
2. **默认绑定随 TLS 模式分叉**：`proxy` → `127.0.0.1`；`direct` → `0.0.0.0` 并尽力补绑 `[::]`。IPv6 补绑的 `EADDRINUSE`/`EAFNOSUPPORT`/`EADDRNOTAVAIL` 降级为 `net.ipv6_bind_skipped` 告警，其余错误失败关闭。
3. **`proxy` 模式禁止非 loopback 绑定**：Daemon 在该模式下不说 TLS，非 loopback 会暴露明文面；配置解析阶段即失败。开发明文同样只允许 loopback。
4. **`port = 0` 只允许开发模式 + loopback**：临时端口只服务测试与本地调试，避免生产出现随机端口暴露。
5. **`public_origin` 只写 `host[:port]`**：内部补 `https://`；继续接受完整 `https://...` 以兼容旧配置；`http://` 只允许开发明文模式；拒绝 path/query/fragment/userinfo 与端口 0。
6. **按需失败**：绑定非 loopback 且 `public_origin` 与 `allowed_hosts` 都为空时启动失败（远程 Host 必被拒）；只有 `allowed_hosts` 时启动告警，配对/Node Link 在使用点返回明确错误并提示配置 `public_origin`；loopback-only 缺失 `public_origin` 不报错。
7. **`acp-remote config origin` 只读**：优先读 `tailscale status --json` 的 MagicDNS 名字与地址，失败回退到 `if-addrs` 网卡枚举；打印建议值与候选，不写配置文件。Tailscale 命令只在用户显式调用时执行，启动路径不依赖它。

## 结果

- 普通用户只需要 `daemon.port`；远程场景要么显式 `direct` + 证书（默认监听所有接口），要么保持 `proxy` + loopback + 同机反代。
- `public_origin` 的输入与用户心智一致（域名或 IP），但内部仍是 canonical origin，设备身份绑定与 Origin 校验语义不变。
- 默认配置仍然不会把明文 HTTP 暴露到 loopback 之外；这一点由 `SECURITY_DESIGN.md` §7.1 与测试共同保证。
- 旧的 `daemon.listen` 被明确拒绝并给出迁移提示，不存在两套配置形状长期共存。

## 拒绝的方案

- **无条件默认 `0.0.0.0`**：`proxy` 模式下等于默认暴露明文 HTTP，违背默认安全 Profile。
- **把 `0.0.0.0` 改成“外部访问白名单”**：绑定地址不是访问控制；来源白名单在反代/NAT 下失真，且与“网络可达 ≠ 应用授权”的模型冲突。
- **让 `public_origin` 从请求 Host/Origin 自动推导**：请求头是攻击者可控输入，会使设备身份绑定与 DNS rebinding 防护失效。
- **多 canonical origin**：与“一个设备身份只绑定一个 canonical origin”的不变量冲突；多个名字的需求由 `allowed_hosts`（Host 白名单）满足，canonical origin 保持唯一。
- **`config origin` 直接写配置文件**：配置模块只读；写错 origin 的代价是设备身份绑定错误，高于多复制一行的成本。
