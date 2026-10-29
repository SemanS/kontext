# Hooky: znalosti v momente úpravy

Agent, ktorý nikdy nezavolá `ctx_brief`, nikdy neuvidí rozhodnutia, ktoré riadia súbor, ktorý upravuje. Hooky hostiteľa doručia tieto znalosti bez volania nástroja: na začiatku session, po kompaktácii, pre subagenta a tesne pred úpravou súboru.

Hooky sú **opt-in**. Inštaluje ich len `kontext connect <host> --hooks` a ostanú opt-in, kým nevyjde A/B porovnanie výsledkov agentov s hookmi a bez nich; ten report nastaví aj predvolený rozpočet.

## Inštalácia {#install}

```sh
kontext connect claude --hooks --write    # .claude/settings.json
kontext connect codex --hooks --write     # .codex/hooks.json
kontext connect cursor --hooks --write    # .cursor/hooks.json
```

Súbor commitni, aby mal celý tím rovnaké hooky. Každý hook spúšťa jeden príkaz:

```sh
kontext hook --host claude    # alebo codex, cursor
```

Udalosť hooku číta ako JSON zo stdin a odpoveď vypíše v tvare, aký hostiteľ čaká.

## Čo ktorá udalosť dostane {#what-each-event-gets}

| Udalosť | Claude Code | Codex | Cursor | Odpoveď |
| --- | --- | --- | --- | --- |
| začiatok session | `SessionStart` so `startup`, `resume`, `clear`, `compact` | `SessionStart` | začiatok session | brief |
| subagent | `SubagentStart` | | | brief pre subagenta |
| pred úpravou | `PreToolUse` na `Edit\|Write` | pred `apply_patch` | `postToolUse` po úprave | riadiaca sekcia pre upravovaný súbor |
| stop (voliteľné) | `Stop` s `[hooks] stop_note = true` | | | jednoriadková pripomienka zachytenia |

Hook pri úprave odpovie záznamami, ktoré riadia `tool_input.file_path`, teda tou istou sekciou, ktorou začína brief s focusom. Pošle len záznamy, ktoré v tejto session ešte nezazneli, alebo ktorých súbor sa odvtedy zmenil (nový blob). Stav session je v `.git/kontext/sessions/<id>.json`.

## Jeden rozpočet {#one-budget}

Na session je jeden rozpočet, `[hooks] session_budget_tokens`, a je to ten istý rozpočet, z ktorého čerpá `ctx_brief`. Hook nie je druhý kanál s vlastným prídelom: čo vložil hook, ďalší brief nevloží znova, a naopak. Navrch platia limity hostiteľov: najviac 10 000 znakov `additionalContext` v Claude Code, približne 2 500 tokenov v Codexe.

## Čo hooky nikdy nerobia {#what-hooks-never-do}

- **Neblokujú.** Hook nikdy nezamietne volanie nástroja ani nezastaví session; poznámka pri Stop je len `additionalContext`.
- **Nezlyhávajú nahlas.** Pri akejkoľvek chybe (žiadny repozitár, žiadne úložisko, timeout) hook skončí kódom 0 a nevypíše nič. Cieľ je 150 ms pri úložisku s 250 záznamami.
- **Nevolajú model** a nič neposielajú po sieti.
- **Nevkladajú nereviewovaný text.** Vkladajú sa len commitnuté, prijaté záznamy. Záznam, ktorého telo linter označí (imperatívy mierené na agenta, `curl | sh`, verify riadky v tele), sa uvedie len cez id, takže otrávený záznam nevie cez hook agentovi prikazovať.

## Vypnutie {#turn-them-off}

- Pre všetkých: `[hooks] enabled = false` v `.ai/kontext.toml`.
- Pre teba, hneď: `KONTEXT_HOOKS=off` v prostredí.
- Natrvalo: odstráň záznamy zo súboru hookov hostiteľa (`kontext connect <host>` bez `--hooks` vypíše súbor bez nich).

## Kontrola {#check}

```sh
echo '{"hook_event_name":"PreToolUse","tool_name":"Edit","tool_input":{"file_path":"src/billing/invoice.rs"},"session_id":"t1"}' \
  | kontext hook --host claude
```

vypíše presne to, čo by dostal Claude Code. `kontext status` ukáže, či sú hooky zapnuté a koľko z rozpočtu session je minuté.
