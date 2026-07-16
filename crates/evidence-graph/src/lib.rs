//! LSEvidence — Evidence Graph 统一记忆输出接口。
//!
//! Evidence Graph 是 Memory Engine 的统一输出格式。
//! 无论底层使用 RAG、GraphRAG、HMS 还是 Episode Memory，
//! 输出都是 EvidenceGraph 结构。
//!
//! # 设计原则
//!
//! - **统一接口**：所有 Memory Workflow 输出 EvidenceGraph
//! - **事实层优先**：第一版只做 `Event` / `Fact` / `Entity` 节点和 `Temporal` / `Related` 边
//! - **不做推理**：`CausedBy` / `Supports` / `Contradicts` 等推理边由 Reasoner 添加
//! - **可序列化**：JSON 序列化用于调试和 API 传输
//!
//! # 未来扩展
//!
//! - `Goal` 节点类型（用户目标跟踪）
//! - `Decision` 节点类型（决策记录）
//! - `CausedBy` 边类型（因果推理）
//! - `Supports` / `Contradicts` 边类型（证据一致性）
//! - `Confidence` 评分机制

mod node;
mod edge;
mod graph;
mod builder;
mod merger;
mod serializer;

pub use node::*;
pub use edge::*;
pub use graph::*;
pub use builder::*;
pub use merger::*;
pub use serializer::*;
