pub mod config;
pub mod detect;
pub mod error;
pub mod languages;
pub mod runner;

pub use config::PolyglotConfig;
pub use detect::Language;
pub use error::{PolyglotError, PolyglotResult};
pub use languages::PolyglotEngine;
pub use runner::LanguageRunner;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_python() {
        let lang = Language::from_filename("script.py").unwrap();
        assert_eq!(lang, Language::Python);
    }

    #[test]
    fn test_detect_rust() {
        let lang = Language::from_filename("main.rs").unwrap();
        assert_eq!(lang, Language::Rust);
    }

    #[test]
    fn test_detect_java() {
        let lang = Language::from_filename("Hello.java").unwrap();
        assert_eq!(lang, Language::Java);
    }

    #[test]
    fn test_detect_unknown() {
        let result = Language::from_filename("unknown.xyz");
        assert!(result.is_err());
    }

    #[test]
    fn test_engine_init() {
        let engine = PolyglotEngine::with_defaults();
        let langs = engine.supported_languages();
        assert_eq!(langs.len(), 31);
    }

    #[test]
    fn test_engine_supports_python() {
        let engine = PolyglotEngine::with_defaults();
        assert!(engine.is_supported(&Language::Python));
    }

    #[test]
    fn test_engine_supports_rust() {
        let engine = PolyglotEngine::with_defaults();
        assert!(engine.is_supported(&Language::Rust));
    }

    #[test]
    fn test_engine_supports_all_30() {
        let engine = PolyglotEngine::with_defaults();
        // Spot-check a few from each group
        assert!(engine.is_supported(&Language::Python));
        assert!(engine.is_supported(&Language::JavaScript));
        assert!(engine.is_supported(&Language::Ruby));
        assert!(engine.is_supported(&Language::C));
        assert!(engine.is_supported(&Language::Rust));
        assert!(engine.is_supported(&Language::Go));
        assert!(engine.is_supported(&Language::Java));
        assert!(engine.is_supported(&Language::Csharp));
        assert!(engine.is_supported(&Language::Kotlin));
        assert!(engine.is_supported(&Language::Cobol));
        assert!(engine.is_supported(&Language::Haskell));
        assert!(engine.is_supported(&Language::Zig));
    }

    #[test]
    fn test_detect_shebang_python() {
        let code = "#!/usr/bin/env python3\nprint('hello')";
        let lang = Language::from_shebang(code);
        assert_eq!(lang, Some(Language::Python));
    }

    #[test]
    fn test_detect_shebang_ruby() {
        let code = "#!/usr/bin/env ruby\nputs 'hello'";
        let lang = Language::from_shebang(code);
        assert_eq!(lang, Some(Language::Ruby));
    }

    #[test]
    fn test_empty_config_defaults() {
        let config = PolyglotConfig::default();
        assert_eq!(config.default_timeout, 30);
        assert_eq!(config.max_output_bytes, 1_048_576);
        assert!(!config.sandbox_enabled);
    }

    #[test]
    fn test_language_names() {
        assert_eq!(Language::Python.name(), "Python");
        assert_eq!(Language::Rust.name(), "Rust");
        assert_eq!(Language::Csharp.name(), "C#");
        assert_eq!(Language::Cpp.name(), "C++");
        assert_eq!(Language::JavaScript.name(), "JavaScript");
        assert_eq!(Language::TypeScript.name(), "TypeScript");
        assert_eq!(Language::ObjectiveC.name(), "Objective-C");
        assert_eq!(Language::VisualBasic.name(), "Visual Basic");
    }
}
