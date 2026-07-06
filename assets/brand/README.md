# substrate brand assets

Source of truth: [`substrate-icon.svg`](substrate-icon.svg) (1024×1024, Backbone-2 palette).

## Palette (Backbone-2, family decision 2026-07-06 by substrate-mesh)

| Token | Hex | Role |
|---|---|---|
| graphite-black | `#0a0d12` | Outer background |
| panel | `#161b22` | Inset panel base (shared with sharecli) |
| sync-violet | `#a371f7` | Primary accent — substrate dominant (dispatch/routing mesh) |
| pulse-green | `#3fb950` | Secondary accent — mesh-line glow + active-edge pulse |
| warm-amber | `#d29922` | Cooldown/warning — central dispatch node (the live router) |

## Files

| Path | Format | Use |
|---|---|---|
| `assets/brand/substrate-icon.svg` | SVG 1024×1024 | Source of truth |
| `assets/icons/substrate.iconset/` | PNG 16/32/48/64/128/256/512/1024 + @2x | macOS `.icns` source |
| `assets/icons/substrate.ico` | ICO multi-res 16/32/48/64/128/256 | Windows app icon |
| `assets/icons/substrate-256x256.png` | PNG 256×256 | Linux app icon |
| `assets/brand/substrate-icon-animated.svg` | SVG 1024×1024 (SMIL) | L101 motion variant — sync-violet mesh pulse + amber dispatch-node breathing (no JS) |

## Mark

Six-node mesh hexagon — substrate's three driver faces (CLI, HTTP, MCP) and three engines (forge/codex/claude) meeting at a central dispatch node. The violet hexagon stroke defines the boundary; the inner mesh lines are the dispatch routes between every pair of drivers/engines; the pulse-green glow on the bottom edge is the "auto-reroute-up" active signal; the warm-amber dot at the center is the live router.

## Regeneration

```bash
# Re-export iconset from SVG (after editing substrate-icon.svg)
for sz in 16 32 48 64 128 256 512 1024; do
  rsvg-convert -w $sz -h $sz assets/brand/substrate-icon.svg \
    -o assets/icons/substrate.iconset/icon_${sz}x${sz}.png
done
for sz in 16 32 128 256; do
  doubled=$((sz*2))
  cp assets/icons/substrate.iconset/icon_${doubled}x${doubled}.png \
     assets/icons/substrate.iconset/icon_${sz}x${sz}@2x.png
done

# Rebuild .ico (Windows)
convert assets/icons/substrate.iconset/icon_{16,32,48,64,128,256}x{16,32,48,64,128,256}.png \
  assets/icons/substrate.ico

# Linux 256
cp assets/icons/substrate.iconset/icon_256x256.png assets/icons/substrate-256x256.png
```

## Bundle wiring (driver-cli Cargo.toml `[package.metadata.bundle]`)

The main `substrate` binary lives in `crates/driver-cli`. The bundle metadata
goes on that crate's manifest:

```toml
[package.metadata.bundle]
name = "substrate"
identifier = "ai.kooshapari.substrate"
icon = ["../../assets/icons/substrate.iconset"]
resources = []
category = "DeveloperTool"
short_description = "Release-ready hexagonal dispatch spine"
long_description = "Three driver faces (CLI/HTTP/MCP) sharing one planner and composition root."

## Motion variant (L101)

`substrate-icon-animated.svg` ships a 3.5-second loop:

- The 6 mesh-edge lines pulse in sequence (sync-violet `#a371f7` — one edge brightens every
  ~0.58s, completing the hex rotation).
- The central amber dispatch-node `#d29922` breathes (radius 48 → 68 → 48).
- Loop is seamless: last frame == first frame.

All animation is SVG-native SMIL — no JavaScript, no external CSS. Safe to inline in HTML, SVG
`<img src>`, and README previews.
```