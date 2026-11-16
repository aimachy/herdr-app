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

## Legibility adjustments

Three `selection` values deliberately differ from NyaTerm's originals.
They are not conversion errors: in each theme the foreground text on the
selection color measured below the WCAG AA text contrast of 4.5, so the
selection was moved in lightness only (hue and saturation untouched, the
smallest shift that clears 4.5):

| Theme | NyaTerm's value (contrast) | Shipped value (contrast) |
|---|---|---|
| `one-dark-pro` | `#465163` (3.76) | `#3c4555` (4.53) |
| `solarized-light` | `#ead6a7` (3.76) | `#f5ebd3` (4.54) |
| `tokyo-night` | `#294f8d` (3.83) | `#24457a` (4.51) |

Re-running `convert.py` restores NyaTerm's values; re-apply these three
adjustments afterwards.
