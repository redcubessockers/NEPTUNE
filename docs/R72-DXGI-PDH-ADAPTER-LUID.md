# R72 — Bind GPU telemetry to the host D3D12 adapter LUID

Date: 2026-11-28. Scope: Windows x64 FPS-box GPU telemetry only. **SCOPED PASS. Production integration BLOCKED.**

## Root cause

R71 preserved physical GPU-engine identity, but the runtime still computed the corner-box GPU load across every adapter visible to `\\GPU Engine(*)\\Utilization Percentage`. On a multi-adapter machine, a busier engine on another GPU could become the displayed load for the D3D12 adapter actually running NEPTUNE.

The host already owns the truth: `ID3D12Device::GetAdapterLuid()`. Microsoft documents this LUID as the current D3D12 device's adapter identifier and as suitable for robust mappings across D3D12 and DXGI.

R72 passes the existing host D3D12 device to the telemetry starter once. The stats thread reads its LUID once, then accepts PDH samples only when the PDH instance's `luid_<high>_<low>` matches that adapter. Process samples are still summed only within one physical engine. Different engines and different adapters are not summed.

No adapter enumeration was added to the frame loop. No vendor API and no dependency were added.

## Causal RED

Tests-only/scaffold commit: `4f78fee0258c2dd87e6ec2053eed81a9aec8d575`.

Fixture: selected adapter engine = 31%, other adapter engine = 99%.

Expected host-adapter metric: 31%. R71 behavior returned 99%.

Native Windows x64:

```text
selected_host_adapter_does_not_take_another_adapters_engine  EXIT 101
copy_engines_and_adapters_are_not_falsely_summed            EXIT 0
```

RED receipt: `265eea9f020eb8759c2ad704`. Ed25519 verification: **VALID**.

## Implementation

- `crates/victory-gpu/src/stats.rs`
  - adapter LUID is carried as exact high/low 32-bit words
  - `busiest_filtered` can restrict samples to one adapter before physical-engine accounting
  - production `run` uses only the host adapter
  - native ignored test creates the same default D3D12 path and checks its LUID against live PDH instances
- `crates/victory-gpu/src/fpsbox.rs`
  - `stats::start(&host.device)`
  - `Once` keeps the D3D12 LUID query out of the per-frame work after first start

Implementation commits:
- `bb941297ace0830d14a2182036d6245ed1d29290`
- `113975116bb31c6245d39e320143b64bc2a9c15e`
- `0ff438ffae051cfe960ec039047399a0f76b6cee` — final Clippy-only cleanup: global metric wrapper is test-only

## Failure found by verification

The first GREEN verification was **not** accepted as final. Runtime tests and Release passed, but strict Clippy returned exit 101 because the old global `busiest` wrapper became dead production code.

Observed receipt: `8a35a230b2fc49547b464526`, signature **VALID**.

Smallest repair: mark that global wrapper `#[cfg(test)]`. Production algorithm unchanged. All gates were then rerun from the new HEAD.

## Final native Windows x64 evidence

Final code HEAD before this report: `0ff438ffae051cfe960ec039047399a0f76b6cee`.

| Gate | Observed |
| --- | --- |
| focused regression | 1 PASS, EXIT 0 |
| physical D3D12 ↔ PDH witness | 1 PASS, EXIT 0 |
| stats tests | 11 PASS, 0 FAIL, 1 IGNORED |
| full Debug victory-gpu lib | 260 PASS, 0 FAIL, 30 IGNORED |
| full Release victory-gpu lib | 260 PASS, 0 FAIL, 30 IGNORED |
| strict Clippy all targets/features | EXIT 0 |

Final physical snapshot:

```text
D3D12 LUID       0x00000000:0x0000ac86
PDH instances    292
selected LUID    266 instances
adapter LUIDs    2
physical engines 27
global busiest   9.240529 %
host busiest     9.240529 %
host engine types 9
```

This snapshot proves the D3D12-selected adapter LUID exists in the live Windows PDH data and that the native witness can bind the two. Because global and selected loads happened to be equal in this snapshot, it does **not** prove a measurable gameplay-number difference on this run. The deterministic RED proves the cross-adapter aliasing defect.

Final stdout SHA-256:

```text
focused  23C2A8E8419A679E9FA0E8ACDE1C8D189F2E09AB1008FBDE00904774F50004E1
physical FA90C11A73163EE0113B4E93FD7E4A4D94B6FE0D3AE3ACC92DA0ED3BE3362020
targeted DF06016F7D9A10EE0E791B9080D9EF2BB23AEDEB96EC84D7721A3F1789A02A59
debug    10D0B33602C1B32037C3913215C9DEA754013651BE8F26F30DA8889E913639BD
release  201B8B79C5839822EB34946E6B8C296249F4FDFE8F5D68F5D08ABB8A634463DA
```

Final execution receipt: `bb03dfc8e8bdcee429517438`. Ed25519 verification: **VALID**.

Protected R50 SHA-256 before/after every native run:

`fc3c8f9be91bc81140c60ed1023698fb21843947997dbd1e0471b86e3b119f0a`

Working tree after final run: clean. `GAME` not opened. No production file modified. No process force-closed.

## Status and limits

**PASS** — adapter identity propagation, deterministic cross-adapter regression, live D3D12-to-PDH LUID binding, Debug/Release regression, strict lint.

**NOT TESTED** — AAA gameplay, frame-time impact, alternate GPU vendors/drivers, adapter hot-plug, device replacement, long-session telemetry drift.

**BLOCKED** — R52 integration/release gate and title/license provenance.

Rollback is branch-local: revert the R72 commits or leave the Draft PR unmerged. R50 is untouched.
