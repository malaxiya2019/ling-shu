//! LSPrompt — Lingshu 提示词管理.
//!
//! 提供版本化模板、变量注入、A/B 测试和提示词版本控制功能。
//!
//! ## 架构
//!
//! ```text
//! ┌──────────────────────────────────────────┐
//! │             PromptManager                 │
//! │  ┌──────────┐ ┌──────────┐ ┌──────────┐ │
//! │  │ Prompt   │ │ Template │ │ ABTest   │ │
//! │  │ Registry │ │ Engine   │ │ Manager  │ │
//! │  └──────────┘ └──────────┘ └──────────┘ │
//! │  ┌──────────────────────────────────┐   │
//! │  │      CompiledPrompt              │   │
//! │  └──────────────────────────────────┘   │
//! └──────────────────────────────────────────┘
//! ```

pub mod template;
pub mod registry;
pub mod abtest;

pub use template::{CompiledPrompt, TemplateEngine, TemplateVariable};
pub use registry::{PromptInfo, PromptRegistry, PromptVersion};
pub use abtest::{ABTestConfig, ABTestManager, ABTestResult};
