use comment_extract::ExtractEvent;
use console_emitter::{ColoredWriter, Style};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use structopt::StructOpt;

mod comment_extract;
mod console_emitter;

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

#[derive(Debug)]
pub struct Regexes {
    pub with_issue: Regex,
    pub with_issue_replace: String,
    pub bad_keywords: Vec<Regex>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // let opt = Opt::from_args(); // TODO

    let opt = Opt {
        // Global conifg? `dirs::config_dir`?
        config: toml::from_str(&std::fs::read_to_string("report_todo.toml")?)?,
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

    for entry in ignore::Walk::new(&opt.root_dir) {
        let entry = entry?;

        let file_path = entry.path();
        dbg!(file_path);

        if file_path.is_file() {
            let file_contents = std::fs::read_to_string(file_path)?;
            let extraction = comment_extract::extract(entry.path(), &file_contents);
            match extraction {
                Ok(iter) => {
                    for event in iter {
                        /// HACK! This copies the layout of `pest::Span` so I can access `input`. Very unsafe!
                        #[derive(Debug, Clone, Copy)]
                        pub struct Span<'i> {
                            input: &'i str,
                            start: usize,
                            end: usize,
                        }

                        impl<'a> std::ops::Deref for Span<'a> {
                            type Target = pest::Span<'a>;
                            fn deref(&self) -> &Self::Target {
                                unsafe { std::mem::transmute(self) }
                            }
                        }

                        impl<'a> From<Span<'a>> for pest::Span<'a> {
                            fn from(span: Span<'a>) -> Self {
                                unsafe { std::mem::transmute(span) }
                            }
                        }

                        impl<'a> From<pest::Span<'a>> for Span<'a> {
                            fn from(span: pest::Span<'a>) -> Self {
                                unsafe { std::mem::transmute(span) }
                            }
                        }

                        impl<'a> Span<'a> {
                            pub fn offset(&self, delta: usize) -> Self {
                                Self {
                                    input: self.input,
                                    start: self.start + delta,
                                    end: self.end,
                                }
                            }
                        }

                        match event {
                            ExtractEvent::RustTodoMacro(span) => {
                                stderr.write_error(&TodoError {
                                level: Level::Error,
                                            span: span,
                                            file_path,
                                            message: "`todo!()` macro invocation",
                                            help_message: "help: replace with `unimplemented!()` and a TODO comment with a linked work item.",
                                        }
                                        )?;
                            }

                            ExtractEvent::Comment(span) => {
                                let span: Span = span.into();

                                for line in span.lines() {
                                    if let Some(capture) = config.with_issue.captures(line) {
                                        // ok todo

                                        let (todo_start_index, todo_end_index) = {
                                            let m = capture.get(0).unwrap();
                                            (m.start(), m.end())
                                        };

                                        let todo_substr = &line[todo_start_index..=todo_end_index];

                                        stderr.write_error(&TodoError {
                                            level: Level::Todo(capture.get(1).unwrap().as_str()),
                                            span: span
                                                .offset({
                                                    // TODO offset based on the line's span!
                                                    // todo_start_index
                                                    0
                                                })
                                                .into(),
                                            file_path,
                                            message: line[todo_end_index + 1..].trim(), // TODO span updates https://github.com/pest-parser/pest/issues/455
                                            help_message: &format!(
                                                "link: {}",
                                                config.with_issue.replace(
                                                    todo_substr,
                                                    config.with_issue_replace.as_str()
                                                )
                                            )
                                            .trim(),
                                        })?;
                                    } else {
                                        for keyword in &config.bad_keywords {
                                            if let Some(m) = keyword.find(line) {
                                                // bad todo

                                                // TODO print out keyword in error message
                                                stderr.write_error(&TodoError {
                                                            level: Level::Warning,
                                                            span: span.offset({
                                                                // TODO offset based on the line's span!
                                                                // todo_start_index
                                                                0
                                                            }).into(),
                                                            file_path,
                                                            message: "todo detected without issue number",
                                                            help_message:
                                                                "help: create a work item and reference it here (e.g. `TODO(#1): ...`)",
                                                        })?;
                                            }
                                        }
                                    }
                                }
                            }

                            ExtractEvent::StringLiteral(span) => {}

                            ExtractEvent::Unhandled(rule, span) => {
                                println!("UNHANDLED RULE! {:?}", rule);
                            }
                        }
                    }
                }
                Err(e) => {
                    dbg!(e);
                }
            }
        }
    }

    Ok(())
}

enum Level<'a> {
    Warning,
    Error,
    Todo(&'a str),
}

struct TodoError<'a> {
    level: Level<'a>,

    span: pest::Span<'a>,

    file_path: &'a Path,

    message: &'a str,
    help_message: &'a str,
}

impl ColoredWriter {
    fn write_error(&mut self, todo: &TodoError<'_>) -> std::io::Result<()> {
        let pos = todo.span.start_pos();
        let (row, col) = pos.line_col();

        let spacing = " ".repeat(format!("{}", row).len());
        let underline = " ".repeat(col - 1) + &"^".repeat(todo.span.as_str().len());

        match todo.level {
            Level::Warning => self.write("warning", Style::Warning)?,
            Level::Error => self.write("error", Style::Error)?,

            // TODO find a way to preserve user-configured pattern?
            Level::Todo(issue) => self.write(format!("TODO(#{})", issue), Style::Info)?,
        }
        self.write(format!(": {}\n", todo.message), Style::Bold)?;
        self.write(format!("{}--> ", spacing), Style::LineNumber)?;
        self.write(
            format!(
                "{p}{l}:{c}\n",
                p = format!("{}:", todo.file_path.display()),
                l = row,
                c = col,
            ),
            Style::Normal,
        )?;
        self.write(format!("{} |\n", spacing), Style::LineNumber)?;
        self.write(format!("{} | ", row), Style::LineNumber)?;
        self.write(format!("{}\n", pos.line_of().trim_end()), Style::Normal)?;
        self.write(format!("{} | ", spacing), Style::LineNumber)?;
        self.write(format!("{}\n", underline), Style::Normal)?;
        self.write(format!("{} |\n", spacing), Style::LineNumber)?;
        self.write(format!("{} = ", spacing), Style::LineNumber)?;
        self.write(format!("{}\n\n", todo.help_message), Style::Normal)?;

        Ok(())
    }
}
