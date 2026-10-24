# omp 工具通知发送顺序修复

`18.12.6-tool-start-delivery.patch` 针对上游 `can1357/oh-my-pi` 的 v18.12.6，提交 `45c35e40c46166dc726ccc52f5d27f40f2449f99`。

问题位于源头：开始事件入内部队列后，工具执行包装层能立即发审批请求，抢在该事件被消费之前。补丁在真实 Agent 的调度路径等待对应开始事件交付后才进入工具包装层；取消会解除等待。它不重排 ACP Remote 的持久事件，也不调整前端排序。

在上游检出中执行：

```powershell
git apply --check <18.12.6-tool-start-delivery.patch 的绝对路径>
git apply <18.12.6-tool-start-delivery.patch 的绝对路径>
bun test packages/agent/test/tool-start-delivery.test.ts packages/agent/test/agent.test.ts
bun test packages/coding-agent/test/acp-tool-order-wire.test.ts packages/coding-agent/test/acp-agent.test.ts
```

完整构建前继续执行上游要求的检查及资源生成流程。不要直接把空的 stats 页面归档占位编译成发布版本。

已执行的回归、真实 ACP 探针、构建与本机部署详情见 [验证记录](../../docs/evidence/omp-acp-tool-order.md)。本目录仅保存源补丁，不登记编译二进制。
