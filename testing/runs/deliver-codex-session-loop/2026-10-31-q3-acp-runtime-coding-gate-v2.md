# 公共ACP runtime revision2编码门禁

2026-10-31。**限定编码PASS。** ACP-PLAN001–003已在同版合同关闭，最后所需的独立同版验收定义现已冻结并复核，可在已授权隔离workspace按下列边界实施。没有新的用户产品取舍或剩余具体编码前阻断。此为设计/可验收性门禁，不是实现或功能PASS。

## 精确绑定

| 输入 | SHA256 |
| --- | --- |
| revision2主合同 | `64cc19beaeb9fd96e66500aeb9b0048519b2038705b2717fc82a5ccfb924a0cb` |
| revision2限制/迁移 | `8bb5d345f34952d47f08f50571d1ebc85cfcbefc7b05abc0ac3aaa4d4bd3e3cf` |
| freeze-v2 | `3dd8a31b225e00127380d7634432890e60a8c1c34def0d7ec8d91005060f8940` |
| tester acceptance-definition-v2 | `f27bfc6213d6b15421dfecc9d9e2f67f64b5ce73409460818c5e83a79e7df4f7` |
| tester definition-manifest-v2 | `077a994a299a54ec9f13fe976861c4e4b1fab2c5315b871b64a02f59c980b93c` |
| reviewer design-review-v2 | `1838f31c4b7fceadca149a9e7f4121ec2bb3de77d1e24771816a71f9afeeae18` |

上述文件均亲读/逐hash核验；freeze引用11份、tester manifest引用6份全部匹配。前报告保留当时“等待最终同版定义”的原文，本附录仅补足该条件；v1文档与旧报告不覆盖。

## 同版独立定义覆盖裁决

- 继承RT17/CFG10/IO27/MIG5及CF8共67行，新增RA8/RC8/DL10/API5共31行，合98条定义。定义行数不等于测试数，不写为已执行通过。
- RA01–08覆盖Reservation、encode错误/副本、EnqueueError退还原物/直接Drop、scope/frame/cell错误优先及他槽不变、setup/reply零释放、initialize一次尝试和等待方取消。与ACP-PLAN001的唯一责任规则一致。
- RC01–08覆盖真实poll_read cut资格、先收后完整写不能补合法性、read-window按LF批次和generation、同批state/response/update次序、多LF未拆帧Stop排空、关闭无新grant/IO、先Received再Err一次后None、并发/取消等待关闭。启动立即EOF/IO Err和首写Err仍须真正登记两handle、join及Weak收敛。与ACP-PLAN002一致。
- DL01–10/API01–05覆盖期限全部写状态、不next仍由owner推进、pre-At有限held cut有效匹配/无匹配两分支、等At不续期限、两轴事实不撤销；精确私有getter、迭代、错误/对象返回和public re-export。公开无法精确控制的短缝/等时/极值/OOM明确静态或未执行，不伪称动态命中。与ACP-PLAN003一致。
- 原33主函数加P1实际导入3函数，合36旧执行函数另列；三个ordering例保全部状态断言，伪造旧攻击改以正对照编译成功及正确负例错误/完整API静态盘点证明不可构造。原wire坏shape、跨scope动态断言继续。实际候选逐hunk与编译报错仍须后验，不能删除旧oracle或添fake品牌。

## 实施范围和后验责任

许可ACP model/strategy/transport及必要reader/writer/state/setup/P1/P2 raw适配、公开导出、开发测试和必要说明；必须交真实可操作公共客户端与受监督IO、自有协议child实际管道，不能只traits/CAS。每poll外部围栏、raw全链去旧正文门槛、完整配置不默清、零/partial/full传输轴、早response、receipt/Stop/关闭所有权均依同版原文实现。

不增依赖、不改raw_json基础/锁文件、SQL/DDL/Node/WSS/管理认证。保留控制额度只能在已冻结原位置；P2/ViewWindows残余预算保持非必经且明确未交付，不签长键全部旧投影或完整无预算共享视图。

产品冻结后须独立同源静态/真实动态验收、36旧函数逐项映射、适用fmt/clippy/check/build及根组合检查。若真实实现需要改变API或合同，另revision重核，不能用本PASS授权未定义语义。真实Harness/Job树、必要派发账本及actual R3 owner接真实IO、持久审批、原生Run因果、Node/WSS/Web和完整WP3均另gate。
