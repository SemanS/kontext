# Presety

Preset je kúsok TOML, ktorý definuje jeden adaptér. kontext pár presetov obsahuje a môžeš pridať vlastné; pre kód nie je žiadny z nich výnimočný.

```sh
kontext presets                         # list bundled and user presets
kontext presets openviking              # print one
kontext adapters add openviking --var user=alice          # append it to ~/.config/kontext/config.toml
kontext adapters add codegraph --scope local              # this clone only
kontext adapters add openviking --var user=alice --force  # replace an existing definition
kontext adapters test                   # health + a sample call of each op
```

`--scope user` (predvolené) zapisuje do tvojej osobnej konfigurácie, `repo` do zdieľaného `.ai/kontext.toml` (kolegovia mu musia dať `kontext trust`), `local` do `.git/kontext/config.toml`.

Vlastné presety patria do `~/.config/kontext/presets/<name>.toml` a objavia sa v `kontext presets`.

## Pribalené presety

| Preset | Driver | Operácie | Udalosti | Poznámky |
| --- | --- | --- | --- | --- |
| `openviking` | http | search, read, store, remember, health | `sync` → store (znalosti tímu do `resources/` peeru), súkromné `capture` → remember (`memories/`) | [návod](../guides/03-openviking.md) |
| `codegraph` | mcp | code | — | aktívny tam, kde existuje `.codegraph/`; [návod](../guides/04-code-intelligence.md) |
| `serena` | mcp | code | — | vyhľadávanie symbolov cez LSP; [návod](../guides/04-code-intelligence.md) |
| `agent-lcm` | mcp | search, history, brief | — | archív sessions naprieč prostrediami agentov; [návod](../guides/05-session-history.md) |
| `sessions` | mcp | history, search, brief | — | `why_did_this_change` pre `ctx_why`; [návod](../guides/05-session-history.md) |
| `llm-claude` | command | llm | — | `claude -p` na prehĺbenie autopilotom |
| `llm-codex` | command | llm | — | `codex exec` (read-only sandbox, ephemeral) |
| `llm-ollama` | http | llm | — | lokálny model cez `/api/generate` |
| `slack-webhook` | http | store | `sync` (rozhodnutia) | oznamuje zlúčené rozhodnutia v kanáli |

Presety, ktorým chýba externý program, sú neaktívne (`when.command`), takže pridať ich na stroji bez daného nástroja je neškodné.

Presety OpenViking a CodeGraph boli overené voči ich aktuálnym verziám; Serena, Agent LCM a sessions vychádzajú z ich zdokumentovaných MCP nástrojov a spoliehajú sa na naväzovanie argumentov zo schémy – ak sa nástroj zmení, `kontext adapters inspect <name>` ukáže, čo upraviť.

## Premenné

Presety ponúkajú nastavenia cez `vars`:

| Preset | Premenná | Predvolene |
| --- | --- | --- |
| `openviking` | `user`, `account`, `peer` | `$OPENVIKING_USER` alebo `default`, `default`, názov projektu |
| `llm-claude` | `model` | `sonnet` |
| `llm-ollama` | `model` | `qwen2.5-coder:14b` |

Nastav ich pri pridávaní (`--var model=haiku`) alebo ich prepíš pre konkrétny repozitár v `~/.config/kontext/repos/<slug>.toml`:

```toml
[adapters.openviking.vars]
peer = "billing-service"
```

Slug ukáže `kontext status` (`github.com-acme-shop`).

## Prispej presetom

Dobrý preset je malý, cez `when` zostane neaktívny tam, kde jeho nástroj chýba, uprednostňuje naväzovanie zo schémy pred natvrdo zadanými `args` a svoje premenné dokumentuje v komentároch. Pridaj ho do `presets/`, zaregistruj v `src/presets.rs` a opíš ho tu.
