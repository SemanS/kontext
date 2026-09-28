# Inštalácia

kontext je jedna binárka. Za behu potrebuje **git 2.31 alebo novší** a nič iné.

## Inštalačný skript (odporúčané)

```sh
curl -fsSL https://raw.githubusercontent.com/SemanS/kontext/main/scripts/install.sh | sh
```

Skript stiahne predpripravenú binárku pre tvoju platformu z posledného [vydania](https://github.com/SemanS/kontext/releases) do `~/.local/bin/kontext`. Ak pre tvoju platformu binárka neexistuje, zostaví ju zo zdrojákov cez `cargo`.

| Premenná | Predvolene | Význam |
| --- | --- | --- |
| `KONTEXT_BIN_DIR` | `~/.local/bin` | kam pôjde binárka |
| `KONTEXT_VERSION` | `latest` | tag vydania, napr. `v0.1.0` |
| `KONTEXT_REPO` | `SemanS/kontext` | inštalácia z forku |

Predpripravené binárky: macOS (Apple Silicon aj Intel) a Linux x86_64.

## Zo zdrojákov

Vyžaduje Rust **1.88** alebo novší ([rustup.rs](https://rustup.rs)).

```sh
cargo install --locked --git https://github.com/SemanS/kontext --root ~/.local
# or from a checkout
git clone https://github.com/SemanS/kontext && cd kontext
cargo build --release && install -m 755 target/release/kontext ~/.local/bin/
```

## Over inštaláciu

```sh
kontext --version
kontext status        # inside any git repository
```

Uisti sa, že adresár je v `PATH`. Git hooky, ktoré kontext nainštaluje, hľadajú binárku aj v `~/.local/bin` a `~/.cargo/bin`, takže ju nájdu aj commity z GUI klientov s minimálnym `PATH`.

## Platformy

- **macOS** a **Linux** sú podporované.
- **Windows** natívne podporovaný nie je (hooky sú POSIX shell skripty); WSL funguje ako Linux.

## Aktualizácia

Spusti inštalačný skript znova (alebo `cargo install … --force`). Lokálny stav v `.git/kontext/` je verzovaný; nekompatibilný vyhľadávací index sa zostaví znova automaticky.

## Odinštalovanie

```sh
kontext hooks uninstall          # in each repository where you installed hooks
rm ~/.local/bin/kontext
rm -rf ~/.config/kontext         # your personal adapters and trust records
```

Súbory so znalosťami v tvojich repozitároch (`.ai/`) sú obyčajný Markdown a zostanú, kde sú. Lokálny stav žije v `$(git rev-parse --git-common-dir)/kontext/` a dá sa kedykoľvek zmazať.
