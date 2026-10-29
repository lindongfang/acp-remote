# 人工验收交接（未签署）

本文件只整理现有 NA-033/NA-034 的执行材料，不修改用例预期，不代表人工或独立 Agent 已验收通过。当前源码清单为 `2026-10-29-snapshot.sha256`，71 项，SHA-256 为 `6B1D981658CE1ACA98D390B3C56650908924B0A7486A55A87836DEEC74200BB9`；配置清单为 `2026-10-29-config-snapshot.sha256`，22 项，SHA-256 为 `13AD9DDE14AFC5235217CBA748B8EAB830162F56625FF4183BAB386172DFB3F1`。二者共同固定 93 个文件，实际签署前应再次核对清单与独立报告。用户已确认产品设计文档由其本人继续调整；后续变化应保留，并重新评估受影响范围和配置摘要。

## NA-033：安全与依赖

执行人安排：用户已明确回复“我来执行人工验收”。这只确认执行人，尚未提交实际结果或签署，不计作 PASS。

执行依据：`openspec/changes/establish-node-access/tests.md` 的 NA-033；合同 §1–7；`app/DEPENDENCIES.md`；Rust/Web 锁文件；独立 tester 和 reviewer 的最新报告。

实际人员需逐项核对：TLS 和本机终端的引导信任、JWS 用途与重放边界、节点身份替换拒绝、Windows 数据目录/管道 DACL、客户端 token 校验与 revert、DPAPI 当前用户范围、不同用户拒绝证据、秘密处理及依赖维护/许可/漏洞信息。相同 Windows 用户下的恶意软件不属于隔离承诺。

已知缺口：Rust 公告库扫描尚未完成；跨用户与部分故障矩阵需以最新独立报告为准。缺口未关闭时不得签为 PASS。

| 签署项 | 待填写 |
| --- | --- |
| 实际执行人员与日期 | 未填写 |
| 核对的清单及额外配置摘要 | 未填写 |
| 实际检查和证据路径 | 未填写 |
| 结论及未解决问题 ID | 未填写；当前未签署 |

## NA-034：配对与错误提示

执行人安排：用户执行；结果待填写，不将执行意向当作已完成。

### 当前机器可运行的人工操作入口

已准备专用配置 `.tmp/establish-node-access-human-20261029/acp-remote.toml`，使用独立 data 目录和端口 `30447`，复用现有测试用受信证书；未启动节点。二进制 SHA-256 已核对为 `90150CAFE9AE40EA6DCE77F436BA8A2241BBFE92C84E108C815452CE70A0D2B1`。此入口依赖测试目录中的证书仍有效、Tailscale 地址可用；不得通过忽略证书错误完成验收。

临时受信私钥已核对并收紧为真实 Windows 所有者 jack 的受保护 DACL，权限修复前后文件内容摘要相同。此前继承权限过宽，当前修复不代表此前也具备 owner-only 保护；人工安全检视需保留该历史事实。未修改任何正常部署密钥。

在真实 Windows 所有者 jack 的 PowerShell 中，从仓库根目录执行：

```powershell
$nodeExe = 'D:/Project/acp-remote/.tmp/establish-node-access-tailscale-20261029/build-target/debug/acp-remote.exe'
$manualConfig = 'D:/Project/acp-remote/.tmp/establish-node-access-human-20261029/acp-remote.toml'
& $nodeExe --config $manualConfig daemon start
```

保持该终端运行，在第二个 PowerShell 中设置相同两项变量，再执行：

```powershell
& $nodeExe --config $manualConfig device pair
```

浏览器打开 `https://faraday.tail74842e.ts.net:30447`，输入第二个终端显示的短码，使用可识别的测试名称，核对网页与终端的节点和客户端指纹，再分别试验显式批准、默认回车拒绝及 Ctrl-C 取消（每种情况重新创建配对）。不要将短码或私钥写入本报告。

完成后在第二个终端运行以下命令，核对停止结果和第一个终端退出：

```powershell
& $nodeExe --config $manualConfig daemon stop
```

密钥丢失、节点替换、超时和批准后取消仍按原 NA-034 用例逐项执行；不能因正常配对通过就签署整例通过。只对专用测试 profile 清理站点数据；不操作正常身份资料。

执行依据：原用例 NA-034。使用专用测试数据目录、浏览器 profile 和测试客户端，按合同中的 CLI 启动/配对命令操作；从本机终端输出的受信 HTTPS origin 打开页面。

实际人员需核对以下交互：终端与 Web 的客户端名称和完整指纹一致；显式批准、默认回车拒绝、Ctrl-C 取消、超时、批准后取消、密钥丢失和节点身份替换。确认提示能区分待确认、已批准、已认证和失败；批准后取消明确指向撤销，不显示移动端已可用。

现有开发截图只能帮助定位界面，不能代替人员操作。只清理本次专用节点和 profile，不清除正常用户身份、数据库或浏览器资料。

| 签署项 | 待填写 |
| --- | --- |
| 实际执行人员与日期 | 未填写 |
| 节点、浏览器与版本摘要 | 未填写 |
| 各交互实际结果和证据路径 | 未填写 |
| 结论及未解决问题 ID | 未填写；当前未签署 |
