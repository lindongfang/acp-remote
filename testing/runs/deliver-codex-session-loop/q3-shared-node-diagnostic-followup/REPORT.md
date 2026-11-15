# 同Node关闭阶段观察：限定开发交付

本包仅为bc658门槛的只读阶段观察，不修关闭算法。固定b8e42的两份补丁在新副本真实check/apply均0、后像逐项相同；369输入为366组件源和3独立overlay，NodeConfig、依赖和锁不变。

首次feature库test --no-run真实通过；第一次require_escalated运行启动被自动审批服务403拒绝，明确未执行且不是unsafe判定，原服务错误另存。经Root确认，只对同require_escalated通道正常重试一次后获准，未改用default或其它工具绕过。现有Storage目标实际一次1/1（0.07秒），IPC目标实际一次1/1（0.06秒），369运行前后逐hash相同，完整源/exe/日志固定在runtime-v1。

Storage原250ms/Storage.pause_next_close/真实IPC stop/原错误断言和release保持；新增断言只核实际Storage=Deadline及对应flag。IPC保原受控任务替换/门闩/5秒、真实Task Err/原shutdown join与报告断言；新增断言只核实际OwnerIpcTask=ReturnedError及对应flag。它们只证明这两个受控原结果被观察，不验证其它十三类失败，也不复现旧auth.close红。阶段Completed/SlotDropped/ReportPublished/FinalResultSelected不代替实际任务join、物理关闭、接收方收据或首close时刻。

正常feature关闭lib检查通过，显式feature integration编译(no-run)通过，**本轮public4运行次数为0**；因此公开CURRENT getter的实际Err分支未执行，不能据存在该代码说旧失败已分类。原FIRST代码/fixture.close原样，CURRENT表示读取点当前/最后状态，不能晚值回填FIRST。

原strict1真实红只因新增被动语句使management原match触发single_match_else。为保原Result结构，单独冻结带理由、仅该match的一处lint维护提案，不按clippy改if-let。原运行版保留；维护后像只做fmt/strict/编译，原两个目标不重跑。实际后像结果及gate以本包对应原日志为准，不把静态后像冒成动态已验。

原Node TestArea::Drop忽略remove_dir_all结果，测试没有额外独占句柄探针，wrapper亦未保存测试子进程PID，因此不签物理清理完成。只读flat temp盘点按原前缀及两命令SystemTime nonce窗口匹配，候选=[]；不读取内容、不删除，不把时间窗口当独占所有权证明。本轮没有新public fixture area。

原8个await表达式、deadline、返回/AppError聚合、stop/abort/join顺序、Storage原两次pool.close和all_joined均不改。per-instance probe只持固定大小静态状态Arc，不持原资源或资格；无global/string错误解析/凭据日志/新控制端口。原full570/2、public3/4、IOUnknown及TRANSFER独立后验仍Open，Shared83/RF/RB、Native桥/NodeWSS/整体Harness未签。

当前真实观察支持这两条受控分类入口；没有新的旧失败现场支持算法修改。下一步是独立后验本包的源/版本/静态与目标证据，若以后获准真实失败现场才按FIRST与CURRENT分别分流，不循环全套撞错、不猜环境或SQLx复发、不第三次close。

维护经068b门槛后实际安装：Node前像2667efc9到后像674317ee，删除唯一四行理由属性可逐字还原。维护后fmt=0、全targets/allfeatures strict=0、正常feature-off lib check=0、feature lib及integration编译(no-run)均0；运行次数为0。原两个绿仍只归runtime-v1。恢复时CIM命令行盘点不可用；Get-Process按精确自有target路径过滤为0，无进程被终止，不能扩称完整命令行或物理资源证明。
