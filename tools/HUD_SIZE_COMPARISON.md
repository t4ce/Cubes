# Separate-frame HUD: pinned Tokio size comparison

Both measurements are release `.bp` files produced by TRUEOS-Blueprints with
`TRUEOS_BLUEPRINT_SKIP_APPS_PUBLISH=1`. The baseline reconstructs the immediately
preceding separate-frame HUD, serviced synchronously from the scene loop, with
the same assets. It does not compare against the older in-scene sprite HUD.

| Build | Packed bytes |
| --- | ---: |
| Before: separate frame, no Tokio | 329,949 |
| After: separate frame, Tokio task | 393,021 |
| Increase | 63,072 (61.59 KiB; 19.12%) |

The new build uses the platform's vendored Tokio **1.52.3**, current-thread
runtime, LocalSet, local HUD task, watch channel and timer. This is not a
measurement of the `rt-multi-thread` scheduler. The packer selects its
`tokio-platform-v6-native-workers-b3a4b6dad26501d7` build lane instead of
`thin-nostd`; the total difference includes that std/runtime transition and
task plumbing. Both use the packer's release settings.

Artifacts and complete build logs are retained in
`target/tokio-size-comparison/{before,after}.bp` and
`target/tokio-size-comparison/{before,after}-build.log`.

SHA-256:

- Before: `422aeb774288889055724dfd03deea338072253b3949a59fec99aca5adf1a408`
- After: `3aa7385909853f6644f862a9f97db730b087c1d85ee95a77d4064b0b3c51809e`

The after artifact was restored to `TRUEOS-Blueprints/dist/cubes.bp` after
the baseline build. No app publishing, rig deployment, or reboot was performed.
