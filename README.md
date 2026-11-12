# sota-graph

**Who beats whom, on which benchmark, by how much, with a source for every number.**

[![ci](https://github.com/Ilyas-Khiat/sota-graph/actions/workflows/ci.yml/badge.svg)](https://github.com/Ilyas-Khiat/sota-graph/actions/workflows/ci.yml)
![rust](https://img.shields.io/badge/rust-1.99-orange)
![license](https://img.shields.io/badge/license-MIT-blue)

sota-graph turns research papers into a trustworthy, queryable knowledge graph
of the state of the art: leaderboards, method lineage, and what changed this
week. Every number in it points back to the exact table cell it was read from,
and nothing is compared unless it was measured the same way.

> **Status: work in progress.** Built and tested: config-driven ontology and
> registry, arXiv ingestion and table parsing, a self-checking gold set, LLM
> table extraction verified cell by cell, a Neo4j graph and leaderboards
> computed by code. GraphRAG querying and the PDF channel come next (see the
> [roadmap](#roadmap)).

![sota leaderboard WN18RR Hits@1: methods ranked by code, each value linked to the arXiv table it was read from, other papers' reports alongside, and HAKE flagged because two papers disagree](docs/img/leaderboard.png)

## Why

Leaderboards built by copying numbers between papers fail quietly. Ten
knowledge-graph-completion papers were enough to find all of these:

- **The same result, two values.** NBFNet reports RotatE's FB15k-237 Hits@10 as
  `0.553`; RotatE's own paper says `.533`. AdaProp reports ConvE's YAGO3-10 MRR as
  `0.520`; ConvE's paper says `.44`.
- **Different protocols side by side.** A\*Net prints a *tail-prediction-only*
  table next to its main one: NBFNet scores `0.509` MRR there and `0.415` under
  the standard protocol. ComplEx reports *filtered* and *raw* MRR in adjacent
  columns. ULTRA mixes *zero-shot* and *fine-tuned* numbers in one table.
- **Settings that are never stated.** ULTRA ranks against the full entity set
  but never says whether the ranking is filtered or raw.
- **Typos.** TuckER prints HypER's WN18 Hits@10 as `958`.
- **Silent mixes.** GraIL is scored with 50 negatives inside a table of
  full-ranking results; MINERVA's tail-only numbers sit unmarked among
  head-and-tail ones.

A copied number with no setting attached is a guess. This project refuses to
guess: what it cannot verify goes to a review queue.

## Principles

- **The LLM extracts facts; code computes comparisons.** The model never decides
  that A beats B.
- **Every number and edge links to its source:** `2106.06935v4#S4.T3/r13/c3` is
  one cell of one table of one pinned arXiv version.
- **Comparable means same dataset, split, metric and settings.** Settings such as
  the ranking protocol are required, or take an explicit default.
- **The ontology is a config file, not code.**
- **Reliability is measured on a hand-checked gold set** and tracked across changes.

## What works today

### A registry that resolves names exactly, or not at all

Datasets, metrics and comparability settings live in [`config/`](config). Mentions
are matched exactly after a small, configured normalization (case, whitespace,
the dash look-alikes PDFs produce). There is no fuzzy matching: a miss is
"unknown", never a guess.

```console
$ sota resolve dataset "FB15k–237"          # en dash
fb15k-237	FB15k-237
$ sota resolve metric "Hits at 10"
hits@10	Hits@10
$ sota resolve dataset "FB15k-237 (filtered)"
unknown	key "fb15k237(filtered)" matches no entry (would go to review)
```

### arXiv ingestion and table parsing

`sota fetch` downloads the HTML that arxiv.org renders for each paper, pinned to a
version, 15 s apart as arXiv's robots.txt asks, and caches it forever (a version
never changes). The parser handles both table markups LaTeXML produces, row and
column spans, several tables per figure each with its own caption, multi-line
cells, math, and citation markers.

```console
$ sota fetch --list papers/kgc.txt
papers  listed 10 | html 10 | no html 0 | failed 0
tables  table figures 95 | tables parsed 109 | figures without a table 0 | tables with holes 0 | tables with conversion errors 0

$ sota tables 1902.10197v1 --table S4.T5
## Table 5 (S4.T5) https://arxiv.org/html/1902.10197v1#S4.T5
|  | **FB15k-237** |  |  |  |  | **WN18RR** |  |  |  |  |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
|  | MR | MRR | H@1 | H@3 | H@10 | MR | MRR | H@1 | H@3 | H@10 |
| ConvE | 244 | .325 | .237 | .356 | .501 | 4187 | .43 | .40 | .44 | .52 |
| RotatE | **177** | **.338** | **.241** | **.375** | **.533** | 3340 | **.476** | **.428** | **.492** | **.571** |
```

### A gold set that checks itself against the papers

[`gold/kgc`](gold/kgc): 10 papers, 17 tables, **985 results**, 47 cells deliberately
held back for review, and 13 tables marked as holding no comparable results. A gold spec does not list numbers; it maps a table's rows
and columns to meanings, and the numbers are read out of the paper. While
reading, the paper itself is checked against the spec: every row label must
read the declared method, every column header must name the declared dataset
and metric, and no header may contradict a declared setting. A mapping mistake
fails loudly:

```console
$ sota gold expand --paper 1902.10197v1      # column 2 wrongly declared as Hits@1
1902.10197v1     FAILED
  - table S4.T5: col 2: header ["FB15k-237", "MRR"] does not name metric hits@1
```

Each result carries its full comparability key, including settings the paper
leaves at the field's default:

```console
$ sota gold show 2206.04798v5
## Table 2 (S4.SS2.tab1.t2) https://arxiv.org/html/2206.04798v5#S4.SS2.tab1
split test | protocol=filtered, queries=tail | evidence: Caption: 'Tail prediction performance'; ...

| row label (as printed) | means | fb15k-237 mrr | fb15k-237 hits@1 | ...
| NBFNet | nbfnet (baseline) | 0.509 | 0.411 | ...
| A*Net | astarnet (proposed) | 0.505 | 0.410 | ...
```

Committed snapshots let CI verify the gold set without the papers; locally,
`sota gold check` re-reads every number from the cached pages. See
[gold/README.md](gold/README.md) for the format.

### LLM extraction that never writes a number

The model reads one table at a time and returns a *mapping* (which rows are
which methods, which columns are which dataset and metric, which settings apply
and which sentence of the paper says so), never a value. Code then reads every
number out of the cell the mapping points to, checks that row labels and column
headers agree with the mapping and that the cited evidence names each setting,
and sends anything that does not hold to a review queue: a whole table, or just
one row, column or cell. Answers are recorded, so every run replays offline and
for free.

### Measured on papers it never saw

The development papers were used to build the extraction, so its perfect
score there measures fit. Eight more papers were then chosen, never opened
during development, and scored with the extraction frozen at git tag
`heldout-v1`, against a gold set drafted before the run and covering every
table ([protocol](gold/kgc-heldout/PROTOCOL.md)):

| | development (10 papers) | held-out, frozen | held-out, + quote fix |
|---|---|---|---|
| gold results | 985 | 902 | 902 |
| precision (leaks of held-back cells count as errors) | 1.000 | **0.773** | **0.780** |
| recall | 1.000 | **0.574** | **0.645** |
| method identity | 985/985 | 396/584 | 472/660 |

Every error is traced to a cause in [the results](gold/kgc-heldout/RESULTS.md).
The first fix reads setting names inside any kind of quote or bracket. The
largest remaining loss is how evidence is chosen: a cap of 15 paragraphs in
document order hides one paper's evaluation section. The main weakness is
analysis tables read as results, which the method-identity gate keeps off the
leaderboards. The worst error is a number obtained under a flawed evaluation
protocol (ConvKB, Top tie-breaking) ranking first, flagged as a conflict.
Each fix is scored as a new run, next to the frozen one.

### Leaderboards computed by code, from the graph

Verified results are merged into Neo4j (`sota graph load`; idempotent, and
every slice is read back and compared with the files). Method identity comes
from [`config/methods`](config/methods), never from the model; a label that
matches no known method is listed but not ranked. Results are grouped by their
full comparability key, ranked by the metric's direction, and a method reported
with different values by different papers is flagged, with every source. The
screenshot at the top is `sota leaderboard WN18RR Hits@1 --top 6`; the command
prints Markdown, shown there rendered. Tail-only results get a board of their
own instead of being mixed in, and every source links to the table on arxiv.org.

**Known limitation.** The extraction cannot yet tell an analysis or ablation
table from a results table: rows named after other methods (NBFNet's message
functions "TransE", "DistMult", "RotatE") or configurations can become
results. The held-out run measures how often, and what reaches a leaderboard.

### The graph in Neo4j

The schema is generated from [`config/ontology.toml`](config/ontology.toml):
one uniqueness constraint per node key, and a writer that may only create the
labels and relationships the ontology declares.

<p align="center"><img src="docs/img/neo4j-schema.png" width="560" alt="Neo4j schema: Paper PROPOSES Method; Paper REPORTS Result; Result FOR_METHOD Method, ON_DATASET Dataset, MEASURED_BY Metric"></p>

Every number is a `Result` node holding the cell text verbatim, linked to the
paper that printed it and the method it measures. Six methods on FB15k-237 MRR,
in the Neo4j Browser: ConvE's `.325` is reported by six papers, and HAKE is
`0.346` in NBFNet's paper but `0.341` in A*Net's, which the leaderboard flags.

![Neo4j Browser: Paper, Result and Method nodes for six methods on FB15k-237 MRR; results captioned with their printed values](docs/img/neo4j-provenance.png)

```cypher
MATCH (p:Paper)-[:REPORTS]->(r:Result)-[:FOR_METHOD]->(m:Method)
WHERE m.id IN ['adaprop', 'astarnet', 'nbfnet', 'compgcn', 'hake', 'conve']
  AND (r)-[:ON_DATASET]->(:Dataset {id: 'fb15k-237'})
  AND (r)-[:MEASURED_BY]->(:Metric {id: 'mrr'})
  AND r.split = 'test'
  AND r.settings = ['protocol=filtered', 'queries=both', 'regime=supervised', 'relation_category=all']
RETURN p, r, m
```

## Architecture

```mermaid
flowchart LR
    subgraph inputs[Inputs]
        H[arXiv HTML]:::done
        P[PDF]:::todo
    end
    H --> F[fetch + cache<br/>pinned versions]:::done --> D[document model<br/>sections, tables, cells<br/>with locators]:::done
    P -.-> D
    R[(ontology + registry<br/>config/)]:::done
    D --> X[LLM extraction<br/>records citing a cell]:::done
    X --> V{code verifies<br/>number in cell,<br/>names in registry}:::done
    R --> V
    V -- ok --> G[(Neo4j<br/>knowledge graph)]:::done
    V -- uncertain --> Q[review queue]:::done
    G --> L[leaderboards<br/>computed by code]:::done
    G --> A[GraphRAG: hybrid BM25 + vector,<br/>then Cypher / graph expansion]:::todo
    S[(gold set)]:::done -. measures .-> X
    classDef done fill:#d4edda,stroke:#2e7d32
    classDef todo fill:#f5f5f5,stroke:#9e9e9e,stroke-dasharray: 4 3
```

Green is built and tested; dashed is planned.

- **Rust workspace:** `sota-core` (pure domain logic: ontology, registry, document
  model, values, gold, method identity, comparison), `sota-ingest` (fetching,
  caching, parsing), `sota-extract` (model clients, table mapping, verification),
  `sota-store` (Neo4j over its HTTP query API, schema from the ontology),
  `sota-cli` (the `sota` command).
- **Two input channels, one document model.** HTML tables are parsed by code;
  PDF tables will be read by a model and every number checked against the
  page's text layer. Papers available in both let the HTML channel measure the
  PDF channel.
- **Model access behind interfaces** (`LlmClient`, `Embedder`) with replay
  backends, so tests never call the network.
- **Query path (planned):** exact alias lookup, then hybrid BM25 + vector
  retrieval fused with reciprocal rank fusion, then Cypher templates or graph
  expansion; answers are built only from verified results.

## Quick start

Requires Rust (the toolchain is pinned in `rust-toolchain.toml`).

```bash
cargo test --all                                   # offline: no network, no paper cache
cargo run -p sota-cli -- check-config              # validate ontology + registry
cargo run -p sota-cli -- fetch --list papers/kgc.txt   # ~2.5 min the first time, then cached
cargo run -p sota-cli -- tables 2106.06935v4 --table S4.T3
cargo run -p sota-cli -- gold check                # re-read all 985 gold results from the papers
cargo run -p sota-cli -- extract --list papers/kgc.txt   # replays the recorded model answers
cargo run -p sota-cli -- eval extract              # score the extraction on the gold set

docker compose up -d                               # local Neo4j on 127.0.0.1:7474
cargo run -p sota-cli -- graph init                # constraints from config/ontology.toml
cargo run -p sota-cli -- graph load                # merge results, read them back, compare
cargo run -p sota-cli -- leaderboard FB15k-237 MRR --setting protocol=filtered
cargo test --all -- --include-ignored              # also the tests that need the cache and Neo4j
```

## Repository layout

```
config/          ontology.toml, normalization.toml, registry/<topic>.toml, methods/<topic>.toml
papers/          pinned arXiv ids per topic
gold/            gold specs (<topic>/<paper>.toml) and their result snapshots
recordings/llm/  recorded model answers, so extraction replays offline
crates/
  sota-core/     ontology, registry, document model, values, gold, methods, comparison (no I/O)
  sota-ingest/   arXiv HTML fetcher, cache, LaTeXML parser
  sota-extract/  model clients (Azure OpenAI, replay), table mapping, verification
  sota-store/    Neo4j writer and reader
  sota-cli/      the `sota` command
```

## Roadmap

- [x] Workspace, CI (fmt, clippy, tests)
- [x] Ontology and registry as config, exact alias resolution
- [x] arXiv HTML channel: polite fetching, caching, table parsing
- [x] Gold set for KG completion with self-checking specs
- [x] LLM extraction with cell citations, verification and a review queue
- [x] Neo4j writer, comparison engine, `sota leaderboard`
- [ ] `sota eval`: extraction quality on the gold set, tracked over time (scoring done; history next)
- [ ] PDF channel, measured against the HTML channel
- [ ] Hybrid retrieval + GraphRAG `sota ask`, with a retrieval ablation
- [ ] Second topic: multi-hop QA and GraphRAG benchmarks

## Data

Only public papers are used. Downloaded pages stay in a local, git-ignored
cache and are never redistributed; the repository holds arXiv ids, table
locators and the numbers themselves. Test fixtures are hand-written HTML that
reproduces the converter's structure, not any paper's content.

## License

[MIT](LICENSE)
