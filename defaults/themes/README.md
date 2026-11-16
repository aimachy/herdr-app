# Shipped theme files

These files are compiled into the app (`src/settings.rs`, `THEMES`) and
written to the user's `themes/` folder on start when missing. A file the
user has edited is never replaced.

`atom-one-dark.json` and `win98.json` are Herdr's own.

## The twenty converted themes

The other twenty files (`catppuccin-latte`, `catppuccin-mocha`, `dracula`,
`dracula-light`, `github-dark`, `github-light`, `gruvbox-dark`,
`monokai-pro`, `nord`, `nord-light`, `nya-high-contrast`,
`nya-high-contrast-white`, `one-dark-pro`, `one-light`, `rose-pine`,
`rose-pine-dawn`, `solarized-light`, `tokyo-night`, `windterm-dark`,
`windterm-light`) were converted from the built-in themes of
[NyaTerm](https://github.com/nyakang/nyaterm)
(`crates/nyaterm-core/src/themes/builtins.json`), which is licensed under
the Apache License 2.0 (https://www.apache.org/licenses/LICENSE-2.0).
Copyright remains with the NyaTerm project and its contributors.

Note that NyaTerm ships its own adaptations of these color schemes, so the
values here are NyaTerm's, not each scheme community's canonical palette.

Conversion: `convert.py` in this folder regenerates the twenty files from
NyaTerm's `builtins.json`. Its header documents the field mapping: the
terminal palette and ANSI 16 map directly, fifteen UI colors land in the
`colors` overrides, and nine secondary decoration colors that have no
counterpart here (section headers, hover variants of buttons, scrollbar
thumb, shadow, intense foreground, line highlight, find-match highlight)
are dropped. The status colors `working` / `blocked` / `done` take
NyaTerm's `warning` / `danger` / `success`.
