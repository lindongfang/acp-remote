# SCHEMA-R003工程合同修正判定

2026-10-31，独立reviewer `/root/baseline_reviewer`，只读项目。

结论：**将“拒绝时文件清单完全不变”修正为准确的SQLite只读完整视图/持久事实不变边界，属于已授权实现细节与验收依据修正，无需再次请求用户产品决策。** 该判断不关闭SCHEMA-R003，不批准未修改的测试直接通过，也不预签具体修复代码。

## 判定依据

已重新读取本change七份spec、总体设计的权威及迁移边界、长期node-access/client-trust，以及归档access-contract的身份初始化/数据目录所有权与未知schema规则。它们要求未知/损坏存储拒绝、已提交身份/信任/撤销/管理及旧正文事实保留、不自动删除或重置，不单独承诺SQLite拒绝路径连临时侧文件也绝不创建。

现有DataDirectory已经将node.sqlite3-wal和node.sqlite3-shm列为owner私有管理文件；Windows目录DACL本就包含子项继承。没有增加访问者或数据来源权威。原CL-004子断言C的零sidecar文件清单要求，是本轮工程验收添加的更强实现假设，不是用户逐项确认的产品或安全要求。

SQLite官方说明只读WAL连接可创建侧文件；immutable跳过锁和变更检测，文件实际仍变化时可能错读。因而只为了零文件创建使用immutable并不当然更安全，完整已提交WAL仍必须参与数据库权威。[SQLite WAL规则](https://www.sqlite.org/wal.html)，[immutable规则](https://www.sqlite.org/uri.html)

主执行者报告固定DB+WAL副本的反证：主DB为schema2、未checkpoint WAL已提交user_version77，immutable候选却返回Ok。reviewer已读取新增夹具，其设计确实可区分主DB与WAL来源；此判定未把尚未交接原始日志的转述冒称reviewer亲自执行。候选应回退，不能为通过原12例而忽略这一新反证。

## 可接受的工程修订范围

1. 仍以标准SQLite read_only连接读取完整视图，包含已提交但未checkpoint的WAL。未知/矛盾/残缺及缺身份情形仍拒绝，不调用应用DDL/迁移/PRAGMA版本写入，不虚报新Storage可用。
2. 主DB与原已存在WAL的字节和提交事实不能改写、截断、删除或恢复。不可清理原WAL后只看主DB，也不能换DELETE fixture代签生产WAL模式。
3. 只允许SQLite按其只读WAL机制创建/维护非权威SHM索引；原先不存在的WAL仅可出现零字节辅助文件，必须实测确认。此例外不扩展为任意新文件或应用管理写入。
4. 所有侧文件仍处于已验证owner私有目录，继承同一私有访问边界；目录锁覆盖生产预检/打开流程，Storage调用前提明确，不新增网络、身份或数据库旁路。不能把“SQLite临时索引”借口扩为其他用户可读的文件。
5. 原7/10、最终11/12及immutable反证完整保留原版本归属。先由主执行明确修§4A、独立tester说明验收修订依据并维护定义，再由reviewer复核新定义及同版实际结果。旧失败不能被改写为通过。

## 关闭所需证据

标准read_only真实读取重要提交只存在于WAL的固定副本，准确拒绝UnknownSchema(77)；前后主DB及原WAL逐字节摘要一致。分别覆盖原无sidecar和原有提交WAL场景，清楚记录新增文件的名称/长度、SHM权限继承与私有目录状态。没有删除、恢复、截断或checkpoint原持久事实；身份哨兵不变。两版迁移、真实COMMIT回滚及R002修复既有用例继续通过。

如果将来需要允许改变含提交事实的WAL、拓宽ACL、替换身份或原生数据，则超出上述工程修正，应按AGENTS.md的产品/安全边界规则处理。本次判断不授权这些变化。

当前SCHEMA-R003保持open，阶段交付仍受阻，等待新合同/独立定义及实际证据。
