# Zavedenie do existujúceho repozitára

`kontext init` funguje v akomkoľvek git repozitári. Tento návod popisuje, čo skontrolovať vo väčších alebo starších repozitároch.

## Spusti ho na vetve

```sh
git switch -c kontext/bootstrap
kontext init
git status -- .ai              # what was written
kontext brief --no-adapters    # what an agent will see
```

Nič sa necommituje a nič neopustí tvoj počítač. Ak sa ti výsledok nepáči, zmaž `.ai/` (a spusti `kontext hooks uninstall`) – žiadny ďalší stav netreba upratovať.

## Skontroluj detekciu

`kontext init` vypíše, čo našiel. V `.ai/architecture/overview.md` skontroluj tri veci:

1. **Moduly.** Tabuľka *Layout* by mala zodpovedať tomu, ako o kóde uvažuješ. Deklarované projekty (manifest, Nx `project.json`, moon `moon.yml`, Cargo crate) sú vždy moduly; veľké moduly sa delia na *oblasti*. Šum vylúč cez `init.exclude` (globy) a spusti ho znova:

   ```toml
   [init]
   exclude = ["legacy/**", "**/generated/**", "tools/playground/**"]
   max_module_docs = 30
   ```

2. **Stack.** Zisťuje sa z manifestov a súborov. Je len informatívny – agenti ho čítajú v briefe.
3. **Účel.** Preberá sa z úvodu README; ak tvoje README začína návodom na inštaláciu, zostane prázdny, kým ho nenapíše úloha `purpose`.

## Existujúce ADR {#existing-adrs}

Ak repozitár uchováva záznamy o rozhodnutiach v `docs/adr`, `docs/decisions`, `adr/`, …, kontext rozpozná adresár, jeho štýl (front matter alebo polia `**Status:**`) a číslovanie a zapíše to do `.ai/kontext.toml`:

```toml
[store.kinds.decision]
path = "docs/adr"
style = "fields"
numbering = "sequential"
trailer = "Decision"
```

Existujúce ADR sa hneď zobrazia v `kontext log`, v briefe aj vo vyhľadávaní a nové rozhodnutia dostanú ďalšie číslo v rovnakom formáte. Commity, ktoré menia iba ADR, sa znova nevyťažujú.

## Hooky v repozitároch, ktoré už hooky majú

- Shell hook v `.git/hooks` alebo v `core.hooksPath` dostane za svojím shebangom blok pre kontext; tvoja logika zostane nedotknutá.
- **Verzovaný** adresár s hookmi (napr. `.githooks/`, ktorý aktivuje `npm install`) sa upraví v pracovnom strome – zmenu commitni, aby sa zdieľala, alebo nechaj kontext mimo zdieľaných hookov a nech si každý člen tímu spustí `kontext hooks install` lokálne.
- husky (`.husky/`) je podporovaný; lefthook a pre-commit si svoje hooky generujú nanovo, preto namiesto toho volaj `kontext hook pre-commit` (atď.) z ich konfigurácie.

Výsledok ukáže `kontext hooks status`.

## Monorepá

- Rozpoznané sú Nx, moon, Turborepo, pnpm/npm workspaces a Cargo workspaces; každý deklarovaný projekt sa stane modulom a hrany závislostí pochádzajú z grafu workspace a z importov.
- Počet dokumentov modulov je obmedzený (`init.max_module_docs`, predvolene 40) a sú zoradené: najprv aplikácie a služby, potom intenzívne používané knižnice. Ostatné sú uvedené v prehľade.
- Zúž `ctx_brief` pomocou `focus` – brief potom uprednostní znalosti modulov, na ktorých sa pracuje.

## Submoduly

Submoduly sa nahlásia, ale neskenujú sa. Zavedenie sprav pre každý submodul v jeho vlastnom repozitári: rozhodnutia o zdieľanom kóde patria tam, kde tento kód žije.

## Repozitáre, ktoré nevlastníš {#repositories-you-do-not-own}

Pri repozitári klienta sa pred commitnutím `.ai/` alebo zdieľaných hookov dohodni s jeho správcami. Aj tak môžeš kontext používať súkromne:

- vynechaj zdieľané časti: `kontext init --no-hooks`, `.ai/` necommituj (pridaj ho do `.git/info/exclude`), alebo
- presuň úložisko na cestu, ktorá je lokálne vylúčená, cez lokálnu konfiguráciu klonu v `.git/kontext/config.toml`:

  ```toml
  [store]
  dir = ".kontext-local"
  ```

Zachytenia, inbox a adaptéry fungujú v oboch prípadoch rovnako; vypnuté je len zdieľanie cez commity.

## Prehĺbenie

Zvyšok prenechaj agentovi (`/mcp__kontext__kontext-init`) alebo [autopilotu](./06-autopilot.md). Začni s `purpose` a najdôležitejšími modulmi; úlohy pre rozhodnutia (`dec:`) premenia commity, ktoré sa najviac podobajú na rozhodnutia, na záznamy. Výsledok posúď ako každý iný pull request.
