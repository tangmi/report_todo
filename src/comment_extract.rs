use pest::Parser;
use pest_derive::Parser;
use std::path::Path;

pub enum Language {
    C,
    CMakeLists,
    Cpp,
    CSS,
    Fish,
    GLSL,
    HLSL,
    HTML,
    JavaScript,
    Lua,
    Makefile,
    Markdown,
    Pest,
    Powershell,
    Python,
    Rust,
    Shell,
    TOML,
    TypeScript,
    WGSL,
    YAML,
}

impl Language {
    pub fn from_path(path: &std::path::Path) -> Option<Self> {
        match path.file_name()?.to_string_lossy().as_ref() {
            "CMakeLists.txt" => Some(Language::CMakeLists),
            "Makefile" => Some(Language::Makefile),

            _ => match path.extension()?.to_string_lossy().as_ref() {
                // "c" | "h" => Some(Language::C),
                // "cc" | "cpp" | "cxx" | "hpp" | "hxx" => Some(Language::Cpp),
                // "css" => Some(Language::CSS),
                // "fish" => Some(Language::Fish),
                // "glsl" => Some(Language::GLSL),
                // "hlsl" => Some(Language::HLSL),
                // "html" => Some(Language::HTML),
                // "js" => Some(Language::JavaScript),
                // "lua" => Some(Language::Lua),
                // "md" => Some(Language::Markdown),
                // "pest" => Some(Language::Pest),
                // "ps1" => Some(Language::Powershell),
                // "py" => Some(Language::Python),
                "rs" => Some(Language::Rust),
                // "sh" => Some(Language::Shell),
                "toml" => Some(Language::TOML),
                // "ts" => Some(Language::TypeScript),
                // "wgsl" => Some(Language::WGSL),
                // "yaml" => Some(Language::YAML),
                _ => None,
            },
        }
    }
}

#[derive(Parser)]
#[grammar = "comment_extract.pest"]
pub struct Comments;

#[derive(Debug)]
pub enum ExtractError {
    LanguageUnrecognized,
    ParserError(pest::error::Error<Rule>),
    IoError(std::io::Error),
}

impl From<std::io::Error> for ExtractError {
    fn from(inner: std::io::Error) -> Self {
        ExtractError::IoError(inner)
    }
}

impl From<pest::error::Error<Rule>> for ExtractError {
    fn from(inner: pest::error::Error<Rule>) -> Self {
        ExtractError::ParserError(inner)
    }
}

impl std::fmt::Display for ExtractError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl std::error::Error for ExtractError {}

/// - Potentially multiline event.
/// - Generalized across many programming languages
#[derive(Debug)]
pub enum ExtractEvent<'a> {
    RustTodoMacro(pest::Span<'a>),
    Comment(pest::Span<'a>),
    StringLiteral(pest::Span<'a>),
    Unhandled(Rule, pest::Span<'a>),
}

pub fn extract<'a>(
    file_path: &Path,
    file_contents: &'a str,
) -> Result<impl Iterator<Item = ExtractEvent<'a>>, ExtractError> {
    let language_rule = Language::from_path(file_path)
        .ok_or(ExtractError::LanguageUnrecognized)
        .map(|language| match language {
            Language::C | Language::Cpp | Language::GLSL | Language::HLSL => Rule::c_file,
            Language::CMakeLists => todo!(),
            Language::CSS => todo!(),
            Language::Fish | Language::Shell | Language::TOML | Language::WGSL | Language::YAML => {
                Rule::shell_file
            }
            Language::HTML => todo!(),
            Language::JavaScript | Language::TypeScript => Rule::js_file,
            Language::Lua => todo!(),
            Language::Makefile => todo!(),
            Language::Markdown => todo!(), // Like HTML, but no string literals
            Language::Powershell => todo!(), // Like shell, but has block comments `<# ... #>`
            Language::Python => todo!(),
            Language::Rust | Language::Pest => Rule::rust_file,
        })?;

    let mut parsed_file = Comments::parse(language_rule, &file_contents)?;

    Ok(parsed_file
        .next()
        .expect("Grammar should have a root node")
        .into_inner()
        .map(|pair| match pair.as_rule() {
            Rule::rust_todo_macro => ExtractEvent::RustTodoMacro(pair.as_span()),

            Rule::rust_file_comment
            | Rule::hash_line_comment
            | Rule::c_file_comment
            | Rule::html_comment => ExtractEvent::Comment(pair.as_span()),

            Rule::c_string_literal => ExtractEvent::StringLiteral(pair.as_span()),

            unhandled_rule => ExtractEvent::Unhandled(unhandled_rule, pair.as_span()),
        }))
}
