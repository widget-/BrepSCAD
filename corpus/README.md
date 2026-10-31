# SCAD corpus

`revisions.tsv` pins the upstream checkouts used for compatibility checks.
`scripts/fetch-scad-corpus.sh` clones them into `.corpus/`, which is ignored by
git. The libraries are not vendored: BOSL2 is BSD-2-Clause, but MCAD is
LGPL-2.1.

`baseline-parse.tsv` and `baseline-evaluate.tsv` are per-file snapshots used by
`scripts/check-scad-corpus.sh --baseline`. Refresh a baseline after changing
the pinned revisions or after intentional compatibility work:

```sh
scripts/fetch-scad-corpus.sh
scripts/check-scad-corpus.sh --parse-only --write-baseline corpus/baseline-parse.tsv .corpus
scripts/check-scad-corpus.sh --evaluate --write-baseline corpus/baseline-evaluate.tsv .corpus
```

See [`docs/development.md`](../docs/development.md#scad-corpus-check) for the
full workflow.
