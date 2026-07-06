use crate::config::PolyglotConfig;
use crate::detect::Language;
use crate::error::{PolyglotError, PolyglotResult};
use crate::runner::{execute_command, LanguageRunner};
use async_trait::async_trait;
use tokio::process::Command;

// Java
pub struct JavaRunner;
impl JavaRunner {
    pub fn boxed() -> Box<dyn LanguageRunner> {
        Box::new(Self)
    }
}
#[async_trait]
impl LanguageRunner for JavaRunner {
    fn language_name(&self) -> &'static str {
        "Java"
    }
    fn language(&self) -> Language {
        Language::Java
    }
    fn file_extension(&self) -> &'static str {
        ".java"
    }
    async fn run(&self, code: &str, config: &PolyglotConfig) -> PolyglotResult<String> {
        let dir = tempfile::tempdir()?;
        let class_name = if let Some(line) = code.lines().find(|l| l.contains("class ")) {
            let parts: Vec<&str> = line.split("class ").collect();
            if parts.len() > 1 {
                parts[1]
                    .split_whitespace()
                    .next()
                    .unwrap_or("Main")
                    .to_string()
            } else {
                "Main".to_string()
            }
        } else {
            "Main".to_string()
        };
        let filename = format!("{}.java", class_name);
        let src = dir.path().join(&filename);
        std::fs::write(&src, code)?;
        let mut javac = Command::new("javac");
        javac.arg(src.to_string_lossy().as_ref());
        execute_command(&mut javac, config)
            .await
            .map_err(|e| PolyglotError::CompilationFailed(e.to_string()))?;
        let mut java = Command::new("java");
        java.arg("-cp")
            .arg(dir.path().to_string_lossy().as_ref())
            .arg(&class_name);
        execute_command(&mut java, config).await
    }
}

// C#
pub struct CsharpRunner;
impl CsharpRunner {
    pub fn boxed() -> Box<dyn LanguageRunner> {
        Box::new(Self)
    }
}
#[async_trait]
impl LanguageRunner for CsharpRunner {
    fn language_name(&self) -> &'static str {
        "C#"
    }
    fn language(&self) -> Language {
        Language::Csharp
    }
    fn file_extension(&self) -> &'static str {
        ".cs"
    }
    async fn run(&self, code: &str, config: &PolyglotConfig) -> PolyglotResult<String> {
        let dir = tempfile::tempdir()?;
        let src = dir.path().join("Main.cs");
        std::fs::write(&src, code)?;
        let out = dir.path().join("Main.exe");
        // Try dotnet script first, then mcs (mono)
        let mut dotnet = Command::new("dotnet");
        dotnet.arg("script").arg(src.to_string_lossy().as_ref());
        let result = execute_command(&mut dotnet, config).await;
        match result {
            Ok(r) => Ok(r),
            Err(_) => {
                let mut mcs = Command::new("mcs");
                mcs.arg("-out:")
                    .arg(&out)
                    .arg(src.to_string_lossy().as_ref());
                execute_command(&mut mcs, config)
                    .await
                    .map_err(|e| PolyglotError::CompilationFailed(e.to_string()))?;
                let mut mono = Command::new("mono");
                mono.arg(&out);
                execute_command(&mut mono, config).await
            }
        }
    }
}

// Kotlin
pub struct KotlinRunner;
impl KotlinRunner {
    pub fn boxed() -> Box<dyn LanguageRunner> {
        Box::new(Self)
    }
}
#[async_trait]
impl LanguageRunner for KotlinRunner {
    fn language_name(&self) -> &'static str {
        "Kotlin"
    }
    fn language(&self) -> Language {
        Language::Kotlin
    }
    fn file_extension(&self) -> &'static str {
        ".kt"
    }
    async fn run(&self, code: &str, config: &PolyglotConfig) -> PolyglotResult<String> {
        let dir = tempfile::tempdir()?;
        let src = dir.path().join("Main.kt");
        std::fs::write(&src, code)?;
        let jar = dir.path().join("Main.jar");
        let mut kotlinc = Command::new("kotlinc");
        kotlinc
            .arg(src.to_string_lossy().as_ref())
            .arg("-include-runtime")
            .arg("-d")
            .arg(&jar);
        execute_command(&mut kotlinc, config)
            .await
            .map_err(|e| PolyglotError::CompilationFailed(e.to_string()))?;
        let mut java = Command::new("java");
        java.arg("-jar").arg(&jar);
        execute_command(&mut java, config).await
    }
}

// Dart
pub struct DartRunner;
impl DartRunner {
    pub fn boxed() -> Box<dyn LanguageRunner> {
        Box::new(Self)
    }
}
#[async_trait]
impl LanguageRunner for DartRunner {
    fn language_name(&self) -> &'static str {
        "Dart"
    }
    fn language(&self) -> Language {
        Language::Dart
    }
    fn file_extension(&self) -> &'static str {
        ".dart"
    }
    async fn run(&self, code: &str, config: &PolyglotConfig) -> PolyglotResult<String> {
        let dir = tempfile::tempdir()?;
        let src = dir.path().join("main.dart");
        std::fs::write(&src, code)?;
        let mut cmd = Command::new("dart");
        cmd.arg("run").arg(src.to_string_lossy().as_ref());
        execute_command(&mut cmd, config).await
    }
}

// Scala
pub struct ScalaRunner;
impl ScalaRunner {
    pub fn boxed() -> Box<dyn LanguageRunner> {
        Box::new(Self)
    }
}
#[async_trait]
impl LanguageRunner for ScalaRunner {
    fn language_name(&self) -> &'static str {
        "Scala"
    }
    fn language(&self) -> Language {
        Language::Scala
    }
    fn file_extension(&self) -> &'static str {
        ".scala"
    }
    async fn run(&self, code: &str, config: &PolyglotConfig) -> PolyglotResult<String> {
        let dir = tempfile::tempdir()?;
        let src = dir.path().join("Main.scala");
        std::fs::write(&src, code)?;
        let mut cmd = Command::new("scala");
        cmd.arg(src.to_string_lossy().as_ref());
        execute_command(&mut cmd, config).await
    }
}

pub fn all_runners() -> Vec<Box<dyn LanguageRunner>> {
    vec![
        JavaRunner::boxed(),
        CsharpRunner::boxed(),
        KotlinRunner::boxed(),
        DartRunner::boxed(),
        ScalaRunner::boxed(),
    ]
}
