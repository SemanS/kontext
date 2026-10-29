# Claude Code

## Pripojenie

**Pre celý tím** (odporúčané): commitni `.mcp.json` s rozsahom projektu.

```sh
kontext connect claude --write
```

```json
{
  "mcpServers": {
    "kontext": { "command": "kontext", "args": ["mcp"] }
  }
}
```

Claude Code sa každého člena tímu raz opýta, či má dôverovať MCP serverom projektu. Členovia tímu potrebujú mať binárku `kontext` v `PATH`.

**Len pre teba**, vo všetkých projektoch:

```sh
claude mcp add --scope user kontext -- kontext mcp
```

Over to cez `/mcp` v Claude Code: server `kontext` by mal vypísať šestnásť nástrojov `ctx_*`. Claude Code odkladá MCP nástroje za tool search a načíta len ich mená a inštrukcie servera; `ctx_brief` nesie `alwaysLoad`, takže je načítaný od prvého kola.

Keď je kontext zaregistrovaný pre všetky projekty, nezavadzia tam, kde ho nikto nezaviedol: v repozitári bez úložiska znalostí to agentovi povie v inštrukciách, poskytuje `ctx_brief`, `ctx_search`, `ctx_read` a `ctx_why` nad dokumentmi a históriou, odmietne tímové zachytenia a povýšenia (súkromné poznámky fungujú ďalej) a zavádza len na požiadanie (`ctx_init` s `bootstrap=true`, ktoré odovzdá prompt `kontext-init`).

## Brief na začiatku session

Pridaj hook `SessionStart`, ktorý vloží brief ako dodatočný kontext:

```sh
kontext connect claude-hooks --write
```

```json
{
  "hooks": {
    "SessionStart": [
      { "hooks": [ { "type": "command", "timeout": 15,
        "command": "command -v kontext >/dev/null 2>&1 && kontext brief --format claude-hook 2>/dev/null || true" } ] }
    ]
  }
}
```

`kontext brief --format claude-hook` vypíše `{"hookSpecificOutput": {"hookEventName": "SessionStart", "additionalContext": "…"}}`. Na počítačoch, kde kontext nie je nainštalovaný, hook nerobí nič.

## Hooky {#hooks}

Hook SessionStart vyššie pošle jeden brief bez focusu. Ak chceš riadiace znalosti v momente úpravy súboru, nainštaluj celú sadu (opt-in do A/B reportu; pozri [Hooky](../guides/09-hooks.md)):

```sh
kontext connect claude --hooks --write
```

Zapíše do `.claude/settings.json`:

| Udalosť | Matcher | Čo agent dostane |
| --- | --- | --- |
| `SessionStart` | `startup`, `resume`, `clear`, `compact` | brief, aj znova po kompaktácii |
| `SubagentStart` | | brief pre subagenta |
| `PreToolUse` | `Edit\|Write` | záznamy riadiace `tool_input.file_path`, ktoré v tejto session ešte nezazneli |

Každý hook spúšťa `kontext hook --host claude`, ktorý číta hook JSON zo stdin a odpovedá cez `additionalContext` (najviac 10 000 znakov). Hooky zdieľajú jeden rozpočet `session_budget_tokens` s `ctx_brief`, nikdy neblokujú nástroj, nikdy nevolajú model a pri akejkoľvek chybe skončia kódom 0 bez výstupu. `KONTEXT_HOOKS=off` alebo `[hooks] enabled = false` ich vypne.

`kontext connect` pridá aj zákaz pre `kontext trust` spúšťaný cez Bash, takže agent za teba nepinne príkazy ani adaptéry (pozri [Bezpečnosť](../concepts/08-security.md#verify)).

## Slash príkazy

| Príkaz | Použitie |
| --- | --- |
| `/mcp__kontext__kontext-init` | zavedenie a prehĺbenie znalostí repozitára (voliteľne s počtom úloh) |
| `/mcp__kontext__kontext-commit` | príprava aktuálnej zmeny na commit |
| `/mcp__kontext__kontext-reflect` | zachytenie toho, čo sa v tejto session rozhodlo alebo zistilo |

## Skills {#skills}

Štyri recepty na údržbu znalostí sa dodávajú s binárkou ako súbory `SKILL.md`. Nainštaluj ich do projektu (a commitni, aby ich zdieľali agenti celého tímu):

```sh
kontext connect skills --write        # .claude/skills/ a .agents/skills/kontext-*/SKILL.md
```

| Skill | Použitie |
| --- | --- |
| `kontext-audit` | v rámci rozpočtu sa pozrie na záznamy, ktoré sú najskôr zastarané; každý potvrdí, spochybní alebo nahradí, nikdy nemaže |
| `kontext-flows` | preusporiada sklad do [flows](../concepts/02-knowledge-store.md#flows): kurátorských trás cez záznamy, jedna jj zmena alebo commit na flow |
| `kontext-verify` | dá záznamom príkaz `verify`, ktorý tvrdenie dokáže, aby ho `kontext verify` vedel spustiť znova |
| `kontext-merge` | nájde záznamy, ktoré hovoria to isté alebo si odporujú (`entries.related`), a navrhne jeden nahrádzajúci záznam na skupinu |

Každý končí v lokálnom inboxe alebo v stagnutých súboroch, ktoré človek skontroluje a commitne; v `.ai/` sa pred tým nič nemení. Každý súbor nesie hash značku dodávaného textu: opätovné `connect skills --write` po aktualizácii obnoví súbory, ktoré nikto neupravil, a súbory upravené tímom ponechá, s poznámkou. Tie isté súbory fungujú v každom klientovi, ktorý číta rozloženie `SKILL.md` (`.agents/skills` pre Codex, `.claude/skills` pre Cursor a OpenCode).

Keď verify príkaz nie je pinnutý, `kontext-audit` a `kontext-verify` ukážu príkaz a zastavia sa; `kontext trust` nikdy nespúšťajú.

## Plugin

Repozitár dodáva plugin pre Claude Code postavený z rovnakých zdrojov ako `connect`: `.claude-plugin/plugin.json` s MCP serverom, hookmi a skills. Nainštaluj ho z marketplace pluginov namiesto jednotlivých príkazov `connect`; hooky v plugine majú rovnaký opt-in aj vypínač.

## CLAUDE.md

```sh
kontext connect agents-md --write
```

pridá do `AGENTS.md` označený blok s pracovnými pravidlami. Claude Code číta `AGENTS.md` natívne; keď repozitár má aj `CLAUDE.md`, kontext doň namiesto druhej kópie pridá import `@AGENTS.md`. Opätovné spustenie aktualizuje blok na mieste.

## Oprávnenia

Aby sa Claude Code nepýtal pred každým volaním servera kontext, povoľ v `.claude/settings.json` nástroje len na čítanie:

```json
{
  "permissions": {
    "allow": ["mcp__kontext__ctx_brief", "mcp__kontext__ctx_search", "mcp__kontext__ctx_read",
              "mcp__kontext__ctx_why", "mcp__kontext__ctx_log", "mcp__kontext__ctx_drift",
              "mcp__kontext__ctx_threads"]
  }
}
```

| Nástroj | Návrh | Prečo |
| --- | --- | --- |
| `ctx_brief`, `ctx_search`, `ctx_read`, `ctx_why`, `ctx_log`, `ctx_drift`, `ctx_threads` | allow | len čítajú |
| `ctx_capture`, `ctx_inbox`, `ctx_prepare_commit`, `ctx_init_submit` | ask, alebo allow, ak diff aj tak kontroluješ | zapisujú do inboxu, pracovného stromu alebo indexu |
| `ctx_init` | ask | s `bootstrap=true` zapíše kostru úložiska |
| `ctx_confirm`, `ctx_challenge` | ask | zapíšu riadok denníka alebo námietku na review |
| `ctx_verify` | ask | spustí pinnutý shellový príkaz z repozitára (`destructiveHint: true`) |
| `ctx_api` | ask | siaha na každú aplikačnú operáciu vrátane zápisov |

## Headless režim (`claude -p`)

```sh
echo '{"mcpServers":{"kontext":{"command":"kontext","args":["mcp"]}}}' > /tmp/kontext-mcp.json
claude -p --mcp-config /tmp/kontext-mcp.json --allowedTools mcp__kontext__ctx_brief < prompt.txt
```

Claude Code v print režime je dostupný aj ako LLM pre autopilot: `kontext adapters add llm-claude`.
