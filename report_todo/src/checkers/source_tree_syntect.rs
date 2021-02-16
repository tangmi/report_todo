//! Inspect all files in a source tree and use `syntect` to only parse comments.

use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use ignore::{DirEntry, WalkState};
use log::debug;
use span::Span;

use crate::todo_error::{Regexes, TodoError};

use super::Checker;

// TODO(#4): capture usages of `todo!()` macro in rust?
struct ScopeTracker<'a> {
    state: syntect::parsing::ParseState,
    syntax_set: &'a syntect::parsing::SyntaxSet,
    lines: span::LinesSpan<'a>,

    /// original text that's being parsed
    file_contents: String,
    original: Span<'a>,

    comment_start_index: Option<usize>,
    comment_level: usize,
    comment_prefix_scope: syntect::parsing::Scope,

    stack: syntect::parsing::ScopeStack,

    pending: std::collections::VecDeque<(syntect::parsing::Scope, &'a str)>,
}

impl<'a> ScopeTracker<'a> {
    // pub fn new(
    //     syntax_set: &'a syntect::parsing::SyntaxSet,
    //     file_path: &Path,
    // ) -> std::io::Result<Option<Self>> {
    //     if let Some(syntax_ref) = syntax_set
    //         .find_syntax_for_file(file_path)
    //         .expect("opening source file should work")
    //     {
    //         let file_contents = std::fs::read_to_string(file_path)?;
    //         let file_span = Span::new(&file_contents, 0, file_contents.len()).unwrap();
    //         let mut stack = CommentScopeStack::new(file_span.clone());

    //         Ok(Some(ScopeTracker {
    //             state: syntect::parsing::ParseState::new(syntax_ref),
    //             syntax_set,
    //             lines: file_span.lines_span(),
    //             file_contents,
    //             original: Span::new(&file_contents, 0, file_contents.len()).unwrap(),
    //             comment_start_index: None,
    //             comment_level: 0,
    //             comment_prefix_scope: syntect::parsing::Scope::new("comment").unwrap(),
    //             stack: syntect::parsing::ScopeStack::new(),
    //             pending: std::collections::VecDeque::new(),
    //         }))
    //     } else {
    //         Ok(None)
    //     }
    // }
}

impl<'a> Iterator for ScopeTracker<'a> {
    type Item = (syntect::parsing::Scope, &'a str);

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(next) = self.pending.pop_front() {
            Some(next)
        } else {
            // get some more
            // self.lines.next().and_then(|line| {
            //     for (offset, op) in self
            //         .state
            //         .parse_line(line.as_str(), self.syntax_set)
            //         .into_iter()
            //     {
            //         self.stack.apply_with_hook(&op, |op, scopes| {
            //             match op {
            //                 syntect::parsing::BasicScopeStackOp::Push(scope) => {}
            //                 syntect::parsing::BasicScopeStackOp::Pop => {}
            //             }
            //         });
            //     }

            //     None
            // });

            todo!()
        }
    }
}

/// Struct to keep track of `syntect`'s scopes and to emit substrings that are comments.
///
/// TODO(#5): this + the syntect parser could be combined into a single iterator. the syntect ScopeStackOps would have to be staged and only processed when another comment is requested.
/// TODO(#4): refactor to allow multiple scopes to scan for?
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
    /// TODO(#5): should "get comments" be a callback?
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

pub struct SourceTreeSyntectChecker {
    pub root_dir: PathBuf,
}

impl Checker for SourceTreeSyntectChecker {
    fn process_spans(&self, config: &Regexes) -> anyhow::Result<Vec<TodoError>> {
        let todo_errors = Arc::new(Mutex::new(Vec::new()));

        let syntax_set = {
            const LINES_INCLUDE_NEWLINE: bool = true;
            let mut builder = if LINES_INCLUDE_NEWLINE {
                syntect::parsing::SyntaxSet::load_defaults_newlines()
            } else {
                syntect::parsing::SyntaxSet::load_defaults_nonewlines()
            }
            .into_builder();
            // Use this instead of `add_from_folder` to compile the syntaxes into the program.
            // TODO(#7): create separate dump? https://github.com/trishume/syntect/blob/master/src/dumps.rs
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

        let num_threads = num_cpus::get() - 2;
        debug!("Using {} threads", num_threads);

        ignore::WalkBuilder::new(&self.root_dir)
            .add_custom_ignore_filename(".todoignore")
            .threads(num_threads)
            .build_parallel()
            .run(|| {
                let todo_errors = todo_errors.clone();
                let syntax_set = syntax_set.clone();

                Box::new(move |entry| {
                    let entry = entry.expect("walking directory entry should not have i/o errors");
                    let file_path = entry.path();
                    if file_path.is_file() {
                        if let Ok(Some(syntax_ref)) = syntax_set.find_syntax_for_file(entry.path())
                        {
                            debug!("working on {}", file_path.display());

                            let mut state = syntect::parsing::ParseState::new(syntax_ref);

                            let file_contents = std::fs::read_to_string(file_path).unwrap();
                            let file_span =
                                Span::new(&file_contents, 0, file_contents.len()).unwrap();
                            let mut stack = CommentScopeStack::new(file_span.clone());
                            for line in file_span.lines_span() {
                                todo_errors.lock().unwrap().extend(
                                    stack
                                        .process_ops_for_line(
                                            state
                                                .parse_line(line.as_str(), &syntax_set)
                                                .into_iter(),
                                            line,
                                        )
                                        .into_iter()
                                        .flat_map(|span| {
                                            TodoError::from_comment(config, file_path, span)
                                        }),
                                );
                            }
                        } else {
                            debug!("Ignoring file: {:?}. No syntax set found.", entry.path());
                        }
                    }

                    WalkState::Continue
                })
            });

        Ok(Arc::try_unwrap(todo_errors).unwrap().into_inner().unwrap())
    }
}
