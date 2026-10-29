# 独立方案与用例检视

状态：当前文档快照的方案/用例/计划/任务检视通过；不是实施授权、兼容性通过或产品验收。真实Codex/ACP版本、协议协商及原生恢复能力是adapter编码前门禁，未满足不得进入适配器实现；上游节点接入须真实交付并重基准。没有产品运行证据不是本轮文档缺陷。

审查读取本change proposal/specs/design/tests/plan/tasks及重设计、AGENTS、OpenSpec规则；不继承coder对话，未修改产品或用例。所有spec场景在tests有精确Requirement/Scenario引用，CL-003/004覆盖保存500ms或64KiB先满足、结束即时保存、缓存阈值、事务/审批/派发竞态，逐场景CL-005–029补正常及失败结果，CL-030要求真实Windows/Codex/Web闭环，CL-031人工检视不允许Agent代签。未验证与不支持明确分开，没有把研究或fixture认作真实能力。

设计保留命令与Run、协议载荷与领域、持久化与Web投影边界；保持前台互斥、取消不等于结束、资源运行状态不等于轮次结果、原生恢复不灌平台历史及重放来源隔离。上游授权/撤销逐次复核。plan明确WP1–6、重叠路径串行合入、固定版本和独立验收，tasks有各WP真实checkbox引用和完成判据。未存在产品检查被列为待建立入口，未虚报可运行。

本次未发现未闭环设计问题。实际性能、500ms触发、固定向量/协议合同、Windows进程树、依赖安全、真实审批/取消/恢复及所有产品用例仍待实现与独立验收；届时必须按固定版本重新检视，不能据本报告签产品通过。

## 文件快照

| 文件 | SHA-256 |
| --- | --- |
| design.md | `03E49960C1B37E8AA2CD0B4EBC090CBBD3905766C7B4B1F66BC24B96D72A26C7` |
| plan.md | `C258314B11983A3AEDA98CF7A038268B816A7F55E5C4AD29FE44DD6274A705A3` |
| proposal.md | `5855FF5AD4BFB71E786102D72B68E0A09E2074E08807390F44BD9C532569E78D` |
| tasks.md | `241B04DCCF3E63F42ADCA00B271529FC55A5AC8D978529C14D41B259D332FFBD` |
| tests.md | `976EAD2D13D8482683BC6CC5EEA5212F1D484F7EF692E2CBF6E614436219145C` |
| specs/harness-management/spec.md | `2FE4F312AEC5E232F06A0F9581DF6ADFC8A3423A1CC5403BC3CB4309050104C1` |
| specs/management-audit/spec.md | `66DD2DD8A304389F9A3B1C50FF80641A89AE85F1314BF882A2366981211D5715` |
| specs/native-session-recovery/spec.md | `8FEFC8E4CCD951644A16FA56429F0C62CA0821BBBF0FAF07D986BA6B77363C2B` |
| specs/permission-approvals/spec.md | `E61FB389DFBAD686F883FEA77E2D679AFEEADBE540D029E91F6632FACCFC7601` |
| specs/session-execution/spec.md | `61067A30D797F0AD3789226CAA3B1491299AA4F45400F9230221298823ED00A6` |
| specs/structured-history/spec.md | `67F05D151AAFE3D069BE1E962529D991F45B71940BBA2378CBDF1589067FD410` |
| specs/workspace-registration/spec.md | `4CF4E9E297EFEB4A5C56FC89B706B3FD46FD866849708FE032EFBCD78C1E9DE9` |

