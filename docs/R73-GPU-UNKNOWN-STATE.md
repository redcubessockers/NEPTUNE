# R73 — GPU FALSE IDLE ELIMINATION

**Status:** native component regression PASS; production integration BLOCKED.

## Contract

A successful PDH call does not imply the selected D3D12 adapter was observed.
The absence of usable samples must not be displayed as measured zero.
A valid zero remains zero. A process absent from an otherwise observed adapter is
treated as zero under the existing sparse-counter policy; completeness of individual
process counters is not independently established.

## Causal evidence

Base: `653bf481e93117bb34f6d4e9cbcbd3d342b7deaf`.
RED: `c27fbe988b43a963c3c834521a7acb50551fc30a`.
The original mapping was extracted without changing behavior and invoked from the
real worker. Both missing-adapter and empty-snapshot fixtures failed with
`(Some(0.0), Some(0.0))` rather than `(None, None)`.
The observed-idle/process-absence control passed. The invalid-only fixture is
asserted after the empty fixture, so it was not reached in the RED run.

GREEN: `f806d4af62235001d425eb592fc223852bc5c323`.
Use the existing per-kind engine map as the presence witness. Empty map means
unknown; retain existing numeric aggregation otherwise. Periodic logs report
unknown consistently with the overlay. No new dependency or public API.

## Native verification

Windows x64; isolated checkout; Cargo locked/offline.

| Check | Exit | Result |
|---|---:|---|
| Focused regression | 0 | 1 PASS |
| Native D3D12/PDH | 0 | 1 PASS, explicitly run ignored witness |
| Stats suite | 0 | 14 PASS, 0 FAIL, 1 IGNORED |
| Debug victory-gpu lib | 0 | 263 PASS, 0 FAIL, 30 IGNORED |
| Release victory-gpu lib | 0 | 263 PASS, 0 FAIL, 30 IGNORED |
| Clippy all targets/features, -D warnings | 0 | PASS |

Native witness: LUID 0x00000000:0x0000ac86; 333 instances; 294 selected;
2 adapter LUIDs; 27 physical engines; 9 selected engine types;
global and host load 0.11814869%. The production aggregation helper was
also checked against the observed host load.

## Evidence bindings

| Evidence | SHA-256 |
|---|---|
| RED runner | 1a592f8be50e3ad4b1fcc56da29ced627a04ae84babb95e5cd05b8d2eb9386e0 |
| RED receipt | affa5dbcd9e0e780b8e2a7ac8ae883d96cf9820871fe93f16097ae000e8b9e1d |
| GREEN runner | c7b647d4d9f505dc3aace9135cfd8ce86577270ec359d30250d0bb22467713a3 |
| GREEN receipt | 058ea09dc5e6ab0a94878879a1a88ea6c42088298909221db478377c2b3dfa86 |
| Focused stdout | ddc1bf625f848f02cb41ee6826d2d2b167cae3c2a3db54eeb4e09c9a95992c1c |
| Physical stdout | 2fc7629ce7246620add9c79dfe195be7f2a39adc374ebf0e4f870b0a6f468fd3 |
| Targeted stdout | 23d8794c2413e5f4b604ab8d9bd0895f46022e1254380a81c6e098a0a328b2f3 |
| Debug stdout | e957ab6848881c48a405935f054f4e71e49d629139a1ff1b8ea7aa546216d3b7 |
| Release stdout | 304433271385fd50806906ff2b64ada9613ccc07d65818ec2c67bdeef6dce52b |
| Protected R50 | fc3c8f9be91bc81140c60ed1023698fb21843947997dbd1e0471b86e3b119f0a |

RED receipt ID `1cacb008859fac2d17145197`; GREEN receipt ID
`07ceb9f937641802405a55d4`. Both signature verifications returned true.
Logs and runners reside on the authorized Windows machine, not in Git.
The native checkout was clean and R50 matched its pinned digest before and after.

## Boundaries and rollback

No title execution, no WARP absence witness, no live overlay capture, no FPS
claim, no cross-driver matrix, no full workspace regression. A valid sample
does not prove every GPU engine or process is represented. R52 release gate
remains BLOCKED. No production merge or activation. Rollback by retaining the
R72 base and not integrating this draft. Native source and test caches remain
in the isolated workspace; the two runner scripts remain under the project root.

## Next action

Inspect PDH query ownership and close behavior. Prove lifetime behavior at the
Win32 API boundary, including failed counter registration, without touching the
protected runtime.
