# Native circle background

Cubes owns the six-ring adaptation of PotatoStamps' native point demo. The
existing background worker displays it continuously, with no timed swap or
additional VM. Inner to outer, colors are red/orange/yellow/green/blue/violet,
read by `build.rs` from `Cube/subcubes-materials.json` using stable material IDs.
Radii are 0.38/0.48/0.58/0.70/0.82/0.94: two inside the old range, one outside.
Dot widths are 12/12/12/16/20/24 pixels, with the old red ring's 12 pixels as
the minimum. Counts are 32/40/48/48/48/48 (264 total), preserving visible gaps
at the default window size. Key2 retains its current Palette Grid compute shader. The background
uses the existing XYZ immediate-color `IndexedDrawBatchV2` POINT_LIST renderer.

Every successful resize increments a producer generation, even for A -> B -> A.
That forces a background publication immediately even though the circle
scene is otherwise idle. The producer still publishes in world and settings
modes to complete UI4's two-layer resize barrier. Transient begin/import/submit contention is retried with the appropriate
lease ownership; tests run the actual worker against a lease/resize mock.

The failed Key6 world-point experiment has been removed. Key6 is unassigned;
its CPU projection, world-point mailbox and marker extraction are gone. Native
point buffers now hold only the 264 circle points. Key2 keeps its shader, and
all background modes retain 50% opacity. This is independent of Key9's
foreground retained-seed and full-cube limits.

Validate with `python3 tools/test_pointlist.py`: the actual worker runs against
a lease/resize mock, including idle and A -> B -> A resize publication.
