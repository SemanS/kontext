# OpenViking

[OpenViking](https://github.com/volcengine/OpenViking) je kontextová databáza pre agentov so sémantickým vyhľadávaním a zhrnutiami L0/L1/L2. kontext z neho urobí sémantickú, osobnú vrstvu vedľa tímovej vrstvy v gite, ktorá prešla review:

- `ctx_search` spojí sémantické výsledky z OpenVikingu s lokálnymi výsledkami BM25, ktoré nájde kontext,
- `ctx_read viking://…` prečíta ľubovoľné URI v OpenVikingu,
- tímové znalosti, ktoré sa dostanú do `HEAD`, sa zrkadlia do `resources/` projektu,
- zo súkromných zachytení sa stanú `memories/` projektu.

## Nastavenie

Spusti OpenViking server (pozri jeho [rýchly štart](https://github.com/volcengine/OpenViking#quick-start)), potom:

```sh
kontext adapters add openviking --var user=<your-openviking-user>
kontext adapters test openviking
```

| Premenná | Predvolená hodnota | Význam |
| --- | --- | --- |
| `user` | `$OPENVIKING_USER` alebo `default` | `X-OpenViking-User` |
| `account` | `default` | `X-OpenViking-Account` |
| `peer` | názov projektu | priestor daného projektu: `viking://user/<user>/peers/<peer>/` |

URL servera je `$OPENVIKING_URL` alebo `http://127.0.0.1:1933`. Preset používa HTTP API s hlavičkami pre dôveryhodný režim (trusted mode); ak tvoj server vyžaduje kľúč, pridaj hlavičku `Authorization` do `[adapters.openviking.headers]`.

## Čo sa zapisuje

| Udalosť | Operácia | URI v OpenVikingu |
| --- | --- | --- |
| `sync` (rozhodnutia, konvencie, poznatky, incidenty v `HEAD`) | `store` | `…/peers/<peer>/resources/kontext/<kind>/<id>.md` |
| `capture` s `visibility = private` | `remember` | `…/peers/<peer>/memories/kontext/<kind>/<id>.md` |

Zápisy používajú `mode: replace`, takže opätovná synchronizácia záznamu ho aktualizuje. Ak chceš zrkadliť aj dokumenty modulov, pridaj `"architecture"` do `kinds` v odbere pre sync. Ak chceš všetko jednorazovo doplniť spätne:

```sh
kontext sync --all
kontext outbox        # pending deliveries and last errors
```

## Len niektoré repozitáre

Obmedz adaptér cez `when`, napríklad na repozitáre, ktoré si vybral:

```toml
[adapters.openviking]
when = { repo = "github.com/acme/(shop|billing)" }
```

a v `~/.config/kontext/repos/<slug>.toml` nastav pre každý repozitár iný peer:

```toml
[adapters.openviking.vars]
peer = "shop"
```

## Rozdelenie práce

| Otázka | Najlepšie odpovie |
| --- | --- |
| „Čo tím rozhodol o X?“ | kontext (git, po review) |
| „Videl som už niečo podobné?“ | OpenViking (sémantická, osobná história) |
| „Prečo je tento riadok taký, aký je?“ | `ctx_why`: história gitu + rozhodnutia + adaptéry |

Rozhodnutia, ktoré prešli review, drž v gite. Všetko ostatné nech si pamätá OpenViking a nech sa cez `sync` naučí rozhodnutia tímu.
