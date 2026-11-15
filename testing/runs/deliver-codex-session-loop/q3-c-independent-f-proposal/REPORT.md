# 独立 tester：C stage2 F 最小挂载准备

已完成本轮可执行前准备：保留原795固定源，在另一proposed-source提出9路径精确测试挂载补丁；独立原34与补12原件、S09已批准三字段overlay、两条新增回归定义及门后命令均已固定。原34、补12、3个拟运行入口、两条新回归实际执行全部0，没有用开发18绿代签。

已验证：candidate826、设计113、tester123、精度定义25、后续S09 reviewer3、stage2 reviewer3，共1093成员逐字节/摘要核同无失配；源795及提案9after保持。rustfmt只做语法/格式解析，退出0；git apply --check与whitespace=error --check各退出0；PowerShell脚本只解析无运行。原34原件/原12原件未改，组合差异准确为V3-S09.steps、V3-S09.requires、WP4C-012.supplement，其余11补充未改。

实际代码自读定位到原CatalogSqlFence、CatalogOperation/原receipt与owner、原管理两事务和Node可信构造；未读取作者活动目录。补丁没有新依赖、pool、owner或恢复实现，没有生产schema安装、生产factory getter或产品公开注入口。fixture仅自有测试库固定三表layout、头仍8；真实0/1–8→9仍未实现/未验。首资源尾拟在每条入口实际shutdown后独立检查DB/WAL/SHM与原owner Weak，首结果不重试；当前没有资源尾执行记录。

新的stage2静态review指出REV001/REV002未闭环，原候选不能签通过。本人未修改产品修复。新的C-REG-REV001/002定义独立列在REGRESSION-DEFINITIONS.json，保原46定义。当前RUN-AFTER-GATE.ps1显式拒绝这个旧候选的正式执行；必须等修复后新固定源、精确重放/重冻本挂载以及独立core/F门。不能先运行再补批准。

完整F仍有未实现的精确资产：真实VM、同连接TEMP、迁移故障点、完整S09受控mutation/settled像、原owner被动lease计数、同步crypto wrapper与Stop、真实双proof、缺receipt分支等。故这里只交最小挂载提案，不请求完整F/完整34+12或产品通过。D/global audit/长native ID/Root组合/真实Harness均未验；audit warning仍Unavailable，全局30天/100000没有由core fixture冒充。

交Root/reviewer：先读F-GATE-REQUEST.md，再按PROPOSAL-MANIFEST.json审F-MINIMAL-PROPOSAL.patch及3个非空测试正文；修复源到达后本tester新包重基线，再审安装门。当前新包只供静态检视与回归定义审核。

边界：本轮写入仅本新目录；Root、旧候选、作者workspace、Git索引、用户数据库/原生数据/密钥均未操作；网络/npm audit/依赖安装/提交/推送/发布均0。
