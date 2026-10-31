#!/usr/bin/env bash
# Clone or update the pinned SCAD corpus into .corpus/.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
manifest=${CORPUS_MANIFEST:-$root/corpus/revisions.tsv}
destination=${1:-$root/.corpus}

if [[ ! -f "$manifest" ]]; then
    echo "missing manifest: $manifest" >&2
    exit 1
fi

mkdir -p "$destination"

while IFS=$'\t' read -r name url commit; do
    [[ -z "$name" || "$name" == \#* ]] && continue
    if [[ -z "$url" || -z "$commit" ]]; then
        echo "invalid manifest line for $name" >&2
        exit 1
    fi

    target="$destination/$name"
    if [[ ! -d "$target/.git" ]]; then
        git clone --quiet --filter=blob:none "$url" "$target"
    fi
    git -C "$target" fetch --quiet origin
    git -C "$target" checkout --quiet --detach "$commit"
    printf '%s\n' "$target"
done < "$manifest"
