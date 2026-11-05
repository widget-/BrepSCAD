#!/usr/bin/env bash
# Record and compare geometry metrics for model files.
#
# The corpus check answers "does it evaluate?". This answers "what did it
# build, and how long did it take?" so a boolean-engine change can be judged
# on geometry as well as speed. A run captures the CLI's mesh summary:
# triangles, vertices, volume, and welded boundary/non-manifold edges.
#
# Timing is informational and never compared: it depends on the machine. The
# baseline comparison checks status, triangle/vertex counts, and welded edge
# counts exactly, and volume with a relative tolerance.
set -euo pipefail

usage() {
    cat <<'EOF'
usage: check-model-metrics.sh [options] [file-or-directory ...]

Options:
  --write-baseline FILE    Write the measurements as a baseline.
  --baseline FILE          Compare measurements against a baseline.
  --repeat N               Measure N times and take the fastest (default 1).
  --timeout SECONDS        Per-run timeout (default 120).
  -v, --verbose            Print every model, not just mismatches.
  -h, --help               Show this help.

With no files, measures the in-repo model fixtures under
crates/scad-eval/tests/data. Directories are searched recursively for .scad.

Environment:
  BREPSCAD_BIN             Binary to run (default: target/release/brepscad).
  METRICS_TIMEOUT          Per-run timeout, overridden by --timeout.
  METRICS_VOLUME_TOL       Relative volume tolerance (default 1e-6).
EOF
}

write_baseline=
baseline_file=
repeat=1
limit=${METRICS_TIMEOUT:-120}
verbose=0
binary=${BREPSCAD_BIN:-target/release/brepscad}
targets=()

while [[ $# -gt 0 ]]; do
    case "$1" in
        --write-baseline) write_baseline=${2:?missing path}; shift 2 ;;
        --baseline) baseline_file=${2:?missing path}; shift 2 ;;
        --repeat) repeat=${2:?missing count}; shift 2 ;;
        --timeout) limit=${2:?missing seconds}; shift 2 ;;
        -v|--verbose) verbose=1; shift ;;
        -h|--help) usage; exit 0 ;;
        --*) echo "unknown option: $1" >&2; usage >&2; exit 2 ;;
        *) targets+=("$1"); shift ;;
    esac
done

if [[ -n "$baseline_file" && -n "$write_baseline" ]]; then
    echo "--baseline and --write-baseline are mutually exclusive" >&2
    exit 2
fi
if (( repeat < 1 )); then
    echo "--repeat must be at least 1" >&2
    exit 2
fi
if [[ ! -x "$binary" ]]; then
    echo "missing $binary; run: cargo build --release -p brepscad" >&2
    exit 1
fi

if [[ ${#targets[@]} -eq 0 ]]; then
    targets=(
        crates/scad-eval/tests/data/opengrid_hull_cut.scad
        crates/scad-eval/tests/data/snap_clip_cutout.scad
        crates/scad-eval/tests/data/opengrid_snap/snap_one_clip.scad
        crates/scad-eval/tests/data/opengrid_snap/snap_two_clips.scad
    )
fi

files=()
for target in "${targets[@]}"; do
    if [[ -d "$target" ]]; then
        while IFS= read -r -d '' file; do
            files+=("$file")
        done < <(find "$target" -type f -name '*.scad' -print0 | sort -z)
    elif [[ -f "$target" ]]; then
        files+=("$target")
    else
        echo "missing model: $target" >&2
        exit 1
    fi
done

if [[ ${#files[@]} -eq 0 ]]; then
    echo "no .scad files found" >&2
    exit 1
fi

# Runs one model and prints: status<TAB>ms<TAB>triangles<TAB>vertices<TAB>
# volume<TAB>welded_boundary<TAB>welded_nonmanifold
measure() {
    local file=$1
    local best_ms= code=0 output= start end ms
    for ((run = 0; run < repeat; run++)); do
        start=$(date +%s%N)
        set +e
        output=$(timeout "$limit" "$binary" "$file" 2>&1)
        code=$?
        set -e
        end=$(date +%s%N)
        ms=$(((end - start) / 1000000))
        if [[ -z "$best_ms" || $ms -lt $best_ms ]]; then
            best_ms=$ms
        fi
    done

    local status=ok
    local fields=("-" "-" "-" "-" "-")
    if [[ $code -eq 124 ]]; then
        status=timeout
    elif [[ $code -ge 128 ]]; then
        status=crash
    elif [[ $code -ne 0 ]]; then
        status=error
    else
        local line
        line=$(printf '%s\n' "$output" | grep -m1 '^mesh 0:' || true)
        if [[ -z "$line" ]]; then
            status=empty
        else
            local triangles vertices volume welded_boundary welded_nonmanifold
            read -r triangles vertices volume welded_boundary welded_nonmanifold < <(
                sed -nE 's/^mesh 0: ([0-9]+) triangles, ([0-9]+) vertices, .*volume ([^,]+), boundary edges [0-9]+ \(([0-9]+) welded\), non-manifold edges [0-9]+ \(([0-9]+) welded\).*/\1 \2 \3 \4 \5/p' <<<"$line"
            )
            if [[ -z ${triangles:-} ]]; then
                status=error
            else
                fields=("$triangles" "$vertices" "$volume" "$welded_boundary" "$welded_nonmanifold")
            fi
        fi
    fi
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$status" "$best_ms" "${fields[@]}"
}

declare -A baseline=()
if [[ -n "$baseline_file" ]]; then
    if [[ ! -f "$baseline_file" ]]; then
        echo "missing baseline: $baseline_file" >&2
        exit 1
    fi
    while IFS= read -r line; do
        [[ -z "$line" || "$line" == '#'* ]] && continue
        path=${line%%$'\t'*}
        baseline["$path"]=${line#*$'\t'}
    done < "$baseline_file"
fi

volume_matches() {
    awk -v a="$1" -v b="$2" -v tol="${METRICS_VOLUME_TOL:-1e-6}" 'BEGIN {
        if (a == "-" || b == "-") { exit (a == b) ? 0 : 1 }
        d = a - b; if (d < 0) d = -d
        m = a < 0 ? -a : a
        if (b < 0) b = -b
        if (b > m) m = b
        if (m < 1) m = 1
        exit (d / m <= tol) ? 0 : 1
    }'
}

rows=()
failures=0
declare -A seen=()

for file in "${files[@]}"; do
    result=$(measure "$file")
    IFS=$'\t' read -r status ms triangles vertices volume welded_boundary welded_nonmanifold <<<"$result"
    rows+=("$file"$'\t'"$result")
    seen["$file"]=1

    if [[ -n "$baseline_file" ]]; then
        if [[ -z ${baseline["$file"]:-} ]]; then
            printf 'NEW\t%s\n' "$file"
            failures=$((failures + 1))
            continue
        fi
        IFS=$'\t' read -r b_status b_ms b_triangles b_vertices b_volume b_welded_boundary b_welded_nonmanifold <<<"${baseline["$file"]}"

        mismatch=
        if [[ "$status" != "$b_status" ]]; then
            mismatch+="status $b_status -> $status; "
        fi
        for pair in \
            "triangles:$b_triangles:$triangles" \
            "vertices:$b_vertices:$vertices" \
            "welded_boundary:$b_welded_boundary:$welded_boundary" \
            "welded_nonmanifold:$b_welded_nonmanifold:$welded_nonmanifold"
        do
            IFS=: read -r field old new <<<"$pair"
            if [[ "$old" != "$new" ]]; then
                mismatch+="$field $old -> $new; "
            fi
        done
        if ! volume_matches "$b_volume" "$volume"; then
            mismatch+="volume $b_volume -> $volume; "
        fi

        if [[ -n "$mismatch" ]]; then
            printf 'MISMATCH\t%s\t%s\n' "$file" "${mismatch%; }"
            failures=$((failures + 1))
        elif [[ $verbose -eq 1 ]]; then
            printf 'ok\t%s\t%s ms\ttriangles %s\tvolume %s\twelded-boundary %s\n' \
                "$file" "$ms" "$triangles" "$volume" "$welded_boundary"
        fi
    elif [[ -z "$baseline_file" || $verbose -eq 1 ]]; then
        printf '%-50s %-8s %6s ms  %5s tris  volume %-20s  welded-boundary %s\n' \
            "$file" "$status" "$ms" "$triangles" "$volume" "$welded_boundary"
    fi
done

if [[ -n "$baseline_file" ]]; then
    for path in "${!baseline[@]}"; do
        if [[ -z ${seen["$path"]:-} ]]; then
            printf 'MISSING\t%s\n' "$path"
            failures=$((failures + 1))
        fi
    done
fi

if [[ -n "$write_baseline" ]]; then
    {
        printf '# mode: metrics\n'
        printf '# path\tstatus\tms\ttriangles\tvertices\tvolume\twelded_boundary\twelded_nonmanifold\n'
        for row in "${rows[@]}"; do
            printf '%s\n' "$row"
        done
    } > "$write_baseline"
    echo "wrote ${#rows[@]} models to $write_baseline"
fi

if [[ -n "$baseline_file" ]]; then
    if (( failures > 0 )); then
        echo "$failures model(s) differ from $baseline_file" >&2
        exit 1
    fi
    echo "${#rows[@]} models match $baseline_file"
fi
