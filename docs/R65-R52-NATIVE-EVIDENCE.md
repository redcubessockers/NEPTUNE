# R65 — R52 native safety gate replay

Status: **SCOPED PASS** on the tested diagnostic fixtures. **Full R52 release BLOCKED.**

## Chain of custody

- Repository review source: [PR #2](https://github.com/redcubessockers/NEPTUNE/pull/2), published runner correction `d94081e2a26cfad3fd01e6434513d43ee5a278af`.
- Target **different, unmerged local R52 candidate**, not the review branch: `profile.rs` SHA-256 `f97badcba82927afcd74a059d086ea43bdfc1a00da47a32e094587fea7feba93`, `profile/tests.rs` `c9c9ad2a34563efd49edfd3f54f5bd57c82b4f3304f00d4abbf2a432e1d5467a`.
- Pin on protected R50 executable: `fc3c8f9be91bc81140c60ed1023698fb21843947997dbd1e0471b86e3b119f0a`.
- Original runner from a clean isolated review checkout: script SHA-256 `f571aa84e4eef58509faa5508691073db5528792dac7fe445b2e706b37461d9b`. Used for targeted, diagnostics and workspace.
- Corrected runner was read back from GitHub and staged outside source to the local R52 integration evidence directory. SHA-256 `82dbe2bf7e97d8e906c64fb9ce5563001d893fea8d5106ac09fc051c7a3f15b5`. Its signed staging receipt: `c53fb96ae8a6e550a720c8e7` (signature valid).

All runs: Windows 11 x64, Rust 1.100.0, Cargo `--locked --offline`, physical GPU where noted, isolated temporary Cargo targets and log directories. Execution scripts compare R50, profiler and test-source SHA-256 **before and after each phase**.

## Executed verification

| Phase | Executed | Evidence | Result |
| --- | --- | --- | --- |
| targeted | Four independent x64 diagnostics, including real RPCRT4 boundary and 60,000-walk stress | Signed receipt `82d53f9ac0c73bf00482fb9d` | 4 PASS, 0 FAIL |
| gpu | `cargo test --profile diag -p victory-gpu --features diagnostics --lib -- --nocapture --test-threads=1` | Signed `1500c63681da8347e5cab8ba`; stdout SHA `50f3e9fa68a6fbbfa88b7e92c9ac32daef34e666df2404a788396f1a77b04682` | 281 PASS, 0 FAIL, 32 IGNORED |
| workspace | `cargo test --locked --offline --profile release --workspace --all-features -- --nocapture --test-threads=1` | Signed `f036b3243d4cead686cd4a68`; stdout SHA `0ec481bc0664b3273b84212c8f8e15e700c3c87eec7a856c4d1438aa9b1bada5` | 721 PASS, 0 FAIL, 45 IGNORED, across 23 test blocks |
| physical | RTX 4060, driver via native D3D12, ignored fixture `texture::title_memory_tests::neptune_physical_prefix_gpu_roundtrip` | Signed `a36d7389723c4be243491ba0`; stdout SHA `d8addf36736460063c902cd7c6aa765576e6591fb89b115300b103fce071ed28` | 1 PASS, 0 FAIL, 4 phases, 2,796,192 checked readback bytes |

Each of the four execution receipts was independently reverified as **Ed25519 signature valid**. The **signed result** states `PREPOST_HASH_PARITY=1` for all four phases.

### Runner counterexample and repair

The original physical runner demanded a nonexistent `MISMATCHES=0` string, while its actual native GPU fixture printed:

```text
PHASE=0 MASK=0x1f8 PREFIX=12288 GPU_ACTIVE_BYTES=699048
PHASE=1 MASK=0x1f8 PREFIX=12288 GPU_ACTIVE_BYTES=699048
PHASE=2 MASK=0x1fc PREFIX=45056 GPU_ACTIVE_BYTES=699048
PHASE=3 MASK=0x1f8 PREFIX=12288 GPU_ACTIVE_BYTES=699048
ADAPTER=10de:2882 READBACK_BYTES_CHECKED=2796192 DRIVER_FOOTPRINT_BYTES=704008
```

Old invocation: Cargo `EXIT=0`, one passing fixture, harness `VALID=False`, exit 1. Signed failure receipt `ead478e2819514670bd39be5`. The change enforces four sequential phase IDs, nonzero per-phase active bytes, a real NVIDIA adapter, positive footprint and exact agreement between the summed checked bytes and reported readback count. It does **not** fake or skip GPU execution. Replayed from the corrected script: `VALID=True`, `EXIT=0` and signed success `a36d7389723c4be243491ba0`.

Separate **review-branch** GPU regression: PR #2 code at `97f32e6dd42e4ff3a1113ebb9ea61dcd7bd46e99`, native Release physical texture round-trip, signed receipt `fede4417c9a2e6fcb6c900e0`, 1 PASS; that is not the unmerged R52 candidate and is not double counted.

## Failures and boundaries

A first workspace replay produced 721 passing test counts in raw log `2340c4daebccb6d937d4f4c3175e94866c316d1b979a4d585e8f94e27fae073d`, but the MCP command failed internally. **Not** counted as signed PASS. The fresh second workspace replay has its own valid receipt above.

Staging a runner into C: temporary storage twice failed with `CREATE_RENAME_NTSTATUS=c0000043`. An attempted exact text replacement of the old script returned `replace_transaction_conflict` and was read back unchanged. The successful alternative staged the verified GitHub source as a new evidence file in the R52 integration directory with a distinct signed receipt. No permission boundary was bypassed.

**Not verified by these tests:** full console ABI mapping, game startup/world traversal, full frame presentation, sustained performance, multi-vendor hardware, long-session leak-free operation, arbitrary corrupt unwind metadata, or redistributable title rights. Historical local R52 source is not merged into production. The protected R50 executable and GAME assets were not written by these scripts. The upstream root CC0 / inherited GPL-3.0-or-later source-license mismatch still blocks publishing.

## Rollback and next gate

Delete disposable temp builds only after retaining their hashes and signed receipts. Revert the review-branch runner correction separately if required. Keep the R50 checkpoint, the source candidate, all release evidence, and GAME intact. Remaining acceptance requires a reviewed integration diff under R52 safety constraints, a controlled title session, rendering correctness and independent license provenance. **No merge, no gameplay or performance claim.**
