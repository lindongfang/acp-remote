# NA012 SMB远程管道子断言独立范围复核

日期：2026-10-30。结论：**PASS（PIPE_REJECT_REMOTE_CLIENTS实效子断言，可从NA012缺口中移除）；NA012整例仍PARTIAL**。本机SMB loopback是本次明确执行环境，不宣称另一机器或另一SID。当前Steps没有要求另一台机器，不将其新增为门禁。

## 判据与来源

已重新读取当前tests.md:115–119：要求owner/另一Windows用户五命令、伪造SID、远程命名管道、模拟失败及独占；预期异用户/远程/校验失败拒绝。没有约定必须由另一台物理机器发起。合同§6要求生产named pipe拒绝remote clients；R17生产create_private_pipe继续设置reject_remote_clients(true)。

读取预算tester报告**末尾新平台段**及自有probe脚本、原始JSON/清理结果。报告读取时SHA256 `FE59AB5D5CC7AD6E7223E548E23E40C1EC5D0AF662D6544DB81D7AEDCF9EEABC`。本reviewer只读检视独立tester已执行证据，没有再次启动SMB fixture、节点或修改系统策略。

| 证据 | SHA256 |
|---|---|
| testing/probes/establish-node-access-budget-20261030/smb-pipe-control.ps1 | C6202EC6FD378FB6AD29421CB700BA1EA80E046029DC224E549A47DE0D6608DF |
| evidence/smb-environment.json | 6124CD5E90D15C011E59F4B097FCFD45E87E6FF00FBE71DB3B4C5CB8EBFB3764 |
| evidence/smb-pipe-control.json | 494B51D1C89713AC6B84091AA848F247F2CFC15394F989FB93C7D8C520DAA9B1 |
| evidence/smb-cleanup.json | 5136D1335F12CB69FBE976DDDFB330E3667A90002491DB6958DF5B646A63C7A1 |

三个evidence文件位于 `.tmp/budget-acceptance-20261030/evidence/`。实际产品为tester自有复制R17普通binary，SHA256 `38AA9D02F334E3C81FDC900ED4F8943BB78339650981BC0046A609381D03E6C2`，与本reviewer此前核对一致。

## 实际对照为何有效

1. 真实owner只读核验LanmanServer运行、445可连、IPC$存在；沙箱早先CIM查询失败未冒充系统不可用。没有启动服务、放宽防火墙、创建账户或新凭据。
2. 自有随机唯一Win32管道采用owner-only DACL，经GetAccessControl确认唯一Allow SID为当前owner；mode=0明确没有PIPE_REJECT_REMOTE_CLIENTS。客户端通过 `\\127.0.0.1\pipe\唯一名`，同owner现有登录态，实际双向读回81/82字节，34ms成功。
3. 正控制成功后，对**同一127.0.0.1 UNC host路径**的自有产品管道尝试open，12ms得到UnauthorizedAccessException，HRESULT -2147024891（0x80070005 ACCESS_DENIED）。此前产品本地owner status=ready；不是产品未启动或只有一个SMB连接失败。
4. 正控制和产品都以owner-only权限为前提，而同owner本地产品可用。结合生产remote-reject设置与上述同UNC成功/拒绝对照，足以支持Windows远程管道拒绝实际生效，排除早期“SMB本身不可用，两个对象都连不上”的假阳性。

这验证的是UNC/SMB远程管道访问路径，不是源IP是否位于另一物理主机；loopback不使它退化成普通 `\\.\pipe` 本地访问。当前用例要求可以据此满足，**无需另加另一机器门禁**。没有将同owner的远程拒绝解释成SID不匹配拒绝或五命令跨SID都已过。

## 清理和剩余范围

脚本finally释放remote/client/server/取消源/安全描述符；普通产品owner stop并等待退出。原始结果passed=true、fixtureDisposed=true、productNodeExited=true、positivePipeStillListed=false、remainingListenerCount=0；独立清理JSON进一步记录productPipeAbsent=true、nodeLockExclusivelyOpened=true、ownedListenerCount=0。没有清理用户30447或日常资源。

最新汇总应将NA012的“远程命名管道实效未验证”收敛为已完成上述子断言；平台旧报告脚注14及旧SMB失败是历史诊断，不能继续覆盖后来的有效正控制。跨SID资格是正交边界：另一真实SID的device pair/daemon stop等仍缺的原定五命令断言继续保留，不能由此代验。伪造SID、R8模拟失败、同目录第二实例等分别按既有负责字节与实际证据记录，不新增跨积或无关重跑。

本次仅新增本范围报告，不改原tester报告/产品/清单/plan/tasks，不提交推送归档；未宣布NA012整例或整个change通过。
