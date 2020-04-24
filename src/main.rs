use std::{
    fs::File,
    io::BufRead,
    io::BufReader,
    path::{Path, PathBuf},
};
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream};

struct Options {
    root_dir: Option<PathBuf>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = Options {
        root_dir: Some(PathBuf::from(".")),
    };

    let mut stderr = StandardStream::stderr(ColorChoice::Auto);

    let todo_with_issue = regex::RegexBuilder::new(r"\btodo\(#(\d+)\):")
        .case_insensitive(true)
        .build()?;

    let todo_macro = regex::RegexBuilder::new(r#"\btodo!\((".*")?\)"#).build()?;

    let todo = regex::RegexBuilder::new(r"\btodo\b")
        .case_insensitive(true)
        .build()?;

    for entry in ignore::Walk::new(&options.root_dir.unwrap_or_else(|| PathBuf::from("."))) {
        let entry = entry?;
        if entry.file_type().unwrap().is_file() {
            let file = File::open(entry.path())?;
            for (i, line) in BufReader::new(file).lines().enumerate() {
                if let Ok(line) = line {
                    if let Some(capture) = todo_with_issue.captures(&line) {
                        // ok todo

                        TodoError {
                            level: Level::Todo(capture.get(1).unwrap().as_str()),
                            row: i + 1,
                            col: capture.get(0).unwrap().start() + 1,
                            file_path: entry.path(),
                            line: &line,
                            message: &line[capture.get(0).unwrap().end()..].trim(),
                            help_message: &format!(
                                "link: https://github.com/tangmi/cargo-report-todo/issues/{}",
                                capture.get(1).unwrap().as_str()
                            ),
                        }
                        .write_colored(&mut stderr)?;
                    } else if let Some(m) = todo_macro.find(&line) {
                        TodoError {
                            level: Level::Error,
                            row: i + 1,
                            col: m.start() + 1,
                            file_path: entry.path(),
                            line: &line,
                            message: "todo macro detected",
                            help_message: "help: TODO: what kind of guidance is good here?",
                        }
                        .write_colored(&mut stderr)?;
                    } else if let Some(m) = todo.find(&line) {
                        // bad todo

                        TodoError {
                            level: Level::Error,
                            row: i + 1,
                            col: m.start() + 1,
                            file_path: entry.path(),
                            line: &line,
                            message: "todo detected without issue number",
                            help_message:
                                "help: create a work item and reference it here (e.g. `TODO(#1): ...`)",
                        }.write_colored(&mut stderr)?;
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

    /// 1-indexed
    row: usize,
    /// 1-indexed
    col: usize,

    file_path: &'a Path,

    line: &'a str,
    message: &'a str,
    help_message: &'a str,
}

impl TodoError<'_> {
    fn write_colored(&self, writer: &mut impl termcolor::WriteColor) -> std::io::Result<()> {
        let spacing = " ".repeat(format!("{}", self.row).len());
        let underline = " ".repeat(self.col - 1) + "^^^^";

        match self.level {
            Level::Warning => {
                writer.set_color(ColorSpec::new().set_fg(Some(Color::Yellow)))?;
                write!(writer, "warning: ")?;
            }
            Level::Error => {
                writer.set_color(ColorSpec::new().set_fg(Some(Color::Red)))?;
                write!(writer, "error: ")?;
            }
            Level::Todo(issue) => {
                writer.set_color(ColorSpec::new().set_fg(Some(Color::Blue)))?;
                write!(writer, "TODO(#{}): ", issue)?;
            }
        }
        writer.reset()?;
        write!(writer, "{}\n", self.message)?;

        let mut gutter_color = ColorSpec::new();
        gutter_color.set_fg(Some(Color::Magenta));

        writer.set_color(&gutter_color)?;
        write!(writer, "{}--> ", spacing)?;
        writer.reset()?;
        write!(
            writer,
            "{p}{l}:{c}\n",
            p = format!("{}:", self.file_path.display()),
            l = self.row,
            c = self.col,
        )?;

        writer.set_color(&gutter_color)?;
        write!(writer, "{} |\n", spacing)?;

        write!(writer, "{} | ", self.row)?;
        writer.reset()?;
        write!(writer, "{}\n", self.line)?;

        writer.set_color(&gutter_color)?;
        write!(writer, "{} | ", spacing)?;
        writer.reset()?;
        write!(writer, "{}\n", underline)?;

        writer.set_color(&gutter_color)?;
        write!(writer, "{} |\n", spacing)?;

        write!(writer, "{} = ", spacing)?;
        writer.reset()?;
        write!(writer, "{}\n\n", self.help_message)?;

        Ok(())
    }
}
