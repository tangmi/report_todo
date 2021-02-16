//! Inspect all files in a source tree and look for TODOs in each line.

use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use ignore::WalkState;
use log::debug;

use crate::todo_error::{Regexes, TodoError};

use super::Checker;

pub struct SourceTreeSimpleChecker {
    pub root_dir: PathBuf,
}

impl Checker for SourceTreeSimpleChecker {
    fn process_spans(&self, config: &Regexes) -> anyhow::Result<Vec<TodoError>> {
        let todo_errors = Arc::new(Mutex::new(Vec::new()));

        let num_threads = num_cpus::get() - 2;
        debug!("Using {} threads", num_threads);

        ignore::WalkBuilder::new(&self.root_dir)
            .add_custom_ignore_filename(".todoignore")
            .threads(num_threads)
            .build_parallel()
            .run(|| {
                let todo_errors = todo_errors.clone();

                Box::new(move |entry| {
                    let entry = entry.expect("walking directory entry should not have i/o errors");
                    let file_path = entry.path();
                    if file_path.is_file() {
                        if let Ok(file_contents) = std::fs::read_to_string(file_path) {
                            for (row_zero_indexed, line) in file_contents.lines().enumerate() {
                                todo_errors.lock().unwrap().extend(TodoError::from_line(
                                    config,
                                    file_path,
                                    line,
                                    row_zero_indexed + 1,
                                ));
                            }
                        }
                    }

                    WalkState::Continue
                })
            });

        Ok(Arc::try_unwrap(todo_errors).unwrap().into_inner().unwrap())
    }
}
