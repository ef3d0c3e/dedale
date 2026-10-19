#!/usr/bin/env bash
# Runs every quality gate. Usage: scripts/check.sh [--fuzz <seconds per target>]
set -euo pipefail
cd "$(dirname "$0")/.."

fuzz_secs=""
while (($#)); do
    case "$1" in
    --fuzz)
        fuzz_secs="${2:?--fuzz needs a number of seconds}"
        shift 2
        ;;
    *)
        echo "unknown argument: $1" >&2
        exit 2
        ;;
    esac
done

step() { printf '\n==> %s\n' "$*"; }

step "rustfmt"
cargo fmt --all --check
cargo fmt --manifest-path fuzz/Cargo.toml --check

step "clippy"
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo clippy --manifest-path fuzz/Cargo.toml --all-targets --locked -- -D warnings

step "tests"
cargo test --workspace --all-features --locked

step "python tests"
(cd bindings/python && PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s tests -t .)

step "stable build"
if rustup toolchain list | grep -q '^stable'; then
    rustup run stable cargo check --workspace --all-targets --all-features --locked
else
    echo "skipped: no stable toolchain installed"
fi

step "cargo-deny"
cargo deny --locked check

if [[ -n $fuzz_secs ]]; then
    step "fuzz (${fuzz_secs}s per target)"
    for target in $(rustup run nightly cargo fuzz list); do
        # Room for a full-size line or map (64 KiB), beyond libFuzzer's 4 KiB default.
        args=(-max_total_time="$fuzz_secs" -max_len=70000)
        if [[ -f fuzz/dicts/$target.dict ]]; then
            args+=(-dict="fuzz/dicts/$target.dict")
        fi
        corpus=(fuzz/corpus/"$target")
        if [[ -d fuzz/seeds/$target ]]; then
            corpus+=(fuzz/seeds/"$target")
        fi
        mkdir -p "${corpus[0]}"
        rustup run nightly cargo fuzz run "$target" "${corpus[@]}" -- "${args[@]}"
    done
fi

step "all checks passed"
