# Fuzzing

Targets live in `fuzz_targets/`, one per parser, decoder or engine entry point.
Fuzzing needs a nightly toolchain and `cargo-fuzz` (`rustup run nightly` if
nightly is not your default):

```sh
cargo fuzz list
cargo fuzz run <target> -- -max_total_time=60
```

`scripts/check.sh --fuzz <seconds>` runs every target for that long.
