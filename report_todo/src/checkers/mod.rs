use std::path::Path;

use span::Span;

use crate::todo_error::{Regexes, TodoError};

pub mod git_diff;
pub mod source_tree;

pub struct TodoBuilder {
    config: Regexes,
}

pub trait Checker {
    fn process_spans(&self, process_span: &Regexes) -> anyhow::Result<Vec<TodoError>>;
}
