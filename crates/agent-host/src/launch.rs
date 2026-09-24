//! 启动描述：profile → 可执行的 `LaunchSpec`。
//!
//! 这一层是「凭据注入边界」的落点（`docs/MODULE_ARCHITECTURE.md` §4.5、`docs/SECURITY_DESIGN.md`
//! §12.2/§13.1）：
//!
//! - 参数以**数组**传递，永不经过 shell 拼接；
//! - profile 只来自 [`LocalConfigStore`]，绝不读启动配置文件；
//! - 环境变量 = 凭据解析端口给出的绑定（端口本身已保证是 `env_allowlist ∩ 绑定` 的交集）加必要的进程环境；
//! - 引用失效、keystore 不可用、或解析结果与「`env_allowlist ∩ 绑定` 的期望集合」不一致（漏给变量）
//!   一律**失败关闭**，不静默跳过变量后继续启动；
//! - Node/Device 密钥不进入这里——本 crate 根本不持有它们，子进程环境会先被清空，且 `ACPR_` 前缀的
//!   注入名在这里被**静态拒绝**（不依赖运行时的环境清空）。

use acp_core::model::{AgentId, AgentProfile};
use acp_core::ports::{CredentialResolver, LocalConfigStore};

use crate::error::HostError;

/// 交给进程监督层的启动描述。`supervisor` 不读 profile、不解析凭据、不知道白名单。
#[derive(Clone, PartialEq, Eq)]
pub struct LaunchSpec {
    /// 可执行文件（或命令名）。
    pub program: String,
    /// 参数数组。
    pub args: Vec<String>,
    /// **已解析完成**的环境变量（键值对）。值是**明文凭据**，只能进子进程环境。
    pub env: Vec<(String, String)>,
}

/// `Debug` 只输出变量名与数量、参数的数量与长度：`env` 的值是明文凭据、`args` 里也可能带凭据，
/// 一旦被打印就进了日志、错误或测试快照，而 `docs/SECURITY_DESIGN.md` §14.1 的「默认日志允许字段」
/// 只允许变量名与数量级、且明确「不记录完整参数中的 secret」（与 `docs/CORE_PORTS_AND_STORAGE.md`
/// §11.6 的日志口径一致）。
impl std::fmt::Debug for LaunchSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let names: Vec<&str> = self.env.iter().map(|(name, _)| name.as_str()).collect();
        // 参数只留「数量 + 每个参数的长度」：长度足以定位「参数形状不对」，又不泄漏正文。
        let arg_lengths: Vec<usize> = self.args.iter().map(String::len).collect();
        f.debug_struct("LaunchSpec")
            .field("program", &self.program)
            .field("arg_count", &self.args.len())
            .field("arg_lengths", &arg_lengths)
            .field("env_names", &names)
            .field("env_count", &names.len())
            .finish()
    }
}

/// 必要的进程环境：没有它们，Agent 常常连自己的工具都找不到。
///
/// 这不是白名单的放松：它们是固定的、与凭据无关的宿主环境项，且只在 profile 没有自己提供同名变量时注入。
const NECESSARY_ENV: &[&str] = &["PATH", "HOME", "USERPROFILE", "SystemRoot", "TEMP", "TMP"];

/// ACP Remote 自己的注入名前缀（Node/Device 密钥、token 等）与 core 保留的 daemon 前缀：子进程环境里
/// 出现它们即视为配置错误（`ACPR_*` 是本 crate 的约定，`ACP_REMOTE_*` 与 `core::model` 的
/// `is_reserved_env_name` 对齐，用于拦住旁路 core 校验的来源）。按大小写不敏感比较。
const RESERVED_ENV_PREFIXES: &[&str] = &["ACPR_", "ACP_REMOTE_"];

/// 解析一个 profile 的启动描述。
pub async fn resolve_launch(
    config: &dyn LocalConfigStore,
    credentials: &dyn CredentialResolver,
    agent: &AgentId,
) -> Result<(AgentProfile, LaunchSpec), HostError> {
    let profile = config
        .profile(agent)
        .await
        .map_err(|error| HostError::SpawnFailed {
            detail: format!("读取 profile 失败：{error}"),
        })?
        .ok_or(HostError::UnknownProfile)?;

    // 凭据解析在启动之前完成；失败关闭，绝不「少注入几个变量继续跑」。
    let resolved = credentials
        .resolve_env(&profile)
        .await
        .map_err(|_| HostError::CredentialUnavailable)?;

    let mut env: Vec<(String, String)> = Vec::with_capacity(resolved.len() + NECESSARY_ENV.len());
    for (name, value) in resolved {
        validate_env_name(&name)?;
        // 白名单是**上限**而不是提示：端口已经保证交集，这里再核一次（纵深防御）。
        // 端口实现出错时宁可失败关闭，也不把未列入白名单的变量注入子进程。
        if !profile
            .env_allowlist()
            .iter()
            .any(|allowed| allowed == &name)
        {
            return Err(HostError::EnvNotAllowed { name });
        }
        if env.iter().any(|(existing, _)| existing == &name) {
            // 端口契约不允许重复；真出现时宁可失败也不猜哪个生效。
            return Err(HostError::SpawnFailed {
                detail: "凭据解析返回了重复的环境变量名".to_owned(),
            });
        }
        env.push((name, value.expose_secret().to_owned()));
    }

    // 期望集合：profile 里**已绑定且在白名单内**的变量必须全部出现在解析结果中。
    // 端口契约（`core::ports::CredentialResolver`）要求返回值恰为这个交集，且「不得静默跳过该变量后
    // 继续启动」；这里按期望集合复核，端口少给一个变量也会失败关闭，而不是带着半个环境启动。
    // 必须在必要进程环境注入**之前**校对：必要的进程环境不得冒充某个绑定变量。
    for name in profile
        .env()
        .iter()
        .map(|binding| binding.name())
        .filter(|name| {
            profile
                .env_allowlist()
                .iter()
                .any(|allowed| allowed == name)
        })
    {
        if !env.iter().any(|(existing, _)| existing == name) {
            return Err(HostError::CredentialUnavailable);
        }
    }

    for name in NECESSARY_ENV {
        if env.iter().any(|(existing, _)| existing == name) {
            continue;
        }
        if let Ok(value) = std::env::var(name) {
            env.push(((*name).to_owned(), value));
        }
    }

    let spec = LaunchSpec {
        program: profile.command().to_owned(),
        args: profile.args().to_vec(),
        env,
    };
    Ok((profile, spec))
}

/// 环境变量名必须合法，且不得使用 ACP Remote 自己的注入前缀。
///
/// 保留前缀是**静态拒绝**：注入集合可能来自配置，只要 profile 或凭据端口给出了 `ACPR_*` 或 core 保留的
/// `ACP_REMOTE_*` 名字，就在启动前失败关闭（而不是把「不得注入节点/设备密钥」全部押在运行时的
/// `env_clear()` 上）。core 的 `AgentProfile` 已在模型边界拒绝 `ACP_REMOTE_*`（`is_reserved_env_name`），
/// 这里是与它对齐的纵深防御：**旁路 core 校验的自定义凭据/配置来源也拦得住。**
fn validate_env_name(name: &str) -> Result<(), HostError> {
    if name.is_empty() || name.contains('=') || name.contains('\0') {
        return Err(HostError::InvalidEnvName);
    }
    let upper = name.to_ascii_uppercase();
    if RESERVED_ENV_PREFIXES
        .iter()
        .any(|prefix| upper.starts_with(prefix))
    {
        return Err(HostError::EnvNotAllowed {
            name: name.to_owned(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_env_names() {
        assert!(validate_env_name("GOOD_NAME").is_ok());
        assert!(matches!(
            validate_env_name("BAD=NAME"),
            Err(HostError::InvalidEnvName)
        ));
        assert!(matches!(
            validate_env_name(""),
            Err(HostError::InvalidEnvName)
        ));
    }

    #[test]
    fn rejects_acpr_prefixed_env_names() {
        // 节点/设备密钥与 daemon 保留名的注入名一律静态拒绝（不依赖 `env_clear`）。
        for name in [
            "ACPR_NODE_KEY",
            "ACPR_DEVICE_KEY",
            "ACPR_token",
            "ACPR_",
            "ACP_REMOTE_CONFIG",
            "acp_remote_config",
        ] {
            assert!(
                matches!(
                    validate_env_name(name),
                    Err(HostError::EnvNotAllowed { .. })
                ),
                "{name} 必须被拒绝"
            );
        }
        // 非保留名仍按普通环境变量处理。
        assert!(validate_env_name("TOKEN").is_ok());
        assert!(validate_env_name("ACP_REMOTE").is_ok());
    }

    #[test]
    fn debug_never_prints_credential_values_or_argument_bodies() {
        let spec = LaunchSpec {
            program: "fake-agent".to_owned(),
            args: vec![
                "--scenario".to_owned(),
                "--token".to_owned(),
                "sk-secret-argument-do-not-log".to_owned(),
            ],
            env: vec![
                (
                    "FAKE_TOKEN".to_owned(),
                    "secret-value-do-not-log".to_owned(),
                ),
                ("PATH".to_owned(), "C:\\".to_owned()),
            ],
        };
        let text = format!("{spec:?}");
        assert!(
            !text.contains("secret-value-do-not-log"),
            "Debug 不得输出凭据值：{text}"
        );
        assert!(
            !text.contains("sk-secret-argument-do-not-log"),
            "Debug 不得输出参数正文（参数里可能带凭据）：{text}"
        );
        assert!(text.contains("FAKE_TOKEN"), "变量名仍应可见：{text}");
        assert!(text.contains("env_count"), "数量仍应可见：{text}");
        assert!(text.contains("arg_count"), "参数数量仍应可见：{text}");
        assert!(text.contains("arg_lengths"), "参数长度仍应可见：{text}");
    }
}
