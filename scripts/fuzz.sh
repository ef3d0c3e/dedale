#!/usr/bin/env bash
# Runs every fuzz target for a while, from its seeds and with its
# dictionary: scripts/fuzz.sh <seconds per target>. Needs nightly and
# cargo-fuzz. A crash stops it and leaves the input in fuzz/artifacts/.
set -euo pipefail
cd "$(dirname "$0")/.."

secs="${1:?usage: scripts/fuzz.sh <seconds per target>}"
for target in $(rustup run nightly cargo fuzz list); do
    printf '\n--> %s (%ss)\n' "$target" "$secs"
    # Room for a full-size line or map (64 KiB), beyond libFuzzer's 4 KiB default.
    args=(-max_total_time="$secs" -max_len=70000)
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
