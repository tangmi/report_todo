# `report-todo`

Work-in-progress standalone replacement for `rustfmt`'s `report_todo` feature (that was removed in [rust-lang/rustfmt#4129](https://github.com/rust-lang/rustfmt/pull/4129)).

Design goals:

- Detect comments with the string `TODO` (or `FIXME`, etc) in the source files of a project (not limited to Rust files!).
- Detect and allow comments in the format `TODO(#{issue_num}): ...`.
- In Rust source, detect the use of `todo!()` and suggest replacing with a TODO comment and `unimplemented!()`.

Open questions:

- How can I handle nested languages (e.g. JS in HTML)
