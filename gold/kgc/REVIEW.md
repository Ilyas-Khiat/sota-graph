# KG completion gold set: review checklist

Drafted on 2026-11-12. 10 papers, 16 tables, **937 results**,
23 cells marked for review. Labels, headers, values and scales are already
machine-checked against each paper (see [../README.md](../README.md)); what is
left is judgment.

For each paper: run `sota gold show <id>`, open the table link, and tick the
box once roles, settings, evidence and review cells look right.

| ok | paper | tables | results | review | look at |
|---|---|---|---|---|---|
| [ ] | 1606.06357v1 ComplEx | S4.T2 | 50 | 0 | raw-MRR columns mapped with `protocol = raw` |
| [ ] | 1707.01476v6 ConvE | Sx6.T3, T4, T5 | 124 | 0 | Inverse Model as a baseline (decision 4) |
| [ ] | 1901.09590v2 TuckER | S6.T3, T4 | 131 | 1 | RotatE row (decision 2), MINERVA (3), HypER typo (7) |
| [ ] | 1902.10197v1 RotatE | S4.T4, T5, A6.T11 | 136 | 0 | pRotatE as a variant |
| [ ] | 1911.03082v2 CompGCN | S5.T3 | 103 | 0 | - |
| [ ] | 2106.06935v4 NBFNet | S4.T3 | 96 | 10 | GraIL row to review (8), RotatE conflict (6) |
| [ ] | 2108.06040v2 RED-GNN | S5.T2 | 66 | 0 | percent Hits columns, MINERVA (3) |
| [ ] | 2205.15319v2 AdaProp | S4.T2 | 96 | 0 | ConvE YAGO3-10 conflict (6), MINERVA (3) |
| [ ] | 2206.04798v5 A*Net | S4.SS2.tab1.t1, t2 | 135 | 0 | t2 is tail-only: `queries = tail` |
| [ ] | 2310.04562v2 ULTRA | A6.T10 | 0 | 12 | everything to review (decision 1) |

## Decisions to confirm or overrule

1. **ULTRA goes entirely to review.** The paper ranks against the full entity
   set for heads and tails, but never says filtered or raw, and `protocol` is
   required. Its code base most likely uses the filtered protocol. If you
   accept that outside knowledge, I set `protocol = "filtered"` with that
   evidence and remove the review entries: 12 results appear.
2. **TuckER's RotatE row is `rotate`.** The caption says it is RotatE without
   self-adversarial negative sampling (0.297 vs 0.338 MRR). Kept as the same
   method so the comparison engine flags the disagreement; the alternative is
   a separate method id such as `rotate-noadv`.
3. **MINERVA is treated as head-and-tail where the paper says nothing**
   (TuckER, RED-GNN, AdaProp). A*Net states RL path-finding methods can only
   be scored on tails, and the numbers match A*Net's tail-only table. A
   single-paper reader cannot know this, so the gold follows what each paper
   says; the cross-paper fact belongs to step 5.
4. **ConvE's Inverse Model is a baseline**, though the paper builds it (as a
   leakage probe, not a contribution).
5. **Method ids merge spellings**: RGCN = R-GCN (`r-gcn`), NeuralLP = Neural LP
   (`neural-lp`), ConvE* = ConvE (`conve`), A*Net = `astarnet`, ComplEx+RP =
   `complex-rp`.
6. **Cross-paper conflicts are kept as printed**: NBFNet prints RotatE's
   FB15k-237 H@10 as 0.553 (RotatE: .533); AdaProp prints ConvE's YAGO3-10 MRR
   as 0.520 (ConvE: .44). These are test cases for conflict detection.
7. **HypER's WN18 Hits@10 in TuckER is printed `958`**, a typo. It goes to
   review, not to the graph.
8. **NBFNet's GraIL row goes to review**: evaluated with 50/100 negatives and
   an estimated MR (caption), not comparable with full ranking.

## Not covered

Datasets outside the registry (Family, UMLS, NELL-995, Countries, CoDEx,
ogbl-wikikg2), inductive tables, ablations, per-relation tables, and the
"Supervised SOTA" compilation in ULTRA.
