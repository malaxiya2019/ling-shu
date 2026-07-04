use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use uuid::Uuid;

/// LsId — 全局唯一标识符 (UUID v7 with time ordering).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LsId(Uuid);

impl LsId {
    /// 生成一个新的 LsId (UUID v7).
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }

    /// 从 `uuid::Uuid` 构建.
    pub const fn from_uuid(u: Uuid) -> Self {
        Self(u)
    }

    /// 返回内部 `uuid::Uuid`.
    pub const fn as_uuid(&self) -> &Uuid {
        &self.0
    }

    /// 解析 nil ID.
    pub const fn nil() -> Self {
        Self(Uuid::nil())
    }

    /// 是否 nil.
    pub const fn is_nil(&self) -> bool {
        self.0.is_nil()
    }
}

impl Default for LsId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for LsId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for LsId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Uuid::from_str(s).map(Self)
    }
}

impl From<Uuid> for LsId {
    fn from(u: Uuid) -> Self {
        Self(u)
    }
}

impl From<LsId> for Uuid {
    fn from(id: LsId) -> Self {
        id.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_id_not_nil() {
        assert!(!LsId::new().is_nil());
    }

    #[test]
    fn test_nil() {
        assert!(LsId::nil().is_nil());
    }

    #[test]
    fn test_display_and_parse() {
        let id = LsId::new();
        let s = id.to_string();
        let parsed: LsId = s.parse().unwrap();
        assert_eq!(id, parsed);
    }
}
