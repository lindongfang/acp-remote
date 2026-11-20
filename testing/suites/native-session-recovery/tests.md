# native-session-recovery Acceptance Suite

- Source change: deliver-codex-session-loop
- Source definitions: openspec/changes/archive/2026-11-16-deliver-codex-session-loop/tests.md
- Long-term specification: openspec/specs/native-session-recovery/spec.md
- Stable IDs: CL-010、CL-011、CL-045、CL-052、CL-056、CL-057、CL-077、CL-102

共用执行条件与归档源用例保持一致，运行结果另存 `testing/runs/deliver-codex-session-loop/`。裸 `specs` 路径在本长期套件中指 `openspec/specs`。

## 需求关联

| 长期规格路径 | Requirement | Scenario | 用例 |
| --- | --- | --- | --- |
| openspec/specs/native-session-recovery/spec.md | 原生身份不依赖启动入口 | 多个入口访问同一会话 | CL-056、CL-057、CL-086 |
| openspec/specs/native-session-recovery/spec.md | 原生身份不依赖启动入口 | 不同来源中的相同短标识 | CL-056 |
| openspec/specs/native-session-recovery/spec.md | 原生身份不依赖启动入口 | 接入来源未配置 | CL-056 |
| openspec/specs/native-session-recovery/spec.md | 原生绑定与按需恢复 | 原生状态缺失 | CL-010、CL-045、STC-008 |
| openspec/specs/native-session-recovery/spec.md | 查看允许激活进程 | 查看需要加载 | CL-065、CL-085 |
| openspec/specs/native-session-recovery/spec.md | 查看允许激活进程 | 查看触发权限请求 | CL-065 |
| openspec/specs/native-session-recovery/spec.md | 释放确认与管理事实 | 关闭或管理记录失败 | CL-008、CL-011 |
| openspec/specs/native-session-recovery/spec.md | 释放确认与管理事实 | 存储失败但仍需安全关闭 | CL-011、CL-077 |
| openspec/specs/native-session-recovery/spec.md | 恢复与历史读取分离 | 无重放恢复 | CL-047、CL-106、STC-007 |
| openspec/specs/native-session-recovery/spec.md | 恢复与历史读取分离 | 已准入连接选择无重放恢复 | CL-052、STC-007、STC-016 |
| openspec/specs/native-session-recovery/spec.md | 恢复与历史读取分离 | 恢复重放旧消息 | CL-012、CL-051、STC-002、STC-009、STC-016 |
| openspec/specs/native-session-recovery/spec.md | 恢复和重放保留有效接入参数 | 已有非空或未知 Client 配置 | CL-102 |

## Cases
### CL-010 原生状态缺失

- 用例级别：L5
- 操作步骤：先有缓存及管理绑定，再让原生会话无法读取/恢复；清缓存后查看与继续输入，检查原生调用及绑定。
- 预期结果：明确原生缺失或读取失败，管理审计仍可查；不拼接命令/审计为聊天、不承诺旧全文、不静默新建或灌入平台副本。

### CL-011 释放时保存失败

- 用例级别：L5
- 操作步骤：在隔离真实节点对显式释放注入管理记录写失败及底层关闭失败，确保操作者与资源仍可验证；共享实例保留另一会话。解除故障后核验并补记，再观察容量。
- 预期结果：不再有聊天强刷前提；仍尝试安全收敛，实际关闭与记录失败分别报告，未持久协调不报完全成功/空闲或复用容量；共享会话不受强杀。

### CL-045 原生确认停止路径必须证明恢复不会重派发

- 用例级别：L5
- 操作步骤：先模型后实际节点+协议对端，给完整可靠旧工作不再活动且恢复不重派发证据，与仅新握手、查询失败、错目标、可能重派发及归属冲突对照；保留失败挂起旧队列并提交新输入，另点击继续。
- 预期结果：只有完整可靠收敛及所需恢复才解除活动阻挡；旧项仍须继续，新输入不因旧项挂起被一并冻结。未知或错配不派发、不改旧副作用未知；替身不证明真实原生恢复保证。

### CL-052 重放完成不代替前台终态或恢复授权

- 用例级别：L5
- 操作步骤：已具标准重放能力的连接显式无重放恢复，再单独重放；在活动前台、待插入输入及有效审批窗口完成读取。
- 预期结果：上下文恢复/历史同步/前台完成分别表达，不因重放解除输入阻挡或复活审批，不造 Run/自动输入。

### CL-056 显式来源与会话标识决定协调边界

- 用例级别：L5
- 操作步骤：先独立模型再实际节点+协议对端：两个 harnessId/入口/环境配置显式同 sourceId/nativeSessionId，交错连接换代与输入；对照不同来源相同短 ID、缺来源后补齐，不提供物理等价证明。比较登记身份、节点运行及连接代际职责。
- 预期结果：同来源同ID共享队列、唯一派发、加载归属及容量，跨入口不能取两个队列；不同来源不自动合并，缺来源拒绝不猜测，补齐即不以物理证明拒绝。运行/连接/请求身份不互作协议完成证据，不要求旧请求时间epoch合同。

### CL-057 真实默认环境与同来源多入口协调

- 用例级别：L6
- 操作步骤：真实 Windows 与已准入 Harness 两启动入口显式同来源同原生 ID，核对非敏感环境继承/覆盖的布尔观测；真实 Web 并发输入、重连并观察资源/名额和完整自有树。
- 预期结果：跨入口唯一协调不因路径/环境/换代重复，默认环境无额外门禁；凭据不输出，缺安全观测或真实组合如实未执行。

### CL-077 故障安全控制代际去重及恢复核验

- 用例级别：L5
- 操作步骤：受理去重/唯一派发/资源责任等必要存储失败但身份资源可验证时，取消、协议安全拒绝、显式关闭并并发重试；对照身份不可验、资源换代/重启、尝试批准/新危险派发。另仅附加审计故障对照，恢复后核验补记。
- 预期结果：安全收敛不无限等审计，仍隔离共享会话；同代际控制串行、旧控制不作用新资源，必要事实恢复协调前不恢复受影响危险派发。身份不可验远程拒绝，只保留合法OS本机关闭；不把仅附加审计故障当全面封锁，不承诺未保存结果跨重启。

### CL-102 恢复重放有效配置保真与未知来源

- 用例级别：L5
- 操作步骤：分别以非空MCP、额外工作根、已确认空集和外部参数未知目标调用恢复、重复重放及翻页；捕获真实协议请求字段。尝试由未授权来源提供配置、缺省值冒保持不变及重放中配置变更。
- 预期结果：使用可信已授权完整有效配置；空集只表示已确认空配置，省略不等于保持。未知路径说明影响并限制该路径或取得授权，不静默清空/扫描私有文件；不阻无关能力，恢复/历史/输入状态独立。
