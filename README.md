# `report_todo`

Work-in-progress standalone replacement for `rustfmt`'s `report_todo` feature (that was removed in [rust-lang/rustfmt#4129](https://github.com/rust-lang/rustfmt/pull/4129)).

Design goals:

- Detect comments with the string `TODO` (or `FIXME`, etc) in the source files of a project (not limited to Rust files!).
- Detect and allow comments in the format `TODO(#{issue_num}): ...`.
- In Rust source, detect the use of `todo!()` and suggest replacing with a TODO comment and `unimplemented!()`.

## License

Licensed under either of

- Apache License, Version 2.0
  ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license
  ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
