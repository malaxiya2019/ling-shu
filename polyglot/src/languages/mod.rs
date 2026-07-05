mod interpreted;
mod compiled;
mod vm;

use crate::config::PolyglotConfig;
use crate::detect::Language;
use crate::error::{PolyglotError, PolyglotResult};
use crate::runner::LanguageRunner;
use std::collections::HashMap;

/// PolyglotEngine — run code in 30 programming languages
pub struct PolyglotEngine {
    runners: HashMap<Language, Box<dyn LanguageRunner>>,
    config: PolyglotConfig,
}

impl PolyglotEngine {
    pub fn new(config: PolyglotConfig) -> Self {
        let mut runners: HashMap<Language, Box<dyn LanguageRunner>> = HashMap::new();

        for r in interpreted::all_runners() {
            runners.insert(r.language(), r);
        }
        for r in compiled::all_runners() {
            runners.insert(r.language(), r);
        }
        for r in vm::all_runners() {
            runners.insert(r.language(), r);
        }

        Self { runners, config }
    }

    pub fn with_defaults() -> Self {
        Self::new(PolyglotConfig::default())
    }

    pub fn supported_languages(&self) -> Vec<Language> {
        let mut langs: Vec<_> = self.runners.keys().cloned().collect();
        langs.sort_by(|a, b| a.name().cmp(b.name()));
        langs
    }

    pub fn is_supported(&self, lang: &Language) -> bool {
        self.runners.contains_key(lang)
    }

    pub fn config(&self) -> &PolyglotConfig {
        &self.config
    }

    pub fn config_mut(&mut self) -> &mut PolyglotConfig {
        &mut self.config
    }

    pub async fn run(&self, code: &str, lang: &Language) -> PolyglotResult<String> {
        let runner = self
            .runners
            .get(lang)
            .ok_or_else(|| PolyglotError::UnsupportedLanguage(lang.name().to_string()))?;
        runner.run(code, &self.config).await
    }
}
