# Znalosti z vlákien agentov

Veľa uvažovania tímu sa odohrá vo vláknach agentov: kolega s Claude Code alebo Codexom zvažuje dva prístupy, narazí na úskalie, dohodne pravidlo a potom vlákno skončí. `kontext distill` také vlákna prečíta a to, čo ich má prežiť, premení na kandidátov znalostí. Pristanú v lokálnom inboxe a zdieľajú sa až vtedy, keď sa povýšia s commitom a prejdú review, ako všetko ostatné zachytené.

Vlákna sa čítajú z tohto počítača: každý kolega spracúva tie svoje. Surové prepisy sa nikdy nedostanú do repozitára, len záznamy, ktoré z nich vzniknú.

## Z príkazového riadka

```sh
kontext distill                       # vypíše vlákna tohto repozitára na tomto počítači
kontext distill --claude last         # najnovšie vlákno Claude Code v tomto repozitári
kontext distill --codex 01a0e6c1      # vlákno Codexu podľa id (alebo prefixu)
kontext distill --superset            # všetky vlákna aktuálneho workspace v Supersete
kontext distill --superset calm-river # workspace podľa id, názvu worktree, cesty alebo vetvy
kontext distill slack-vlakno.md       # akýkoľvek text: skopírované vlákno zo Slacku, diskusia v PR, poznámky
pbpaste | kontext distill -           # to isté zo stdin
```

```text
Superset workspace 7c41e0d2 · Fix rounding in the cart · fix/rounding · 1 thread(s) here
Distilling 1 thread(s)…
  superset:7c41e0d2/claude:4f1c2a9b · 1 part(s), 42s · 2 entries

[decision] Round prices half-even  (libs/pricing/src/round.ts)
→ inbox:2026-09-29-round-prices-half-even
```

Čo sa deje:

1. Vlákno sa prečíta do kompaktného prepisu: čo sa vývojár pýtal, čo agent odpovedal, čo upravil a spustil (`→ edit src/cache.ts`, `→ $ git commit …`). Výstupy nástrojov, uvažovanie a vložené inštrukcie (AGENTS.md, prostredie, hooky) sa vynechajú, cesty sa prepíšu relatívne k repozitáru a tajné údaje sa zamaskujú.
2. Prepis sa rozdelí na časti po zhruba 45 000 znakoch (`--max-parts`, predvolene 12, pri dlhšom vlákne rovnomerne rozložené) a pošle sa tvojmu LLM adaptéru, viacero naraz (`--jobs`).
3. Model pomenuje rozhodnutia, konvencie, úskalia a incidenty, ktoré má tím vedieť aj neskôr, najviac `--max` na vlákno (5). Vidí, čo už je zapísané, takže to neopakuje. Zistenia podobné existujúcemu záznamu alebo kandidátovi v inboxe sa vyradia.
4. Každé zistenie sa stane kandidátom v inboxe s cestami, commitmi, v ktorých ho vlákno urobilo, `origin: thread` a zdrojom (`claude:4f1c2a9b`, ukáže ho `kontext inbox`, pri povýšení sa zahodí).

Prejdi ich cez `kontext inbox`, každý povýš so zmenou, ku ktorej patrí (`kontext prepare-commit --promote <id>`, alebo `ctx_prepare_commit` z agenta), zvyšok zahoď (`kontext inbox drop <id>`). `--dry-run` zistenia len vypíše a nezachytí ich (funguje aj tam, kde kontext ešte nie je zavedený).

| Voľba | Predvolene | |
| --- | --- | --- |
| `--llm` | `init.llm` | adaptér s operáciou `llm` (`llm-codex`, `llm-claude`, …); treba ho, keď je ich nastavených viac |
| `--max` | 5 | najviac záznamov na vlákno |
| `--max-parts` | 12 | najviac častí prepisu na vlákno |
| `--jobs` | 3 | súbežné volania modelu |
| `--list` | | vlákna len vypíše, nespracuje ich |
| `--dry-run` | | vypíše, nezachytí |

## Citlivé vlákna {#sensitive-threads}

Pracovné vlákna nesú to, z čoho je projekt: mená a adresy ľudí, vložené e-maily a logy, hosty, zákaznícke dáta. Namerané na skutočnom klientskom vlákne: z 52 MB prepisu sa k modelu dostane 240 KB (0,5 %), výstupy nástrojov a obsah súborov nikdy. To, čo zostane, sa ďalej maskuje:

- tajné údaje, ako všade v kontexte;
- e-mailové adresy, telefónne čísla, IBAN, čísla kariet a IP adresy (`127.0.0.1` zostane);
- tvoje vlastné vzory, napr. id integrácií klienta alebo mená zákazníkov:

  ```toml
  # .git/kontext/config.toml (len tento klon) alebo ~/.config/kontext/repos/<slug>.toml
  [secrets]
  redact = ["\\bINT-\\d+\\b", "\\bAcme Corp\\b"]
  ```

Model má pokyn písať pre tím (roly namiesto mien ľudí, žiadne kontakty, záznamy, prístupové údaje ani hosty) a to, čo napíše, sa znova zamaskuje. Nič sa nezdieľa, kým to neprejdeš v inboxe.

Pri repozitári klienta ešte rozhodni:

- **kam ide prepis**: číta ho model LLM adaptéra. Použi poskytovateľa, ku ktorému už dáta klienta idú (pri vlákne z Claude Code `--llm llm-claude`), alebo ho pre repozitár zafixuj cez `[init] llm = "llm-claude"` v `~/.config/kontext/repos/<slug>.toml`;
- **kde znalosti žijú**: `.ai/` v repozitári klienta je rozhodnutie klienta. Kým nesúhlasí, drž úložisko súkromne, v lokálne vylúčenom adresári ([repozitáre, ktoré nevlastníš](./01-bootstrap-an-existing-repository.md#repositories-you-do-not-own)).

## Z agenta

Prompt `kontext-distill` (`/mcp__kontext__kontext-distill` v Claude Code), alebo len prosba („vytiahni trvalé znalosti z vlákna claude:4f1c2a9b“), nechá prácu na pripojenom agentovi a jeho vlastnom modeli, bez LLM adaptéra:

1. `ctx_threads` bez argumentov vypíše posledné vlákna tohto repozitára a aktuálny workspace v Supersete;
2. `ctx_threads thread="claude:4f1c2a9b"` vráti prepis po častiach (`part=2`, … keď ohlási, že je ich viac);
3. agent overí `ctx_search` a každé zistenie zapíše cez `ctx_capture` (so `source` a `commits`).

## Superset

Superset spúšťa agentov vo workspaces (vo worktree alebo v adresári session) a každému dá id (`$SUPERSET_WORKSPACE_ID` v jeho termináloch). kontext číta vlastné záznamy Supersetu (`~/.superset/host/<organizácia>/host.db`, príkazom `sqlite3`): cestu a vetvu workspace a sedenia agentov, ktoré jeho terminály spustili. Ich prepisy nájde vo všetkých účtoch Claude Code (`~/.claude`, `~/.claude-*`, `$CLAUDE_CONFIG_DIR`) a v sedeniach Codexu (`$CODEX_HOME`, `~/.codex`), plus každé vlákno spustené v adresári workspace.

- Samotné `--superset` znamená workspace terminálu, v ktorom si; názov môže byť adresár worktree (`calm-river`), cesta, vetva alebo prefix id.
- Superset viaže agentov na terminály, nie na adresáre: vlákno, ktoré niektorý z jeho terminálov spustil v inom repozitári, sa preskočí („ran outside this repository“). Spracuj ho z toho repozitára.
- Behy, ktoré si prepis nenechávajú (napríklad `codex exec --ephemeral`), sa zarátajú ako „without a transcript“.

## Odkiaľ prepisy pochádzajú

| Agent | Súbory |
| --- | --- |
| Claude Code | `<config>/projects/<cesta ako slug>/<session>.jsonl` pre každý konfiguračný adresár |
| Codex | `<CODEX_HOME>/sessions/RRRR/MM/DD/rollout-<čas>-<id>.jsonl` a `archived_sessions/` |
| čokoľvek iné | súbor alebo stdin, čítaný ako obyčajný text |
