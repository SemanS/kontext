| scenario | item | result | found | missed | wrongly suppressed | noise | tokens | ms |
|---|---|---|---|---|---|---|---|---|
| proposed vs accepted in the brief | 1.1 | pending 1.1 | 1/2 | 1 | 0 | 0 | 624 | 5697 |
| deprecated entry marked in search | 1.1 | pass | 2/2 | 0 | 0 | 0 | 160 | 1262 |
| locally edited entry is not presented as approved | 1.1 | pending 1.1 | 0/2 | 2 | 0 | 0 | 861 | 1064 |
| near-duplicate learnings both survive | 1.2 | pending 1.2 | 1/2 | 0 | 1 | 0 | 192 | 648 |
| similar titles, different content | 1.2 | pass | 2/2 | 0 | 0 | 0 | 166 | 650 |
| memory copies collapse, a different memory stays | 1.2 | pending 1.2 | 1/2 | 0 | 1 | 0 | 254 | 907 |
| session start stays under the shared budget | 1.4 | pass | 2/2 | 0 | 0 | 0 | 1246 | 1258 |
| unavailable hub: local answers still come | 1.4 | pass | 2/2 | 0 | 0 | 0 | 269 | 1935 |
| unavailable hub skipped when local hits suffice | 1.4 | pending 1.4 | 2/2 | 0 | 0 | 1 | 287 | 3001 |
| query in own words | 3.3 | pass | 0/0 | 0 | 0 | 0 | 254 | 1224 |
| query with an exact phrase | 3.3 | pending 3.3 | 0/0 | 1 | 0 | 0 | 258 | 1130 |
| why a path is the way it is | baseline | pass | 1/1 | 0 | 0 | 0 | 195 | 336 |
| old decision whose code kept moving | baseline | pass | 2/2 | 0 | 0 | 0 | 670 | 509 |
| candidates of another worktree stay there | baseline | pass | 2/2 | 0 | 0 | 0 | 882 | 3848 |
| review date passed: marked in brief, search and status | 2.2 | pending 2.2 | 0/4 | 4 | 0 | 0 | 1246 | 2677 |
| review date before the entry date is a validation error | 2.2 | pending 2.2 | 0/1 | 1 | 0 | 0 | 12 | 322 |
| per-kind review default applies to new incidents | 2.2 | pending 2.2 | 0/1 | 1 | 0 | 0 | 302 | 1690 |
| evidence: sources counted in the brief, listed on read | 2.1 | pending 2.1 | 0/3 | 3 | 0 | 0 | 1024 | 1345 |
| evidence: a code quote is verified against its revision | 2.1 | pending 2.1 | 0/3 | 3 | 0 | 0 | 289 | 2889 |
| changed since the last brief, then quiet | 2.5 | pending 2.5 | 1/2 | 1 | 0 | 0 | 1304 | 576 |
| changed recently when the brief cache is missing | 2.5 | pending 2.5 | 1/2 | 1 | 0 | 0 | 646 | 352 |
| challenge: brief marks it, prepare-commit offers retire, retire deprecates | 2.3 | pending 2.3 | 1/8 | 7 | 0 | 0 | 742 | 1667 |
| retire refuses a changed revision and a dirty file | 2.3 | pending 2.3 | 0/2 | 2 | 0 | 0 | 60 | 592 |
| retire and supersede work for every kind | 2.3 | pending 2.3 | 2/4 | 2 | 0 | 0 | 417 | 2839 |
| related entries without a model | 3.1 | pending 3.1 | 0/3 | 3 | 0 | 0 | 31 | 215 |
| related shown when capturing | 3.1 | pending 3.1 | 0/1 | 1 | 0 | 0 | 50 | 748 |
| contradiction flagged by the model, never as replace | 3.1 | pending 3.1 | 0/3 | 3 | 0 | 0 | 31 | 196 |
| revise without a model is refused | 3.2 | pending 3.2 | 0/1 | 1 | 0 | 0 | 30 | 356 |
| revise drafts a superseding entry | 3.2 | pending 3.2 | 0/3 | 3 | 0 | 0 | 30 | 369 |

8/29 scenarios pass · found 23/64 · wrongly suppressed 2 · noise 1 · 12532 tokens · 40302 ms
