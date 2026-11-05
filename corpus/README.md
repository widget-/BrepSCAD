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

`baseline-metrics.tsv` is the geometry snapshot for the in-repo model fixtures,
used by `scripts/check-model-metrics.sh`: status, triangle and vertex counts,
volume, and welded boundary/non-manifold edge counts. Timing is recorded for
information but not compared. Refresh it when the fixtures change:

```sh
scripts/check-model-metrics.sh --write-baseline corpus/baseline-metrics.tsv
```

See [`docs/development.md`](../docs/development.md#scad-corpus-check) for the
full workflow.
