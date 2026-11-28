# NEPTUNE / ENGINEERING CONTRACT

> **Purpose:** make every claim falsifiable at the machine boundary.
>
> **Product state:** experimental Windows x64 compatibility runtime. Not a playable PC port. No release authorization is implied by these rules.

## 0x00 — Reality boundary

The title's x86-64 machine code executes on the **host x86-64 CPU** through the Windows process loader path. NEPTUNE is **not** a bare-metal operating system and does **not** run without Windows, D3D12, the host graphics driver, and supporting runtime services. "zero-abstraction" means inspecting exact bytes, ABI crossings and resource state; it is not a claim that the host stack vanishes.

Do not confuse `cargo test` with a title session, a physical-GPU fixture with rendered gameplay, shader conversion with shader parity, or successful process startup with a stable open world.

## 0x01 — Partition source from expectation

| Lane | Examples | Evidence rule |
| --- | --- | --- |
| In current public source | `crates/victory-loader`, `victory-hle`, `victory-gpu`, `victory-shader`, `victory-pipeline`, `apps/victory` | cite the exact file and commit; test the actual callable path |
| Local / unpublished candidate | R42–R52 experiments described in the journal | do not mark merged or use their test counts for public HEAD |
| Separate review branch | [R54–R61 PR #2](https://github.com/redcubessockers/NEPTUNE/pull/2) | CI belongs to the PR commit, not the protected R50 binary |
| Missing / unproven | end-to-end gameplay, renderer parity, verified AMD/Intel paths, comparative frame-time win | status `NOT TESTED` or `UNKNOWN`, not `PASS` |

The `victory-game::KNOWN_BUILDS` table is empty in the public baseline. A type or format handler is not evidence of support for a particular retail game build.

## 0x02 — CPU and Windows ABI

Required checks before claiming native-title compatibility:

1. Inspect PE32+ machine type, relocations, TLS, import/export directories and unwind tables. Reject truncated or mismatched metadata.
2. Trace entry, imports, register preservation, stack alignment and x64 unwind convention with owned synthetic fixtures.
3. Differentiate loader binding without execution from a process actually entering native title code.
4. Use actual Win32/NT status codes, fault addresses, module+RVA and disassembly offsets. Preserve the first failing boundary.
5. For asynchronous stack sampling, fail closed on invalid RSP/RIP, non-progress, and partial x86 instructions. Do not call a successful unwinder result "safe" until copied-stack bounds and native failure controls are checked.

**R52 open gate:** the RPCRT4 chained-unwind counterexample at RVA `0x589c0` demonstrates a dangerous mid-instruction PC. The bounded `RtlVirtualUnwind2` witness is evidence about that native fixture alone. A full diagnostics regression and title-session proof are still required.

## 0x03 — GPU, shaders and open world

| Observable path | Required oracle |
| --- | --- |
| D3D12.X command record and replay | command ordering, queue/fence ownership, returned HRESULT and GPU readback |
| Descriptor/resource lifetime | no reuse before known fence completion; teardown, cancellation and failure injection |
| GFX10 → IR → structured control flow → host shader | independently compared lane masks, resource indices, generated DXIL and image readback |
| Texture tiling, mip residency and sparse mapping | byte-for-byte CPU oracle, exact mip footprints, physical GPU readback when relevant |
| Streaming and caches | data identity, eviction/reload equivalence, explicit byte budget, I/O latency and missing-resource behavior |
| Present and frame pacing | captured output plus per-frame timestamps, not just a synthetic GPU copy |

The current host backend is D3D12. **Vulkan support is not established.** Mentioning Vulkan, RT, or mesh shaders in a goal does not prove their implementation.

## 0x04 — Differential test packet

For every defect, preserve all fields below:

```text
test_id          stable identifier
source           git SHA; binary SHA-256; toolchain; driver; OS build
input            owned synthetic fixture and expected input hash
reference        independent implementation, hardware or exact specified invariant
first_divergence first differing byte / command / event / fence / timestamp
red              observed failure, raw exit code, original logs
green            corrected result from changed source, separate process
negative         mutation / malformed / boundary test that detects false success
scope            unit | native | GPU | title | gameplay
side_effects     files, caches, processes, driver state
rollback         exact known-good commit and artifact hashes
verdict          PASS | FAIL | BLOCKED | NOT TESTED | UNKNOWN
```

`PASS` requires the observable boundary; a source-level diff or synthetic proof never silently upgrades itself to a hardware result.

## 0x05 — Hardware-aware optimization

Never ship ISA code for its name. Record CPUID availability, fallback, alignment, allocator ownership, cache effects and actual workload. Evaluate `REP MOVSB`, `REP STOSB`, `AVX2`, `POPCNT`, `BMI2`, non-temporal writes, prefetch and TSC only against a measured scalar/standard baseline.

Minimum performance packet: binary identity, CPU/driver/clock context, warm-up, multiple paired A/B samples, median and distribution, memory traffic, 1%/0.1% low when measuring actual frames, plus negative cases. Large setup costs and slow shapes remain part of the result. FPS from a menu, GPU throughput or a synthetic kernel is **not** gameplay FPS.

Runtime code modification requires a design proof for W^X, target-thread quiescence, instruction cache visibility and rollback. Do not enable it by default.

## 0x06 — Validation matrix

| Gate | Execution environment | May establish |
| --- | --- | --- |
| Source / asset policy | Windows CI or local source check | allowed text paths, forbidden binary signatures |
| Unit and malformed fixtures | Rust tests | tested implementation contracts |
| Native Win64 ABI | real Windows process | loader, imports, TLS, unwind and crash behavior of exercised fixtures |
| Concurrency / worker lifetime | controlled process tests | join, fence, cancellation and race invariants of exercised paths |
| Physical GPU | named adapter, driver, debug layer, captured readback | specific D3D12 operations on that adapter |
| Full diagnostics | optimized diagnostics binary | profiling, tracing, unwinding and counters in exercised workloads |
| Actual title session | real title on Windows and physical GPU | actual startup, rendering, interaction and execution path |
| Long session / release | repeated real play, error collection, artifact hashes | bounded claims for that configuration and duration |

CI on GitHub-hosted Windows runners is not evidence of an RTX 4060. Tests marked ignored are **not** pass counts unless run explicitly. Never treat a `0 tests` result as a pass.

## 0x07 — Protected production

```text
INVESTIGATE  ->  ISOLATE  ->  REPRODUCE  ->  PATCH  ->  VERIFY
                                                        |
                                    R52 SAFETY GATE ----+
                                                        |
                                           REVIEW / INTEGRATE
                                                        |
                                             REAL TITLE TEST
                                                        |
                                                 RELEASE
```

The `R50/NEPTUNE.exe` known-good checkpoint hash reported by native R61 evidence is:

`fc3c8f9be91bc81140c60ed1023698fb21843947997dbd1e0471b86e3b119f0a`

Never mutate R50, GAME, user data, inherited release binaries or a production branch as a side effect of CI or documentation work. Explicitly check the pre/post hashes before claiming unchanged local artifacts. Do not bypass a denied filesystem mutation or forcibly terminate a process to get green tests.

Every integration change needs R52 safety gate acceptance, fresh current-source checks and a reversible rollback. Documentation and review-branch work does not make that gate pass.

## 0x08 — Publication rights

The repo carries a CC0 `LICENSE`, but inherited VIctory crates declare `GPL-3.0-or-later`. Those labels conflict and the root LICENSE is **not** proof of a valid relicensing grant. The BCPack table exception is expressly outside the project's claimed license. Preserve upstream attribution and resolve provenance before redistribution claims. Never commit title-derived assets, code excerpts, screenshots or shader dumps. See [asset policy](asset-policy.md).

## 0x09 — Current priorities

**1.** Re-run the corrected R52 safety harness in an isolated native Windows process with a hash-pinned preimage and logs outside source. Its earlier host staging failed with `CREATE_RENAME_NTSTATUS=c0000043`; this is not an execution success.

**2.** Validate complete diagnostics without access violations, including failure injection, stack bounds, unwind metadata and low-rate sampling of real threads.

**3.** Measure title-level command, shader, presentation and I/O divergences by exact first failing event. Bind observations to source and binary identities.

**4.** Only then measure paired frame-time distributions and long-session behavior. Do not promise an FPS improvement or a playable release.
