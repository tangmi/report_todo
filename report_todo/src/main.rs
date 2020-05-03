use serde::{Deserialize, Serialize};
use span::*;
use std::path::PathBuf;
use structopt::StructOpt;

mod console_emitter;
mod todo_error;

use todo_error::Regexes;
use todo_error::TodoError;

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
    with_issue_regex: String,

    /// Regex replace string used to format the output link. e.g. `https://github.com/tangmi/report-todo/issues/${issue_number}`
    #[structopt(long = "issue-link-format")]
    with_issue_replace: String,

    #[structopt(long = "match")]
    todo_keywords: Vec<String>,
    // TODO: add ignore directories?
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
    // let opt = Opt::from_args(); // TODO

    let opt = Opt {
        // Global conifg? `dirs::config_dir`?
        config: toml::from_str(&std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/report_todo.toml"
        ))?)?,
        root_dir: PathBuf::from("."),
    };

    let config = Regexes {
        with_issue: regex::RegexBuilder::new(&format!(r"\b{}", opt.config.with_issue_regex))
            .case_insensitive(true)
            .build()?,
        with_issue_replace: opt.config.with_issue_replace,
        bad_keywords: opt
            .config
            .todo_keywords
            .iter()
            .map(|keyword| {
                regex::RegexBuilder::new(&format!(r"\b{}\b", keyword))
                    .case_insensitive(true)
                    .build()
            })
            .collect::<Result<Vec<_>, _>>()?,
    };

    let mut stderr = console_emitter::ColoredWriter::new();

    let ps = {
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

    for entry in ignore::Walk::new(&opt.root_dir) {
        let entry = entry?;

        let file_path = entry.path();
        dbg!(file_path);

        if file_path.is_file() {
            if let Some(syntax_ref) = ps.find_syntax_for_file(entry.path())? {
                let mut state = syntect::parsing::ParseState::new(syntax_ref);

                let file_contents = std::fs::read_to_string(file_path)?;
                let file_span = Span::new(&file_contents, 0, file_contents.len()).unwrap();
                let mut stack = CommentScopeStack::new(file_span.clone());
                for line in file_span.lines_span() {
                    for todo_error in stack
                        .process_ops_for_line(
                            state.parse_line(line.as_str(), &ps).into_iter(),
                            line,
                        )
                        .into_iter()
                        .flat_map(|comment| {
                            TodoError::from_comment(&config, file_path, comment).into_iter()
                        })
                    {
                        stderr.write_error(&todo_error)?;
                    }
                }
            } else {
                eprintln!("NO SYNTAX FOUND FOR: {:?}", entry.path());
            }
        }
    }

    Ok(())
}
