# Held-out evaluation: protocol

Written before the extraction was run on these papers.

## What is held out

Eight knowledge-graph-completion papers ([papers/kgc-heldout.txt](../../papers/kgc-heldout.txt))
that were never opened while the extraction was developed. They were chosen
before being opened, by one rule: methods that the development papers cite as
baselines, whose own papers had not been read, across model families
(distance, rotation, hyperbolic, convolutional, text-based, LLM-based) plus one
paper about evaluation protocols.

## Freeze

The extraction is scored as it stood at git tag `heldout-v1`: code, prompt,
registry (`config/registry`) and method identities (`config/methods`). Nothing
is changed before the first score is recorded. That first score is published
as is; any later fix is scored again and reported next to it, never instead
of it.

## Gold

Drafted from the tables and the papers' text before any extraction of these
papers existed, then checked by hand. Unlike the development set, the gold
covers every table of every paper:

- **mapped** (14 tables): every cell is either a result with its full meaning,
  or held back for review with a reason;
- **no comparable results** (41 tables, `negative_tables`): statistics,
  hyperparameters, examples, analyses whose rows are configurations rather
  than methods, and results only on datasets outside the registry;
- **not scored** (4 tables, declared in each spec): shapes the gold format
  cannot express (metrics in rows; settings in rows; a metric named only in
  the caption; two stacked datasets with inner headers). Results extracted
  there are counted and listed, not scored.

Judgment calls the reviewer should confirm are in [REVIEW.md](REVIEW.md).

## Scoring

Cell by cell, as for the development set (`sota eval extract`): a result is a
true positive only if method label, role, dataset, metric, every setting and
the value all match.

- **Precision** counts as errors: wrong results, results from tables with no
  comparable results, and results from cells the gold holds back (leaks).
- **Recall** counts gold results not extracted, including those sent to
  review.
- **Method identity** is reported separately: how many extracted labels
  resolve, through `config/methods`, to the gold method id. New methods are
  expected to be unknown at the freeze.

The development-set score is reported next to it, unchanged.
