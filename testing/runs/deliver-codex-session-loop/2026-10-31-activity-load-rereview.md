# WP2 / 2.1 加载代际补充复核

2026-10-31，独立reviewer `/root/baseline_reviewer`，只读项目。

结论：**限定静态复核通过，原“同连接重建目标时局部序号可能复用”的装配限制已在模型上提供解决方式。** 真实加载所有者仍须为每次实际加载分配不复用LoadEpoch；类型本身不证明运行时已经正确生成和传递代际。

实际摘要核对：

| 文件 | SHA-256 |
| --- | --- |
| app/src/domain/native.rs | 8d01cef25c7ba12ffd2aff94cc78fc8bfbe41d604feaf2312f13fe47cadf9abd |
| app/src/domain/activity/tests.rs | bacabf34418a74545551751910ddd0318e563706ef57f2fad5c55c75aee7169c |
| app/src/domain/activity.rs | 4ce09688e51216fb9bd6f1ebac7f43e67152bb569240c4bdf66669f15d9d8b32 |
| app/src/domain/mod.rs | b376908d82df340dbf8f106242b193f7d30b8284b7d30584cf4161875884032c |

ResourceEpoch新增独立LoadEpoch，WorkToken和CancelScope既有完整epoch相等性校验自动覆盖该维度，不需改变activity逻辑。新增回归在相同目标、运行和连接代际下，仅改变LoadEpoch并重新建立对象；旧work得到StaleEpoch，新work仍保留。这与实施合同§2的加载代际约束一致，没有增加ACP原生执行ID要求。

本reviewer未自行执行测试。补充收到后已实际读取独立tester的`activity-acceptance.md`、`activity-source.json`和`activity-probe/test.log`：逐文件快照与上表一致，最终日志9个公开API测试实际通过、0失败，含同连接新LoadEpoch拒绝旧token。此独立证据支持任务2.1纯L3子范围；doc-tests为0不计覆盖。没有以开发自评或只有报告路径代替原始证据。

静态检视与上述独立模型证据均无阶段阻断。真实来源发行、停止/恢复保证、加载代际实际分配、SQL/WSS/进程能力仍未验证；完整L5/L6不得标通过。原报告其余证据边界继续有效。
