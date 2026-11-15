# 管理输入 revision3 独立复查

2026-10-31，reviewer `/root/baseline_reviewer`，MGT-CODE-003原问题复核人。只读外部冻结候选，未修改产品/独立预期，未应用patch或读取用户身份/数据库、启动真实Harness。

当前结论：**003的窄修静态通过；等待同版独立原29及两个003反例执行证据后关闭。** 原001/002已关闭，不重新打开；真实产品接线与当前主区schema4集成仍未签。

## 固定版本

- 新根 `../deliver-codex-session-loop-management-revision3/`；独立读取HANDOFF、manifest、完整delta及应用实际源码。
- 独立计算manifest SHA256 `0e116d1f4a191d336298be79137921007beac25743317931016b3b9b1419dcae`、delta `0d628e843583334be5f1f9d9e89dd26b33a76bdba53fe5e7197c3bd22ace9734`、完整patch `29bf3f49ad34883728efe05d34ba2ad0141d630781d453acbfaa7ad8a17aa492`，均与交接相符。
- 78个新workspace文件逐一匹配；同78个旧revision2来源逐一匹配其冻结摘要，旧版未被改写。仅4文件变化：应用模块`d16eb1cdb8cdc97b14382dac91569a0c0137575b7d1ecd7e5277f5e171db53c2`、内层命令错误`5c38e20dd4f22c196c673a539ccc58c74b52b9bc2723316ee9523f13baae3456`及两开发测试。SQL生产事务、DDL、锁文件无变更。
- 合同8686d779及独立22M定义009f48b0的行为要求不变。原版与revision2的反证、报告、原29及新增003预期保留，不把新绿覆盖旧失败归属。

## 分类与安全边界复查

- 临时对象期限现在返回有语义的`InputExpired`，文案明确temporary input；append/finish/digest/收紧/borrow/submit对象检查与请求签名失效分开。已撤销/消费等保留InvalidInput，未将对象TTL改成可以续期。
- dispatch保存独立signature_deadline、input_deadline和transport_budget；实际等待继续取最早者，但不从合并预算反推签名已过期。签名保守单调截止到达时通过新可信时间重验实际expiresAt，只有实际签名截止到达才Expired/Command expired；操作/阶段预算及取消为Cancelled，单独对象到期为InputExpired，可靠零发送结果NotSent→failed。
- body借用失败也经签名实证才可转Expired，避免仅改最后一个status分支而遗漏对象错误。真实签名过期的原分支仍记录Expired，而非把所有期限统一降成Cancelled。
- 新分类需要的最后同步时间采样之后，仍重查共享fault并在首次write_once前拒绝；不存在新await把原002缺口重新引入。原001共享cell期限只缩不续/监督清理保留。可能已交字节后的失败仍unknown，不用新分类将其改成可靠零发送。
- 新增错误不携带正文/JWS/SQL；未新增授权构造器、permit复制、查询重造票或危险ready恢复入口。没有借这次修复接入真实writer/认证/资源发行者。

开发37/37、21/21模型、8/8签名及fmt/clippy只作输入，不代签独立验收。新增开发断言针对对象TTL/10.3秒最终采样/真实exp及生命周期语义；相应旧开发预期从Expired改成InputExpired属于本次修复，不改变tester原独立反例。

## 交付与集成范围

独立执行需在同一0e116/d16e新快照运行原29与两个003反例，并保留真exp对照、writer零调用/正文清理、原001/002回归及源码/测试前后摘要。之后只可关闭外部应用/SQLite子范围，不外推为真实Harness或L5/L6。

外部候选仍基于schema3；主区schema4是另一个已经单独放行、尚待独立运行的布局阶段。最终管理patch须逐hunk核对Storage版本/迁移、模块声明、views/ACP及新增错误，保留主区schema4和现有修改，运行实际集成新快照回归。外部3绿不能代签主区4。最终reconciliation/故障安全控制/恢复ready、审批owner/WSS/真实进程树及Web审计继续未验；未运行npm audit，未提交/推送。

## MGT-CODE-003同版原反例复验后关闭

2026-10-31追加：原提出者已实际读取request_tester的`management-independent-revision3.log`及`management-independent-revision3-exit.json`：原29项与两个003新增分支合计31通过、0失败/忽略、exit0，实际15.78秒。原001/002断言亦在这31项中再次通过，没有因为003修复降预期。

独立资产逐字复制关系记录在`management-test-unchanged-revision3.json`，reviewer再计算当前测试资产：主M源`f36d97eda8e326a6dbf7bd29c7c3a07ed0e8b08709f432470e6a2d202d1438e9`、原因分类源`124890c8423b9537d6927ce3622c26d5b99bdf892bd98170b9f66d263f0ed4aa`，都与revision2原失败资产相同。reviewer另将当前独立副本对冻结78源逐一计算，仅SQL源多3行cfg(test)include；已实际diff确认无生产逻辑差异，其他77份包括d16e/5c38新生产源均相符。

两原反例现在既保持签名仍可验，又零writer/正文清零：10.3秒操作预算耗尽记failed而非expired，对象TTL到达不再返回描述产品请求失效的Expired。真实exp路径在原矩阵保留，SQL期限拒绝及水位规则未被取消。

**MGT-CODE-003正式关闭（revision3静态及同版独立原失败断言通过）。** 原revision2的2项失败、31项组成和旧冻结均保留；本次关闭不改写旧版本为通过。001/002继续保持关闭，没有残余由这三项产生的合入阻断。外部候选完整M子范围交付、剩余独立检查/报告及主区schema4实际集成检查继续分别记录，不把本次ID关闭扩大为任务整体或L5/L6完成。

## 外部候选限定交付结论

已继续实际读取正式`management-revision3-acceptance.md`（SHA256 `c72759c667b369e03dda6fec254aef38c62cf2a05feabe5412fd961c0dd08b6f`）、终态来源清单、31项/21模型原始日志、11类型负向编译日志及分类、lib clippy日志和最终exit汇总。冻结原78源终态全相符；副本77源匹配、唯一SQL test-only include逐行已核，独立资产仍同原预期。31管理/21模型exit0，11类型编译错误来自预期E0277/E0423、exit101是预期拒绝，lib clippy exit0；没有把负向crate算作正常通过单测。

**外部revision3的约定已执行L3/L4管理输入子范围限定通过，可交主执行按实际主区差异集成。** 保留原独立M报告的细分not-run：随机/分配故障与COMMIT后owner关联失败未逐一独立注入，默认100000 audit自然累积不可达，缩限SQL/checked算术不冒充完整CL-083。完整清理、真实来源/实际writer/WSS、故障安全控制/reconciliation/ready和L5/L6仍未交付。

该外部结论不证明schema4组合通过。合入必须保留根目录版本4/迁移和其他已审修改，随后在新组合快照运行独立管理31、布局迁移、公开模型及适用完整检查；无需额外用户授权即可进行已请求范围内的逐hunk集成，但不能把外部3证据标成主区4实际运行。
