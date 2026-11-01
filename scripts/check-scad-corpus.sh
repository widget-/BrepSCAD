#!/usr/bin/env bash
# Measure OpenSCAD compatibility across a directory tree.
#
# Parse-only mode (default) is the language-syntax signal. Evaluation mode also
# exercises the evaluator and kernel, so it separates missing language features
# from missing geometry modules.
set -euo pipefail

usage() {
    cat <<'EOF'
usage: check-scad-corpus.sh [options] <directory> [directory ...]

Options:
  --parse-only             Measure parser coverage only (default)
  --evaluate, -e           Run full evaluation
  --results FILE           Write per-file results as PATH<TAB>STATUS<TAB>ERROR
  --write-baseline FILE    Write a baseline as PATH<TAB>STATUS
  --baseline FILE          Compare results against a baseline
  --check FILE             Alias for --baseline
  --verbose, -v            Print every non-ok result
  -h, --help               Show this help

Environment:
  BREPSCAD_BIN             Binary to run (default: target/debug/brepscad)
  CORPUS_TIMEOUT           Per-file timeout in seconds (default: 8)
EOF
}

mode=parse
results_file=
baseline_file=
write_baseline=
verbose=0
directories=()

while [[ $# -gt 0 ]]; do
    case "$1" in
        --parse-only) mode=parse; shift ;;
        --evaluate|-e) mode=evaluate; shift ;;
        --results) results_file=${2:?missing path}; shift 2 ;;
        --write-baseline) write_baseline=${2:?missing path}; shift 2 ;;
        --baseline|--check) baseline_file=${2:?missing path}; shift 2 ;;
        --verbose|-v) verbose=1; shift ;;
        -h|--help) usage; exit 0 ;;
        --*) echo "unknown option: $1" >&2; usage >&2; exit 2 ;;
        *) directories+=("$1"); shift ;;
    esac
done

if [[ ${#directories[@]} -eq 0 ]]; then
    usage >&2
    exit 2
fi
if [[ -n "$baseline_file" && -n "$write_baseline" ]]; then
    echo "--baseline and --write-baseline are mutually exclusive" >&2
    exit 2
fi

# The pinned corpus resolves <BOSL2/std.scad> and <MCAD/units.scad> from its
# root. Default OPENSCADPATH to .corpus when the caller points us inside it.
if [[ -z ${OPENSCADPATH:-} ]]; then
    for directory in "${directories[@]}"; do
        directory=${directory%/}
        if [[ "$directory" == ".corpus" || "$directory" == *"/.corpus" ]]; then
            export OPENSCADPATH="$directory"
            break
        fi
        if [[ "$directory" == .corpus/* ]]; then
            export OPENSCADPATH=".corpus"
            break
        fi
        if [[ "$directory" == *"/.corpus/"* ]]; then
            export OPENSCADPATH="${directory%%/.corpus/*}/.corpus"
            break
        fi
    done
fi

binary=${BREPSCAD_BIN:-target/debug/brepscad}
limit=${CORPUS_TIMEOUT:-8}

if [[ ! -x "$binary" ]]; then
    echo "missing $binary; run: cargo build -p brepscad" >&2
    exit 1
fi

declare -A statuses=()
declare -A first_errors=()
declare -A counts=(
    [ok]=0 [ok-empty]=0 [parse]=0 [language]=0
    [geometry]=0 [kernel]=0 [assertion]=0 [eval]=0 [timeout]=0
)
total=0

normalize_path() {
    local path=$1
    path=${path#"$PWD"/}
    path=${path#./}
    path=${path#.corpus/}
    printf '%s' "$path"
}

classify() {
    local status=$1 output=$2 first=$3
    if [[ $status -eq 124 ]]; then
        printf 'timeout'
        return
    fi
    if [[ "$first" == "Error: parse error:"* ]]; then
        printf 'parse'
        return
    fi
    if [[ "$mode" == parse ]]; then
        if [[ $status -eq 0 ]]; then
            printf 'ok'
        else
            printf 'eval'
        fi
        return
    fi
    case "$first" in
        "Error: unknown identifier:"*|"Error: unknown function:"*) printf 'language' ;;
        "Error: unsupported module:"*) printf 'geometry' ;;
        "Error: kernel error:"*) printf 'kernel' ;;
        "Error: assertion failed:"*) printf 'assertion' ;;
        "Error:"*) printf 'eval' ;;
        *)
            if [[ $status -ne 0 ]]; then
                # Crashes and aborts must not be mistaken for successful files.
                printf 'eval'
            elif printf '%s\n' "$output" | grep -q '^meshes: 0$'; then
                printf 'ok-empty'
            else
                printf 'ok'
            fi
            ;;
    esac
}

while IFS= read -r -d '' file; do
    total=$((total + 1))
    if [[ "$mode" == parse ]]; then
        args=(--parse-only "$file")
    else
        args=("$file")
    fi

    set +e
    output=$(timeout "$limit" "$binary" "${args[@]}" 2>&1)
    status=$?
    set -e
    first=$(printf '%s\n' "$output" | head -n 1)
    kind=$(classify "$status" "$output" "$first")
    path=$(normalize_path "$file")

    statuses["$path"]=$kind
    first_errors["$path"]=$first
    counts["$kind"]=$((counts["$kind"] + 1))

    if [[ $verbose -eq 1 && "$kind" != ok && "$kind" != ok-empty ]]; then
        printf '%s\t%s\t%s\n' "$path" "$kind" "$first"
    fi
done < <(find "${directories[@]}" -type f -name '*.scad' -print0 | sort -z)

if [[ $total -eq 0 ]]; then
    echo "no .scad files found under: ${directories[*]}" >&2
    exit 1
fi

if [[ -n "$results_file" ]]; then
    {
        for path in "${!statuses[@]}"; do
            printf '%s\t%s\t%s\n' "$path" "${statuses[$path]}" "${first_errors[$path]}"
        done | sort
    } > "$results_file"
fi

if [[ -n "$write_baseline" ]]; then
    {
        printf '# mode: %s\n' "$mode"
        for path in "${!statuses[@]}"; do
            printf '%s\t%s\n' "$path" "${statuses[$path]}"
        done | sort
    } > "$write_baseline"
fi

printf 'mode: %s\n' "$mode"
printf 'total: %d\n' "$total"
printf 'ok: %d\n' "$((counts[ok] + counts[ok-empty]))"
if [[ "$mode" == evaluate ]]; then
    printf 'ok (no geometry): %d\n' "${counts[ok-empty]}"
    printf 'language errors: %d\n' "${counts[language]}"
    printf 'geometry errors: %d\n' "${counts[geometry]}"
    printf 'kernel errors: %d\n' "${counts[kernel]}"
    printf 'assertion errors: %d\n' "${counts[assertion]}"
    printf 'other evaluation errors: %d\n' "${counts[eval]}"
fi
printf 'parse errors: %d\n' "${counts[parse]}"
printf 'timeouts: %d\n' "${counts[timeout]}"

if [[ -n "$baseline_file" ]]; then
    if [[ ! -f "$baseline_file" ]]; then
        echo "missing baseline: $baseline_file" >&2
        exit 1
    fi
    declare -A baseline=()
    baseline_mode=
    while IFS=$'\t' read -r path status; do
        [[ -z "$path" ]] && continue
        if [[ "$path" == "# mode:"* ]]; then
            baseline_mode=${status}
            continue
        fi
        [[ "$path" == \#* ]] && continue
        baseline["$path"]=$status
    done < "$baseline_file"
    if [[ -n "$baseline_mode" && "$baseline_mode" != "$mode" ]]; then
        echo "baseline mode is $baseline_mode, current mode is $mode" >&2
        exit 1
    fi

    severity() {
        case "$1" in
            ok|ok-empty) printf '0' ;;
            parse) printf '3' ;;
            timeout) printf '4' ;;
            *) printf '2' ;;
        esac
    }

    regressions=0
    improvements=0
    new_files=0
    missing=0
    for path in "${!statuses[@]}"; do
        if [[ -v baseline["$path"] ]]; then
            before=${baseline[$path]}
            after=${statuses[$path]}
            if [[ $(severity "$after") -gt $(severity "$before") ]]; then
                regressions=$((regressions + 1))
                printf 'REGRESSION %s: %s -> %s\n' "$path" "$before" "$after"
            elif [[ $(severity "$after") -lt $(severity "$before") ]]; then
                improvements=$((improvements + 1))
                printf 'improved %s: %s -> %s\n' "$path" "$before" "$after"
            fi
        else
            new_files=$((new_files + 1))
            printf 'new: %s\t%s\n' "$path" "${statuses[$path]}"
        fi
    done
    for path in "${!baseline[@]}"; do
        if [[ ! -v statuses["$path"] ]]; then
            missing=$((missing + 1))
            printf 'missing: %s\n' "$path"
        fi
    done

    printf 'regressions: %d\n' "$regressions"
    printf 'improvements: %d\n' "$improvements"
    printf 'new files: %d\n' "$new_files"
    printf 'missing files: %d\n' "$missing"
    if [[ $regressions -gt 0 || $new_files -gt 0 || $missing -gt 0 ]]; then
        exit 1
    fi
fi
