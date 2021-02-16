//! Find TODOs in the diff since the fork point.

use std::{collections::VecDeque, iter::Peekable, ops::Range, path::PathBuf, str::Lines};

use anyhow::{anyhow, Context};
use log::debug;
use span::Span;

use crate::todo_error::{Regexes, TodoError};

use super::Checker;

pub struct GitDiffChecker {}

impl Checker for GitDiffChecker {
    fn process_spans(&self, config: &Regexes) -> anyhow::Result<Vec<TodoError>> {
        debug!("Running `git remote -v`");
        let remote = duct::cmd!("git", "remote", "-v")
            .read()?
            .lines()
            .find_map(|line| {
                if line.trim().starts_with("upstream") {
                    Some("upstream")
                } else {
                    None
                }
            })
            .unwrap_or("origin");

        debug!("Running `git remote show {}`", remote);
        let remote_ref = format!(
            "{}/{}",
            remote,
            duct::cmd!("git", "remote", "show", remote)
                .read()?
                .lines()
                .find_map(|line| {
                    let line = line.trim();
                    if line.starts_with("HEAD branch: ") {
                        Some(&line["HEAD branch: ".len()..])
                    } else {
                        None
                    }
                })
                .unwrap_or("master")
        );

        debug!("Running `git merge-base --fork-point {}`", remote_ref);
        let fork_point = duct::cmd!("git", "merge-base", "--fork-point", remote_ref).read()?;

        debug!("Running `git diff --unified=0 {}`", fork_point);
        let diff = duct::cmd!("git", "diff", "--unified=0", &fork_point)
            .stderr_null()
            .read()?;

        let mut patch = UnifiedDiffParser::new(&diff)?;

        let mut todo_errors = Vec::new();

        loop {
            if !patch.has_more() {
                break;
            }

            let hunk = patch.read_hunk()?;
            let path = PathBuf::from(&format!(
                ".{}{}",
                std::path::MAIN_SEPARATOR,
                hunk.file
                    .replace("/", &std::path::MAIN_SEPARATOR.to_string())
            ));

            for line in &hunk.added {
                todo_errors.extend(TodoError::from_line(config, &path, line.line, line.row));
            }
        }

        Ok(todo_errors)
    }
}

#[derive(Debug)]
pub struct UnifiedDiffParser<'a> {
    source: &'a str,

    lines: Peekable<Lines<'a>>,

    current_file: &'a str,
    current_patch_remove: Range<usize>,
    current_patch_add: Range<usize>,
}

impl<'a> UnifiedDiffParser<'a> {
    pub fn new(source: &'a str) -> anyhow::Result<Self> {
        let mut parser = UnifiedDiffParser {
            source,
            lines: source.lines().peekable(),
            current_file: "",
            current_patch_remove: Range { start: 0, end: 0 },
            current_patch_add: Range { start: 0, end: 0 },
        };

        parser.eat_file_header()?;

        Ok(parser)
    }

    fn has_more(&mut self) -> bool {
        self.lines.peek().is_some()
    }

    fn eat_file_header(&mut self) -> anyhow::Result<()> {
        loop {
            let line = self.lines.peek().context("next line exists")?;
            if line.starts_with("--- ") || line.starts_with("+++ ") {
                break;
            } else {
                // ignore line
                self.lines.next().unwrap();
            }
        }

        let source_file_line = self.lines.next().context("source line exists")?;
        if !source_file_line.starts_with("--- ") {
            return Err(anyhow!("remove line invalid: {}", source_file_line));
        }

        let target_file_line = self.lines.next().context("target line exists")?;
        if !target_file_line.starts_with("+++ ") {
            return Err(anyhow!("add line invalid: {}", target_file_line));
        }

        self.current_file = target_file_line.strip_prefix("+++ b/").unwrap();

        Ok(())
    }

    fn read_hunk(&mut self) -> anyhow::Result<Hunk> {
        // @@ -26,0 +27,6 @@ dependencies = [
        let line = self.lines.next().context("next line exists")?;

        let mut parts = line
            .strip_prefix("@@ ")
            .context(anyhow!("patch line invalid: {}", line))?
            .split(" ");

        let current_patch_remove = {
            let removed = parts
                .next()
                .unwrap()
                .strip_prefix("-")
                .context(anyhow!("patch missing removed section: {}", line))?;
            let mut parts = removed.split(",");
            let removed_row: usize = parts
                .next()
                .unwrap()
                .parse()
                .context("failed to parse row")?;
            let removed_len: usize = parts
                .next()
                .context("missing removed length")
                .and_then(|l| l.parse().context("failed to parse length"))
                .unwrap_or(1);

            Range {
                start: removed_row,
                end: removed_row + removed_len,
            }
        };

        let current_patch_add = {
            let added = parts
                .next()
                .unwrap()
                .strip_prefix("+")
                .context(anyhow!("patch missing added section: {}", line))?;
            let mut parts = added.split(",");
            let added_row: usize = parts
                .next()
                .unwrap()
                .parse()
                .context("failed to parse row")?;
            let added_len: usize = parts
                .next()
                .context("missing added length")
                .and_then(|l| l.parse().context("failed to parse length"))
                .unwrap_or(1);

            Range {
                start: added_row,
                end: added_row + added_len,
            }
        };

        let mut hunk = Hunk {
            file: self.current_file,
            removed: Vec::new(),
            added: Vec::new(),
        };

        for row in current_patch_remove {
            let line = self.lines.next().context("missing removed line")?[1..].trim_end();
            hunk.removed.push(ChangedLine { line, row })
        }

        for row in current_patch_add {
            let line = self.lines.next().context("missing added line")?[1..].trim_end();
            hunk.added.push(ChangedLine { line, row })
        }

        if self
            .lines
            .peek()
            .map(|line| line.starts_with("diff "))
            .unwrap_or(false)
        {
            self.eat_file_header()?;
        }

        Ok(hunk)
    }
}

#[derive(Debug)]
struct Hunk<'a> {
    file: &'a str,
    removed: Vec<ChangedLine<'a>>,
    added: Vec<ChangedLine<'a>>,
}

#[derive(Debug)]
struct ChangedLine<'a> {
    line: &'a str,
    row: usize,
}
