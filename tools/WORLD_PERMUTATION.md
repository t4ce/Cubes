# Shared Rubik world topology

Key 2 and Key 5 now share one persistent Rubik permutation. World identities
come from their authored theme sets, mapped onto cubie identities; roster index
is not a lattice position. Each portal follows its cubie's local face to the
neighbor at the current committed lattice position. Intermediate turn angles
do not alter adjacency. Leave exits stay on their authored room faces, and
Void remains connected only to the six pure worlds.

Entering a world immediately applies the current destination palette. A turn
that completes while the world is open starts a one-second deconstruction and
one-second reconstruction of affected portal pieces. Floors, ramps, and
unaffected portals retain their authored geometry. Portal colors use the same
one/two/three-theme territory mixture as the floor builder. A later commit
queues the latest destination until the active two-second cycle finishes.
Completely collapsed pieces are excluded before visibility/occlusion submission.

The active page is a copy of the cached asset with metadata for portal parts
9/10. Cached exports are never modified. This establishes dynamic routes and
their visual presentation; portal travel is not implemented by the existing
world demo.

R toggles a display-only 3×3×3 Rubik companion at the top right of Key 5.
It uses the same 27 cubies and 54 sticker seeds as Key 2, with the same current
permutation. It starts compact, opens for one second, and closes for one second
on a 7.5-second cycle (5.5 seconds compact). It creates no turns or input target;
first-person mouse look continues normally. Its 81 seeds are reserved from the
existing 2048-seed budget. It is camera-relative geometry in the foreground
scene, sharing that scene's depth test and lighting.

A Key-2 action already underway continues when switching modes, allowing its
commits to animate portals while in a world. Returning to Key 2 preserves the
permutation and allows another selection after the previous action finishes.
Picking transforms the ray into each cubie's actual orientation, so a scrambled
cube remains selectable.

Validation: `cargo check` in Cubes and TRUEOS, plus
`python3 tools/test_world_permutation.py` in Cubes. The host suite exercises
actual assets, unchanged floors, adjacency reciprocity, fixed exits/Void,
selection after permutation, collapse/rebuild timing and queued changes,
R-key edge handling, and companion bounds through 4K/portrait/wide windows.

For the rig: select a Key-2 corner/edge and switch to Key 5 while its action is
running. Affected portals should complete their collapse/rebuild cycles. Press
R, keep looking/walking normally, and watch the companion open/close without
starting another permutation. Return to Key 2 and select the scrambled cube.
Packing and deployment remain with the user; the latest companion sizing
adjustment was made while the user's second redeploy was in flight.
