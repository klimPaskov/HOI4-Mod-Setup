# Fuzzing

The parser and path-boundary targets are kept separate from the production workspace so contributors can install `cargo-fuzz` only when they need it.

```text
cargo install cargo-fuzz
cargo fuzz run manifest -- -max_total_time=60
cargo fuzz run relative_path -- -max_total_time=60
cargo fuzz run codex_analysis -- -max_total_time=60
cargo fuzz run descriptor -- -max_total_time=60
cargo fuzz run toml_merge -- -max_total_time=60
cargo fuzz run flatten -- -max_total_time=60
```

Targets must never receive real credentials or private mod projects. Crashes are minimized locally and attached as sanitized regression tests in `src-tauri/src/`.
