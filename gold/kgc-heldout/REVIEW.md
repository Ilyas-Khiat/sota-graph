# KG completion held-out gold: review checklist

8 papers, 59 tables: 14 mapped (**902 results**, 382 cells held back), 41
with no comparable results, 4 not scorable. Labels, headers, values and
scales are machine-checked against each paper; what is left is judgment. How
the papers were chosen and how they are scored: [PROTOCOL.md](PROTOCOL.md).

For each paper: run `sota gold show <id> --dir gold/kgc-heldout`, open the
table links, and tick the box once roles, settings, held-back cells and the
negative tables look right.

| ok | paper | mapped tables | results | held back | look at |
|---|---|---|---|---|---|
| [x] | 1911.09419v3 HAKE | S4.T3 | 76 | 8 | ModE as a variant (10); ablation S4.T4 negative (9) |
| [x] | 1904.10281v3 QuatE | S5.T3, T4 | 220 | 30 | three QuatE versions as one method (1); a-RotatE (2) |
| [x] | 2005.00545v1 AttH/RotH | S5.T2, T5, A1.T6 | 170 | 130 | d = 32 table held back (3); "Quaternion" (4); typos (11) |
| [x] | 2011.03798v3 PairRE | S5.T4, T8 | 94 | 36 | "filtered" never named (5); T8 over both directions (6) |
| [x] | 1911.00219v3 InteractE | S7.T2 | 83 | 37 | - |
| [x] | 2203.02167v1 SimKGC | S4.T3 | 85 | 3 | four SimKGC versions as one method (1); analyses negative (9) |
| [x] | 1911.03903v3 Re-evaluation | S4.T2, A1.T3 | 49 | 95 | which columns count (7) |
| [x] | 2402.02389v2 KICGPT | S4.T1, T4 | 125 | 43 | ChatGPT rows held back (8); KICGPT_tsa (1) |

## Decisions (all 12 confirmed on review)

1. **Versions of one method are one method.** QuatE^1/^2/^3 (type constraints,
   N3 + reciprocal), SimKGC_IB/IB+PB/IB+SN/IB+PB+SN, KICGPT and KICGPT_tsa,
   MEM-KGC with and without EP: same method id, all with the paper's role, so
   the comparison engine shows the different values side by side. The
   alternative is one id per version (`quate-type-constraints`, ...).
2. **QuatE's "RotatE" (without self-adversarial sampling) and "a-RotatE" are
   both `rotate`**, as decided for TuckER's RotatE row in the development set.
3. **AttH's low-dimensional table (d = 32) is held back entirely.** The registry
   has no embedding-dimension setting; ranking these next to 500-dimensional
   results would be wrong. Adding a `dimension` setting would make them
   results.
4. **AttH's "Quaternion" row is QuatE** (`quate`): its numbers are QuatE^3's.
5. **PairRE never writes "filtered"**, but S5.SS1.p3 describes it exactly
   (ranks computed after removing all other observed triples). Gold: filtered.
   The extraction requires evidence that names the setting value, so it is
   expected to send this table to review: a deliberate test of that rule.
6. **PairRE Table 8 is over both query directions**: its FB15k baselines are
   the means of RotatE's head and tail numbers (ComplEx 1-to-N:
   (.969 + .823) / 2 = .896).
7. **Re-evaluation paper.** Tie-breaking (Random, Top, Bottom) is not a
   registry setting. Results: the Random columns (the protocol the paper
   recommends, and the one ConvE, RotatE and TuckER use), and the reported
   numbers of ConvE, RotatE and TuckER. Held back: Top and Bottom columns, the
   reported numbers of ConvKB, CapsE and KBAT (Top protocol; KBAT leaked test
   triples), and CapsE entirely on WN18RR (pre-trained GloVe, "unfair" per the
   caption).
8. **KICGPT's ChatGPT zero-shot / one-shot rows are held back**: generated
   answers, Hits@1 only, run as in Zhu et al. (2023), not a filtered ranking.
9. **Analyses are not results.** Tables whose rows are component combinations,
   hyperparameter values or score-function formulas (HAKE S4.T4, QuatE S5.T7,
   SimKGC S5.T4, A2.T12, A2.T13) are negative; KICGPT's ablation keeps its
   full-model row and holds back the others. This is the class of table where
   the development run went wrong (NBFNet's message-function ablation).
10. **Roles.** HAKE's ModE and AttH's RefH, RotH, RefE, RotE, AttE are variants
    introduced by the paper; HAKE, QuatE, AttH, PairRE, InteractE, SimKGC and
    KICGPT are proposed; the Re-evaluation paper proposes no method.
11. **Typos held back**: AttH Table 5 MurP YAGO3-10 H@10 `567`, Table 6
    Dihedral WN18RR H@10 `557` (and `559` in the held-back d = 32 table).
12. **Not scorable (4 tables)**: HAKE S4.T5 (metrics in rows), QuatE S7.T9
    (stacked WN18 / WN18RR blocks), InteractE S9.T4 (query direction and
    relation category in rows), Re-evaluation S2.T1 (metric named only in the
    caption). Extracted results there are counted and listed.
