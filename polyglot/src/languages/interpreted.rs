use crate::config::PolyglotConfig;
use crate::detect::Language;
use crate::error::PolyglotResult;
use crate::runner::{execute_command, LanguageRunner};
use async_trait::async_trait;
use std::io::Write;
use tokio::process::Command;

macro_rules! interpreted_runner {
    ($name:ident, $lang:expr, $ext:expr, $cmd:expr) => {
        pub struct $name;
        impl $name {
            pub fn boxed() -> Box<dyn LanguageRunner> { Box::new(Self) }
        }
        #[async_trait]
        impl LanguageRunner for $name {
            fn language_name(&self) -> &'static str { $lang.name() }
            fn language(&self) -> Language { $lang }
            fn file_extension(&self) -> &'static str { $ext }
            async fn run(&self, code: &str, config: &PolyglotConfig) -> PolyglotResult<String> {
                let mut tmp = tempfile::Builder::new().suffix($ext).tempfile()?;
                tmp.write_all(code.as_bytes())?;
                let path = tmp.path().to_string_lossy().to_string();
                let mut cmd = Command::new($cmd);
                cmd.arg(&path);
                execute_command(&mut cmd, config).await
            }
        }
    };
}

interpreted_runner!(PythonRunner, Language::Python, ".py", "python3");
interpreted_runner!(JavaScriptRunner, Language::JavaScript, ".js", "node");
interpreted_runner!(RubyRunner, Language::Ruby, ".rb", "ruby");
interpreted_runner!(PerlRunner, Language::Perl, ".pl", "perl");
interpreted_runner!(LuaRunner, Language::Lua, ".lua", "lua");
interpreted_runner!(JuliaRunner, Language::Julia, ".jl", "julia");
interpreted_runner!(PhpRunner, Language::Php, ".php", "php");
interpreted_runner!(RRunner, Language::R, ".r", "Rscript");
interpreted_runner!(GroovyRunner, Language::Groovy, ".groovy", "groovy");
interpreted_runner!(PowershellRunner, Language::Powershell, ".ps1", "pwsh");

// TypeScript — via ts-node
pub struct TypeScriptRunner;
impl TypeScriptRunner {
    pub fn boxed() -> Box<dyn LanguageRunner> { Box::new(Self) }
}
#[async_trait]
impl LanguageRunner for TypeScriptRunner {
    fn language_name(&self) -> &'static str { "TypeScript" }
    fn language(&self) -> Language { Language::TypeScript }
    fn file_extension(&self) -> &'static str { ".ts" }
    async fn run(&self, code: &str, config: &PolyglotConfig) -> PolyglotResult<String> {
        let mut tmp = tempfile::Builder::new().suffix(".ts").tempfile()?;
        tmp.write_all(code.as_bytes())?;
        let path = tmp.path().to_string_lossy().to_string();
        let mut cmd = Command::new("npx");
        cmd.arg("ts-node").arg(&path);
        execute_command(&mut cmd, config).await
    }
}

// MATLAB/Octave
pub struct MatlabRunner;
impl MatlabRunner {
    pub fn boxed() -> Box<dyn LanguageRunner> { Box::new(Self) }
}
#[async_trait]
impl LanguageRunner for MatlabRunner {
    fn language_name(&self) -> &'static str { "MATLAB" }
    fn language(&self) -> Language { Language::Matlab }
    fn file_extension(&self) -> &'static str { ".m" }
    async fn run(&self, code: &str, config: &PolyglotConfig) -> PolyglotResult<String> {
        let mut tmp = tempfile::Builder::new().suffix(".m").tempfile()?;
        tmp.write_all(code.as_bytes())?;
        let path = tmp.path().to_string_lossy().to_string();
        let mut cmd = Command::new("octave");
        cmd.arg("-q").arg(&path);
        execute_command(&mut cmd, config).await
    }
}

// VBA runner
pub struct VbaRunner;
impl VbaRunner {
    pub fn boxed() -> Box<dyn LanguageRunner> { Box::new(Self) }
}
#[async_trait]
impl LanguageRunner for VbaRunner {
    fn language_name(&self) -> &'static str { "VBA" }
    fn language(&self) -> Language { Language::Vba }
    fn file_extension(&self) -> &'static str { ".bas" }
    async fn run(&self, _code: &str, _config: &PolyglotConfig) -> PolyglotResult<String> {
        Ok("VBA execution requires Microsoft Excel or LibreOffice Calc".to_string())
    }
}

// ABAP runner
pub struct AbapRunner;
impl AbapRunner {
    pub fn boxed() -> Box<dyn LanguageRunner> { Box::new(Self) }
}
#[async_trait]
impl LanguageRunner for AbapRunner {
    fn language_name(&self) -> &'static str { "ABAP" }
    fn language(&self) -> Language { Language::Abap }
    fn file_extension(&self) -> &'static str { ".abap" }
    async fn run(&self, _code: &str, _config: &PolyglotConfig) -> PolyglotResult<String> {
        Ok("ABAP execution requires SAP system".to_string())
    }
}

pub fn all_runners() -> Vec<Box<dyn LanguageRunner>> {
    vec![
        PythonRunner::boxed(),
        JavaScriptRunner::boxed(),
        TypeScriptRunner::boxed(),
        RubyRunner::boxed(),
        PerlRunner::boxed(),
        LuaRunner::boxed(),
        JuliaRunner::boxed(),
        PhpRunner::boxed(),
        RRunner::boxed(),
        GroovyRunner::boxed(),
        PowershellRunner::boxed(),
        MatlabRunner::boxed(),
        VbaRunner::boxed(),
        AbapRunner::boxed(),
    ]
}
