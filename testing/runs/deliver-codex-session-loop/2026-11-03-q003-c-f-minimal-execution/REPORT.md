# 最小 F 独立实跑

2026-11-03：获门798源真实编译通过；真实 --list 恰3入口。沙箱外逐项结果：F01通过、F02失败、F03通过（2/3），首红保留，无改预期或F02重跑。日志和逐轴结果见 RESULTS.json。

F01沙箱内首次尝试先在Node身份DPAPI启动失败（os error 2），未达catalog正文和尾；按隔离环境权限规则获准沙箱外重跑，真实DPAPI才通过。共4次测试尝试，2通过2失败；不能只报告最后绿色。

F02在测试叶147行断言 GateWaitPending 存在失败。第二个公开apply future只poll一次即检查gate事件；源码 current authority await 位于repository.start/fence.enter之前，因此这条红尚不能判产品gate取消失效，也不能计同gate等待已证。首红没有重试。

沙箱外三入口均到达原shutdown/owner Weak断言与首次物理尾：DB首次独占成功，WAL/SHM首次检查不存在。F03原shutdown的预期错误也被断言保留。没有扩写成整个目录首次删除能力。

精确源前后798成员无变化、Cargo.lock不变。原stage3、最小proposal与gate先逐成员核同；安装时catalog_sql.rs/mod.rs仅CRLF差异单独存档后固定为获审LF后像。执行源只9路径、6改3新；扩展F和schema9未装。原34+补12均未完整执行/未签父项。
