# R75 — Release CLI native execution witness

**Status: WINDOWS CI NATIVE RELEASE WITNESS PASS.** Target-machine gameplay NOT TESTED; R52 integration and release BLOCKED.

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

## R75 actual execution evidence — CI run 46161475863

- Run URL: https://github.com/redcubessockers/NEPTUNE/actions/runs/46161475863
- GitHub job: `141029541110`, Windows CI, completed success on 2026-11-28.
- Tested source commit: `7e9b69ffbf78279a80c1e99617e03fb6b9d1990d`.
- Exact command: `cargo build --release --locked -p victory` then
  `& .\\tests\\native-cli-smoke.ps1 -Runtime .\\target\\release\\victory.exe`.
- The script checked `$LASTEXITCODE`, exact stdout witnesses,
  one-byte-only difference `0x201`, and SHA-256 pre/post parity.

Raw GitHub Windows runner output from that job:

```text
not executing (pass --execute to start the title)
title exited with 42
title exited with 43
R75_RELEASE_SHA256=05A103C458E562F04A546DE3E4200845F73389B82DA941F807B4F498AB72B6A0
R75_FIXTURE_A_SHA256=5ACEC4B5B020EA74E60346977345CE59B27DF3E8D71871DA76245EF4E7E04DF3
R75_FIXTURE_B_SHA256=6D188B0928BE4854BDC4E5D361803EFAA3E4951E4A97E76F57C6454DA96892EE
R75_NATIVE_ENTRY_WITNESS bind_only=1 entry_a=42 entry_b=43 diff_offset=0x201 input_hash_parity=1
```

Other same-run Windows steps: Clippy success; diagnostic build
success; R52 PowerShell syntax only success; heap worker test success;
R74 native PDH 2/2 success with opens=1 closes=1; workspace tests success;
serialized victory-gpu regression 263 passed, 0 failed, 32 ignored.
The full workflow ended `completed/success` on the tested commit.

**Scope of PASS:** real Release CLI process and synthetic AMD64 PE
mapping/import-bind/native entry, with hashes. The hash values are
observed in CI stdout; there is no independent signed local execution
receipt for this run. This does **not** prove a commercial game boots,
renders, plays or shuts down correctly. It does not run on the user's
RTX 4060 and does not execute the R52 safety gate.

## Historical evidence, not a new gameplay result

The inherited diagnostic index documents RTX 4070 runs, including 301
(nine minutes in a heavy area), 302 (three minutes on water), and 307
(remapping costs). It records black surfaces, refused descriptor tables,
and system-call-dominated remapping. These are reference hypotheses for
NEPTUNE, not current RTX 4060 results. The index is
[diagnostics/README.md](../diagnostics/README.md); its descriptions were
read, not all raw traces independently replayed.

## Verification and rollback

R74's two native PDH cases passed Windows CI run 46160105205 on an
earlier revision. R75's current code was exercised by Windows CI run
46161475863 and produced the exact native entry witness above.
No target-machine execution, new signed local receipt, physical GPU
result or actual gameplay is available: coding and autonomous leases
are disarmed (epoch-revoked). The fixed-base synthetic fixture was
successfully mapped on that CI runner; other hosts may differ.

Keep the draft unmerged. Rollback by reverting the test and workflow
commits on this branch, or retain the R74 parent. No local R50 access,
release activation, production writes or authorization bypass.
