# R71 — Native GPU engine identity and utilization

Date: 2026-11-28. Scope: Windows x64 PDH telemetry in `crates/victory-gpu/src/stats.rs`. **SCOPED PASS. Production integration BLOCKED.**

## Source, baseline and defect

Base R70 SHA: `85aeb9a1bda90f91311d8e4041502704a6dcfb10`. Native green implementation SHA: `d3175e982a0232c14d3cfe1a1e5a60f230085081`. R50 protected `D:\Projects\NEPTUNE\R50\NEPTUNE.exe` SHA-256 pinned before and after every local run: `fc3c8f9be91bc81140c60ed1023698fb21843947997dbd1e0471b86e3b119f0a`.

R70 used `HashMap<kind, sum_of_all_process_engine_samples>`. Two distinct `Copy` engines, or GPU adapters sharing an engine type, alias and falsely add their utilization. It also accepted partial/malformed PDH instance names. This distorts performance telemetry, not the GPU's native workload.

R71 parses the real Windows instance layout `pid_<pid>_luid_<high>_<low>_phys_<node>_eng_<index>_engtype_<type>`. It keys the physical engine by (LUID high, LUID low, physical node, engine index), sums samples from processes **only within that engine**, clamps each engine to 100%, then takes the busiest engine per type and the maximum of those engines. Invalid negative/non-finite samples are skipped; malformed instance names are ignored. No dependencies added; no GPU execution/command submission changed. Across adapters the aggregate represents the busiest engine on any adapter, **not** utilization of a specifically selected host/DXGI adapter.

Reference: Microsoft DirectX Developer Blog, [GPUs in the Task Manager](https://devblogs.microsoft.com/directx/gpus-in-the-task-manager/), describes the busiest engine representative rather than summing unlike engines. Native Windows PDH format is corroborated independently with `Get-Counter` and with the runtime's own `PdhGetFormattedCounterArrayW`.

## Causal RED and negative controls

- RED tests-only commit: `99e1804f2b747dcd9fc9b65e4d4ed67374c76745`.
- `stats::tests::distinct_gpu_engines_keep_distinct_instance_identity`: expected **EXIT 101**, observed **101**. Two different engine indices were incorrectly identical under the R70 parser.
- `stats::tests::distinct_gpu_adapters_keep_distinct_instance_identity`: expected **101**, observed **101**. Two adapter LUIDs were incorrectly identical.
- `stats::tests::malformed_gpu_instance_is_not_counted`: expected **101**, observed **101**. Partial names were accepted.
- Existing valid instance-format control: expected **EXIT 0**, observed **0**.
- Native Windows execution receipt: `58469cc51f2852f61a38ae8b`, independently verified Ed25519 **VALID**.

## GREEN and hardware witnesses

Final native rerun after the hardware fixture commit:

| Profile | Command scope | Observed |
| --- | --- | --- |
| hardware | `cargo test --locked --offline -p victory-gpu --lib stats::tests::physical_gpu_engine_pdh_snapshot -- --ignored --exact --nocapture --test-threads=1` | 1 PASS; EXIT 0; actual Windows PDH collector |
| targeted | `cargo test --locked --offline -p victory-gpu --lib stats::tests:: -- --nocapture --test-threads=1` | 10 PASS; 0 FAIL; 1 IGNORED |
| Debug | `cargo test --locked --offline -p victory-gpu --lib -- --nocapture --test-threads=1` | 259 PASS; 0 FAIL; 30 IGNORED |
| Release | `cargo test --locked --offline --release -p victory-gpu --lib -- --nocapture --test-threads=1` | 259 PASS; 0 FAIL; 30 IGNORED |
| strict lint | `cargo clippy --locked --offline -p victory-gpu --all-targets --all-features -- -D warnings` | EXIT 0 |

Physical snapshot from the **NEPTUNE native Rust PDH collector** (not mocked): 319 usable process/engine instances, 2 distinct adapter LUIDs, 27 physical engines, 9 engine types, busiest engine 7.79395% **at that instant**. Separate PowerShell `Get-Counter` inspection also observed 319 instances and 2 distinct LUIDs; it is a separate snapshot, not byte-for-byte equality.

Native final runner: `D:\Projects\NEPTUNE\R71-PDH-FINAL-20261128.ps1`. Native command receipt `ae916a1b1f3633a2ea40b844`, independently verified Ed25519 **VALID**. Final stdout hashes:

```text
physical AE2D31964A456F6DE643050AF7D717468FC690B176DDF35F870AA919BA53BBBE
targeted 49C37CC5708F1E92C91656E9CD2A54D2DDBEC31C27456D240D59753CBF85E805
debug    4F05DF3039609B02BEAF1F96EE1BBEC52363E614829E2AAFA2801D92066C52D5
release  F742F59A01ADB2FA4D12D103373EE901B37E7475028BF013F0FF9E1A548B6653
```

After all steps: detached HEAD `d3175e982a0232c14d3cfe1a1e5a60f230085081`; working tree clean; protected R50 hash unchanged. `GAME` and R52 production were not modified, and no process was force-closed.

## Limits / rollback / next gate

**Not tested:** active AAA gameplay, title startup, render fidelity, DXGI adapter-to-PDH LUID selection, sustained device-loss recovery, frame-time improvement, alternate GPU vendors and driver versions, or release packaging. Native PDH verification is a telemetry/hardware-observation witness, not proof of GPU rendering. The 7.79395% sample is not a gameplay benchmark.

R52 safety and license provenance gates still block integration/release. Keep this branch as a Draft PR stacked on R70; do not merge into production. Rollback: revert the R71 source commits or leave the Draft PR unmerged; R50 is untouched. Next verification: select the active host DXGI adapter LUID when a per-card metric is needed, then add a captured same-sample differential oracle and sustained frame-time/gameplay instrumentation.
