use anyhow::anyhow;
use checkers::{git_diff::GitDiffChecker, source_tree::SourceTreeChecker, Checker};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use structopt::StructOpt;
use thiserror::Error;

mod checkers;
mod console_emitter;
mod todo_error;

use todo_error::Regexes;

/// Will ignore files listed in `.todoignore` and `.gitignore`.
#[derive(Debug, StructOpt)]
struct Opt {
    #[structopt(flatten)]
    config: Config,

    #[structopt(long = "diff")]
    parse_diff: bool,

    #[structopt(name = "ROOT_DIR")]
    root_dir: Option<PathBuf>,
}

// TODO(#7): add custom sublime-syntax files?
// TODO(#6): json output? ide-friendly output?
// TODO(#6): find todo by tracking number
#[derive(Debug, StructOpt, Serialize, Deserialize)]
struct Config {
    /// Regex to detect an issue with tracking idenfitied (i.e. GitHub issue number).
    #[structopt(
        long = "match-issue",
        default_value = r"todo\(#(?P<issue_number>\d+)\):"
    )]
    match_issue: String,

    /// Regex replace string used to format the output link. e.g. `https://github.com/tangmi/report_todo/issues/${issue_number}`
    #[structopt(long = "issue-link-format")]
    issue_link_format: Option<String>,

    /// Expected to match `\w+`.
    #[structopt(long = "forbid", default_value = "todo")]
    forbidden_keywords: Vec<String>,

    /// Report tracked issues as well as untracked.
    #[structopt(long = "all")]
    report_all: bool,
}

fn main() -> anyhow::Result<()> {
    // TODO(#6): try finding a config file in current directory first
    let opt = Opt::from_args();

    // let opt = Opt {
    //     // Global conifg? `dirs::config_dir`?
    //     config: toml::from_str(&std::fs::read_to_string(concat!(
    //         env!("CARGO_MANIFEST_DIR"),
    //         "/report_todo.toml"
    //     ))?)?,
    //     root_dir: PathBuf::from("."),
    // };

    if cfg!(debug_assertions) {
        env_logger::builder()
            .filter_level(log::LevelFilter::Debug)
            .init();
    } else {
        env_logger::init();
    }

    let regexes = Regexes {
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

    let mut issues_found_count = 0_usize;
    let mut has_untracked = false;

    let config = opt.config;

    let checker: Box<dyn Checker> = if opt.parse_diff {
        Box::new(GitDiffChecker {})
    } else {
        Box::new(SourceTreeChecker {
            root_dir: opt.root_dir.unwrap_or(PathBuf::from(".")),
        })
    };

    for todo_error in checker
        .process_spans(&regexes)?
        .into_iter()
        .filter(|todo_error| {
            if !todo_error.is_tracked() || (config.report_all && todo_error.is_tracked()) {
                true
            } else {
                false
            }
        })
    {
        issues_found_count += 1;
        if !todo_error.is_tracked() {
            has_untracked = true;
        }

        stderr.write_error(&todo_error)?;
    }

    if issues_found_count > 0 {
        eprintln!("{} issues found.", issues_found_count)
    }

    if has_untracked {
        return Err(anyhow!("untracked issues found!"));
    }

    Ok(())
}
