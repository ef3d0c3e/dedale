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

step "Rust version"
# The declared minimum is the toolchain CI builds with, so CI proves it.
declared=$(sed -n 's/^rust-version = "\(.*\)"$/\1/p' Cargo.toml)
pinned=$(sed -n 's/^  RUST_VERSION: "\(.*\)"$/\1/p' .github/workflows/ci.yml)
if [[ $pinned != "$declared" && $pinned != "$declared".* ]]; then
    echo "Cargo.toml declares Rust $declared, but CI builds with $pinned" >&2
    exit 1
fi
echo "$declared (CI: $pinned)"

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

step "web"
if ! command -v node >/dev/null || (($(node -p 'process.versions.node.split(".")[0]') < 22)); then
    echo "the web checks need Node 22 or later (https://nodejs.org)" >&2
    exit 1
fi
(
    cd web
    # Installs the pinned tools when the lock file changed since the last install.
    if ! cmp -s package-lock.json node_modules/.dedale-lock; then
        npm ci --no-audit --no-fund
        cp package-lock.json node_modules/.dedale-lock
    fi
    npm run --silent check
    npm test --silent
    fresh=$(mktemp -d)
    trap 'rm -rf "$fresh"' EXIT
    node scripts/build.mjs "$fresh"
    if ! diff -r "$fresh" dist; then
        echo "web/dist is stale: run 'npm run build' in web/" >&2
        exit 1
    fi
)

step "end to end"
# The browser app in headless Chromium (or DEDALE_CHROMIUM), driving the
# debug build of dedale and the fake bot that the tests above built.
(cd web && npm run --silent e2e)

step "stable build"
if rustup show active-toolchain | grep -q '^stable'; then
    echo "skipped: the checks above already ran on stable"
elif rustup toolchain list | grep -q '^stable'; then
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
