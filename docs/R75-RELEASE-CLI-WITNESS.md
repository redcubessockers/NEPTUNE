# R75 — Release CLI native execution witness

**Status: IMPLEMENTED, NOT TESTED.** R52 integration and release remain BLOCKED.

## Contract

R74's Windows CI tests run Cargo test binaries. They do not establish that
the actual optimized command-line executable can map, bind and run a
title entry. R75 adds an explicit Release build and launches the resulting
CLI in three separate processes on the existing Windows CI runner.

- Source: `tests/native-cli-smoke.ps1`.
- Generate two 1,024-byte synthetic PE32+ AMD64 images outside the repository.
- Machine code: a six-byte function returning 42 or 43. No imports,
  title data, graphics, network or writes by the fixture.
- Independently require exactly one differing file byte at offset 0x201.
- Bind-only must emit the non-execution marker and no title exit result.
- Execute must return through the native entry point and report the exact
  respective return value. Require CLI exit zero and exactly one witness.
- Hash the executable and both inputs before/after; reject drift.
- Log all three SHA-256 values, not precomputed guesses.

The new CI step is `R75 native Release CLI witness`. It builds with
`cargo build --release --locked -p victory`. The smoke script uses a
fresh RUNNER_TEMP directory for fixture, configuration, cache and data.
No game data or binary is committed or uploaded.

## Historical evidence, not a new gameplay result

The inherited diagnostic index documents RTX 4070 runs, including 301
(nine minutes in a heavy area), 302 (three minutes on water), and 307
(remapping costs). It records black surfaces, refused descriptor tables,
and system-call-dominated remapping. These are reference hypotheses for
NEPTUNE, not current RTX 4060 results. The index is
[diagnostics/README.md](../diagnostics/README.md); its descriptions were
read, not all raw traces independently replayed.

## Verification and rollback

R74's two native PDH cases passed Windows CI run 46160105205 on its
earlier exact source revision. R75 is a new revision and requires its own
CI evidence. No target-machine execution, new signed receipt, physical
GPU result or actual gameplay is available: coding and autonomous leases
are disarmed (epoch-revoked). The fixed-base synthetic image may also fail
to map if its preferred address is unavailable; that is a failing witness,
not a reason to mark the test skipped or change runtime code blindly.

Keep the draft unmerged. Rollback by reverting the test and workflow
commits on this branch, or retain the R74 parent. No local R50 access,
release activation, production writes or authorization bypass.
