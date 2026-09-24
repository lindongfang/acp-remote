//! 系统熵源端口实现（`getrandom`）。
//!
//! `identity-auth` 只定义 [`identity_auth::EntropySource`]（同步、只有「不可用」一种失败）；
//! 平台取随机数在这里，因此状态机不需要知道 `getrandom` 存在。
//!
//! 失败即失败关闭：`getrandom` 的失败**不**回退到时间戳/`std::collections::hash_map::RandomState`
//! 之类的弱来源（`docs/SECURITY_DESIGN.md` §13.1）。

use identity_auth::{EntropyError, EntropySource};

/// 操作系统 CSPRNG。
#[derive(Debug, Default, Clone, Copy)]
pub struct OsEntropy;

impl OsEntropy {
    /// 构造。
    pub const fn new() -> Self {
        Self
    }
}

impl EntropySource for OsEntropy {
    fn fill(&self, out: &mut [u8]) -> Result<(), EntropyError> {
        getrandom::fill(out).map_err(|_| EntropyError::Unavailable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn os_entropy_fills_and_does_not_repeat() {
        let entropy = OsEntropy::new();
        let mut first = [0u8; 32];
        let mut second = [0u8; 32];
        entropy.fill(&mut first).expect("系统熵源必须可用");
        entropy.fill(&mut second).expect("系统熵源必须可用");
        assert_ne!(first, second, "两次取值不得相同");
        assert!(first.iter().any(|byte| *byte != 0));
        assert!(second.iter().any(|byte| *byte != 0));
    }
}
