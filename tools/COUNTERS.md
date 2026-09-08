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
