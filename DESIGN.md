# Design

## Theme

Physical scene: a dim Isan night studio — quiet focus, soft material depth on near-black. Surface is almost entirely `#121212`; white carries type, icons, and active states (~10%).

## Color

- **Well / body:** `#121212`
- **Raised / panel:** `#1a1a1a`
- **Border:** `rgba(255,255,255,0.08)`
- **Ink:** `#ffffff`
- **Muted ink:** `rgba(255,255,255,0.55)`
- **Faint:** `rgba(255,255,255,0.32)`
- **Danger:** `#ff6b6b` (errors only)
- **Success:** `#7dffa3` (ready only)
- No brand gradient. No second accent hue except status.

## Typography

One geometric sans with strong Thai support: `"IBM Plex Sans Thai", "Sarabun", "Noto Sans Thai", system-ui, sans-serif`. Compact product scale (1.125). Monospace for BPM numbers: `"IBM Plex Mono", ui-monospace, monospace`.

## Motion

150–220ms ease-out for hover/focus/panel. Progress rings and mode selection use opacity + transform only. `@media (prefers-reduced-motion: reduce)` → instant or crossfade.

## Layout

Centered single column on home (max ~720px). Track stage expands after ingest. Mode buttons are icon tiles in a row of three — not cards with borders unless needed for hit targets.

## Components

Buttons: solid raised `#1a1a1a`, white label/icon, brighter border on hover. Chips: pill-free rounded rectangles. Progress: determinate ring + stem list. Empty states teach with one icon + one short line.
