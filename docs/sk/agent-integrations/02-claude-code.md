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

Over to cez `/mcp` v Claude Code – server `kontext` by mal vypísať jedenásť nástrojov `ctx_*`.

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

## Slash príkazy

| Príkaz | Použitie |
| --- | --- |
| `/mcp__kontext__kontext-init` | zavedenie a prehĺbenie znalostí repozitára (voliteľne s počtom úloh) |
| `/mcp__kontext__kontext-commit` | príprava aktuálnej zmeny na commit |
| `/mcp__kontext__kontext-reflect` | zachytenie toho, čo sa v tejto session rozhodlo alebo zistilo |

## CLAUDE.md

```sh
kontext connect agents-md --write
```

pridá do `AGENTS.md` označený blok s pracovnými pravidlami – alebo do `CLAUDE.md`, ak tvoj repozitár používa tento súbor. Opätovné spustenie aktualizuje blok na mieste.

## Oprávnenia

Aby sa Claude Code nepýtal pred každým volaním servera kontext, povoľ v `.claude/settings.json` nástroje len na čítanie:

```json
{
  "permissions": {
    "allow": ["mcp__kontext__ctx_brief", "mcp__kontext__ctx_search", "mcp__kontext__ctx_read",
              "mcp__kontext__ctx_why", "mcp__kontext__ctx_log", "mcp__kontext__ctx_init"]
  }
}
```

Zapisovacie nástroje (`ctx_capture`, `ctx_inbox`, `ctx_prepare_commit`, `ctx_init_submit`) nechaj na „ask“, ak chceš potvrdzovať každé zachytenie.

## Headless režim (`claude -p`)

```sh
echo '{"mcpServers":{"kontext":{"command":"kontext","args":["mcp"]}}}' > /tmp/kontext-mcp.json
claude -p --mcp-config /tmp/kontext-mcp.json --allowedTools mcp__kontext__ctx_brief < prompt.txt
```

Claude Code v print režime je dostupný aj ako LLM pre autopilot: `kontext adapters add llm-claude`.
