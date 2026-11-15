# 准备期基线与最终组合的两项差异复核

2026-10-31。原始legacy-move-comparison.json保留29项直接一致、2项SHA不一致，不把不一致改写成字节相同。已逐hunk读取完整差异：

- session_storage/product_commands.rs：原491e9651...到当前e4d1f446...，只在cfg(test)区将mod tests和independent_acceptance声明按rustfmt排序重排；全部生产业务实现/SQL不变。独立外部正常依赖已编译成功，最终31项实际运行使用当前声明。
- management_independent.rs：原72315f4c...到当前f9359628...，为原管理tester的clippy整理：新增有理由的窄lint允许、局部变量重命名、布尔值等价i64转换、Option::or变等价惰性or_else、持有gate变量命名。完整diff未见输入/预期/故障步骤削弱或新增ignore。该文件不由本tester编辑，实际31项必须在当前组合重跑，不引用准备期源或之前31绿代签。

上述与8个隔离源码的机械白名单比对分开登记。旧legacy.rs业务体、旧SQL两个文件、旧history子进程断言及原其他旧测试、schema/schema4DDL、新应用输入/签名/领域模型和依赖锁分别保留自己的逐字/摘要比较。任一进一步变化需要重新核对，不允许用本复核泛化接受未知差异。
