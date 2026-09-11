# Native circle background

Cubes links PotatoStamps' library geometry. It does not launch PotatoStamps,
open another window or add another VM. The existing background worker displays
Key1's four 64-point circles continuously, with no timed swap. Positions,
colors and circle point widths come from PotatoStamps' scene
module. Key2 retains its current Palette Grid compute shader. The background
uses the existing XYZ immediate-color `IndexedDrawBatchV2` POINT_LIST renderer.

Every successful resize increments a producer generation, even for A -> B -> A.
That forces a background publication immediately even though the circle
scene is otherwise idle. The producer still publishes in world and settings
modes to complete UI4's two-layer resize barrier. Transient begin/import/submit contention is retried with the appropriate
lease ownership; tests run the actual worker against a lease/resize mock.

The failed Key6 world-point experiment has been removed. Key6 is unassigned;
its CPU projection, world-point mailbox and marker extraction are gone. Native
point buffers now hold only the 256 circle points. Key2 keeps its shader, and
all background modes retain 50% opacity. This is independent of Key9's
foreground retained-seed and full-cube limits.

Validate with `python3 tools/test_pointlist.py`: the actual worker runs against
a lease/resize mock, including idle and A -> B -> A resize publication.
