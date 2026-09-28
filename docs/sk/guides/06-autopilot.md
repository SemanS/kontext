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

## Modely

| Preset | Spúšťa | Premenná |
| --- | --- | --- |
| `llm-claude` | `claude -p --output-format text --model <model>` | `model` (predvolene `sonnet`; `haiku` je lacnejší a na zhrnutia modulov stačí) |
| `llm-codex` | `codex exec --sandbox read-only --ephemeral`, poslednú správu číta zo súboru | — |
| `llm-ollama` | `POST /api/generate` na `$OLLAMA_HOST` | `model` (predvolene `qwen2.5-coder:14b`) |

```sh
kontext adapters add llm-claude --var model=haiku --force
```

## Kvalita

- Pred commitom si pozri diff – autopilot zapisuje len do pracovného stromu.
- Najprv spusti pár úloh (`--max 3`) a prečítaj ich; ak sú zhrnutia plytké, uprav `init.task_max_files` alebo `init.task_inline_chars`.
- Najpozornejšie kontroluj úlohy rozhodnutí: zámer odvodzujú zo správ commitov.
- Kombinovať je v poriadku: nechaj autopilota zhrnúť moduly a úlohy `dec:` nech urobí tvoj agent s plným prístupom k nástrojom.
