//! 启动描述：profile → 可执行的 `LaunchSpec`。
//!
//! 这一层是「凭据注入边界」的落点（`docs/MODULE_ARCHITECTURE.md` §4.5、`docs/SECURITY_DESIGN.md`
//! §12.2/§13.1）：
//!
//! - 参数以**数组**传递，永不经过 shell 拼接；
//! - profile 只来自 [`LocalConfigStore`]，绝不读启动配置文件；
//! - 环境变量 = 凭据解析端口给出的绑定（端口本身已保证是 `env_allowlist ∩ 绑定` 的交集）加必要的进程环境；
//! - 引用失效或 keystore 不可用一律**失败关闭**，不静默跳过变量后继续启动；
//! - Node/Device 密钥不进入这里——本 crate 根本不持有它们，且子进程环境会先被清空。

use acp_core::model::{AgentId, AgentProfile};
use acp_core::ports::{CredentialResolver, LocalConfigStore};

use crate::error::HostError;

/// 交给进程监督层的启动描述。`supervisor` 不读 profile、不解析凭据、不知道白名单。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchSpec {
    /// 可执行文件（或命令名）。
    pub program: String,
    /// 参数数组。
    pub args: Vec<String>,
    /// **已解析完成**的环境变量（键值对）。
    pub env: Vec<(String, String)>,
}

/// 必要的进程环境：没有它们，Agent 常常连自己的工具都找不到。
///
/// 这不是白名单的放松：它们是固定的、与凭据无关的宿主环境项，且只在 profile 没有自己提供同名变量时注入。
const NECESSARY_ENV: &[&str] = &["PATH", "HOME", "USERPROFILE", "SystemRoot", "TEMP", "TMP"];

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

fn validate_env_name(name: &str) -> Result<(), HostError> {
    if name.is_empty() || name.contains('=') || name.contains('\0') {
        return Err(HostError::InvalidEnvName);
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
}
