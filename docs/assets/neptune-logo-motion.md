# NEPTUNE — animated README identity

## Artifact contract

The canonical **static** logo remains `docs/assets/neptune-logo.svg` (original authored vector; SHA-256 `feadee383e3569b64357d041dbe877d1a49189ce8d81bb26abf9a24b71b0d617`). An independent pixel-raster version with the same N-emblem, seven-letter wordmark, fixed geometry and fixed color palette is encoded as the **animated** `docs/assets/neptune-logo-animated.gif`. Letter caption rasterization uses an embedded 5×7 bitmap font; it is not a byte-for-byte rendering of the SVG's system fonts.

**Source:** `tools/neptune-logo-animation.cjs` — dependency-free Node script. This is a build-only recipe, not browser or game JavaScript. No SVG script, web fonts, external URLs, AI image provider or original game materials.

```text
CANVAS             1280 x 300, opaque, 16-color palette
ENCODING           GIF89a, looping LZW; 16 full frames
FRAME DURATION     160 ms (16 centiseconds), 2.56 s cycle
FIRST FRAME        pure static wordmark
LAST FRAME         identical to first (zero transition flash)
FRAMES 1..14       tiny hardware indicator motion only
TOP LED            one of 3 lights at x850+12*j / y36, j=(frame/2)%3
GLYPH HIGHLIGHT    14x14 tile x354+120*((frame-1)%7), y83
LOWER BUS          20x3 marker x355+48*(frame-1), y226
FAULT LED          9x9 amber light x1058+17*((frame/3)%3), y261
PALETTE            dark navy, hard cyan, off-white, amber; no gradient
EASING             stepped, discrete integer-coordinate transitions
REDUCED MOTION     existing static SVG selected by media query
LOOP               forever; no interactive control / no JS in README
```

Commands from a repository checkout:

```powershell
node --check tools/neptune-logo-animation.cjs
node tools/neptune-logo-animation.cjs --check
# regeneration only when intentionally changing the brand:
node tools/neptune-logo-animation.cjs --write
cargo test -p xtask --locked
cargo xtask policy
```

The `--check` path does not write files and checks **byte parity** against the deterministic recipe; the actual GIF is additionally pinned in `xtask/src/policy.rs` by SHA-256 `9dc5967c39676fad0a55ec528d502b690b1ad6b570ac0f60fa6f0483d857c535` and a 256 KiB ceiling. All other binary artwork paths remain denied. The Windows CI job independently decodes all 16 frames with GDI+, verifies dimensions and a changing control pixel, and checks the static SVG XML.

The README uses a GitHub-supported `<picture>` element. A `prefers-reduced-motion` source offers the static SVG; the `<img>` fallback is the GIF. Reduced-motion support and playback are **not guaranteed on every GitHub client** until inspected there. Both files remain valid standalone documents.

## Scope boundary

This is branding only. No PE mapping, shader translation, D3D12 commands, title assets, executable, existing R50 checkpoint, production branch or GAME state is modified. R52 safety remains blocked. This animation is not evidence of runtime performance, playable gameplay or license clearance.
