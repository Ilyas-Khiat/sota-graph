# Held-out evaluation: results

8 papers, 59 tables, scored against the gold drafted before the first run
([PROTOCOL.md](PROTOCOL.md)). Model answers are in `recordings/llm`, so both
runs replay offline and for free:

```bash
# Run 1: check out tag heldout-v1 first. Run 2: the current code.
cargo run -p sota-cli -- extract --list papers/kgc-heldout.txt --out out/heldout
cargo run -p sota-cli -- eval extract --dir gold/kgc-heldout --out out/heldout
```

- **Run 1**: extraction frozen at tag `heldout-v1`. 58 model calls
  (gpt-5.4-mini, 238k input and 90k output tokens).
- **Run 2**: run 1 plus one fix. Words in running text are now matched
  without the marks around them, whatever the script (quotes, guillemets,
  brackets, emphasis), and invisible characters inside words (soft hyphen,
  zero-width spaces) are ignored. 58 answers replayed, 1 new call: the fix
  adds an evidence paragraph to one prompt.

## Score

| | development set (10 papers) | held-out run 1 | held-out run 2 |
|---|---|---|---|
| gold results | 985 | 902 | 902 |
| true positives | 985 | 518 | 582 |
| wrong results | 0 | 110 | 122 |
| held-back cells extracted (leaks) | 0 | 42 | 42 |
| **precision** (wrong and leaks count as errors) | **1.000** | **0.773** | **0.780** |
| **recall** | **1.000** | **0.574** | **0.645** |
| held-back cells sent to review | 39/47 | 261/382 | 253/382 |
| method identity (label resolves to the gold method) | 985/985 | 396/584 | 472/660 |
| tables covered by the gold | 17 of 95 (219 results unscored) | 55 of 59 (5 unscored) | 55 of 59 (5 unscored) |

The development set was used to build the extraction, so its perfect score
measures fit, not generalization. The fix leaves it unchanged: every one of
its 109 prompts is identical, and it still scores 985/985.

## Where the errors come from

By cause, in run 2 (run 1 in parentheses where it differs):

| cause | wrong | leaks | missed | papers |
|---|---|---|---|---|
| **Fixed in run 2: quote marks.** The evidence check read `“Filtered”` as a word other than *filtered* | 0 (0) | | 0 (76) | HAKE |
| **Bug: evidence paragraphs capped at 15, in document order.** KICGPT's first 15 paragraphs use "head", "tail" and "raw" in their everyday sense ("head entity", "raw text"), so the paragraph that says “filtered” is never shown to the model | | | 125 | KICGPT |
| **Strict evidence rule.** PairRE describes the filtered protocol without the word; a required setting needs a sentence that names it | | | 94 | PairRE |
| **Gold decision 1.** The model calls QuatE^2/^3 and three of SimKGC's four versions *variants*; the gold calls every version *proposed* | 60 | | 60 | QuatE, SimKGC |
| **Analysis tables read as results.** Rows that are score-function formulas or loss names (the weakness seen on NBFNet's ablation) | 44 | | | QuatE S5.T7, SimKGC S5.T4 |
| **Tie-breaking protocols.** Top / Bottom / as-reported columns of the Re-evaluation paper; the registry has no tie-breaking setting | | 42 | | Re-evaluation |
| **Dataset in the caption, mapped from the header**, caught by the header check | | | 23 | Re-evaluation A1.T3 |
| **Role, gold decision 10.** ModE, which HAKE introduces, called a baseline (only visible once HAKE's table passes) | 12 (0) | | 12 (0) | HAKE |
| **Role.** AttE (a variant) called the proposed method in an appendix comparison | 6 | | 6 | AttH |

Run 1's write-up blamed KICGPT's 125 misses on the quote marks too. That was
wrong: the fix did not change KICGPT's prompts, and the cause is the
paragraph cap. HAKE's ablation table, whose labels are check marks, went to
review on its own in run 2: no ablation result leaked from it.

What reaches a leaderboard is narrower than the precision suggests:

- The 44 analysis rows carry labels that match no known method ("InfoNCE",
  formulas), so the leaderboard lists them as unranked instead of ranking them.
- The 78 role errors have the right method, value and settings; only the
  role is different, and the role only decides which value is primary.
- **The leaks do reach a leaderboard.** ConvKB's FB15k-237 MRR under the Top
  protocol (0.407, inflated by the evaluation bug the paper exposes) ranks
  first, flagged as a conflict with its other three values. This is the
  worst error of both runs.

## What this says

- The quote-mark fix is general: it is tested on every setting spelling of
  the registry inside 23 kinds of quotes and brackets and 10 trailing marks,
  and against matches it must not create ("unfiltered", "filtering").
- The next recall loss is how evidence is chosen, not how it is read: a fixed
  cap in document order lets ordinary words crowd out the evaluation section.
- The extraction's main weakness is the same on new papers as on the
  development set: it cannot tell an analysis table from a results table.
  The method-identity gate keeps that damage off the leaderboards.
- A comparability setting the registry lacks (tie-breaking, embedding
  dimension) is either caught (AttH's d = 32 table: 122/130 held back) or
  leaks (tie-breaking: 42 leaks), depending on whether the table says so in a
  way the model recognizes.
- Method identity on new papers is 72%: new methods are unknown by design,
  and LaTeX markers (`\dagger`, `\diamond`) are not stripped from labels.
