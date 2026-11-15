# P2A / Domain公共资产合入记录

2026-10-31。此文件是主执行者对本次工具会话的事实记录，**不是原始命令流日志，也不是独立检视结论**。未另存git apply的stdout/stderr文件；不能引用不存在的独立raw日志。

- P2A生产patch ea8bac49和独立公共资产patch71395568分别在同一PowerShell命令内先`git apply --check --no-index --whitespace=error`，检查非0即退出，再`git apply --no-index --whitespace=error`。工具调用最终exit0，随后显示实际五文件摘要。未使用`--index`或`--cached`。
- Domain独立公共资产patch4c40c334使用命令局部`-c core.autocrlf=false`，同样先check、非0退出、再apply；工具调用exit0并显示实际三文件摘要。没有持久改Git配置；仍发生CRLF生成，不能声称该命令已保留LF。
- 两批实际字节与外部原文件逐一比对，先证明规范化换行后全文相等，才将**限定五文件/三文件**恢复为外部原LF字节。证据分别是`2026-10-31-p2a-patch-newlines.json`和`2026-10-31-dbr-patch-newlines.json`，保存恢复前与原文件摘要；比较失败会在任何写入前终止。没有用此操作覆盖其他模块或既有修改。
- 当前模块4cf2b6d3、开发f9cbf51a、acp声明934d3b0b、P2A公共主a4a85b41/support e733dbcb；Domain两生产源仍01e8ad73/ad3bf864，公共三文件2538c111/36292daa/3ca0debb。完整实际新组合由`2026-10-31-p2a-dbr-integration-source.json`固定（1896b236，107项），独立角色另核对实际字节、资产差异和组合执行结果。

历史101/103来源清单保持其原受测范围，不回填成107。README只有Domain已验状态链接变化。上述主执行记录不能替代原作者对测试断言完整性的复核、独立组合验收或真实会话产品验收。
