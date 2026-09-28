---
id: documentation-is-bilingual-with-mirrored-trees
kind: convention
title: Documentation is bilingual with mirrored trees
date: 2026-09-28
paths: [docs/**, README.md, README_SK.md, CONTRIBUTING.md, CONTRIBUTING_SK.md]
author: SemanS
origin: cli
---

English sources live in docs/en/ and Slovak in docs/sk/ with identical file names and structure; the English site is served at the root and the Slovak one under /sk/. Translated headings keep the English anchor ({#…}) so cross-page links work in both languages. A change to one language is mirrored in the other in the same pull request, or the pull request says that a translation follows. README and CONTRIBUTING have _SK counterparts.
