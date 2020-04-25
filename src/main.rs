use std::path::{Path, PathBuf};
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream};

use pest::Parser;
use pest_derive::Parser;

#[derive(Parser)]
#[grammar = "mini_languages.pest"]
pub struct CommentParser;

struct Options {
    root_dir: Option<PathBuf>,

    todo_with_issue_regex: String,
    todo_issue_link_replace: String,
    todo_keywords: Vec<String>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = Options {
        root_dir: Some(PathBuf::from(".")),
        todo_with_issue_regex: r"todo\(#(?P<issue_number>\d+)\):".to_owned(),
        todo_issue_link_replace: r"https://github.com/tangmi/report-todo/issues/${issue_number}"
            .to_owned(),
        todo_keywords: vec!["todo".to_lowercase(), "fixme".to_lowercase()],
    };

    let mut stderr = StandardStream::stderr(ColorChoice::Auto);

    let todo_with_issue =
        regex::RegexBuilder::new(&format!(r"\b{}", options.todo_with_issue_regex))
            .case_insensitive(true)
            .build()?;

    let todo_keyword_regexes = options
        .todo_keywords
        .iter()
        .map(|keyword| {
            regex::RegexBuilder::new(&format!(r"\b{}\b", keyword))
                .case_insensitive(true)
                .build()
        })
        .collect::<Result<Vec<_>, _>>()?;

    for entry in ignore::Walk::new(&options.root_dir.unwrap_or_else(|| PathBuf::from("."))) {
        let entry = entry?;

        dbg!(entry.path());

        if let Some(extension) = entry.path().extension() {
            let file = std::fs::read_to_string(entry.path()).unwrap();
            let mut a = CommentParser::parse(
                // TODO many more languages!
                match extension.to_string_lossy().as_ref() {
                    "rs" => Rule::rust_file,

                    "toml" => Rule::toml_file,

                    "c" | "h" | "cc" | "c++" | "cpp" => Rule::c_file,

                    // Unrecognized language, should skip?
                    _ => continue,
                },
                &file,
            )
            .unwrap_or_else(|err| {
                println!("{}", err);
                // print_error(src, err);
                panic!();
            });

            for pair in a.next().unwrap().into_inner() {
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

                // TODO: this is super duper hacky
                let span: Span = pair.as_span().into();

                match pair.as_rule() {
                    Rule::rust_todo_macro => {
                        TodoError {
                            level: Level::Error,
                            span: span.offset(0).into(),
                            file_path: entry.path(),
                            message: "`todo!()` macro invocation",
                            help_message: "help: replace with `unimplemented!()` and a TODO comment with a linked work item.",
                        }
                        .write_colored(&mut stderr)?;
                    }

                    Rule::rust_file_comment | Rule::hash_line_comment | Rule::c_file_comment => {
                        for line in span.lines() {
                            if let Some(capture) = todo_with_issue.captures(line) {
                                // ok todo

                                let (todo_start_index, todo_end_index) = {
                                    let m = capture.get(0).unwrap();
                                    (m.start(), m.end())
                                };

                                let todo_substr = &line[todo_start_index..=todo_end_index];

                                TodoError {
                                    level: Level::Todo(capture.get(1).unwrap().as_str()),
                                    span: span
                                        .offset({
                                            // TODO offset based on the line's span!
                                            // todo_start_index
                                            0
                                        })
                                        .into(),
                                    file_path: entry.path(),
                                    message: line[todo_end_index + 1..].trim(), // TODO span updates https://github.com/pest-parser/pest/issues/455
                                    help_message: &format!(
                                        "link: {}",
                                        todo_with_issue.replace(
                                            todo_substr,
                                            options.todo_issue_link_replace.as_str()
                                        )
                                    )
                                    .trim(),
                                }
                                .write_colored(&mut stderr)?;
                            } else {
                                for keyword in &todo_keyword_regexes {
                                    if let Some(m) = keyword.find(line) {
                                        // bad todo

                                        // TODO print out keyword in error message
                                        TodoError {
                                            level: Level::Warning,
                                            span: span.offset({
                                                // TODO offset based on the line's span!
                                                // todo_start_index
                                                0
                                            }).into(),
                                            file_path: entry.path(),
                                            message: "todo detected without issue number",
                                            help_message:
                                                "help: create a work item and reference it here (e.g. `TODO(#1): ...`)",
                                        }.write_colored(&mut stderr)?;
                                    }
                                }
                            }
                        }
                    }

                    Rule::c_string_literal => {
                        // println!("ignoring string literal! {:?}", pair.as_str());
                    }

                    unhandled_rule => {
                        println!("UNHANDLED RULE! {:?}", unhandled_rule);
                    }
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

impl TodoError<'_> {
    fn write_colored(&self, writer: &mut impl termcolor::WriteColor) -> std::io::Result<()> {
        let pos = self.span.start_pos();
        let (row, col) = pos.line_col();

        let spacing = " ".repeat(format!("{}", row).len());
        let underline = " ".repeat(col - 1) + &"^".repeat(self.span.as_str().len());

        match self.level {
            Level::Warning => {
                writer.set_color(ColorSpec::new().set_bold(true).set_fg(Some(Color::Yellow)))?;
                write!(writer, "warning")?;
            }
            Level::Error => {
                writer.set_color(ColorSpec::new().set_bold(true).set_fg(Some(Color::Red)))?;
                write!(writer, "error")?;
            }
            Level::Todo(issue) => {
                writer.set_color(ColorSpec::new().set_bold(true).set_fg(Some(Color::Blue)))?;
                write!(writer, "TODO(#{})", issue)?;
            }
        }
        writer.set_color(ColorSpec::new().set_bold(true))?;
        write!(writer, ": {}\n", self.message)?;
        writer.reset()?;

        let color_line_number = {
            let mut spec = ColorSpec::new();
            spec.set_bold(true);
            spec.set_intense(true);
            if cfg!(windows) {
                spec.set_fg(Some(Color::Cyan));
            } else {
                spec.set_fg(Some(Color::Blue));
            }
            spec
        };

        writer.set_color(&color_line_number)?;
        write!(writer, "{}--> ", spacing)?;
        writer.reset()?;
        write!(
            writer,
            "{p}{l}:{c}\n",
            p = format!("{}:", self.file_path.display()),
            l = row,
            c = col,
        )?;

        writer.set_color(&color_line_number)?;
        write!(writer, "{} |\n", spacing)?;

        write!(writer, "{} | ", row)?;
        writer.reset()?;
        write!(writer, "{}\n", pos.line_of().trim_end())?;

        writer.set_color(&color_line_number)?;
        write!(writer, "{} | ", spacing)?;
        writer.reset()?;
        write!(writer, "{}\n", underline)?;

        writer.set_color(&color_line_number)?;
        write!(writer, "{} |\n", spacing)?;

        write!(writer, "{} = ", spacing)?;
        writer.reset()?;
        write!(writer, "{}\n\n", self.help_message)?;

        Ok(())
    }
}
