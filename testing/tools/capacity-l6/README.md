# 真实 Harness 容量验收启动器

本目录只负责显式输入检查、固定测试程序枚举、调用 tester 的真实 Rust 入口并映射结构化结果。`Validate`、`List` 成功均输出 `acceptancePassed: false`，不能证明 CP 或 4.1 完成。真实 Node、认证、50 棵树、1800 秒、原生任务、审批、子关系、DOM 和 SQL 故障由各 Rust 入口与独立浏览器证据负责；场景文件存在不能证明任何真实事实已发生。

下表是与 tester 对齐的四个约定入口名称，不是四项已经完整实现或通过的声明。当前 Rust 候选的未实现分支由 `missingBranches` 明列并整体 blocked；本次启动器开发未运行真实认证、真实 50 棵树或 1800 秒验收。

需要 Windows PowerShell **7.4+**（`pwsh`），不安装依赖。调用者提供已构建的 lib-test `.exe` 和已核 SHA256；脚本不编译、不下载、不运行登录，也不从默认身份取得认证。

## 专用输入

复制 `config.example.json` 到自己拥有的位置，修改显式路径。示例无认证、无真实场景，不能用于签收。

- `format=1`、`defaultLimit=50`、`idleSeconds=1800` 固定。降低容量或缩短计时会被拒绝。
- `runRoot` 必须为本项目 `.tmp/capacity-request-tester` 下一个**尚不存在**的独占子目录。四入口在同一根下各自拥有 Node/Job/数据。脚本不创建或递归删除目录。
- `launchProfile` 是现有产品 `format:1` 的原 JSON：`launch.executable_utf16le_base64`、`arguments_utf16le_base64`、`environment` 和 `recipes`。不是明文 `schemaVersion` 配置。只读取指定文件及摘要，不复制档案内容。
- `dedicatedHome`、`dedicatedCodexHome` 指向调用者已经准备且已认证的专用目录；只验证目录元数据，不探测认证是否可用、不读取/复制认证文件。不得使用默认 HOME、USERPROFILE、CODEX_HOME 或默认 `.codex`，不得与 `runRoot` 重叠。认证目录与原生历史在所有结束路径保留。
- 本启动器接受的安全子集是现成 `node.exe` 加**一个绝对安装好的 `.js/.mjs/.cjs` adapter 入口参数**。不支持 npm/npx、下载器、额外参数、MCP、额外工作目录或 recipe `extra`。这些限制只约束本次测试启动器，不改变产品 LaunchProfile 规范。
- profile 必须显式列出 `HOME`、`USERPROFILE`、`CODEX_HOME`、`APPDATA`、`LOCALAPPDATA`、`TEMP`、`TMP`。前两者必须等于 `dedicatedHome`，CODEX_HOME 等于 `dedicatedCodexHome`；后四者必须是已存在的 `dedicatedHome` 子目录。只额外允许 `CODEX_PATH`、`PATH`、`SystemRoot`、`WINDIR`、`ComSpec`、`PATHEXT`，后四者不可改写当前 OS 值。拒绝 token/key 环境项。
- 子测试进程清空继承环境，仅传必要 OS 环境、已校验的 profile 环境及 `ACP_CAPACITY_L6_*` 运行合同变量。`Run` 在替换子进程环境前，从原 HOME、USERPROFILE、CODEX_HOME 及 USERPROFILE 下的 `.codex` 收集默认目录路径，作为 `ACP_CAPACITY_L6_DEFAULT_IDENTITY_DIRECTORIES` 的非空 JSON 字符串数组；只读取环境字符串，不读取目录中的文件。Rust 使用这些原路径拒绝误用默认身份，不能拿已替换的子进程 HOME 当原默认路径。`List` 不传配置、原身份目录事实或运行摘要。显式程序按实际文件 SHA256 固定，Node/Codex 的文件版本从文件元数据读取，未执行 `--version`。adapter 只记录入口摘要，不假称完整依赖树供应链已验证。
- `allowModelRequests` 缺省为 false；true 必须由调用者在配置中明确设置。它只是请求授权开关，不证明场景已执行。
- `scenarioFiles` 只接受 `cp011`、`cp023`、`cp026`、`cp032` 键，值为绝对已存在文件路径。可以缺项，但 Rust 必须按实际缺口输出 blocked。runner 只 hash，不打印或复制正文，不猜测 prompt 能触发任务/审批/child。
- `webControl` 可选，若给出须包含布尔 `enabled` 与正整数 `timeoutSeconds`。Rust 在本轮私有目录提供 info/control；配对码不进入永久报告。浏览器 receipt 必须匹配原 Node/source/native/operation，由独立操作者实际生成；缺失不能用自动预检补签。

配置字段及 JSON 重复键、未知键、类型错误均拒绝。认证、配置、场景和可清理目录不允许通过 junction/symlink 绕过路径边界；已安装程序可经过安装器链接，但每次调用前后重新核内容摘要。不会枚举用户认证目录内容。

## 命令

以下占位符必须替换为实际绝对路径/核定摘要。`RepositoryRoot` 是实际仓库根。

```powershell
pwsh -NoProfile -File .\Invoke-CapacityL6.ps1 -Mode Validate `
  -Config 'D:\OwnedL6\config.json' -RepositoryRoot 'D:\Project\acp-remote'

pwsh -NoProfile -File .\Invoke-CapacityL6.ps1 -Mode List `
  -Config 'D:\OwnedL6\config.json' -RepositoryRoot 'D:\Project\acp-remote' `
  -TestExecutable 'D:\OwnedBuild\acp_remote-actual.exe' `
  -TestExecutableSha256 '<64位实际小写SHA256>'

# 此命令才执行真实 L6，可能长时等待并产生已明确授权的模型请求。
pwsh -NoProfile -File .\Invoke-CapacityL6.ps1 -Mode Run -Cases cp011 `
  -Config 'D:\OwnedL6\config.json' -RepositoryRoot 'D:\Project\acp-remote' `
  -TestExecutable 'D:\OwnedBuild\acp_remote-actual.exe' `
  -TestExecutableSha256 '<64位实际小写SHA256>' `
  -SourceManifest 'D:\Project\acp-remote\.tmp\OwnedBuild\source-manifest.json'
```

`Run` 默认按顺序执行四项；可用 `-Cases cp011,cp023`（交互 PowerShell 调用）选择子集。结果仅覆盖实际选中入口。一次运行结束后不能复用已有 `runRoot` 来覆盖证据；另建新根。4.1 的入口准备要求程序和全部必需步骤已可执行；4.2 的实际验收结果另按独立矩阵取证，二者均不能由本启动器的预检代替。

`-CaseMaxSeconds` 默认 10800（3小时），接受 1801–86400 秒，限制单例**整个进程**运行时间，不修改产品 1800 秒政策。调用者应给多段真实观察及浏览器操作留足预算；预算不足会失败，不会缩短等待签通过。超时后停止本脚本启动的进程树，最多30秒等待其退出、最多5秒等待输出管道结束，本例 failed 且 `cleanupConfirmed:false`，不执行下一例。取消执行也进入同一有界清理路径；原 Rust 的 Job/端口实际清理事实仍须单独确认。

| 键 | ignored exact Rust 入口 |
| --- | --- |
| cp011 | `node::tests::r3_source_wss_development::capacity_l6_cp011_real_fifty_and_thirty` |
| cp023 | `node::tests::r3_source_wss_development::capacity_l6_cp023_real_unopened_root_children` |
| cp026 | `node::tests::r3_source_wss_development::capacity_l6_cp026_real_replay_clock_and_pressure` |
| cp032 | `node::tests::r3_source_wss_development::capacity_l6_cp032_real_known_work_and_finish_fault` |

`Run` 额外要求 `-SourceManifest`：仓库内的固定构建清单，含 `testExecutableSha256` 和非空 `files: [{path, sha256}]`。清单必须绑定实际测试程序；路径不可重复，摘要为64位小写SHA256。清单的完整性和实际构建来源仍须独立检视，不能靠填写清单证明构建正确。runner 为执行生成唯一 `runId`，将程序、清单、配置和分支矩阵摘要交给 Rust，并在执行后记录原退出码及结果摘要。原 `execution-manifest.json` 只可新建，不能覆盖。

每项先核 `--list --ignored` 中的 exact 名，再用 `--exact --ignored --test-threads=1` 调用。枚举有 30 秒上限，不传 `--nocapture`。运行的原始 stdout/stderr 并行排空且不归档，防止故障输出携带原生正文；输出只含固定状态、摘要和 exitCode。Rust 的 `<runRoot>/<cp>/<cp>.result.json` 保留在原拥有目录，按 `status` 与进程状态共同判断：

- 新执行使用 `format:2`、`kind:"capacity_l6_execution"`。所有节点分支与资源清理完成后，状态仍为 `pending_independent_review`、`fullCpPassed:false`，仅 `independent_review` 留待确认；libtest 此时 exit0，runner exit2、`acceptancePassed:false`。它表示执行已完成、页面证据等待独立审查，不等于 CP 通过。
- 旧 `format:1` 的 `passed` 仍校验分支、默认策略及清理合同，但即使全部有效也映射为 `blocked`。旧格式不能绕过独立审查；须重新获得 format2 执行结果。
- `blocked` 必须配非零退出，runner exit2，绝不转成通过。
- 缺文件、无效结果、状态/退出码不一致或 failed，runner exit1。
- `Validate`/`List` 的 exit0 仅说明该阶段完成。

每次执行前后验证配置/profile/场景/程序 hash；修改已验证输入会停止。结果只从本次 `<runRoot>/<cp>/<cp>.result.json` 读取，不接受配置中的任意证据路径或链接。脚本不会自动重试测试，不会复用陈旧结果。公共异常只输出固定类别，不输出异常原文、JSON片段或原生响应。

## 独立审查后汇合结果

`Complete-CapacityL6.ps1` 读取原始执行、同节点页面证据和独立报告，生成单独的 `reviewed-aggregate.json`。它不修改 Rust 结果、执行清单、页面抓取或原报告。没有显式 `-ConfirmIndependentReview` 时只返回待确认，也不创建汇总文件；该开关必须由实际独立审查流程使用。JSON 中的角色名称与命令行开关均不是身份认证，也不能代替实际浏览器操作和独立判断。

独立报告保存在原 `runRoot` 内，所有引用使用该根下的相对路径与实际 SHA256。`assessment.json` 结构如下；占位符必须来自同一次真实执行与实际审查：

```json
{
  "format": 1,
  "kind": "capacity_l6_independent_assessment",
  "runId": "<原execution-manifest中的runId>",
  "executionManifestSha256": "<原清单摘要>",
  "cases": [{
    "case": "cp026",
    "executionSha256": "<原Rust结果摘要>",
    "branch": "same_node_real_harness_web",
    "decision": "approved",
    "reportFile": "independent-review/report.md",
    "reportSha256": "<实际独立报告摘要>",
    "phases": [{
      "phaseId": "<原页面阶段ID>",
      "summarySha256": "<原summary摘要>",
      "captureSha256": "<原capture摘要>",
      "actionsSha256": "<原actions摘要>",
      "captureSourceSha256": "<原captureSource摘要>"
    }]
  }]
}
```

汇合检查要求：请求的 CP 集合与原执行、独立报告完全相同；每个 CP 的非页面分支均实际通过，页面分支只处于 `pending_review`；分支 ID 与固定矩阵完全一致。全部页面阶段和清理证据必须绑定原执行的 `nodeId/sourceId/url`，阶段不可重复，浏览器身份已撤销，DOM 是非空对象且有实际操作记录。程序、配置、构建清单及矩阵摘要须一致；原资源诊断须确认 join、必要管理、端口关闭和 Job 活动进程为0。报告不能补签缺失节点分支或未完成清理。

```powershell
# 仅在独立审查完成后由该流程执行；默认不带开关仅验证并返回待确认。
pwsh -NoProfile -File .\Complete-CapacityL6.ps1 `
  -RunRoot 'D:\Project\acp-remote\.tmp\capacity-request-tester\owned-run' `
  -Cases cp026 -Assessment 'independent-review/assessment.json' `
  -AssessmentSha256 '<实际报告索引摘要>' -ConfirmIndependentReview
```

成功仅新建一次汇总文件，exit0；待独立确认 exit2；任何缺证、拒绝、输入变化或已有汇总均 exit1。接受部分 CP 时，汇总只覆盖这些 CP，不能据此签整个 change。原生正文、配对码和认证目录仍不得复制进永久证据。

## 取消和清理

正常 Node/adapter 任务、Job、端口由 Rust 用例所有者按合同关闭，必须以其真实 cleanup 结果为证。脚本在取消/异常中只停止自己启动的测试进程树并等待退出；没有 cleanup 结果就不声称清理通过，也不声称 CP 通过。调用者强杀整个 PowerShell 宿主的情况仍须检查测试程序和 Rust Job 的实际状态。

脚本从不删除专用认证 HOME/CODEX_HOME 或原生历史，也不递归清理 runRoot。仅本次 Rust 所有者可按既定合同清自己的 Node数据、临时文件和 Job。保留原目录中的无敏感结构化证据；`info.json`、配对码、配置全文、原生正文不能归档。每次枚举和执行前后也核对 test executable 摘要，不能让同路径换程序沿用旧结果。

## 开发验证

`Test-CapacityL6Runner.ps1` 在显式 evidence 根下新建惰性配置测试目录，使用不执行的 adapter 文件，只验证规则及结果映射。无认证、无模型调用、无真实 L6。脚本保留该专属目录供审查，不替用户删除。

```powershell
pwsh -NoProfile -File .\Test-CapacityL6Runner.ps1 `
  -RepositoryRoot 'D:\Project\acp-remote' `
  -EvidenceRoot 'D:\Project\acp-remote\.tmp\capacity-request-runtime\evidence'
```

`Test-CapacityL6Deadline.ps1` 接受上一步新建的惰性 `config.json`，用专属 PowerShell 枚举 helper 及其子进程测试共同的30秒超时路径；实际核两个原进程消失，不启动 Node/adapter，不更改真实政策。该开发检查仅证明启动器的进程所有权与终止路径。

`Test-CapacityL6Review.ps1 -EvidenceRoot <专属目录>` 检查汇合工具的拒绝、原证据保真、同源绑定和只新建一次的规则，并以无害 PowerShell 子进程核对实际传入的运行摘要。测试数据明确标记 `INERT`；测试成功始终输出 `acceptancePassed:false`，不代表真实 Harness、浏览器或任何 CP 已通过。

`Test-CapacityL6Identity.ps1 -EvidenceRoot <专属目录>` 使用无害 PowerShell 子进程，核对原默认身份路径与显式专属环境分别传递，以及枚举时不传运行身份事实。它不读取认证、不启动Node或Harness。Rust入口要求原默认目录事实，直接运行ignored测试但未经过启动器准备这些事实时会拒绝；认证目录本身可以位于项目外，不要求搬入tester目录。

场景字段、各 CP 必需 phase、正常命令及占位符合同见 [SCENARIOS.md](SCENARIOS.md)。该说明区分入口实现、动态验证和真实 Harness/独立 Web 签收。
