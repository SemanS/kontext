# Integrácia s gitom

kontext používa ako vrstvu spolupráce samotný git: súbory pre pravdu, commity pre review, trailery pre odkazy, hooky pre vynucovanie.

## Hooky

| Hook | Čo kontext robí | Môže zablokovať? |
| --- | --- | --- |
| `pre-commit` | zvaliduje stagnuté znalosti a preskenuje ich na tajné údaje; znova vygeneruje `.ai/README.md` zo stagnutého stromu a stagne ho; pripomenie kandidátov z inboxu, ktorí sa týkajú stagnutých ciest | áno, pri chybách |
| `prepare-commit-msg` | pridá pre stagnuté záznamy trailery `Decision:`, `Convention:`, `Learning:`, `Incident:` (pri merge a squash sa preskočí) | nie |
| `post-commit` | zaradí udalosti `sync` pre znalosti, ktoré sa zmenili v `HEAD`, odstráni povýšené položky inboxu, ktoré sú už commitnuté, spustí doručovanie na pozadí | nie |
| `post-merge`, `post-rewrite` | zaradí udalosti `sync` po pulle, merge a rebase | nie |

Jednorazovo obídeš cez `KONTEXT_SKIP=1 git commit …` alebo `git commit --no-verify`.

### Pravidlá inštalácie

`kontext hooks install` (spúšťa ho aj `kontext init`) nikdy nenahradí tvoje hooky:

- Cieľom je `core.hooksPath`, ak je nastavený (`.husky/_` od husky sa mapuje na `.husky/`), inak `.git/hooks` (`git rev-parse --git-path hooks`, zdieľané všetkými worktree).
- Do existujúceho shellového hooku sa vloží označený blok hneď za shebang, takže sa spustí, aj keby zvyšok skriptu skončil predčasne.
- Hook, ktorý nie je shellový (napr. Node skript), sa premenuje na `<hook>.kontext-chained` a zavolá sa po bloku kontextu.
- Každý blok hľadá `kontext` v `PATH`, v `~/.local/bin` a `~/.cargo/bin` a bez neho neurobí nič – zdieľané adresáre hookov teda nerozbijú kolegov, ktorí kontext nemajú.
- Ak je adresár hookov sledovaný (napr. `.githooks/`), zmena sa ukáže v `git status`: commitni ju, aby hooky zdieľal celý tím.
- Ak sledovaný adresár hookov existuje, ale ešte nie je aktívny (napríklad `core.hooksPath` nastaví až `npm install`), kontext ťa na to upozorní a `kontext hooks install --dir .githooks` ho pokryje.
- Správcovia hookov, ktorí súbory hookov generujú znova (lefthook, pre-commit), sa rozpoznajú; namiesto toho pridaj `kontext hook <name>` do ich konfigurácie.

`kontext hooks status` ukáže, čo je nainštalované; `kontext hooks uninstall` bloky odstráni a obnoví zreťazené originály.

## Trailery

Pre každý súbor znalostí stagnutý v commite pridá `prepare-commit-msg` trailer (`git interpret-trailers --if-exists addIfDifferent`):

```text
fix(billing): round in cents

Decision: 2026-09-28-prices-are-integer-cents
```

Kľúče trailerov pochádzajú z `store.kinds.<kind>.trailer`. Obyčajný git s nimi odpovedá na otázky:

```sh
git log --format='%h %s%n%(trailers:key=Decision,valueonly)' -- src/billing
git log --grep 'Decision: 0007'
```

`kontext why` zoradí commity s trailermi kontextu na prvé miesta.

## Udalosti sync

Po commite, merge alebo rebase porovná kontext súbory znalostí v `HEAD` so snapshotom poslednej synchronizácie (pre každý worktree) a zaradí udalosť `sync` pre každý pridaný alebo zmenený záznam. Prvé spustenie vo worktree snapshot len zapíše; `kontext sync --all` raz zrkadlí všetky záznamy. Nič sa nedeje, kým udalosť `sync` neodoberá žiadny adaptér.

## Outbox

Hooky nikdy nekomunikujú so sieťou. Udalosti sa pripisujú do `<git-common-dir>/kontext/outbox.jsonl` a odpojený `kontext outbox flush` ich doručí. Doručenia sa sledujú pre každý adaptér a operáciu, skúšajú sa znova až osemkrát a v jednom klone beží naraz len jedno doručovanie. `kontext outbox` vypíše, čo čaká, a poslednú chybu.

## Merge

- Záznamy sú samostatné súbory: paralelné vetvy sa toho istého súboru dotknú len zriedka.
- `.ai/README.md` sa generuje znova a je označený `merge=union` (v `.ai/.gitattributes`).
- Sekvenčné čísla ADR môžu medzi vetvami kolidovať – rovnako ako pri akejkoľvek praxi s ADR; ID podľa dátumu (predvolené pre nové úložiská) sa tomu vyhnú.

## Worktree

Všetko lokálne žije v spoločnom git adresári, takže všetky worktree klonu zdieľajú inbox, outbox a postup zavádzania, zatiaľ čo každý worktree má vlastný vyhľadávací index a snapshot synchronizácie (riadia sa checkoutnutou vetvou).
