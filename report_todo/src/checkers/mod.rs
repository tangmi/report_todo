use crate::todo_error::{Regexes, TodoError};

pub mod git_diff;
pub mod source_tree_simple;
pub mod source_tree_syntect;

pub trait Checker {
    fn process_spans(&self, process_span: &Regexes) -> anyhow::Result<Vec<TodoError>>;
}
