# Úvod

kontext je **tímový kontext pre coding agentov**. Je to jedna Rust binárka, ktorá je zároveň tromi vecami:

- **MCP server** (`kontext mcp`), ktorý môžu volať Claude Code, Codex, Cursor, OpenCode a každý ďalší MCP klient,
- **CLI** pre ľudí (`kontext brief`, `kontext why`, `kontext log`, …),
- sada **git hookov**, ktoré udržiavajú zdieľané znalosti platné, prepojené s commitmi a synchronizované.

Jej úloha je úzka a dôležitá: dať každému agentovi, ktorého používaš, rovnakú krátku, posúdenú pamäť toho, *prečo je kód taký, aký je*, a cez bežné commity premeniť „agent na niečo prišiel“ na „tím to vie“.

## Problém

Coding agenti medzi sessions všetko zabudnú a každé prostredie agenta si pamätá inak. Tímy na to odpovedajú nástrojmi na pamäť (vektorovými úložiskami, archívmi sessions, znalostnými grafmi) a rýchlo skončia s tromi novými problémami:

1. **Nikto neposudzuje, čo sa agent „naučil“.** Zlý záver uložený raz sa vybavuje navždy, všetkým.
2. **Pamäť je per stroj a per nástroj.** Čo vie Claude Code na tvojom notebooku, to Codex u kolegu nevie.
3. **Každý nástroj chce byť stredom.** Zapojiť päť systémov pamäte do piatich agentov je sieť, ktorú nikto neudržiava.

## Myšlienka

Rozdeľ kontext na vrstvy a nechaj každú robiť to, v čom je dobrá:

| Vrstva | Typické systémy | Úloha kontextu |
| --- | --- | --- |
| Surová história (sessions, prepisy) | Agent LCM, sessions, tvoje prostredie agenta | číta sa cez adaptér `history`/`search`/`brief`; nikdy sa nekopíruje do gitu |
| Inteligencia kódu | CodeGraph, Serena (LSP) | číta sa cez adaptér `code`, používa ju `ctx_why <symbol>` |
| Dlhodobá / sémantická pamäť | OpenViking, akékoľvek HTTP alebo MCP úložisko | federovaná do `ctx_search`; prijíma udalosti `sync` a súkromné `capture` |
| **Zdieľaná pravda tímu** | **Markdown v repozitári** | **patrí kontextu**: zachytenie → inbox → povýšenie → commit → review → pull |

Prvé tri vrstvy sú **adaptéry**, teda konfigurácia, nie kód. Štvrtá je tá, ktorej sa musí dať veriť, preto žije tam, kde tím už veci posudzuje: v gite.

## Princípy

- **Git + Markdown je zdroj pravdy; všetko ostatné je odvodené.** Vyhľadávací index, cache, snapshoty synchronizácie a outbox udalostí sa dajú kedykoľvek zmazať a znova sa zostavia zo súborov a histórie.
- **Agenti nikdy sami nemenia zdieľanú pravdu.** Zachytenie skončí v lokálnom inboxe. Znalosťou tímu sa stane, až keď ho povýšiš *do commitu*. Vtedy sa objaví v diffe a posudzuje sa spolu s kódom, ktorý vysvetľuje.
- **Stručné je lepšie než úplné.** Každá odpoveď pre agenta má tokenový rozpočet. Detaily sú dostupné na požiadanie cez úrovne (L0 jeden riadok, L1 prehľad, L2 celé).
- **Adaptéry sú konfigurácia.** Jadro pozná schopnosti (`search`, `read`, `store`, `history`, `code`, `brief`, `llm`), udalosti (`capture`, `promote`, `sync`) a tri drivery (`mcp`, `http`, `command`). Produkt je preset, ktorý si môžeš upraviť.
- **Hooky nikdy nečakajú na sieť.** Commity zostávajú okamžité; doručenie do adaptérov ide cez outbox, ktorý to na pozadí skúša znova.

## Ako vyzerá deň s kontextom

1. Agent začne úlohu a zavolá `ctx_brief` s cestami, na ktoré siahne. Dostane rozhodnutia a úskalia, ktoré pre ne platia.
2. Počas práce volá `ctx_search`, `ctx_read` a `ctx_why` namiesto toho, aby históriu objavoval znova.
3. Keď sa ustáli niečo trvalé (rozhodnutie, konvencia, úskalie), zavolá `ctx_capture`.
4. Pred commitom zavolá `ctx_prepare_commit`, ktorý povýši relevantných kandidátov do zmeny. Hook ich zvaliduje, preskenuje na tajné údaje a doplní trailery `Decision: <id>`.
5. Pull request nesie kód aj znalosti spolu. Po merge hooky kolegov doručia nové znalosti do ich vlastných adaptérov pamäte.

## Kam ďalej

- [Nainštaluj kontext](./02-installation.md)
- [Rýchly štart](./03-quickstart.md): zavedenie do repozitára za päť minút
- [Architektúra](../concepts/01-architecture.md): ako do seba časti zapadajú
