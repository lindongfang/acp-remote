# 本地设备配对验证手册

> 状态：操作手册（非协议权威）  
> 日期：2026-10-18  
> 适用范围：Windows x64 开发机，单一 Daemon + 浏览器/PWA 的设备配对验收。
>
> 本文只说明“怎么在本机跑通并观察”，不定义协议语义。配对、认证与 wire 字段的权威来源是
> [SYNC_PROTOCOL.md](./SYNC_PROTOCOL.md)；传输与信任边界见 [SECURITY_DESIGN.md](./SECURITY_DESIGN.md)；
> 配置键见 [CONFIG_REFERENCE.md](./CONFIG_REFERENCE.md)；本地管理方法见
> [LOCAL_ADMIN_PROTOCOL.md](./LOCAL_ADMIN_PROTOCOL.md)。

## 1. 目的与边界

本手册验证的是一条完整链路：

```text
浏览器读取配对地址
  -> POST /sync/v1/pairing/claim
  -> 主机侧 device.pair.status 轮询
  -> 本机用户确认 device.pair.confirm
  -> 浏览器轮询到 approved 并落盘
  -> WSS /sync/v1 设备签名认证
  -> 首次快照，连接进入 online
```

不包含：

- Node Link（A 的 Daemon 导入 B 的 Agent）：`node-link-client` 尚未落地，
  `node pair --mode access` 当前返回 `local.unsupported`。
- 生产部署、TLS 反向代理、证书续期自动化。
- 设备授权范围：当前设备方向没有 scope，配对即完全信任。

## 2. 前置条件

- Windows x64。
- 仓库位于 `D:\Project\acp-remote`。
- Rust 工具链可用（版本以仓库 `rust-toolchain.toml` 为准）。
- Node.js ≥ 22.12，`clients/app` 已执行过 `npm ci`。
- PowerShell（交互式终端，配对确认需要输入 `y`）。
- 能运行 Git for Windows 自带的 OpenSSL：
  `D:\Application\Git\usr\bin\openssl.exe`。
- TLS 证书：首次运行必须先生成，步骤见 §4。`tls.mode = "direct"` 时没有证书
  Daemon 无法启动。
- 本机端口 `18765` 未被占用。

检查端口：

```powershell
Get-NetTCPConnection -LocalPort 18765 -ErrorAction SilentlyContinue
```

没有输出表示端口可用。

## 3. 目录约定

本机采用“证书长期保存、运行态按环境分目录”的结构：

```text
D:\Data\
├─ certificate\                 # 长期：TLS 证书，可轮换，路径稳定
│  ├─ cert.pem
│  └─ key.pem
└─ acp-remote\
   └─ dev\                      # 当前开发环境
      ├─ config.toml            # 可频繁调整
      └─ data\                  # 节点权威运行状态，尽量稳定
```

创建目录：

```powershell
New-Item -ItemType Directory -Force D:\Data\certificate
New-Item -ItemType Directory -Force D:\Data\acp-remote\dev\data
```

不要把 `key.pem`、`data\` 放进仓库：`data\` 包含 SQLite、日志、附件、配对与信任记录，
`key.pem` 是私钥。

## 4. 前置条件：生成 TLS 证书

Daemon 在 `tls.mode = "direct"` 下自己终止 TLS，因此需要一份带 SAN 的服务器证书。
本机使用自签证书即可；浏览器第一次访问时接受一次例外。

### 4.1 用 OpenSSL 生成

```powershell
$OpenSSL = "D:\Application\Git\usr\bin\openssl.exe"

& $OpenSSL req -x509 -newkey rsa:2048 -sha256 -days 365 -nodes `
  -keyout "D:\Data\certificate\key.pem" `
  -out "D:\Data\certificate\cert.pem" `
  -subj "/CN=127.0.0.1" `
  -addext "subjectAltName=IP:127.0.0.1,DNS:localhost,IP:::1"
```

关键点是 `subjectAltName`。现代浏览器不再看 `CN`，证书里没有 `IP:127.0.0.1`
会直接报名称不匹配。

### 4.2 验证证书

```powershell
& $OpenSSL x509 -in "D:\Data\certificate\cert.pem" -noout -subject -ext subjectAltName
```

期望包含：

```text
subject=CN=127.0.0.1
X509v3 Subject Alternative Name:
    IP Address:127.0.0.1, DNS:localhost, IP Address:0:0:0:0:0:0:0:1
```

### 4.3 可选：用 mkcert 消除浏览器警告

自签证书会让浏览器提示“不受信任”。如果不想每次接受例外：

```powershell
winget install --id FiloSottile.mkcert -e
mkcert -install
mkcert -cert-file D:\Data\certificate\cert.pem -key-file D:\Data\certificate\key.pem 127.0.0.1 localhost ::1
```

这会覆盖已有证书文件，Daemon 配置不变。

### 4.4 私钥权限

确保私钥只对当前用户可读。开发机上至少不要把它放进仓库或共享目录：

```powershell
icacls D:\Data\certificate\key.pem /inheritance:r /grant:r "$($env:USERNAME):(R,W)"
```

## 5. 写 Daemon 配置

`D:\Data\acp-remote\dev\config.toml`：

```toml
[daemon]
data_dir = "D:/Data/acp-remote/dev/data"
port = 18765
bind = "127.0.0.1"            # 本地开发只绑定 loopback；direct 模式缺省会绑定所有接口
public_origin = "127.0.0.1:18765"

[daemon.tls]
mode = "direct"
cert_path = "D:/Data/certificate/cert.pem"
key_path = "D:/Data/certificate/key.pem"
```

说明：

- `data_dir` 必须是绝对路径，保存 SQLite、日志、附件和锁文件。
- `port` 是 Daemon 实际监听的端口；`bind` 可选，本地开发建议显式写 `127.0.0.1`。
- `public_origin` 是外部看到的权威地址；可只写 `host[:port]`（自动补 `https://`），
  配对 URL 与 `Origin`/`Host` 校验都依赖它，必须与浏览器地址栏逐字一致。
- `tls.mode = "direct"` 表示 Daemon 自己终止 TLS，因此需要 `cert_path` 与 `key_path`。
- 改为 `tls.mode = "proxy"` 时删除证书两行，由同机反代持有证书；此时 Daemon 只允许绑定 loopback。

配置键的完整说明见 [CONFIG_REFERENCE.md](./CONFIG_REFERENCE.md) §1。
不确定该把 `public_origin` 写成什么时，运行 `acp-remote config origin`：它会列出本机地址
（Tailscale 场景优先给出 MagicDNS 名字）并打印建议值，不会自动改配置文件。

## 6. 构建

Daemon 在编译期把前端产物嵌入二进制，因此顺序必须是“先导出前端，再构建 Rust”。

```powershell
cd D:\Project\acp-remote\clients\app
npm run export:web

cd D:\Project\acp-remote
cargo build -p app
```

如果之前启动过 Daemon，先停止：

```powershell
.\target\debug\acp-remote.exe --config D:\Data\acp-remote\dev\config.toml daemon stop
```

## 7. 启动 Daemon

终端 1：

```powershell
cd D:\Project\acp-remote
.\target\debug\acp-remote.exe --config D:\Data\acp-remote\dev\config.toml daemon start
```

`daemon start` 是前台进程，保持窗口打开。

另开终端确认：

```powershell
cd D:\Project\acp-remote
.\target\debug\acp-remote.exe --config D:\Data\acp-remote\dev\config.toml daemon status
```

## 8. 发起设备配对

终端 2（必须是交互式终端）：

```powershell
cd D:\Project\acp-remote
.\target\debug\acp-remote.exe --config D:\Data\acp-remote\dev\config.toml device pair
```

终端会输出类似：

```text
https://127.0.0.1:18765/pair#data=...
正在等待设备认领…（有效期至 ...，Ctrl-C 取消）
```

保持终端 2 不要关闭，它在轮询 `device.pair.status`。

## 9. 浏览器侧操作

在浏览器打开终端 2 打印的 URL。

第一次访问会提示证书不受信任，点“高级 → 继续前往”。

应观察到：

1. 地址栏从 `/pair#data=...` 变为 `/pair`，片段被读取后立即清除。
2. 页面显示居中转圈与 `m:ss` 倒计时；没有输入框、粘贴框或提交按钮。
3. 页面自动发起 `POST /sync/v1/pairing/claim`，正常返回 `201`。
4. 页面用二维码里的主机公钥验证响应里的 `hostProof`。

## 10. 本机确认

浏览器认领后，终端 2 打印设备名称、指纹与过期时间，并提示：

```text
确认与本机记录该设备（配对即完全信任）？[y/N]:
```

输入 `y` 并回车。

- 只有明确的 `y`/`Y` 才算确认。
- 其他输入（包括直接回车）按拒绝处理，CLI 会调用 `device.pair.reject`，
  不建立信任。
- 确认后打印：

```text
已配对：deviceId=<uuid>
```

浏览器随后会：

1. 轮询 `POST /sync/v1/pairing/status` 并看到 `approved`；
2. 校验返回的主机公钥与二维码里的逐字节相同；
3. 写入 `paired-host/<canonicalOrigin>`；
4. 连接 `wss://127.0.0.1:18765/sync/v1` 并完成设备签名 challenge-response；
5. 收到快照后进入 `online`；
6. 约 1.6 秒后自动跳转到目录页 `/`。

目录页可能为空，这是正常的；配对成功的判据是连接进入 `online`，不是目录里一定有内容。

## 11. 正向验收清单

| 检查 | 通过标准 |
|---|---|
| 页面来源 | `https://127.0.0.1:18765/` 能打开 |
| 片段处理 | 打开配对地址后 `#data=...` 立即消失 |
| 认领 | 主机侧 `POST /sync/v1/pairing/claim` 返回 201 |
| 确认 | CLI 展示指纹后才允许 `y`；确认后打印 `deviceId` |
| 落盘 | `paired-host/<origin>` 记录存在，含主机标识与公钥 |
| 连接 | WebSocket 路径为 `/sync/v1`，协商子协议 `acp-remote.sync.v1.json` |
| 在线 | 页面 `data-connection-state = online` |
| 重载 | 刷新后仍在线，不重新配对 |
| 设备列表 | `device list` 出现该设备 |
| 撤销 | `device revoke --device-id <uuid>` 后活动连接立即关闭 |

设备列表：

```powershell
.\target\debug\acp-remote.exe --config D:\Data\acp-remote\dev\config.toml device list
```

撤销：

```powershell
.\target\debug\acp-remote.exe --config D:\Data\acp-remote\dev\config.toml device revoke --device-id <uuid>
```

## 12. 负向验证

| 操作 | 期望 |
|---|---|
| 直接打开 `https://127.0.0.1:18765/pair`（无片段） | `no_payload`，不发出配对请求 |
| 用 `http://` 打开 | `insecure_context`，零请求 |
| 用与 `public_origin` 不同的地址打开 | `origin_mismatch`，零请求 |
| 把 node-link 二维码内容喂给 `/pair` | `kind_mismatch`，不回显载荷、不发请求 |
| CLI 提示时输入 `n` 或直接回车 | 调用 `device.pair.reject`，不建立信任 |
| 配对超过 5 分钟 | `local.expired`，页面停止轮询 |
| 改动 `paired-host` 记录里的主机公钥 | 连接进入阻断态 `host_identity_changed` |

## 13. 故障排查

### 13.1 `cargo build` 报 `os error 4551`

Windows 的 Smart App Control 拦截了 Cargo 生成的未签名构建脚本，典型错误：

```text
failed to run custom build command for `serde_json`
应用程序控制策略已阻止此文件。 (os error 4551)
```

确认状态：

```powershell
Get-ItemProperty 'HKLM:\SYSTEM\CurrentControlSet\Control\CI\Policy' |
  Select-Object VerifiedAndReputablePolicyState
```

`1` 表示强制执行。处理方式：Windows 安全中心 → 应用和浏览器控制 → 智能应用控制 → 关闭，
然后重启并重试。

注意：关闭 Smart App Control 不可逆，关闭后除非重置或重装 Windows 才能重新开启。
公司管理的机器可能需要 IT 放行。

如果关闭后仍失败：

```powershell
Remove-Item -Recurse -Force D:\Project\acp-remote\target\debug\build\serde_json-*
cargo build -p app
```

### 13.2 其他常见失败

| 现象 | 先查什么 |
|---|---|
| `device pair` 没有打印 URL，返回 `local.unavailable` | 是否配置了 `public_origin`；keystore 是否可用 |
| 页面提示 `origin_mismatch` | 浏览器地址是否严格等于 `public_origin` |
| 页面提示 `insecure_context` | 是否误用了 `http://` |
| 页面一直停在等待 | 终端 2 是否停在 `[y/N]`；配对是否已过期 |
| 页面能打开但一直不在线 | Daemon 日志中的 WSS 握手与证书错误 |
| `local.conflict` | 该配对已被批准或拒绝，重新 `device pair` |
| `host_identity_changed` | 本机记录与当前主机公钥不一致，清掉旧的 `paired-host` 记录后重新配对 |

## 14. 长期维护

| 变更 | 是否要重新配对 |
|---|---|
| 更换 TLS 证书/私钥 | 不需要 |
| 修改日志、会话、存储参数 | 不需要 |
| 修改 `port`/`bind`，但 `public_origin` 不变（反代场景） | 一般不需要 |
| 修改 `public_origin` | 需要，设备身份绑定在 canonical origin 上 |
| 更换 `data_dir` | 需要，旧设备、信任和会话都在旧库里 |
| 删除 `data_dir` | 需要，等同于清空节点状态 |

`data_dir` 是节点的权威运行状态，不要为了换配置而随手更换。需要独立实验时，另建一套环境目录：

```text
D:\Data\acp-remote\
├─ dev\
│  ├─ config.toml
│  └─ data\
└─ test\
   ├─ config.toml
   └─ data\
```

备份时同时处理 SQLite 的主库、`-wal`、`-shm` 文件与附件目录；只复制主库可能得到陈旧副本。

## 15. 不在本文范围

- Node Link 节点对接：Access 侧客户端 `node-link-client` 尚未落地。
- 生产 TLS：应使用正式证书或同机反向代理（Caddy、Tailscale Serve 等），并确认
  WebSocket upgrade 与 `Origin`/`Host` 透传。
- 多设备并发、撤销传播、慢客户端恢复等协议级行为：以
  [SYNC_PROTOCOL.md](./SYNC_PROTOCOL.md) 与对应 Rust 测试为准。
