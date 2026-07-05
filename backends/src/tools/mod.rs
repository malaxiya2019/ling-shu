//! LSTools — 具体工具实现集合.
//!
//! 提供通用工具函数，每个工具均实现 `lingshu_traits::tool::Tool` trait.
//! 使用时通过 `ToolRegistry::register()` 注册即可.

pub mod file_tool;
#[cfg(any(feature = "openai", feature = "anthropic", feature = "groq"))]
pub mod http_tool;
pub mod shell_tool;
pub mod utility_tool;

pub use file_tool::{FileReadTool, FileWriteTool, ListDirTool};
#[cfg(any(feature = "openai", feature = "anthropic", feature = "groq"))]
pub use http_tool::{HttpGetTool, HttpPostTool};
pub use shell_tool::ShellTool;
pub use utility_tool::{CalculatorTool, CurrentTimeTool};
