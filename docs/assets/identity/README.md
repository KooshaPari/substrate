# substrate — Identity Demo Media (L105)

Animated SVG + MP4 showcasing the [Backbone-2 sync-violet + warm-amber palette](../../assets/tokens.css) in motion.

## Files

| File | Purpose |
|---|---|
| `demo.svg` | 480×270 animated SVG — mesh nodes + dispatch packets + hex core (looped CSS animation, ~5s) |
| `demo.mp4` | H.264/MP4 rendered from `demo.svg` via playwright + ffmpeg (24fps, 5s loop) |

## Palette (Backbone-2 — sync-violet + warm-amber)

- Outer background `#0a0d12`
- Inset panel `#161b22`
- Sync-violet `#a371f7` (dominant — dispatch mesh)
- Warm-amber `#d29922` (2 orbit nodes — route telemetry)

## Animation

- Core pulse: 1.6s cubic-bezier heartbeat (slower than sharecli — mesh tempo)
- Mesh packets: 2.4s ease-in-out triangle rotations, 3 packets staggered (-0.8s, -1.6s)
- Orbit ring: 9s linear rotation with 1 violet + 2 amber anchor dots

## Render command

```sh
python /tmp/svg2mp4.py demo.svg demo.mp4 480 270 24 5
```

## Source of truth

- Tokens: [`../../assets/tokens.css`](../../assets/tokens.css)
- Source icon: [`../../assets/brand/substrate-icon.svg`](../../assets/brand/substrate-icon.svg)
- Scorecard: `.claude/audit/.vision/L96-L107.md`