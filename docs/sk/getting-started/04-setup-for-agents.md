# Nastavenie agentov

kontext hovorí protokolom Model Context Protocol cez stdio. Každý klient, ktorý vie spustiť lokálny MCP server príkazom `kontext mcp`, dostane celú sadu nástrojov. Server spúšťaj v repozitári (klienti to zvyčajne robia za teba – MCP servery spúšťajú v adresári projektu).

## Pripoj klienta

| Klient | Príkaz | Čo zapíše |
| --- | --- | --- |
| Claude Code | `kontext connect claude --write` | `.mcp.json` v repozitári (rozsah projektu; každý kolega ho raz schváli) |
| Claude Code, len pre teba | `claude mcp add --scope user kontext -- kontext mcp` | tvoju používateľskú konfiguráciu |
| Claude Code, brief na začiatku session | `kontext connect claude-hooks --write` | hook `SessionStart` v `.claude/settings.json` |
| Codex | `kontext connect codex` / `--write` | vypíše / pripojí `[mcp_servers.kontext]` do `~/.codex/config.toml` (zachová zálohu) |
| Cursor | `kontext connect cursor --write` | `.cursor/mcp.json` |
| OpenCode | `kontext connect opencode --write` | `opencode.json` |
| Čokoľvek, čo číta `AGENTS.md` | `kontext connect agents-md --write` | označený blok v `AGENTS.md` (alebo `CLAUDE.md`) |

Bez `--write` iba vypíše, čo by pridal. Existujúce JSON súbory sa zlúčia, neprepíšu (komentáre v JSON súboroch sa nezachovajú).

Stránky pre jednotlivých agentov: [Claude Code](../agent-integrations/02-claude-code.md) · [Codex](../agent-integrations/03-codex.md) · [Cursor, OpenCode a ďalší](../agent-integrations/04-other-clients.md).

## Povedz agentom, ako ho používať

MCP server posiela každému klientovi pri pripojení krátke inštrukcie. Pre prostredia agentov, ktoré čítajú `AGENTS.md`, pridá `kontext connect agents-md --write` to isté do súboru:

- každú úlohu začni cez `ctx_brief` (v `focus` uveď cesty, na ktoré siahneš),
- na detaily použi `ctx_search` / `ctx_read` a na „prečo je to takto?“ `ctx_why`,
- trvalé rozhodnutia, konvencie a úskalia zachyť cez `ctx_capture`,
- pred commitom spusti `ctx_prepare_commit`.

## Slash príkazy (MCP prompty)

kontext publikuje tri prompty. Claude Code ich ukazuje ako slash príkazy:

| Prompt | Claude Code | Čo robí |
| --- | --- | --- |
| `kontext-init` | `/mcp__kontext__kontext-init` | spustí cyklus úloh zavádzania (`ctx_init` → práca → `ctx_init_submit`) |
| `kontext-commit` | `/mcp__kontext__kontext-commit` | pripraví aktuálnu zmenu: povýši kandidátov, zachytí, čo chýba |
| `kontext-reflect` | `/mcp__kontext__kontext-reflect` | prejde session a zachytí najviac tri trvalé veci |
| `kontext-distill` | `/mcp__kontext__kontext-distill` | vytiahne trvalé znalosti z iného vlákna agenta alebo workspace v Supersete |

## Viac agentov, viac worktree

Orchestrátory ako Superset spúšťajú veľa agentov v samostatných worktree jedného klonu. kontext drží inbox, outbox a postup zavádzania v **spoločnom git adresári**, takže ich zdieľajú všetky worktree klonu; vyhľadávací index je pre každý worktree zvlášť (riadi sa checkoutnutou vetvou). Stav zmazaných worktree sa automaticky uprace.

## Over

```sh
kontext call ctx_brief '{"focus": ["src"]}'   # the tool, without a client
kontext status                                # adapters, hooks, inbox, bootstrap progress
```
