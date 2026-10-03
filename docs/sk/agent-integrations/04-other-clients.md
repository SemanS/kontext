# Cursor, OpenCode a ďalší MCP klienti

## Cursor

```sh
kontext connect cursor --write     # writes .cursor/mcp.json
```

```json
{ "mcpServers": { "kontext": { "command": "kontext", "args": ["mcp"] } } }
```

Cursor číta aj `.cursorrules`; kontext ho v briefe uvádza v časti *Rules for agents*. `kontext connect agents-md --write` pridá pracovné pravidlá do `AGENTS.md`.

## OpenCode

```sh
kontext connect opencode --write   # writes opencode.json
```

```json
{
  "$schema": "https://opencode.ai/config.json",
  "mcp": { "kontext": { "type": "local", "command": ["kontext", "mcp"], "enabled": true } }
}
```

## Superset a ďalšie orchestrátory {#superset-and-other-orchestrators}

Orchestrátory, ktoré spúšťajú veľa agentov v paralelných worktree, nepotrebujú nič špeciálne: prostredie každého agenta si vo svojom worktree spustí vlastný `kontext mcp`. Všetky worktree jedného klonu zdieľajú inbox, outbox a postup zavedenia (sú uložené v spoločnom git adresári); každý worktree má vlastný vyhľadávací index. Stav worktree, ktoré už neexistujú, sa vyčistí.

Čo prináša práca s orchestrovanými agentmi:

- **Čerstvé klony nemajú hooky.** Projekt, ktorý naklonuje orchestrátor (projekt v Superset, CI checkout), nespúšťa žiadne kontroly kontextu pri commite, kým v ňom raz nebeží `kontext hooks install` (worktree hooky zdieľajú). Brief na to upozorní a `ctx_prepare_commit` povie, keď treba trailery dopísať ručne.
- **Práca mimo adresára relácie.** Agent často pracuje v inom worktree, submodule alebo susednom projekte. kontext odpovedá z repozitára, ktorému patria cesty, ktoré dostane (cesty vo focuse, ciele `ctx_why`, cesty zachytenia), a každý nástroj prijíma `dir`, ktorým sa dá pomenovať iný. Brief vo vetve bez tímových znalostí pomenuje worktree, ktorý ich má.

Užitočné rozdelenie práce:

- jeden agent spustí `/mcp__kontext__kontext-init`, aby prehĺbil zavedenie, kým ostatní pracujú,
- každý agent volá `ctx_brief` s cestami, na ktoré sa sústreďuje,
- zachytenia od všetkých agentov sa stretávajú v zdieľanom inboxe a každý agent cez `ctx_prepare_commit` povýši tie, ktoré patria k jeho vlastnej zmene.

## Akýkoľvek iný MCP klient

Nakonfiguruj stdio server s príkazom `kontext` a argumentom `mcp` a spúšťaj ho v adresári repozitára. Ak klient spúšťa servery inde, zadaj repozitár explicitne:

```json
{ "command": "kontext", "args": ["-C", "/path/to/repo", "mcp"] }
```

alebo nastav `KONTEXT_DIR=/path/to/repo` v prostredí servera.

## Klienti bez MCP

Používaj CLI z ľubovoľného prostredia agenta, ktoré vie spúšťať shell príkazy:

```sh
kontext brief --focus src/api
kontext search "retry policy" --json
kontext call ctx_prepare_commit '{}'
```
