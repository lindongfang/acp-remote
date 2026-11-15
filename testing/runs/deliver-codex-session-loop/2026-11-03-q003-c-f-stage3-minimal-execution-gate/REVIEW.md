# C 最小 F 精确 stage3 重基：独立执行门

Pass-limited-execution-gate。允许独立 tester 在新 execution-source 从固定 stage3 795源重建，核同基线后，只安装 patch 0062ddf80751a43995d88ab1a2a881df365528f3826dd823ce9388ad1fe5864b，作类型检查及原三个 F 入口独立运行。此门不授 Root 安装，不授扩展 patch，不签编译、SQL、真实 DPAPI 或测试已经通过。

基线：candidate-stage3 freeze 2965dd57d361802afbd9c18bc3bb90fec1099755527c098eac19ba4bf69b1853 /795源，manifest cad55b913fbb59683a798a29627ebd2d51e1f028dc8123c8692730d43afc913f。
输入：configuration-independent-assets-v2/rebased-f-stage3 freeze 08aee6e58687406d6bec910ade6f2c4408631db5adac7271ebf63f0f51604289，803是包成员数，后像实际798源（6改3新）；不是产品803组合候选。

本 reviewer 逐成员核803全同；9路径前像/后像均匹配manifest；stage3其余789源逐SHA保留。只读 git apply --check --whitespace=error 及后像反向 check 均exit0；没有apply、编译或测试。原旧最小 F gate c7b0f018… 的机制/三入口限制继续适用。

关键重基合格：serial_wait 只在原 d.run(scope.while_open(...)) 的 lock future位置；GateWaitObservation未丢，原scope封闭及同deadline仍负责取消；原获取后retained失败判断保留。recovery_guard完整函数块与stage3逐字相同。management的同链same_registration、settled_fact、Registered完整before比较及reconcile代际/时间重验，除三处cfg(test)观察外未改。其余旧F观察/失报/原收据机制无扩围，不替换业务SQL或原close。

独立执行限定：node::tests::configuration_independent_acceptance 下的 c_f01_real_dpapi_register_only_then_same_cipher_apply、c_f02_same_gate_pending_is_observed_then_original_result、c_f03_original_close_report_loss_keeps_gate_until_original_recovery。先 --no-run，真实 --list 核恰好三个命中，再逐 --exact 运行；使用新自有data/target和 --locked --offline。固定execution-source安装后应798源，前后清单记录相同。原runner的throw不改；可在新执行包建立绑定本门的明确runner。

F01仅真实DPAPI/登记仅存储/同cipher移动/重复单结果；F02仅同gate Pending后第二调用短读原结果，不能签两最终写事务；F03仅原实际ACK后失报、重建仍封闭、原receipt recovery，self关系不能冒foreign/missing矩阵。每条保留原shutdown、实际owner Weak、首次DB/WAL/SHM独占检查和实际失败，不扩写成整个目录首次删除已证。

产品正式34+12、C-REG-REV001/002完整正文、schema9生产迁移、D、Root/C803组合、strict仍未签；测试style四叶新候选未混入本795基线。额外18路径/14入口/51向量另审，不因本门获准。