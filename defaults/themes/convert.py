#!/usr/bin/env python3
"""Convert NyaTerm built-in themes to herdr-app user theme files.

Source : NyaTerm (Apache-2.0) `crates/nyaterm-core/src/themes/builtins.json`
         https://github.com/nyakang/nyaterm
Target : herdr-app `themes/<name>.json` (flat palette + `colors` overrides,
         see herdr-app src/settings.rs `ThemeFile`, deny_unknown_fields).

Field mapping follows the mapping table in this file's header (worked out in
the fork's theme study, nyaterm-theme-study.md section 3):

  palette   background <- terminal.background
            foreground <- terminal.foreground
            selection  <- terminal.selectionBackground
            cursor     <- terminal.cursor
            ansi[0..15] <- black red green yellow blue magenta cyan white
                           brightBlack ... brightWhite (in order)
  colors    canvas <- bg            chrome  <- bgPanel   popover <- bgPanel
            input  <- bgInput       hover   <- bgHover   border  <- border
            foreground <- text      muted   <- textMuted dim     <- textDimmed
            accent <- primary       accent_foreground <- onPrimary
            ring   <- focusRing     working <- warning   blocked <- danger
            done   <- success
  dropped   bgSectionHeader, primaryHover, dangerHover, scrollThumb, shadow,
            terminal.foregroundIntense, terminal.lineHighlight,
            terminal.findMatchBackground, terminal.findMatchBorder
            (no herdr counterpart); bgTerminal (duplicates
            terminal.background), link (same family as accent) and
            NyaTerm's own UI `accent` are not carried over either.

Usage: python3 convert.py [builtins.json] [outdir]
With no arguments it downloads builtins.json from the NyaTerm repo.
The catppuccin-mocha output is regression-checked against the hand-made
sample from the study; any mismatch aborts before files are written.
"""

import json
import re
import sys
import urllib.request
from pathlib import Path

RAW_URL = (
    "https://raw.githubusercontent.com/nyakang/nyaterm/main/"
    "crates/nyaterm-core/src/themes/builtins.json"
)

ANSI_ORDER = [
    "black", "red", "green", "yellow", "blue", "magenta", "cyan", "white",
    "brightBlack", "brightRed", "brightGreen", "brightYellow",
    "brightBlue", "brightMagenta", "brightCyan", "brightWhite",
]

# (herdr colors key, NyaTerm UI colors key)
COLOR_MAP = [
    ("canvas", "bg"),
    ("chrome", "bgPanel"),
    ("popover", "bgPanel"),
    ("input", "bgInput"),
    ("hover", "bgHover"),
    ("border", "border"),
    ("foreground", "text"),
    ("muted", "textMuted"),
    ("dim", "textDimmed"),
    ("accent", "primary"),
    ("accent_foreground", "onPrimary"),
    ("ring", "focusRing"),
    ("working", "warning"),
    ("blocked", "danger"),
    ("done", "success"),
]

# Every named color herdr-app's resolve() can produce (settings.rs).
NAMED_COLORS = {
    "background", "terminal_foreground", "foreground", "surface", "canvas",
    "chrome", "popover", "raised", "hover", "selected", "border", "input",
    "muted", "dim", "accent", "accent_foreground", "solid",
    "solid_foreground", "ring", "selection", "cursor", "working", "blocked",
    "done", "idle", "unknown",
}

TOP_KEYS = {"background", "foreground", "selection", "cursor", "ansi", "colors"}
HEX6 = re.compile(r"^#[0-9a-f]{6}$")
HEX6_OR_8 = re.compile(r"^#[0-9a-f]{6}([0-9a-f]{2})?$")

# Hand-converted sample from the study (section 3) — the regression baseline.
EXPECTED_CATPPUCCIN_MOCHA = {
    "background": "#1e1324",
    "foreground": "#cdd6f4",
    "selection": "#4a3758",
    "cursor": "#f5e0dc",
    "ansi": [
        "#45475a", "#f38ba8", "#a6e3a1", "#f9e2af",
        "#89b4fa", "#f5c2e7", "#94e2d5", "#bac2de",
        "#585b70", "#f5a0b7", "#b6f0b2", "#fbe8c0",
        "#9cc5ff", "#f7cfed", "#a6eee2", "#cdd6f4",
    ],
    "colors": {
        "canvas": "#221827",
        "chrome": "#1a1420",
        "popover": "#1a1420",
        "border": "#4d3a57",
        "input": "#1d1624",
        "hover": "#33263c",
        "foreground": "#cdd6f4",
        "muted": "#a6adc8",
        "dim": "#6c7086",
        "accent": "#cba6f7",
        "accent_foreground": "#1e1e2e",
        "ring": "#cba6f7",
        "working": "#f9e2af",
        "blocked": "#f38ba8",
        "done": "#a6e3a1",
    },
}


def norm(value, where):
    """Normalize a source color to lowercase #rrggbb, or die trying."""
    v = str(value).strip().lower()
    if not HEX6.match(v):
        raise ValueError(f"{where}: {value!r} is not a #rrggbb color")
    return v


def convert(theme):
    ui = theme["colors"]
    term = ui["terminal"]
    tid = theme["id"]
    out = {
        "background": norm(term["background"], f"{tid}.terminal.background"),
        "foreground": norm(term["foreground"], f"{tid}.terminal.foreground"),
        "selection": norm(term["selectionBackground"], f"{tid}.terminal.selectionBackground"),
        "cursor": norm(term["cursor"], f"{tid}.terminal.cursor"),
        "ansi": [norm(term[k], f"{tid}.terminal.{k}") for k in ANSI_ORDER],
        "colors": {
            dst: norm(ui[src], f"{tid}.colors.{src}") for dst, src in COLOR_MAP
        },
    }
    return out


def validate(name, theme):
    """Check a converted theme against herdr-app's ThemeFile rules."""
    extra = set(theme) - TOP_KEYS
    assert not extra, f"{name}: unknown top-level keys {extra}"
    for key in ("background", "foreground", "selection", "ansi"):
        assert key in theme, f"{name}: missing required {key}"
    for key in ("background", "foreground", "selection", "cursor"):
        if key in theme:
            assert HEX6.match(theme[key]), f"{name}: {key} not #rrggbb"
    assert len(theme["ansi"]) == 16, f"{name}: ansi has {len(theme['ansi'])} entries"
    for v in theme["ansi"]:
        assert HEX6.match(v), f"{name}: ansi value {v} not #rrggbb"
    for k, v in theme.get("colors", {}).items():
        assert k in NAMED_COLORS, f"{name}: colors.{k} is not a herdr color role"
        assert HEX6_OR_8.match(v), f"{name}: colors.{k} not #rrggbb(aa)"


def main():
    src = sys.argv[1] if len(sys.argv) > 1 else None
    outdir = Path(sys.argv[2]) if len(sys.argv) > 2 else Path(__file__).parent
    if src:
        themes = json.loads(Path(src).read_text())
    else:
        with urllib.request.urlopen(RAW_URL) as r:
            themes = json.loads(r.read())
    assert isinstance(themes, list) and len(themes) == 20, (
        f"expected 20 built-in themes, got {len(themes)}")

    converted = {}
    for t in themes:
        converted[t["id"]] = convert(t)

    # Regression: catppuccin-mocha must match the study's hand-made sample
    # field for field before anything is written.
    got = converted["catppuccin-mocha"]
    assert got == EXPECTED_CATPPUCCIN_MOCHA, (
        "catppuccin-mocha regression mismatch:\n"
        f"  got      {json.dumps(got, sort_keys=True)}\n"
        f"  expected {json.dumps(EXPECTED_CATPPUCCIN_MOCHA, sort_keys=True)}")
    print("regression: catppuccin-mocha matches the study sample field for field")

    outdir.mkdir(parents=True, exist_ok=True)
    for tid, theme in converted.items():
        validate(tid, theme)
        (outdir / f"{tid}.json").write_text(
            json.dumps(theme, indent=2, ensure_ascii=False) + "\n")
        print(f"wrote {tid}.json")
    print(f"done: {len(converted)} themes -> {outdir}")


if __name__ == "__main__":
    main()
