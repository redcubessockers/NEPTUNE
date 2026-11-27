# NEPTUNE

native execution. native graphics. no full-system emulation.

NEPTUNE is an experimental x86-64 Windows compatibility and graphics runtime
derived from VIctory. the objective is a native PC execution path for Xbox
Series software, with the smallest platform translation layer that actually
works. the title's CPU code runs on the host processor; its console services,
GPU commands and shader programs require translation.

**development workspace, not a playable release.**

## machine path

```text
 PE32+ image
     |
 native x86-64 execution
     |
 platform ABI / runtime services
     |
 D3D12.X -> host D3D12 -> physical GPU
     |
 GPU ISA -> SSA -> DXIL -> driver
```

this is not a source-level port of the original game. it is a compatibility
runtime with native CPU execution and shader recompilation. not every console
API or GPU behavior is implemented. CPU-native execution is not proof of
correct rendering, security, or gameplay.

## current evidence

checkpoint: R39E, 2026-11-27. Windows 11 x64, Intel Core i5-12400F,
NVIDIA GeForce RTX 4060 8 GB. Rust 1.100.0.

| boundary | evidence |
| --- | --- |
| workspace regression | 683 passed, 0 failed, 39 ignored (exit 0) |
| source asset-policy check | 228 files passed in integrated regression |
| physical D3D12 two-flight publication | fence ordering verified on the test GPU |
| command-list retirement | fresh physical fixtures checked readback and completion |
| experimental AVX2 detiling | 24 held-out shapes matched scalar bytes; not default |
| real-game FPS / frame time / visual correctness | not verified by NEPTUNE |
| long-session stability / VRAM peaks | not verified by NEPTUNE |

R39C's synthetic AVX2 test measured up to 1.80x on a held-out CPU detiling
workload. some shapes regressed. this is not game FPS. the experimental
path stays disabled unless `VICTORY_DETILE_AVX2=1` is explicitly set.
unsupported CPU features and other layouts use the scalar path.

R38D's four-list admission bound is integrated. the bound is a count of
lists, not an exact memory limit; worker shutdown and starvation proof are
still open. R39E fixes duplicate event ticks under concurrent waits. the
RED test observed four successful waits sharing tick 1; the corrected
implementation delivered four distinct ticks. eight further concurrent
rounds passed, together with seven GPU/worker fixture executions.
These are scoped tests, not an end-to-end gameplay or FPS result.

## tree

| path | purpose |
| --- | --- |
| `apps/victory` | native command line executable |
| `crates/victory-loader` | PE mapping, binding, execution |
| `crates/victory-hle` | console ABI and platform services |
| `crates/victory-gpu` | D3D12 objects, submission, memory, fences |
| `crates/victory-shader` | GPU instruction decoding, IR, DXIL generation |
| `crates/victory-formats` | texture layouts, detiling, codecs |
| `crates/victory-pipeline` | conversion and content-addressed cache |
| `crates/victory-pe` | executable and import-library inspection |
| `crates/victory-core` | configuration, logging, asset guards |
| `crates/victory-game` | title-specific interoperability |
| `xtask` | repository asset checks |

internal crate names, binary name and environment variables retain the
VIctory names for compatibility. no cosmetic rename of the ABI.

## build and verify

Windows x64. Rust toolchain pinned in [rust-toolchain.toml](rust-toolchain.toml).
D3D12 GPU with shader model 6.6; mesh shaders required for the world path.
hardware support is not a compatibility guarantee.

```powershell
cargo xtask install-hooks
cargo test --locked --release --workspace -- --test-threads=1
cargo build --locked --release -p victory
target\release\victory.exe --version
target\release\victory.exe --help
cargo xtask policy
```

add `--offline` to Cargo build and test commands when all dependencies are
already cached. ordinary synthetic tests do not require game files. ignored
tests are not passes; run the appropriate physical fixture separately.

For architecture and configuration see [architecture](docs/architecture.md)
and [example configuration](victory.example.toml).
Keep title files, cache and data outside the repository. the runtime is not
a security sandbox. do not execute untrusted images or use irreplaceable
original data as writable test input. use only files you are entitled to use.

## engineering rules

- fence completion before reclamation or descriptor publication
- bounded queues and explicit resource ownership
- actual native execution before a runtime claim
- scalar parity before ISA specialization; measure setup cost too
- no claimed speedup without paired measurements and raw samples
- preserve the known-good artifact and the rollback path
- no release label until rendering, gameplay, frame pacing and stability
  have been tested on hardware

R36 descriptor lifetime; R37 completion and retirement; R38 asynchronous
preparation; R39 SIMD; R40 build optimization; R41 integrated runtime.
A roadmap entry is not a completed feature.

## origin and license

NEPTUNE builds on [VIctory](https://github.com/Marvisak/VIctory) by Marvisak
and contributors, and the prior VIctory fork work. It is independent of
the original game's developers and publisher.

Preserved attribution from the inherited README:

- [VIctory](https://github.com/Marvisak/VIctory): loader, platform services,
  D3D12 backend, shader recompiler; GPL-3.0-or-later.
- [XCL](https://github.com/ivall/xcl): reference implementation, GPL-2.0-only;
  the inherited README describes its inspired fixes as written anew.
- myokex, [victory-amd](https://github.com/myokex/victory-amd) commit
  `c6dc4d8`, with contributions by Spexivus, joroyt and tightfall:
  memory-view guards; Joshua Odom, commit `10ea38e`: read-beside fix.
- inherited fork changes were developed with Claude Code, as recorded by
  the original author.

The [inherited README at R39C](https://github.com/redcubessockers/NEPTUNE/blob/7ed56c1f27d7f2237c2cc37f7ac810d574278a1c/README.md)
preserves the full credits, historical measurements and upstream notes.
Those measurements are not NEPTUNE results.

Source: GPL-3.0-or-later. See [LICENSE](LICENSE). Third-party components
retain their own licenses. This repository is intended for source and
synthetic tests, not distribution of game files or derived game assets.
See [asset policy](docs/asset-policy.md), including its inherited BCPack
table exception (not covered by the project license). An allowlist pass
is not independent rights clearance.

No game downloads or executable game distributions are provided here.
