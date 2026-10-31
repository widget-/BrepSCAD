#!/usr/bin/env bash
# Evaluate every .scad file under the given directories and report the first
# failure per file. This is a compatibility probe, not a test suite: it exits
# successfully even when files fail to parse or evaluate.
set -euo pipefail

if [[ $# -eq 0 ]]; then
    echo "usage: $0 <directory> [directory ...]" >&2
    exit 2
fi

binary=${BREPSCAD_BIN:-target/debug/brepscad}
limit=${CORPUS_TIMEOUT:-8}

if [[ ! -x "$binary" ]]; then
    echo "missing $binary; run: cargo build -p brepscad" >&2
    exit 1
fi

total=0
ok=0
parse=0
eval=0
timed_out=0
failures=()

while IFS= read -r -d '' file; do
    total=$((total + 1))
    set +e
    output=$(timeout "$limit" "$binary" "$file" 2>&1)
    status=$?
    set -e
    first=$(printf '%s\n' "$output" | head -n 1)

    if [[ $status -eq 124 ]]; then
        timed_out=$((timed_out + 1))
        failures+=("$file: timed out after ${limit}s")
    elif [[ "$first" == "Error: parse error:"* ]]; then
        parse=$((parse + 1))
        failures+=("$file: $first")
    elif [[ "$first" == "Error:"* ]]; then
        eval=$((eval + 1))
        failures+=("$file: $first")
    else
        ok=$((ok + 1))
    fi
done < <(find "$@" -type f -name '*.scad' -print0)

if [[ $total -eq 0 ]]; then
    echo "no .scad files found under: $*" >&2
    exit 1
fi

if [[ ${#failures[@]} -gt 0 ]]; then
    printf '%s\n' "${failures[@]}"
    echo
fi

printf 'total: %d\n' "$total"
printf 'ok: %d\n' "$ok"
printf 'parse errors: %d\n' "$parse"
printf 'evaluation errors: %d\n' "$eval"
printf 'timeouts: %d\n' "$timed_out"
