# Held-out evaluation: results

8 papers, 59 tables, scored against the gold drafted before the first run
([PROTOCOL.md](PROTOCOL.md)). Model answers are in `recordings/llm`, so every
run replays offline and for free:

```bash
# Run 1: check out tag heldout-v1 first. Later runs: the commit named below.
cargo run -p sota-cli -- extract --list papers/kgc-heldout.txt --out out/heldout
cargo run -p sota-cli -- eval extract --dir gold/kgc-heldout --out out/heldout
```

- **Run 1**: extraction frozen at tag `heldout-v1`. 58 model calls
  (gpt-5.4-mini, 238k input and 90k output tokens).
- **Run 2**: + words matched without the marks around them, whatever the
  script (quotes, guillemets, brackets, emphasis), and invisible characters
  inside words ignored. 1 new call.
- **Run 3**: + evidence paragraphs chosen by what the checks need, not by
  position. At most 15 are offered; those citing the table come first, then
  those naming a value of a required setting, then the rest. Before, the
  first 15 in document order were offered, and optional values that are
  also everyday words ("head entity", "raw text") could fill them. 20 new
  calls (SimKGC and KICGPT, the only held-out papers where the cap is
  reached), plus 11 on the development set (ULTRA).

## Score

| | development (10 papers) | held-out run 1 | run 2 | run 3 |
|---|---|---|---|---|
| gold results | 985 | 902 | 902 | 902 |
| true positives | 985 | 518 | 582 | 699 |
| wrong results | 0 | 110 | 122 | 142 |
| held-back cells extracted (leaks) | 0 | 42 | 42 | 66 |
| **precision** (wrong and leaks count as errors) | **1.000** | **0.773** | **0.780** | **0.771** |
| **recall** | **1.000** | **0.574** | **0.645** | **0.775** |
| held-back cells sent to review | 39/47 | 261/382 | 253/382 | 210/382 |
| method identity (label resolves to the gold method) | 985/985 | 396/584 | 472/660 | 528/785 |
| tables covered by the gold | 17 of 95 (219 unscored) | 55 of 59 (5 unscored) | same | same |

The development set was used to build the extraction, so its perfect score
measures fit, not generalization. It stays at 985/985 after both fixes.

**Evidence coverage** (no model call): for each gold table, is a sentence
naming its required settings offered to the model, when the paper has one?
26/28 before run 3, 28/28 after
(`cargo test -p sota-extract --test evidence_coverage -- --ignored --nocapture`).
Three more tables come from papers that never write the value (ULTRA, PairRE
twice): no selection can help there.

**This set is no longer fully held out.** Runs 2 and 3 fix causes found on
these papers. The fixes are general and tested apart from them, but each
one was chosen by looking at this set's errors. The frozen run 1 is the
generalization number; a new clean measure needs a fresh batch of unseen
papers.

## Where the errors come from

By cause, in run 3 (runs 1 and 2 in parentheses where they differ):

| cause | wrong | leaks | missed | papers |
|---|---|---|---|---|
| Fixed in run 2: quote marks. The evidence check read `“Filtered”` as a word other than *filtered* | | | 0 (76, 0) | HAKE |
| Fixed in run 3: evidence cap. KICGPT's first 15 paragraphs use "head", "tail" and "raw" in their everyday sense, so the paragraph that says “filtered” was never offered | | | 0 (125, 125) | KICGPT |
| **Strict evidence rule.** PairRE describes the filtered protocol without the word; a required setting needs a sentence that names it | | | 94 | PairRE |
| **Analysis tables read as results.** Rows that are formulas, loss names, margins or ablated components | 56 (44, 44) | 20 (0, 0) | | QuatE S5.T7, SimKGC S5.T4 and A2.T13, KICGPT S4.T4 |
| **Gold decision 1.** The model calls some versions of a method (QuatE^2/^3, SimKGC, KICGPT_tsa) *variants*; the gold calls every version *proposed* | 68 (60, 60) | | 68 (60, 60) | QuatE, SimKGC, KICGPT |
| **Tie-breaking protocols.** Top / Bottom / as-reported columns of the Re-evaluation paper; the registry has no tie-breaking setting | | 42 | | Re-evaluation |
| **LLM prompting rows.** ChatGPT zero/one-shot Hits@1, not a filtered ranking | | 4 (0, 0) | | KICGPT |
| **Dataset in the caption, mapped from the header**, caught by the header check | | | 23 | Re-evaluation A1.T3 |
| **Role, gold decision 10.** ModE, which HAKE introduces, called a baseline | 12 (0, 12) | | 12 (0, 12) | HAKE |
| **Role.** AttE (a variant) called the proposed method in an appendix comparison | 6 | | 6 | AttH |

Each fix lets more tables through the checks, and with them errors the checks
were hiding: run 3's new wrong results and leaks all come from KICGPT and
SimKGC tables that went to review in run 2 for lack of evidence.

What reaches a leaderboard is narrower than the precision suggests:

- The 76 analysis rows carry labels that match no known method ("InfoNCE",
  "w/o ICL", "0.02", formulas), so the leaderboard lists them as unranked
  instead of ranking them. So do the ChatGPT rows.
- The 86 role errors have the right method, value and settings; only the
  role is different, and the role only decides which value is primary.
- **The tie-breaking leaks reach a leaderboard.** ConvKB's FB15k-237 MRR
  under the Top protocol (0.407, inflated by the evaluation bug the paper
  exposes) ranks first, flagged as a conflict with its other three values.
  This is the worst error of every run.

## What this says

- Both fixes are general: quote marks are tested on every setting spelling
  of the registry inside 23 kinds of quotes and brackets; evidence selection
  on documents where everyday words would fill the cap. Each changed only the
  prompts it had to (1, then 31 of 168).
- Recall lost to how evidence is read and chosen is recovered (+201
  results). What remains is the strict evidence rule (94), a gold judgment
  to settle (decision 1, 68), and one dataset mapping the checks caught (23).
- The extraction's main weakness is now the largest source of errors: it
  cannot tell an analysis table from a results table (76 results). The
  method-identity gate keeps all of them off the leaderboards.
- A comparability setting the registry lacks (tie-breaking, embedding
  dimension) is either caught (AttH's d = 32 table: held back) or leaks
  (tie-breaking: 42), depending on whether the table says so in a way the
  model recognizes.
- Method identity on new papers is 67%: new methods are unknown by design,
  and LaTeX markers (`\dagger`, `\diamond`) are not stripped from labels.
