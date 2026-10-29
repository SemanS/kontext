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

Hooky vložia brief na začiatku session a riadiace záznamy pred úpravou (opt-in do A/B reportu; pozri [Hooky](../guides/09-hooks.md)):

```sh
kontext connect claude --hooks --write    # zaregistruje aj MCP server v .mcp.json
kontext connect claude-hooks --write      # len hooky
```

`kontext brief --format claude-hook` stále vypíše `{"hookSpecificOutput": {"hookEventName": "SessionStart", "additionalContext": "…"}}`; hook SessionStart, ktorý s ním zapisovali staršie verzie, nahradí `kontext hook --host claude`.

## Hooky {#hooks}

Oba príkazy zapíšu do `.claude/settings.json`:

| Udalosť | Matcher | Čo agent dostane |
| --- | --- | --- |
| `SessionStart` | `startup\|resume\|clear\|compact` | brief; po kompaktácii alebo resume kratší brief a id, ktoré už session dostala |
| `SubagentStart` | | brief pre subagenta |
| `PreToolUse` | `Edit\|Write\|MultiEdit` | záznamy riadiace upravené súbory, ktoré v tejto session ešte nezazneli |
| `PreToolUse` | `Bash` | zamietnutie `kontext trust`, inak nič |

Každý hook spúšťa `kontext hook --host claude`, ktorý číta hook JSON zo stdin a odpovedá cez `hookSpecificOutput.additionalContext` (najviac 10 000 znakov). Hooky čerpajú z jedného rozpočtu `[hooks] session_budget_tokens` (6000), nikdy neblokujú úpravu, nikdy nevolajú model a pri akejkoľvek chybe skončia kódom 0 bez výstupu. `KONTEXT_HOOKS=off` alebo `[hooks] enabled = false` ich vypne.

Hook na `Bash` zapíše každé `kontext connect claude --write`, s `--hooks` aj bez neho: zamietne `kontext trust` spustený agentom, takže agent za teba nepinne príkazy ani adaptéry (pozri [Bezpečnosť](../concepts/08-security.md#verify)).

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

Každý končí v lokálnom inboxe alebo v stagnutých súboroch, ktoré človek skontroluje a commitne; v `.ai/` sa pred tým nič nemení. Každý súbor končí značkou `<!-- kontext-skill-hash: <hash> -->` dodávaného textu, z ktorého vznikol: opätovné `connect skills --write` po aktualizácii obnoví súbory, ktoré nikto neupravil, a súbory upravené tímom ponechá, s poznámkou; `--force` ich nahradí. Tie isté súbory fungujú v každom klientovi, ktorý číta rozloženie `SKILL.md` (`.agents/skills` pre Codex, `.claude/skills` pre Cursor a OpenCode).

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
