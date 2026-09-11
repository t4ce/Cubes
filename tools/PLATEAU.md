# Key 4: custom plateau

Key 4 connects to cubesrv as `t4ce`. A missing profile opens the six-theme
choice; the first choice creates the profile. Later visits load the saved
terrain and placements and put the walking camera at the center surface.

The volume is one existing world chunk: 512 c1 per side, instead of 4 chunks
per axis. Its only generated terrain is the editor's minimum 16-c4-wide
platform, with two terraced c4 layers and clipped corners (428 cubes).
The top is at authored Y = -88 c1, approximately one third above the volume's
bottom. Theme affects the terrace color. There are no portals.

Walking, flight, target outlines and asset placement use the Key 5 code.
Middle-click opens the existing asset picker when the tool is off. Confirming
an asset returns to the plateau and the same camera; a subsequent left-click
places it. Middle-click with an active tool disables it. Placements save
automatically; saves and database reads run off the rendering thread.

Key 0, while on the plateau or its picker, deletes the current user's profile
record. It waits for an outstanding request to finish, then returns to the
theme choice. Other users remain intact. The client sends the username in
both its slideshow hello and profile requests; change `cubes_protocol::USERNAME`
to select a different development profile.

If a request fails, Key 4 retries. “Reload Saved” explicitly discards unsaved
local placements and reads the server record. Revision conflicts never
silently overwrite another client's save.

Profiles live in one server-owned redb database:
`apps/common/cubesrv/cubeusers.db` (`common/cubesrv/cubeusers.db` through the
Blueprint filesystem API). The database stores the existing `.cubes` terrain
bytes, selected theme and placed cube poses/materials under each username.
The shared TRUEOS redb image backend closes before its native async file
write and reopens afterward. A successful RAM transaction alone is not a save
acknowledgment; the file write must complete. This uses the existing image
backend's persistence guarantees, not a new disk-backed redb backend.

Profile routes on cubesrv's HTTP port 18:

- `GET /plateau/{username}`: load, or 404 for a missing profile.
- `POST /plateau/{username}` with `{ "theme": 1 }`: create once, or return existing.
- `PUT /plateau/{username}`: append placements using generation and revision checks.
- `DELETE /plateau/{username}`: delete only that profile.

Usernames are development identities supplied by clients, with 1–32 ASCII
letters, digits, `_` or `-`. Terrain/theme cannot be changed through save.
Both Cubes and cubesrv must be rebuilt for the username-bearing UDP hello.

Validation: `python3 tools/test_plateau.py`, `python3 tools/test_asset_picker.py`,
`python3 tools/test_cube_interface.py`, `python3 tools/test_slideshow_network.py`,
and the native Blueprint packer for both applications. The plateau test uses
real redb and HTTP with a host filesystem adapter, including reopen, failed
persistence, deletion, conflicts, editor geometry and center-spawn checks.
