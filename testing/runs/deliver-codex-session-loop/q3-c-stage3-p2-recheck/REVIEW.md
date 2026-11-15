# C stage3 两项 P2：有限静态复查

结论：C-STAGE2-REV001、REV002 均为 Fixed-static。对固定 stage3 的本次修复未发现新的稳定阻断，允许继续有限核心/F精确重基准备。原A–E编码门不回填；本结论不签完整独立验收、不授schema9生产安装、D接入或Root合入。

独立逐SHA核827成员无漂移，795源与cad55b91… manifest相同；增量1732170f…为7路径，5个行为/测试路径与2个模块顺序格式路径。读取实际补丁及调用链，未仅采用作者REPORT。

## REV001

management.rs:151 的 same_registration 逐项比原request、actor、register generation、action、registered_at、digest；management.rs:424 settled_fact 拒绝Registered，保留原Command::from_row闭合形态和原fact/Profile/Binding SQL校验。committed额外匹配解密intent导出的harness/source/profile/revision、target/workspace；创建要求原cipher字节相同，绑定要求launch/canonical同预期。Profile::load继续检查payload_format、cipher摘要、原creator及关联；原Command::fact继续核创建/选择关系。最终写事务的已结算分支在management.rs:717调用该方法，仍Registered整行原before比较保留。

合法settled不强求本次admission/time。reconcile复用同关联/effect校验，同时保留本次原提交generation/time的更强要求；没有以新SELECT改写原COMMIT知识或移除原receipt职责。因此原漏验分支的稳定缺陷静态关闭。

## REV002

catalog_sql.rs:40 enter 把原lock_owned等待包在同一个 d.run(scope.while_open(...))；management.rs:532 start传真实actor原scope，node_configuration.rs:63 selected传同registry原scope。关闭seal能取消等待，待者原operation lease由原调用栈Drop释放。获得gate后的retained失败观察仍原样返回，未被scope包装吞掉。recovery_guard函数与stage2逐字节相同，保留seal后原授权cleanup；未新建deadline/owner/gate。新增GateWaitObservation仅cfg测试只读原锁等待计数，无放行或ready接口。

实际开发回归在持原gate时轮流轮询5种kind，确认真实等待后seal，250ms内NodeStopping；原gate仍持有、retained仍原operation、无SQL槽，释放持gate方lease后原close_connections可完成。这是共享seam回归，不是完整真实授权register/apply/query/双port执行。

## 已验证与未验证

- 读取真实日志：修前机械提取/观察副本17绿2红；修后21绿0红。新增rejected登记字段回归实际调用settled_fact，结果键回归是纯对象matcher；尚无完整committed SQL mutation，不能写成schema9业务验收。
- all-targets/all-features check退出0、fmt退出0。日志和退出码已冻结；本review没有重跑或增加开发测试。
- 独立解析strict两份原JSON，多重集按level/code/message/primary file和原文比较，381→382、3增2移均dead_code；strict退出101，未通过。核心仍未生产装配，不能用pub/allow/假consumer抹除Open。
- 正式原34+补12、真实DPAPI、完整settled mutation、schema9生产迁移、默认D/F均0。两ID仅Fixed-static，后续独立回归由F精确资产及同源实际结果关闭动态边界。

Root、旧候选、tester、原冻结报告均未修改；本包仅新review文书及独立核对收据。
