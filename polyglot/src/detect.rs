use crate::error::{PolyglotError, PolyglotResult};

/// Language identification from filename or shebang
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Language {
    Python,
    Java,
    C,
    Cpp,
    Rust,
    Go,
    JavaScript,
    TypeScript,
    Ruby,
    Perl,
    Lua,
    Julia,
    Php,
    R,
    Groovy,
    Csharp,
    Kotlin,
    Swift,
    Dart,
    Scala,
    Haskell,
    Zig,
    Powershell,
    ObjectiveC,
    Ada,
    Matlab,
    Vba,
    Abap,
    Delphi,
    VisualBasic,
    Cobol,
}

impl Language {
    /// Name for display purposes
    pub fn name(&self) -> &'static str {
        match self {
            Language::Python => "Python",
            Language::Java => "Java",
            Language::C => "C",
            Language::Cpp => "C++",
            Language::Rust => "Rust",
            Language::Go => "Go",
            Language::JavaScript => "JavaScript",
            Language::TypeScript => "TypeScript",
            Language::Ruby => "Ruby",
            Language::Perl => "Perl",
            Language::Lua => "Lua",
            Language::Julia => "Julia",
            Language::Php => "PHP",
            Language::R => "R",
            Language::Groovy => "Groovy",
            Language::Csharp => "C#",
            Language::Kotlin => "Kotlin",
            Language::Swift => "Swift",
            Language::Dart => "Dart",
            Language::Scala => "Scala",
            Language::Haskell => "Haskell",
            Language::Zig => "Zig",
            Language::Powershell => "PowerShell",
            Language::ObjectiveC => "Objective-C",
            Language::Ada => "Ada",
            Language::Matlab => "MATLAB",
            Language::Vba => "VBA",
            Language::Abap => "ABAP",
            Language::Delphi => "Delphi/Pascal",
            Language::VisualBasic => "Visual Basic",
            Language::Cobol => "COBOL",
        }
    }

    pub fn from_filename(path: &str) -> PolyglotResult<Self> {
        let ext = path.rsplit('.').next().unwrap_or("").to_lowercase();
        match ext.as_str() {
            "py" => Ok(Self::Python),
            "java" => Ok(Self::Java),
            "c" => Ok(Self::C),
            "cpp" | "cc" | "cxx" | "hpp" => Ok(Self::Cpp),
            "rs" => Ok(Self::Rust),
            "go" => Ok(Self::Go),
            "js" | "mjs" => Ok(Self::JavaScript),
            "ts" | "tsx" => Ok(Self::TypeScript),
            "rb" => Ok(Self::Ruby),
            "pl" | "pm" => Ok(Self::Perl),
            "lua" => Ok(Self::Lua),
            "jl" => Ok(Self::Julia),
            "php" => Ok(Self::Php),
            "r" | "rdata" => Ok(Self::R),
            "groovy" | "gvy" => Ok(Self::Groovy),
            "cs" => Ok(Self::Csharp),
            "kt" | "kts" => Ok(Self::Kotlin),
            "swift" => Ok(Self::Swift),
            "dart" => Ok(Self::Dart),
            "scala" | "sc" => Ok(Self::Scala),
            "hs" | "lhs" => Ok(Self::Haskell),
            "zig" => Ok(Self::Zig),
            "ps1" => Ok(Self::Powershell),
            "m" | "mm" => Ok(Self::ObjectiveC),
            "ada" | "adb" | "ads" => Ok(Self::Ada),
            "pas" | "pp" => Ok(Self::Delphi),
            "vb" => Ok(Self::VisualBasic),
            "cob" | "cbl" => Ok(Self::Cobol),
            // MATLAB uses .m too, but we prioritize Objective-C
            // For MATLAB detection, use shebang or manual override
            _ => Err(PolyglotError::UnsupportedLanguage(format!(
                "No language mapped for extension '.{}'",
                ext
            ))),
        }
    }

    pub fn from_shebang(code: &str) -> Option<Self> {
        let first = code.lines().next()?;
        if !first.starts_with("#!") {
            return None;
        }
        let lower = first.to_lowercase();
        if lower.contains("python") { Some(Self::Python) }
        else if lower.contains("ruby") { Some(Self::Ruby) }
        else if lower.contains("perl") { Some(Self::Perl) }
        else if lower.contains("lua") { Some(Self::Lua) }
        else if lower.contains("node") || lower.contains("deno") || lower.contains("bun") {
            Some(Self::JavaScript)
        }
        else if lower.contains("groovy") { Some(Self::Groovy) }
        else if lower.contains("julia") { Some(Self::Julia) }
        else if lower.contains("rscript") || lower.contains("r ") { Some(Self::R) }
        else if lower.contains("php") { Some(Self::Php) }
        else if lower.contains("octave") || lower.contains("matlab") { Some(Self::Matlab) }
        else { None }
    }
}
