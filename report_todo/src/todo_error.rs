use crate::console_emitter::{ColoredWriter, Style};
use regex::Regex;
use span::*;
use std::path::Path;

#[derive(Debug)]
pub struct Regexes {
    /// Expects a single capture
    pub match_issue: Regex,

    /// Expects a single string interpolation (`{replace_name}`) in which the capture from
    pub issue_link_format: String,

    /// List of regexes of forbidden words
    pub bad_keywords: Vec<Regex>,
}

#[derive(Debug)]
pub enum Level<'a> {
    Warning,
    Error,
    Todo(&'a str),
}

#[derive(Debug)]
pub struct TodoError<'a> {
    level: Level<'a>,

    span: Span<'a>,

    file_path: &'a Path,

    message: String,
    help_message: String,
}

impl<'a> TodoError<'a> {
    pub fn is_tracked(&self) -> bool {
        matches!(self.level, Level::Todo(_))
    }

    /// `comment` is potentially multiline.
    pub fn from_comment(
        config: &Regexes,
        file_path: &'a Path,
        comment: Span<'a>,
    ) -> Vec<TodoError<'a>> {
        let mut issues = Vec::new();

        for line in comment
            .lines_span()
            .filter(|line| !line.as_str().trim().is_empty())
        {
            if let Some(capture) = config.match_issue.captures(line.as_str()) {
                let (todo_start_index, todo_end_index) = {
                    let m = capture.get(0).unwrap();
                    (m.start(), m.end())
                };

                let todo_substr = line
                    .sub_span(todo_start_index..=todo_end_index)
                    .unwrap()
                    .as_str();

                issues.push(TodoError {
                    level: Level::Todo(capture.get(1).unwrap().as_str()),
                    span: line.sub_span(todo_start_index..).unwrap(),
                    file_path,
                    message: line.as_str()[todo_end_index + 1..].trim().to_owned(),
                    help_message: format!(
                        "link: {}",
                        config
                            .match_issue
                            .replace(todo_substr, config.issue_link_format.as_str())
                            .trim()
                    ),
                });
            } else {
                for keyword in &config.bad_keywords {
                    if let Some(m) = keyword.find(line.as_str()) {
                        issues.push(TodoError {
                            level: Level::Error,
                            span: line.sub_span(m.start()..).unwrap(),
                            file_path,
                            message: format!("{} found without issue number", m.as_str().to_uppercase()),

                            // TODO(#7): Try and generate an example from `config.match_issue` regex?
                            help_message: "help: create a work item and reference it here (e.g. `TODO(#1): ...`)".to_owned(),
                        });
                    }
                }
            }
        }

        issues
    }
}

impl ColoredWriter {
    pub fn write_error(&mut self, todo: &TodoError<'_>) -> std::io::Result<()> {
        let pos = todo.span.start_pos();
        let (row, col) = pos.line_col();

        let spacing = " ".repeat(format!("{}", row).len());
        let underline = " ".repeat(col - 1)
            + &"^".repeat({
                // `.trim()` ignores the newline characters
                todo.span.as_str().trim().len()
            });

        match todo.level {
            Level::Warning => self.write("warning", Style::Warning)?,
            Level::Error => self.write("error", Style::Error)?,

            // TODO(#7): find a way to preserve user-configured pattern?
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
