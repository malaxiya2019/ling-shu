use crate::config::PolyglotConfig;
use crate::detect::Language;
use crate::error::{PolyglotError, PolyglotResult};
use crate::runner::{execute_command, LanguageRunner};
use async_trait::async_trait;
use tokio::process::Command;

macro_rules! compiled_runner {
    ($name:ident, $lang:expr, $ext:expr, $compiler:expr,
     $src_flag:expr, $out_flag:expr, $out_name:expr, $run_cmd:expr) => {
        pub struct $name;
        impl $name {
            pub fn boxed() -> Box<dyn LanguageRunner> {
                Box::new(Self)
            }
        }
        #[async_trait]
        impl LanguageRunner for $name {
            fn language_name(&self) -> &'static str {
                $lang.name()
            }
            fn language(&self) -> Language {
                $lang
            }
            fn file_extension(&self) -> &'static str {
                $ext
            }
            async fn run(&self, code: &str, config: &PolyglotConfig) -> PolyglotResult<String> {
                let dir = tempfile::tempdir()?;
                let src_path = dir.path().join(format!("main{}", $ext));
                let out_path = dir.path().join($out_name);
                std::fs::write(&src_path, code)?;
                // Compile
                let mut comp = Command::new($compiler);
                if !$src_flag.is_empty() {
                    comp.arg($src_flag);
                }
                comp.arg(src_path.to_string_lossy().as_ref());
                if !$out_flag.is_empty() {
                    comp.arg($out_flag);
                }
                comp.arg(out_path.to_string_lossy().as_ref());
                execute_command(&mut comp, config)
                    .await
                    .map_err(|e| PolyglotError::CompilationFailed(e.to_string()))?;
                // Run
                let mut run = Command::new($run_cmd);
                run.arg(out_path.to_string_lossy().as_ref());
                execute_command(&mut run, config).await
            }
        }
    };
}

compiled_runner!(
    CRunner,
    Language::C,
    ".c",
    "gcc",
    "",
    "-o",
    "main_out",
    "./main_out"
);
compiled_runner!(
    CppRunner,
    Language::Cpp,
    ".cpp",
    "g++",
    "",
    "-o",
    "main_out",
    "./main_out"
);
compiled_runner!(
    SwiftRunner,
    Language::Swift,
    ".swift",
    "swiftc",
    "",
    "-o",
    "main_out",
    "./main_out"
);
compiled_runner!(
    AdaRunner,
    Language::Ada,
    ".adb",
    "gnatmake",
    "",
    "-o",
    "prog",
    "./prog"
);
compiled_runner!(
    CobolRunner,
    Language::Cobol,
    ".cob",
    "cobc",
    "-x",
    "-o",
    "prog",
    "./prog"
);
compiled_runner!(
    DelphiRunner,
    Language::Delphi,
    ".pas",
    "fpc",
    "",
    "-o",
    "prog",
    "./prog"
);

// Rust
pub struct RustRunner;
impl RustRunner {
    pub fn boxed() -> Box<dyn LanguageRunner> {
        Box::new(Self)
    }
}
#[async_trait]
impl LanguageRunner for RustRunner {
    fn language_name(&self) -> &'static str {
        "Rust"
    }
    fn language(&self) -> Language {
        Language::Rust
    }
    fn file_extension(&self) -> &'static str {
        ".rs"
    }
    async fn run(&self, code: &str, config: &PolyglotConfig) -> PolyglotResult<String> {
        let dir = tempfile::tempdir()?;
        let src = dir.path().join("main.rs");
        let content = if !code.contains("fn main") {
            format!("fn main() {{\n{}\n}}", code)
        } else {
            code.to_string()
        };
        std::fs::write(&src, content)?;
        let out = dir.path().join("prog");
        let mut comp = Command::new("rustc");
        comp.arg(src.to_string_lossy().as_ref()).arg("-o").arg(&out);
        execute_command(&mut comp, config)
            .await
            .map_err(|e| PolyglotError::CompilationFailed(e.to_string()))?;
        let mut run = Command::new(&out);
        execute_command(&mut run, config).await
    }
}

// Go
pub struct GoRunner;
impl GoRunner {
    pub fn boxed() -> Box<dyn LanguageRunner> {
        Box::new(Self)
    }
}
#[async_trait]
impl LanguageRunner for GoRunner {
    fn language_name(&self) -> &'static str {
        "Go"
    }
    fn language(&self) -> Language {
        Language::Go
    }
    fn file_extension(&self) -> &'static str {
        ".go"
    }
    async fn run(&self, code: &str, config: &PolyglotConfig) -> PolyglotResult<String> {
        let dir = tempfile::tempdir()?;
        let src = dir.path().join("main.go");
        std::fs::write(&src, code)?;
        let mut cmd = Command::new("go");
        cmd.arg("run").arg(src.to_string_lossy().as_ref());
        execute_command(&mut cmd, config).await
    }
}

// Zig
pub struct ZigRunner;
impl ZigRunner {
    pub fn boxed() -> Box<dyn LanguageRunner> {
        Box::new(Self)
    }
}
#[async_trait]
impl LanguageRunner for ZigRunner {
    fn language_name(&self) -> &'static str {
        "Zig"
    }
    fn language(&self) -> Language {
        Language::Zig
    }
    fn file_extension(&self) -> &'static str {
        ".zig"
    }
    async fn run(&self, code: &str, config: &PolyglotConfig) -> PolyglotResult<String> {
        let dir = tempfile::tempdir()?;
        let src = dir.path().join("main.zig");
        std::fs::write(&src, code)?;
        let out = dir.path().join("prog");
        let mut comp = Command::new("zig");
        comp.arg("build-exe")
            .arg(src.to_string_lossy().as_ref())
            .arg("--name")
            .arg("prog");
        execute_command(&mut comp, config)
            .await
            .map_err(|e| PolyglotError::CompilationFailed(e.to_string()))?;
        let mut run = Command::new(&out);
        execute_command(&mut run, config).await
    }
}

// Haskell (GHC)
pub struct HaskellRunner;
impl HaskellRunner {
    pub fn boxed() -> Box<dyn LanguageRunner> {
        Box::new(Self)
    }
}
#[async_trait]
impl LanguageRunner for HaskellRunner {
    fn language_name(&self) -> &'static str {
        "Haskell"
    }
    fn language(&self) -> Language {
        Language::Haskell
    }
    fn file_extension(&self) -> &'static str {
        ".hs"
    }
    async fn run(&self, code: &str, config: &PolyglotConfig) -> PolyglotResult<String> {
        let dir = tempfile::tempdir()?;
        let src = dir.path().join("Main.hs");
        std::fs::write(&src, code)?;
        let out = dir.path().join("prog");
        let mut comp = Command::new("ghc");
        comp.arg(src.to_string_lossy().as_ref()).arg("-o").arg(&out);
        execute_command(&mut comp, config)
            .await
            .map_err(|e| PolyglotError::CompilationFailed(e.to_string()))?;
        let mut run = Command::new(&out);
        execute_command(&mut run, config).await
    }
}

// Objective-C
pub struct ObjectiveCRunner;
impl ObjectiveCRunner {
    pub fn boxed() -> Box<dyn LanguageRunner> {
        Box::new(Self)
    }
}
#[async_trait]
impl LanguageRunner for ObjectiveCRunner {
    fn language_name(&self) -> &'static str {
        "Objective-C"
    }
    fn language(&self) -> Language {
        Language::ObjectiveC
    }
    fn file_extension(&self) -> &'static str {
        ".m"
    }
    async fn run(&self, code: &str, config: &PolyglotConfig) -> PolyglotResult<String> {
        let dir = tempfile::tempdir()?;
        let src = dir.path().join("main.m");
        std::fs::write(&src, code)?;
        let out = dir.path().join("prog");
        let mut comp = Command::new("clang");
        comp.arg(src.to_string_lossy().as_ref())
            .arg("-o")
            .arg(&out)
            .arg("-lobjc")
            .arg("-framework")
            .arg("Foundation");
        execute_command(&mut comp, config)
            .await
            .map_err(|e| PolyglotError::CompilationFailed(e.to_string()))?;
        let mut run = Command::new(&out);
        execute_command(&mut run, config).await
    }
}

// Visual Basic (mono-basic/vbnc)
pub struct VisualBasicRunner;
impl VisualBasicRunner {
    pub fn boxed() -> Box<dyn LanguageRunner> {
        Box::new(Self)
    }
}
#[async_trait]
impl LanguageRunner for VisualBasicRunner {
    fn language_name(&self) -> &'static str {
        "Visual Basic"
    }
    fn language(&self) -> Language {
        Language::VisualBasic
    }
    fn file_extension(&self) -> &'static str {
        ".vb"
    }
    async fn run(&self, code: &str, config: &PolyglotConfig) -> PolyglotResult<String> {
        let dir = tempfile::tempdir()?;
        let src = dir.path().join("Main.vb");
        std::fs::write(&src, code)?;
        let out = dir.path().join("prog.exe");
        let mut comp = Command::new("vbnc");
        comp.arg(src.to_string_lossy().as_ref())
            .arg("-out:")
            .arg(&out);
        execute_command(&mut comp, config)
            .await
            .map_err(|e| PolyglotError::CompilationFailed(e.to_string()))?;
        let mut run = Command::new("mono");
        run.arg(&out);
        execute_command(&mut run, config).await
    }
}

pub fn all_runners() -> Vec<Box<dyn LanguageRunner>> {
    vec![
        CRunner::boxed(),
        CppRunner::boxed(),
        RustRunner::boxed(),
        GoRunner::boxed(),
        SwiftRunner::boxed(),
        ZigRunner::boxed(),
        HaskellRunner::boxed(),
        AdaRunner::boxed(),
        CobolRunner::boxed(),
        ObjectiveCRunner::boxed(),
        DelphiRunner::boxed(),
        VisualBasicRunner::boxed(),
    ]
}
