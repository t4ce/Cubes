# Counter logging (no HUD or Tokio)

Cubes uses its synchronous render loop and 16 ms platform sleep again. The
microfont dependency, HUD task, and separate counter window have been removed.
`HUD_SIZE_COMPARISON.md` is a historical measurement of the removed experiment.

After each successful frame publish, an allocation-free sampler records the
latest mode and expanded/unexpanded counts in the current 250 ms bin. Once a
second, the normal Blueprint INFO logger emits all four bins in one line:

`Cubes: counts t=12000ms bins=250ms [0:+240ms M2 E27 U0 F15] ...`

`M` is mode, `E/U` are counts, and `F` is successfully published frames in that
bin. The offset identifies the actual latest sample, not an invented exact
timer tick. A bin with no completed frame is `[-]` (printed with its index).
After a stall, logging resumes without fabricated samples or catch-up bursts.
Logging is serviced by the render loop; a blocked render also delays its log.

Key 4 records the complete authored scene, including cubes removed before seed
upload. Its bins use separate visibility labels so frustum counts cannot be
confused with the frame count:

`[0:+240ms M4 S900 F900 O243 V657 P28908 X10692 frames15]`

- `S`: authored source cubes, including those never submitted this frame.
- `F`: cubes surviving the frustum test.
- `O`: cubes removed by conservative occlusion (`F − V`).
- `V`: visible cubes submitted for expansion.
- `P`: submitted cube patch references (`V × 44`).
- `X`: cube patch references avoided (`(S − V) × 44`).
- `frames`: successfully published frames in the bin.

The empty-view ABI placeholder is excluded from `S`, `V` and `P`. Thus an empty
900-cube view reports `S900 F0 O0 V0 P0 X39600`; it does not become a zero-cube
scene. `P/X` describe logical cube submissions, not GPU performance counters or
measured HS invocations. Surviving cubes still expand all 44 patches. Modes
1–3 retain their existing `E/U/F` output.

Host check: `rustc --edition=2024 --test src/counters.rs -o /tmp/cubes-counter-tests`
followed by `/tmp/cubes-counter-tests`.
