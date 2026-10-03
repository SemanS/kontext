# Viac repozitárov

Session agenta začína v jednom adresári. Jeho práca tam často nezostane:

- **Submodul.** Session beží v repozitári shop, ale rozhodnutia, ktoré riadia kód, sú v `pricing/`, submodule s vlastným `.ai/`.
- **Iný worktree.** Orchestrátor ako Superset spustí agenta v jednom worktree. Agent pracuje v inom, na vetve, ktorá má tímové znalosti.
- **Susedný projekt.** Agent upravuje zdieľanú knižnicu naklonovanú vedľa aplikácie.

Pamäť viazaná na adresár session odpovie „tu nie sú žiadne tímové znalosti“, hoci rozhodnutia ležia o adresár ďalej. Zachytenia o tom kóde skončia v inboxe, z ktorého sa nikdy nepovýšia.

## Rozhodujú cesty {#paths-decide}

kontext odpovedá z repozitára, ktorému patria cesty, ktoré dostane:

| Agent zadá | kontext odpovie z |
| --- | --- |
| `ctx_brief` s `focus: ["pricing/src"]` | `pricing/`: jeho rozhodnutia, pravidlá a moduly, s odkazmi `kx:pricing/<id>` |
| `focus: ["src/round.ts"]`, cestu, ktorú má len `pricing/` | `pricing/` (tak pomenúva cesty jeho vlastné `AGENTS.md`) |
| `ctx_why pricing/src/round.ts:40` | `pricing/`: jeho rozhodnutia, blame a história |
| absolútnu cestu do iného worktree alebo projektu | ten worktree alebo projekt |
| `ctx_capture` s `paths: ["pricing/src/round.ts"]` | inbox `pricing/`, ako `src/round.ts`, na commit tam |

`dir` pomenuje repozitár priamo, pre každý nástroj: `ctx_search` s `dir: "pricing"`, `ctx_prepare_commit` s `dir: "/work/shop-checkout"`. Odpoveď z iného repozitára začína riadkom, ktorý povie, odkiaľ prišla.

## Čo ukáže vonkajší repozitár {#what-the-outer-repository-shows}

- Submodul s vlastnými znalosťami dostane v briefe sekciu. Je prvá, keď v ňom ležia cesty z focusu, a zaplní celý brief, keď vonkajší repozitár úložisko nemá. Inak naň ukazuje jeden riadok.
- `ctx_search` prehľadá aj submoduly s úložiskom. Ich výsledky nesú cestu submodulu a `ctx_read` ich otvorí tak, ako sú.
- Vetva bez tímových znalostí sa dozvie, ktorá vetva ich má a ktorý lokálny worktree:

  ```text
  - Team knowledge exists on `origin/main` but not on this branch yet … A worktree that has it: /work/shop-main (tools take `dir: "/work/shop-main"`).
  ```

- `kontext status` vypíše submoduly s vlastnými znalosťami.

## Obmedzenia {#limits}

- Zachytenie sa presunie, len keď všetky cesty patria jednému repozitáru s tímovými znalosťami. Zmiešané cesty, submodul bez úložiska alebo cesty, ktoré neexistujú nikde, ho nechajú tam, kde je, a poznámka povie prečo.
- Počítajú sa len submoduly vnútri worktree. Absolútna cesta alebo symlink v `.gitmodules` sa ignoruje. Worktree toho istého klonu uložený v tomto (`.claude/worktrees/<name>`) je iný worktree, nie submodul.
