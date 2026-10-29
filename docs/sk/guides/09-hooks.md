# Hooky: znalosti v momente úpravy

Agent, ktorý nikdy nezavolá `ctx_brief`, nikdy neuvidí rozhodnutia, ktoré riadia súbor, ktorý upravuje. Hooky hostiteľa doručia tieto znalosti bez volania nástroja: na začiatku session, po kompaktácii, pre subagenta a tesne pred úpravou súboru.

Hooky sú **opt-in**. Kontextové hooky inštaluje len `kontext connect <host> --hooks --write` a ostanú opt-in, kým nevyjde A/B porovnanie výsledkov agentov s hookmi a bez nich; ten report nastaví aj predvolený rozpočet.

## Inštalácia {#install}

```sh
kontext connect claude --hooks --write    # .claude/settings.json
kontext connect codex --hooks --write     # .codex/hooks.json
kontext connect cursor --hooks --write    # .cursor/hooks.json
```

Súbor commitni, aby mal celý tím rovnaké hooky. `--write` bez `--hooks` nainštaluje len strážcu shellu, ktorý agentovi odmietne `kontext trust`. Každý hook spúšťa jeden príkaz:

```sh
kontext hook --host claude    # alebo codex, cursor
```

Udalosť hooku číta ako JSON zo stdin a odpoveď vypíše v tvare, aký hostiteľ čaká.

## Čo ktorá udalosť dostane {#what-each-event-gets}

| Udalosť | Claude Code (`.claude/settings.json`) | Codex (`.codex/hooks.json`) | Cursor (`.cursor/hooks.json`) | Odpoveď |
| --- | --- | --- | --- | --- |
| začiatok session | `SessionStart`, matcher `startup\|resume\|clear\|compact` | `SessionStart`, rovnaký matcher | `sessionStart` | brief; po `compact` alebo `resume` kratší brief a id, ktoré už session dostala |
| subagent | `SubagentStart` | | | brief pre subagenta |
| úprava | `PreToolUse`, matcher `Edit\|Write\|MultiEdit` | `PreToolUse`, matcher `apply_patch\|Edit\|Write` | `postToolUse` (po úprave) | záznamy, ktoré riadia upravené súbory |
| príkaz v shelli | `PreToolUse`, matcher `Bash` | `PreToolUse`, matcher `Bash` | `beforeShellExecution` | odmietne `kontext trust`; inak nič |

Hooky Claude Code a Codexu majú tvar `{"hooks": {"<Event>": [{"matcher": "…", "hooks": [{"type": "command", "command": "…", "timeout": 15}]}]}}` (timeout 15 s na začiatku session, inak 5 s). Súbor Cursora je `{"version": 1, "hooks": {"<event>": [{"command": "…", "timeout": 5}]}}`. Každý príkaz je

```sh
command -v kontext >/dev/null 2>&1 && kontext hook --host <host> 2>/dev/null || true # kontext:managed
```

takže na stroji bez kontextu sa nespustí nič; podľa značky `connect` nahradí svoje položky a tímové nechá. Claude Code a Codex čítajú odpoveď ako `{"hookSpecificOutput": {"hookEventName": "…", "additionalContext": "…"}}`, odmietnutie ako `permissionDecision: "deny"`; Cursor číta `{"additional_context": "…"}` a `{"permission": "deny", "user_message": "…", "agent_message": "…"}`. Codex spúšťa projektové hooky len s `[features] codex_hooks = true` vo svojom `config.toml`; `connect` to povie a nezapíše to.

Hook pri úprave odpovie commitnutými, prijatými záznamami v `HEAD`, ktoré riadia upravené súbory. Pošle len záznamy, ktoré v tejto session ešte nezazneli, alebo ktorých súbor sa odvtedy zmenil (nový blob). Stav session je v `.git/kontext/sessions/<id session>.json`.

`connect` neinštaluje hook Stop. Ak si ho pridáš sám a nastavíš `[hooks] stop_note = true`, odpovie jedným riadkom: riadiace záznamy, ktoré session dostala, a minuté tokeny.

## Jeden rozpočet {#one-budget}

Na session je jeden rozpočet, `[hooks] session_budget_tokens` (6000): čerpá z neho všetko, čo hooky v jednej session vložia, a jedna odpoveď na úpravu nesie najviac `[hooks] edit_budget_tokens` (900). Navrch platia limity hostiteľa: najviac 10 000 znakov `additionalContext` v Claude Code a Cursore, okolo 2 500 tokenov v Codexe. `ctx_brief` čerpá z toho istého rozpočtu, keď pozná session (argument `session_id` alebo `CLAUDE_SESSION_ID` v prostredí servera): jeho rozpočet obmedzí to, čo session ostáva, a jeho odpoveď sa zaráta, takže to, čo vložil hook, sa nedá dvakrát.

## Čo hooky nikdy nerobia {#what-hooks-never-do}

- **Neblokujú úpravu.** Jediné odmietnutie je strážca, keď agent spúšťa `kontext trust`; kontextové odpovede nikdy nezamietnu volanie nástroja ani nezastavia session.
- **Nezlyhávajú nahlas.** Pri akejkoľvek chybe (žiadny repozitár, žiadny store, timeout) hook skončí s 0 a nevypíše nič.
- **Nevolajú model** a nič neposielajú po sieti. Brief na začiatku session beží bez adaptérov.
- **Nevkladajú nereviewovaný text.** Vkladajú sa len commitnuté, prijaté záznamy. Záznam, ktorého text označí lint inštrukcií (pokyny mierené na agenta, značky rolí, `curl | sh`, riadok `verify:` v tele), je uvedený len id s `text withheld`, takže otrávený záznam nemôže cez hook riadiť agenta.

## Vypnutie {#turn-them-off}

- Pre všetkých: `[hooks] enabled = false` v `.ai/kontext.toml`.
- Pre teba, hneď: `KONTEXT_HOOKS=off` (alebo `0`, `false`, `no`) v prostredí.
- Natrvalo: odstráň položky so značkou `kontext:managed` zo súboru hookov hostiteľa. `kontext connect <host> --write` bez `--hooks` kontextové hooky, ktoré tam už sú, nechá a pridá len strážcu `kontext trust`.

## Kontrola {#check}

```sh
echo '{"hook_event_name":"PreToolUse","tool_name":"Edit","tool_input":{"file_path":"src/billing/invoice.rs"},"session_id":"t1"}' \
  | kontext hook --host claude
```

vypíše presne to, čo by dostal Claude Code.
