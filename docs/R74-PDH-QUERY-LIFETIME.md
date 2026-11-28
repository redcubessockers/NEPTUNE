# R74 — PDH query lifetime and ownership

**Scope**: `crates/victory-gpu/src/stats.rs` and one Windows-only CI step.  
**State**: RED reproduced on target Windows; GREEN not yet verified.  
**Safety gate**: R52 BLOCKED. Draft only, no production merge.

## Defect and independent contract

Microsoft `PdhCloseQuery` closes the query, its contained counters, and related handles.
Original `Engines::open` returned `None` if `PdhAddEnglishCounterW`
failed, without closing a successfully opened query. The `Engines` owner also
had no `Drop` implementation. This violates the resource lifetime contract.

Primary Win32 API:
https://learn.microsoft.com/en-us/windows/win32/api/pdh/nf-pdh-pdhclosequery

## Causal RED — native Windows x64

Parent: `fbfa1f53413e5e8a92ecbe009c17f31ba1fa9921` (R73).
RED commit: `2516700c23b2075a5a095a325f65d601b55dd36b`.

The test-only thread-local witness counts successful `PdhOpenQueryW` and
actual `PdhCloseQuery` calls, and records the close return code.
It does not infer kernel handle allocation counts.

| Reproduction | Result | Observed | Expected |
|---|---|---|---|
| Invalid PDH counter registration | exit 101, FAILED | opens=1 closes=0 | opens=1 closes=1 status=0 |
| Successful PDH open followed by owner drop | exit 101, FAILED | opens=1 closes=0 | opens=1 closes=1 status=0 |

Both tests were explicitly run with `--ignored --exact --nocapture --test-threads=1`
on native Windows, isolated checkout, Cargo `--locked --offline`.
The invalid-path test registered a nonexistent counter object; the
owner-drop test registered `\\Processor(_Total)\\% Processor Time`.

RED script: `D:\Projects\NEPTUNE\R74-PDH-LIFETIME-RED-20261128.ps1`  
Script SHA-256: `f234aac2fdd95cd913550118db64c18fdecd3876882ca0b3d684b0e332eed56b`  
Execution receipt: `e1424b5d090f4a4133d8397d`  
Receipt SHA-256: `6569f0450409a3548b85c9f7086098831b7db27a829058e95d694c776dea8d2f`  
Signature verification: true.  
Failed-registration stdout SHA-256:
`90db4954e6c6b19935b2ef7f3a853711bfef5a2bd8b45ae76efd52aad2d0923c`  
Owner-drop stdout SHA-256:
`205b1fc60ac5ea5e6a9b6120bdb116be8b65eb793e135a2d6ee88e8450965c5a`.

## Candidate GREEN — not yet verified

Implementation commit: `5813dc8e450b235faa7aef14006cd4d55718d068`.

- Construct query owner immediately after successful `PdhOpenQueryW`.
- `PdhAddEnglishCounterW` failure drops the owner and calls `PdhCloseQuery`.
- `Drop` calls `PdhCloseQuery` exactly once, checks return code, logs errors.
- Test-only per-thread witness measures close count and status.
- No new dependency, public interface, runtime pool, or vendor SDK.

Workflow commit: `013c62a82ed9b63d315910c29fdd16d5d240f491`.
The existing `windows-latest` job now explicitly runs both
previously ignored lifetime regressions. A workflow success would prove
only the CI runner's Windows native path, not the user's hardware.

## Gate and remaining risks

The DEADBYTE coding lease reported `armed=false`,
`reason=epoch-revoked` after the source implementation. Its permitted
status was verified again; no alternative host command path was used.
Therefore **R74 GREEN, Debug, Release, Clippy, native target witness,
new post-change R50 hash, and full title execution are NOT TESTED**.

The last R74 RED run pinned the protected R50 executable to
SHA-256 `fc3c8f9be91bc81140c60ed1023698fb21843947997dbd1e0471b86e3b119f0a`.
No attempt to mutate R50 was made.
A future native run must re-check pre/post SHA, isolated source HEAD and
tree cleanliness, two ignored lifetime cases, Debug and Release unit suites,
strict Clippy and receipt signatures. Avoid treating CI as target hardware.

Code rollback: retain R73 parent and do not merge this draft. No production
activation. Post-implementation direct Windows verification remains BLOCKED
until a fresh authorized coding lease is available.
