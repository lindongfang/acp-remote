//! 领域模型：`docs/CORE_PORTS_AND_STORAGE.md` §2 的错误类型与 §3 的**全部**值对象与不变量。
//!
//! 该文档是唯一权威；本层只做三件事：把 §3 的形状表达成 Rust 类型、把 §3/§7 的不变量变成构造校验或领域
//! 方法、把 §2 的错误类型放在一处供 `ports`/`use_cases`/`broker` 共用（避免循环依赖）。
//!
//! 约定（§3 的实现口径）：
//!
//! - **ID 与标量**：newtype + 构造校验；非法输入返回具名 [`InvalidValue`]（`From<InvalidValue> for
//!   PortError` 把它收敛成 `PortError::InvalidRequest`）。没有 `From<String>`，也不提供可变字段。
//! - **带不变量的记录**：字段私有 + `try_new(..)` + 只读访问器；纯由已验证类型组合、且没有跨字段规则的
//!   聚合（`PendingEvent`/`CommittedEvent`/`ClientCommand` 等）用公有字段，跨字段规则另给 `validate()`。
//! - **状态机**：`Session`/`Turn` 的状态字段私有，只能经 `transition(..)` 前进；终止态不可离开
//!   （`SessionState::Failed`/`Closed`、`TurnState::Completed`/`Failed`/`Cancelled`）。
//! - **不读系统时间**：所有需要时间的入口都把 `Timestamp` 作为参数传入（§2）；本层不提供 `now()`。
//! - **不 panic**：正常路径没有 `unwrap`/`expect`/`panic`，错误都是返回值。
//!
//! 本模块不依赖 Tokio、Axum、SQLite、WebSocket、子进程、ACP DTO、任何 wire protocol，也不使用
//! `serde_json::Value`：`EventPayload.view` 等开放 JSON 位置用本层自己的不透明文本类型 [`ViewJson`]。

/// 生成一个 `String` 支撑、带构造校验的 newtype（含 `Display`/`AsRef`/`FromStr`/`TryFrom`）。
macro_rules! newtype {
    ($(#[$meta:meta])* $name:ident, $check:path) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(String);

        impl $name {
            /// 构造：非法输入返回具名 [`InvalidValue`]。
            pub fn new(text: &str) -> Result<Self, InvalidValue> {
                ($check)(text)?;
                Ok(Self(text.to_owned()))
            }

            /// 底层文本。
            pub fn as_str(&self) -> &str {
                &self.0
            }

            /// 取出底层文本。
            pub fn into_string(self) -> String {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }

        impl FromStr for $name {
            type Err = InvalidValue;

            fn from_str(text: &str) -> Result<Self, Self::Err> {
                Self::new(text)
            }
        }

        impl TryFrom<&str> for $name {
            type Error = InvalidValue;

            fn try_from(text: &str) -> Result<Self, Self::Error> {
                Self::new(text)
            }
        }

        impl TryFrom<String> for $name {
            type Error = InvalidValue;

            fn try_from(text: String) -> Result<Self, Self::Error> {
                Self::new(&text)
            }
        }
    };
}

/// 生成一个封闭 token 枚举：`ALL`、`as_str()`（wire 与数据库共用的稳定 token）、`Display`、`FromStr`。
macro_rules! token_enum {
    ($(#[$meta:meta])* $name:ident { $($variant:ident => $token:literal),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum $name {
            $($variant),+
        }

        impl $name {
            /// 全部取值，顺序即声明顺序（供存储层与测试穷举）。
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];

            /// 稳定 token（wire 与数据库共用，不得随语言本地化改变）。
            pub const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $token),+
                }
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl FromStr for $name {
            type Err = InvalidValue;

            fn from_str(text: &str) -> Result<Self, Self::Err> {
                match text {
                    $($token => Ok(Self::$variant),)+
                    _ => Err(InvalidValue::Field),
                }
            }
        }
    };
}

mod backend;
mod config;
mod elicitation;
mod error;
mod event;
mod export;
mod identity;
mod ids;
mod json;
mod scalars;
mod session;

pub use backend::*;
pub use config::*;
pub use elicitation::*;
pub use error::*;
pub use event::*;
pub use export::*;
pub use identity::*;
pub use ids::*;
pub use json::{JsonValueText, ViewJson};
/// 供 crate 内其他模块（broker 组装交互事件时读 payload 的 `interactionId`）使用的最小 JSON 读取面。
pub(crate) use json::{
    MemberValue, decode_json_string, encode_json_string, insert_string_member_front,
    object_members, top_level_member,
};
pub use scalars::*;
pub use session::*;

#[cfg(test)]
mod tests;
