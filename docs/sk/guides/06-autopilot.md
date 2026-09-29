# Prehĺbenie cez LLM

Fázu prehĺbenia v `kontext init` zvyčajne riadi agent, s ktorým pracuješ. Autopilot spúšťa tie isté úlohy cez akýkoľvek LLM, ktorý nakonfiguruješ ako adaptér s operáciou `llm` – hodí sa na prvý prechod veľkým repozitárom.

```sh
kontext adapters add llm-claude              # or llm-codex, llm-ollama, or your own
kontext init --deepen --llm llm-claude --jobs 3 --max 20
```

```text
[5/5] deepen   … 1/47 done, running up to 20 task(s) with 'llm-claude' (3 at a time)
  [1/20] mod:libs-schemas ✓ (32.0s)
  [2/20] mod:apps-runner ✓ (41.3s)
  …
      deepen   partial ✓ 20 · ✗ 0 · progress 21/47
```

| Voľba | Predvolene | Význam |
| --- | --- | --- |
| `--llm` | `init.llm` | adaptér, ktorý sa použije |
| `--jobs` | 2 | úlohy paralelne |
| `--max` | 50 | úlohy v tomto behu |

Nastav predvolenú hodnotu v konfigurácii a `--llm` môžeš vynechať:

```toml
[init]
llm = "llm-claude"
```

## Čo model dostane

Každú úlohu tak, ako by ju videl agent, plus **priložené zdroje** (najviac `init.task_inline_chars`, 24000 znakov): dokumenty a kľúčové súbory modulu alebo `git show --stat` commitov v zhluku rozhodnutí. Súbory s tajnými údajmi sa nikdy nepriložia a priložený text sa zamaskuje. Model musí odpovedať jedným JSON objektom – v rovnakom tvare ako `ctx_init_submit`:

```json
{
  "summary": "One line: what the module is for",
  "overview": "5–15 lines of Markdown",
  "decisions": [{ "title": "…", "decision": "…", "context": "…", "consequences": "…", "paths": ["…"], "commits": ["abc1234"] }],
  "learnings": [{ "title": "…", "body": "…", "paths": ["…"] }]
}
```

Text alebo code fence okolo JSON sa toleruje. Zlyhané úlohy sa nahlásia a zostanú čakať.

Výsledkom je odpoveď: model dostane pokyn nevolať nástroje a presety držia vlastný MCP server kontextu mimo volania (`-c mcp_servers.kontext.enabled=false` pre Codex, `--strict-mcp-config` pre Claude Code) – keď je kontext pripojený k tvojmu klientovi, model by inak úlohu zapísal sám cez `ctx_init_submit` a druhýkrát cez odpoveď. Kým beh trvá, `ctx_init_submit` z akéhokoľvek iného procesu jeho úlohy odmietne a opakovane odovzdaná úloha nahradí vlastné skoršie záznamy namiesto pridávania kópií.

## Modely

| Preset | Spúšťa | Premenná |
| --- | --- | --- |
| `llm-claude` | `claude -p --strict-mcp-config --tools "" --no-session-persistence --system-prompt … --model <model> --effort <effort>`, bez `ANTHROPIC_API_KEY` / `ANTHROPIC_AUTH_TOKEN` | `model` (predvolene `sonnet`; `claude-opus-5-5` pre Opus, `haiku` je lacnejší), `effort` (`low` … `max`, predvolene `high`) |
| `llm-codex` | `codex exec --sandbox read-only --ephemeral -c mcp_servers.kontext.enabled=false`, poslednú správu číta zo súboru | — |
| `llm-ollama` | `POST /api/generate` na `$OLLAMA_HOST` | `model` (predvolene `qwen2.5-coder:14b`) |

```sh
kontext adapters add llm-claude --force --var model=claude-opus-5-5 --var effort=high
```

Presety sa pri pridaní skopírujú do tvojho configu; po aktualizácii ich obnov cez `--force` (aj s tvojimi `--var`).

`llm-claude` beží na subscription účtu, do ktorého je `claude` prihlásený – API kľúče z prostredia sa pre tieto volania zahodia, takže sa nič neúčtuje na API účet. Modelu nedá žiadne nástroje, MCP servery a len krátky systémový prompt: úloha tak stojí desatinu toho, čo by k nej pridal plný prompt Claude Code, na čom pri limitoch subscription záleží. Volania sa neukladajú ako sessions Claude Code, takže sa neobjavia v `claude --resume` ani v `kontext distill`.

## Kvalita

- Pred commitom si pozri diff – autopilot zapisuje len do pracovného stromu.
- Najprv spusti pár úloh (`--max 3`) a prečítaj ich; ak sú zhrnutia plytké, uprav `init.task_max_files` alebo `init.task_inline_chars`.
- Najpozornejšie kontroluj úlohy rozhodnutí: zámer odvodzujú zo správ commitov.
- Kombinovať je v poriadku: nechaj autopilota zhrnúť moduly a úlohy `dec:` nech urobí tvoj agent s plným prístupom k nástrojom.
