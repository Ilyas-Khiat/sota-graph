# Gold set

Hand-checked meaning of results tables, used to measure extraction (step 4)
and the comparison engine (step 5). One spec per paper: `<topic>/<arxiv-id>.toml`.

A spec does not list numbers. It maps a table's rows and columns to meanings,
and `sota gold expand` reads the numbers out of the paper, into
`<arxiv-id>.results.jsonl` next to the spec (committed, so CI can check it
without the papers).

```toml
paper = "1902.10197v1"            # pinned arXiv version

[[table]]
id = "S4.T5"                      # table id as printed by `sota tables`
header_rows = 2                   # rows above the data
label_col = 0                     # column holding the row labels
settings = { protocol = "filtered" }      # registry setting ids and value ids
evidence = "S4.SS1.SSS0.Px2.p1: ..."      # where the paper says so
notes = "..."                     # anything a reviewer should know
columns = [{ col = 2, dataset = "fb15k-237", metric = "mrr" }, ...]
rows    = [{ row = 7, method = "RotatE", method_id = "rotate", role = "proposed" }, ...]
review_cells = [{ row = 12, reason = "..." }]   # must NOT become results (col optional)
```

Optional per column: `scale = "percent"` (as printed), `settings = { ... }`
(overrides the table's). `cell_scales` fixes single cells. A transposed table
(datasets in rows) sets `orientation = "datasets_in_rows"`: rows then carry
`dataset`, columns carry `method`, `method_id` and `role`.

## What the code checks, so a reviewer does not have to

- Every row label reads exactly the declared method (or names the declared
  dataset in a transposed table).
- Every column header resolves, through the registry, to the declared metric
  and dataset (or contains the declared method), joining header rows ("Hits"
  above "@10").
- No header contradicts a declared setting, defaults included (a "Raw" column
  cannot be filtered; a "0-shot" column cannot be supervised).
- Every number is read from its cell and put on the metric's canonical scale
  within its bounds; anything unreadable fails instead of being guessed.
- Results carry every applicable setting: required ones stated, the rest at
  their registry default (`queries = both`, `regime = supervised`).
- Snapshots agree with their specs (in CI, without the papers) and with the
  papers (locally, with the cache).

## What a reviewer checks

Meanings the paper does not print in the cell: the role of each row, the
settings and their evidence, the cells sent to review, and the notes. See
[kgc/REVIEW.md](kgc/REVIEW.md).

```bash
cargo run -p sota-cli -- gold show 1902.10197v1   # mapping + values + links
cargo run -p sota-cli -- gold check               # everything, re-expanded from the cache
cargo run -p sota-cli -- gold expand              # after editing a spec
```
