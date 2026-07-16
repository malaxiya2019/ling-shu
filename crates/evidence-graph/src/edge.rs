//! EvidenceGraph 边类型。

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::NodeId;

/// 边 ID。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(transparent)]
pub struct EdgeId(Uuid);

impl EdgeId {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }
}

impl Default for EdgeId {
    fn default() -> Self {
        Self::new()
    }
}

/// 边类型。
///
/// # 当前支持的边（事实层）
///
/// - `Temporal` — 时间先后关系
/// - `Related` — 一般关联关系
/// - `StateChange` — 状态变更关系
///
/// # 未来扩展（推理层）
///
/// - `CausedBy` — 因果关系
/// - `Supports` — 支持关系
/// - `Contradicts` — 矛盾关系
/// - `DependsOn` — 依赖关系
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum EdgeKind {
    /// 时间先后关系（A 先于 B 发生）
    Temporal,
    /// 一般关联关系（A 和 B 有关）
    Related,
    /// 状态变更关系（A 导致 B 状态变化）
    StateChange,
}

/// EvidenceGraph 中的边。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    /// 边唯一标识
    pub id: EdgeId,
    /// 边类型
    pub kind: EdgeKind,
    /// 源节点 ID
    pub source_id: NodeId,
    /// 目标节点 ID
    pub target_id: NodeId,
    /// 关系描述
    pub label: String,
    /// 可信度 (0.0 ~ 1.0)
    pub confidence: f64,
    /// 可选属性
    pub metadata: serde_json::Value,
}

impl Edge {
    /// 创建一个时间关系边。
    pub fn temporal(source_id: NodeId, target_id: NodeId) -> Self {
        Self {
            id: EdgeId::new(),
            kind: EdgeKind::Temporal,
            source_id,
            target_id,
            label: "发生于之前".into(),
            confidence: 1.0,
            metadata: serde_json::json!({}),
        }
    }

    /// 创建一个关联关系边。
    pub fn related(
        source_id: NodeId,
        target_id: NodeId,
        label: impl Into<String>,
    ) -> Self {
        Self {
            id: EdgeId::new(),
            kind: EdgeKind::Related,
            source_id,
            target_id,
            label: label.into(),
            confidence: 1.0,
            metadata: serde_json::json!({}),
        }
    }

    /// 创建一个状态变更边。
    pub fn state_change(
        source_id: NodeId,
        target_id: NodeId,
        label: impl Into<String>,
    ) -> Self {
        Self {
            id: EdgeId::new(),
            kind: EdgeKind::StateChange,
            source_id,
            target_id,
            label: label.into(),
            confidence: 1.0,
            metadata: serde_json::json!({}),
        }
    }

    /// 设置可信度。
    pub fn with_confidence(mut self, confidence: f64) -> Self {
        self.confidence = confidence.clamp(0.0, 1.0);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_edge_creation() {
        let n1 = NodeId::new();
        let n2 = NodeId::new();

        let e = Edge::temporal(n1, n2);
        assert_eq!(e.kind, EdgeKind::Temporal);
        assert_eq!(e.source_id, n1);
        assert_eq!(e.target_id, n2);
    }
}
