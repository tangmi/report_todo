use crate::console_emitter::{ColoredWriter, Style};
use regex::Regex;
use span::*;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct Regexes {
    /// Expects a single capture
    pub match_issue: Regex,

    /// Expects a single string interpolation (`{replace_name}`) in which the capture from
    pub issue_link_format: Option<String>,

    /// List of regexes of forbidden words
    pub bad_keywords: Vec<Regex>,
}

#[derive(Debug)]
pub struct TodoError {
    /// An identifier tracking the issue, e.g. a GitHub issue number.
    tracking_id: Option<String>,

    /// The line containing the issue, with no trailing whitespace
    original_line: String,

    /// Length of just the matching issue span.
    span_len: usize,

    row: usize,
    col: usize,

    file_path: PathBuf,

    message: String,
    help_message: Option<String>,
}

impl TodoError {
    pub fn is_tracked(&self) -> bool {
        self.tracking_id.is_some()
    }

    pub fn from_line(config: &Regexes, file_path: &Path, line: &str, row: usize) -> Vec<TodoError> {
        let mut issues = Vec::new();
        if let Some(capture) = config.match_issue.captures(line) {
            let (todo_start_index, todo_end_index) = {
                let m = capture.get(0).unwrap();
                (m.start(), m.end())
            };

            issues.push(TodoError {
                tracking_id: Some(capture.get(1).unwrap().as_str().to_owned()),
                file_path: file_path.to_owned(),

                original_line: line.to_owned(),
                span_len: line[todo_start_index..].trim().len(),
                row,
                col: todo_start_index + 1,

                message: line[todo_end_index + 1..].trim().to_owned(),
                help_message: config.issue_link_format.as_ref().map(|issue_link_format| {
                    format!(
                        "link: {}",
                        config
                            .match_issue
                            .replace(
                                &line[todo_start_index..=todo_end_index],
                                issue_link_format.as_str()
                            )
                            .trim()
                    )
                }),
            });
        } else {
            for keyword in &config.bad_keywords {
                if let Some(m) = keyword.find(line) {
                    issues.push(TodoError {
                        tracking_id: None,

                        original_line: line.to_owned(),
                        span_len: line[m.start()..].trim().len(),
                        row,
                        col: m.range().start + 1,

                        file_path: file_path.to_owned(),
                        message: format!(
                            "{} found without issue number",
                            m.as_str().to_uppercase()
                        ),

                        // TODO(#7): Try and generate an example from `config.match_issue` regex?
                        help_message: Some(
                            "help: create a work item and reference it here (e.g. `TODO(#1): ...`)"
                                .to_owned(),
                        ),
                    });
                }
            }
        }

        issues
    }

    /// `comment` is potentially multiline.
    pub fn from_comment(config: &Regexes, file_path: &Path, comment: Span) -> Vec<TodoError> {
        comment
            .lines_span()
            .filter(|line| !line.as_str().trim().is_empty())
            .flat_map(|line| {
                Self::from_line(
                    config,
                    file_path,
                    line.as_str(),
                    line.start_pos().line_col().0,
                )
            })
            .collect()
    }
}

impl ColoredWriter {
    pub fn write_error(&mut self, todo: &TodoError) -> std::io::Result<()> {
        let line_trimmed = todo.original_line.trim();
        let display_col = todo.col
            - todo
                .original_line
                .find(|c| !char::is_whitespace(c))
                .unwrap_or(0);

        let spacing = " ".repeat(format!("{}", todo.row).len());
        let underline = " ".repeat(display_col - 1)
            + &"^".repeat({
                // `.trim()` ignores the newline characters
                todo.span_len
            });

        match &todo.tracking_id {
            None => self.write("error", Style::Error)?,

            // TODO(#7): find a way to preserve user-configured pattern?
            Some(issue) => self.write(format!("TODO(#{})", issue), Style::Info)?,
        }
        self.write(format!(": {}\n", todo.message), Style::Bold)?;
        self.write(format!("{}--> ", spacing), Style::LineNumber)?;
        self.write(
            format!(
                "{p}{l}:{c}\n",
                p = format!("{}:", todo.file_path.display()),
                l = todo.row,
                c = todo.col,
            ),
            Style::Normal,
        )?;
        self.write(format!("{} |\n", spacing), Style::LineNumber)?;
        self.write(format!("{} | ", todo.row), Style::LineNumber)?;
        self.write(format!("{}\n", line_trimmed), Style::Normal)?;
        self.write(format!("{} | ", spacing), Style::LineNumber)?;
        self.write(
            format!("{}\n", underline),
            if todo.tracking_id.is_some() {
                Style::Info
            } else {
                Style::Error
            },
        )?;
        self.write(format!("{} |\n", spacing), Style::LineNumber)?;
        if let Some(help_message) = &todo.help_message {
            self.write(format!("{} = ", spacing), Style::LineNumber)?;
            self.write(format!("{}\n", help_message), Style::Normal)?;
        }
        self.write("\n", Style::Normal)?;

        Ok(())
    }
}
