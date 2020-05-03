use serde::{Deserialize, Serialize};
use span::*;
use std::path::PathBuf;
use structopt::StructOpt;

mod console_emitter;
mod todo_error;

use todo_error::Regexes;
use todo_error::TodoError;

/// Will ignore files listed in `.todoignore` and `.gitignore`.
#[derive(Debug, StructOpt)]
struct Opt {
    #[structopt(flatten)]
    config: Config,

    #[structopt(name = "ROOT_DIR")]
    root_dir: PathBuf,
}

#[derive(Debug, StructOpt, Serialize, Deserialize)]
struct Config {
    /// Regex to detect a valid TODO with issue number. e.g. `todo\(#(?P<issue_number>\d+)\):`
    #[structopt(long = "match-issue")]
    match_issue: String,

    /// Regex replace string used to format the output link. e.g. `https://github.com/tangmi/report_todo/issues/${issue_number}`
    #[structopt(long = "issue-link-format")]
    issue_link_format: String,

    /// Expected to match `\w+`.
    #[structopt(long = "forbid")]
    forbidden_keywords: Vec<String>,

    /// Can be `Untracked` (show only matches for forbidden keywords) or `All` (show tracked issues as well)
    #[structopt(long = "report")]
    report_kind: ReportKind,
    // TODO: add custom sublime-syntax files?
    // TODO: json output? ide-friendly output?
    // TODO: warnings or errors? return code?
}

#[derive(Debug, StructOpt, Serialize, Deserialize, Copy, Clone)]
enum ReportKind {
    /// Report tracked issues and forbidden keywords
    All,

    /// Report only forbidden keywords
    Untracked,
}

#[derive(Debug)]
struct ParseReportKindError;

impl std::string::ToString for ParseReportKindError {
    fn to_string(&self) -> String {
        format!("{:?}", self)
    }
}

impl std::str::FromStr for ReportKind {
    type Err = ParseReportKindError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "All" => Ok(ReportKind::All),
            "Untracked" => Ok(ReportKind::Untracked),
            _ => Err(ParseReportKindError),
        }
    }
}

struct CommentScopeStack<'a> {
    /// original text that's being parsed
    original: Span<'a>,

    current_comment_start: Option<usize>,
    comment_level: usize,

    prefix_scope: syntect::parsing::Scope,

    scopes_stack: Vec<syntect::parsing::Scope>,
    cleared_scopes_stack: Vec<Vec<syntect::parsing::Scope>>,
}

impl<'a> CommentScopeStack<'a> {
    fn new(text: Span<'a>) -> Self {
        Self {
            original: text,
            current_comment_start: None,
            comment_level: 0,
            prefix_scope: syntect::parsing::Scope::new("comment").unwrap(),
            scopes_stack: Vec::new(),
            cleared_scopes_stack: Vec::new(),
        }
    }

    /// Returns any comments that were finished in `ops`. This means it does not nessecarily return the comment if it appears in the current line and may return the comment in a subsequent line.
    ///
    /// TODO should "get comments" be a callback?
    ///
    /// Note: we need to pass the original line because syntect doesn't preserve mappings to the original source.
    fn process_ops_for_line(
        &mut self,
        ops: impl Iterator<Item = (usize, syntect::parsing::ScopeStackOp)>,
        original_line: Span<'a>,
    ) -> Vec<Span<'a>> {
        let mut comments = Vec::new();
        for op in ops {
            match op.1 {
                syntect::parsing::ScopeStackOp::Push(scope) => {
                    if self.prefix_scope.is_prefix_of(scope) {
                        if self.comment_level == 0 {
                            self.current_comment_start = Some(original_line.start() + op.0);
                        }
                        self.comment_level += 1;
                    }
                    self.scopes_stack.push(scope);
                }

                syntect::parsing::ScopeStackOp::Pop(count) => {
                    for _ in 0..count {
                        let scope = self.scopes_stack.pop().unwrap();
                        if self.prefix_scope.is_prefix_of(scope) {
                            self.comment_level -= 1;

                            if self.comment_level == 0 {
                                comments.push(
                                    self.original
                                        .sub_span(
                                            self.current_comment_start.unwrap()
                                                ..(original_line.start() + op.0),
                                        )
                                        .unwrap(),
                                );
                            }
                        }
                    }
                }

                syntect::parsing::ScopeStackOp::Clear(amount) => {
                    let cleared = match amount {
                        syntect::parsing::ClearAmount::TopN(n) => {
                            // don't try to clear more scopes than are on the stack
                            let to_leave =
                                self.scopes_stack.len() - std::cmp::min(n, self.scopes_stack.len());
                            self.scopes_stack.split_off(to_leave)
                        }
                        syntect::parsing::ClearAmount::All => {
                            let mut cleared = Vec::new();
                            std::mem::swap(&mut cleared, &mut self.scopes_stack);
                            cleared
                        }
                    };

                    self.cleared_scopes_stack.push(cleared);
                }

                syntect::parsing::ScopeStackOp::Restore => {
                    for scope in self.cleared_scopes_stack.pop().unwrap().into_iter() {
                        self.scopes_stack.push(scope);
                    }
                }

                syntect::parsing::ScopeStackOp::Noop => {}
            }
        }

        comments
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // TODO try finding a config file in current directory first
    // let opt = Opt::from_args();

    let opt = Opt {
        // Global conifg? `dirs::config_dir`?
        config: toml::from_str(&std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/report_todo.toml"
        ))?)?,
        root_dir: PathBuf::from("."),
    };

    let config = Regexes {
        match_issue: regex::RegexBuilder::new(&format!(r"\b{}", opt.config.match_issue))
            .case_insensitive(true)
            .build()?,
        issue_link_format: opt.config.issue_link_format.clone(),
        bad_keywords: opt
            .config
            .forbidden_keywords
            .iter()
            .map(|keyword| {
                regex::RegexBuilder::new(&format!(r"\b{}\b", keyword))
                    .case_insensitive(true)
                    .build()
            })
            .collect::<Result<Vec<_>, _>>()?,
    };

    let mut stderr = console_emitter::ColoredWriter::new();

    let syntax_set = {
        const LINES_INCLUDE_NEWLINE: bool = true;
        let mut builder = if LINES_INCLUDE_NEWLINE {
            syntect::parsing::SyntaxSet::load_defaults_newlines()
        } else {
            syntect::parsing::SyntaxSet::load_defaults_nonewlines()
        }
        .into_builder();
        // Use this instead of `add_from_folder` to compile the syntaxes into the program.
        // TODO create separate dump? https://github.com/trishume/syntect/blob/master/src/dumps.rs
        builder.add(
            syntect::parsing::syntax_definition::SyntaxDefinition::load_from_str(
                include_str!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/syntaxes/TOML.sublime-syntax"
                )),
                LINES_INCLUDE_NEWLINE,
                None,
            )?,
        );
        builder.build()
    };

    let mut has_untracked = false;
    let mut issues_found_count = 0;
    for entry in ignore::WalkBuilder::new(&opt.root_dir)
        .add_custom_ignore_filename(".todoignore")
        .build()
    {
        let entry = entry.expect("walking directory entry should not have i/o errors");
        let file_path = entry.path();
        if file_path.is_file() {
            if let Some(syntax_ref) = syntax_set
                .find_syntax_for_file(entry.path())
                .expect("opening source file should work")
            {
                let mut state = syntect::parsing::ParseState::new(syntax_ref);

                let file_contents = std::fs::read_to_string(file_path)?;
                let file_span = Span::new(&file_contents, 0, file_contents.len()).unwrap();
                let mut stack = CommentScopeStack::new(file_span.clone());
                for line in file_span.lines_span() {
                    // TODO capture usages of `todo!()` macro in rust?
                    for todo_error in stack
                        .process_ops_for_line(
                            state.parse_line(line.as_str(), &syntax_set).into_iter(),
                            line,
                        )
                        .into_iter()
                        .flat_map(|comment| {
                            TodoError::from_comment(&config, file_path, comment).into_iter()
                        })
                        .filter(|todo_error| match opt.config.report_kind {
                            ReportKind::Untracked => {
                                if !todo_error.is_tracked() {
                                    true
                                } else {
                                    false
                                }
                            }
                            ReportKind::All => true,
                        })
                    {
                        stderr.write_error(&todo_error)?;
                        issues_found_count += 1;

                        if !todo_error.is_tracked() {
                            has_untracked = true;
                        }
                    }
                }
            } else {
                eprintln!("Ignoring file: {:?}. No syntax set found.", entry.path());
            }
        }
    }

    if issues_found_count > 0 {
        eprintln!("{} issues found.", issues_found_count)
    }

    if has_untracked {
        Err(UntrackedIssuesFoundError)?;
    }

    Ok(())
}

#[derive(Debug)]
struct UntrackedIssuesFoundError;

impl std::fmt::Display for UntrackedIssuesFoundError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "untracked issues found!")
    }
}

impl std::error::Error for UntrackedIssuesFoundError {}
