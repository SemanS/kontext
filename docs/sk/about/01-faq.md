# Časté otázky

## Je kontext systém pamäte ako OpenViking alebo Mem0?

Nie. Vlastní len malú, posúdenú časť – rozhodnutia, konvencie a poznatky tímu uložené v gite – a všetko ostatné pripája. Sémantická pamäť, archívy sessions a grafy kódu sa zapájajú ako adaptéry. Mnohé tímy používajú kontext *spolu so* systémom pamäte: pozri [návod pre OpenViking](../guides/03-openviking.md).

## Prečo neukladať všetko do vektorovej databázy?

Pretože to, na čo sa tím spolieha, musí byť posúditeľné, porovnateľné v diffe a verzované spolu s kódom, ktorý opisuje. Vektorové úložiská sú výborné vo vybavovaní a zlé v úlohe pravdy. kontext drží pravdu v Markdowne a sémantické vybavovanie dostáva od adaptérov.

## Volá kontext LLM?

Sám od seba nie. Premýšľa agent, ktorého už používaš; kontext mu dáva malé úlohy a kontext s rozpočtom. LLM volá len voliteľný [autopilot](../guides/06-autopilot.md), a to cez adaptér, ktorý nakonfiguruješ.

## Koľko to stojí tokenov?

Brief má približne 1–2k tokenov (nastaviteľné). Vyhľadávanie vráti jeden riadok na výsledok v rámci rozpočtu. Detaily sa čítajú len na požiadanie (L1/L2).

## Posiela niekam môj kód?

Nie. Všetko je lokálne, pokiaľ nenakonfiguruješ adaptér, ktorý dáta posiela (a aj vtedy len to, čo deklarujú jeho operácie a udalosti). Autopilot posiela priložené úryvky LLM, ktorý si vybral. Súbory s tajnými údajmi sa nikdy nečítajú a zachytený text sa maskuje.

## Náš repozitár už má ADR.

kontext ich nájde a nové rozhodnutia zapisuje do toho istého adresára, v rovnakom štýle a číslovaní. Pozri [Zavedenie do existujúceho repozitára](../guides/01-bootstrap-an-existing-repository.md#existing-adrs).

## Musia si kontext nainštalovať aj kolegovia?

Nie. `.ai/` je obyčajný Markdown a bloky v hookoch na strojoch bez binárky nerobia nič. Inštalácia im dá nástroje, hooky a synchronizáciu do ich vlastných adaptérov.

## Môžem ho používať v repozitári, ktorý nevlastním?

Áno, súkromne: vynechaj hooky, nedávaj `.ai/` do commitov alebo presuň úložisko do lokálne vylúčeného adresára. Pozri [Repozitáre, ktoré nevlastníš](../guides/01-bootstrap-an-existing-repository.md#repositories-you-do-not-own).

## A čo monorepá?

Workspaces (Nx, moon, Turborepo, pnpm/npm, Cargo, Go, uv) sa rozpoznajú; každý deklarovaný projekt sa stane modulom, veľké sa rozdelia na oblasti a brief sa zužuje cez `focus`.

## Windows?

Natívne nie (hooky sú POSIX shell). WSL funguje.

## Ako všetko vrátim späť?

`kontext hooks uninstall`, zmaž `.ai/`, ak ho nechceš, a odstráň `.git/kontext/`. Žiadny iný stav neexistuje.

## Čím sa to líši od AGENTS.md / CLAUDE.md?

Tieto súbory sú pravidlá pre agentov, ktoré píšu ľudia a ktoré sa načítavajú celé. kontext drží veľa malých záznamov s cestami, stavmi a históriou, zoraďuje ich pre konkrétnu úlohu, prepája ich s commitmi a udržiava ich platné – a do `AGENTS.md` pridá krátky blok, ktorý agentom povie, aby ho používali.
