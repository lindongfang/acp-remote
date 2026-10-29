# 独立 SQLite 故障验收探针

仅使用本地 app 公共 API、真实节点/DPAPI/本机管道/直接 TLS，以及隔离 SQLite 故障夹具。不是开发单测，也不代表 NA-007/NA-027 整例通过。覆盖和限制见 `testing/runs/establish-node-access/2026-10-29-storage-acceptance.md`。

Windows x64、仓库 Rust 工具链、已缓存的 Cargo 依赖及现有 `clients/web/dist/index.html` 是前提。使用当前用户正常上下文；沙箱可能使 DPAPI 失败。无须安装证书或修改用户配置。TLS 客户端仅信任当次生成的 localhost 证书，不跳过校验。

在仓库根目录运行以下 PowerShell 命令（命令将切换到专用临时目录，避免生成数据进入探针源码目录）：

```powershell
$repo = (Get-Location).Path
$probe = Join-Path $repo 'testing/probes/establish-node-access-storage/Cargo.toml'
$work = Join-Path $repo '.tmp/establish-node-access-storage-acceptance'
New-Item -ItemType Directory -Force $work | Out-Null
$env:CARGO_TARGET_DIR = Join-Path $work 'target'
cargo build --manifest-path $probe --offline --locked
if ($LASTEXITCODE -ne 0) { throw 'probe build failed' }
Push-Location $work
try {
    & (Join-Path $env:CARGO_TARGET_DIR 'debug/storage-acceptance.exe')
    if ($LASTEXITCODE -ne 0) { throw 'probe failed' }
} finally { Pop-Location }
```

成功输出 58 条 `PASS`、最终状态及 `PROBE COMPLETE`（原 52 条加 6 条目录权限/资源清理断言）。断言失败退出非零，不能只以编译完成判断通过。生成任何 secret 前，调用 app 公共 `protect_private_directory` 为新建 `data-run` 设置受保护、仅当前用户 FullControl、向子文件/目录继承的 DACL，并使用 Windows Get-Acl 独立断言。TLS 私钥再显式调用 `protect_private_file` 并断言权限。

180 秒预算只取消业务体；其资源所有者在预算之外显式关闭 SQLite pool、发送 owner stop、等待 runtime join，再验证监听端口/管道/目录锁释放，最后删除 `data-run`。stop 最多等待 20 秒，join 最多等待 25 秒；join 超时则 abort 并 await，关闭未确认时保留受限目录、非零退出并报告路径。删除失败也非零退出并报告路径，不冒充清理成功。初始化阶段尚未启动节点时的错误只需清理本轮目录。

可在相同临时工作目录将二进制参数改为 `failure` 或 `timeout`，分别在真实节点和外部数据库连接建立后注入错误或让业务体 pending、20 毫秒后触发超时。两种模式应输出 9 条权限/清理 PASS、删除 data-run，最终**预期退出 1**并给出注入错误；不能把非零码改记成产品功能 PASS。正常模式为 `normal`（默认）。DACL 检查需要 Windows PowerShell 的 Get-Acl。

OS 强制终止进程不保证执行任何清理代码；遗留材料仍受预先收紧的目录/文件权限保护。强杀后若 `data-run` 已存在，程序拒绝覆盖；只允许核实并清理本探针拥有的目录，不将其复用为新一轮安全证据。当前未实测强杀。端口先由系统分配再用于节点启动，存在短暂释放窗口；若被并行进程抢占，明确失败后重跑。

故障为审计写入前 `RAISE(ABORT)` 和审计写入后延迟外键约束（语句成功、COMMIT 返回 SQLite 787），不是磁盘满。容量数据由隔离数据库直接填充；过期用修改隔离行的 `expires_at` 模拟，不是推进真实时钟。非浏览器 Rust 客户端完成配对 claim；撤销经 owner 管道，未验证 WSS 身份/浏览器恢复/跨 SID。无需也不会读取用户现有数据库或密钥。
