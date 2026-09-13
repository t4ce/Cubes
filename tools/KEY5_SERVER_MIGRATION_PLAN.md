# Key5 server migration: restart plan

## Restored baseline

Key5 again loads the original 27 local `.cubes` files and baked platform metadata.
The showcase and exporter are back in Cubes. CubeSrv is restored to its previous
Key8 implementation, including snake and worm. The bottom-of-viewport flight
preview adjustment is retained. The HTTP world loader, separate indexed UDP
loader, runtime JSON world bundles, and diagnostic follow-ups are rolled back.

## What the failure establishes

Cubes and CubeSrv run in separate VMs. Key8 works in that arrangement. The latest
failed Key5 capture reached chunk transfer after receiving a manifest. It does
not establish which operation deadlocked or that transfer completed. Switching
TCP to UDP and changing the reply mailbox did not resolve the system stall.

A normal guest data or loading error must not wedge the BSP, network shell, or
UI AP1. That symptom is a host isolation/progress defect to investigate separately
from the migration. Shared host I/O, ownership of VM resources, lock ordering,
and native worker context are investigation targets, not established causes.
Linux host tests do not exercise this dual-VM boundary and cannot establish that
it is safe. Rollback removes the trigger, not the underlying kernel defect.

## Smaller implementation sequence

1. Trace Key8's existing world request, binary transfer, worker lifetime, and
   installation path. Define one shared loader with explicit demo/indexed-world
   selection. Preserve the demo's existing behavior.
2. First request the identical demo bytes through Key5 using that loader, with
   the existing local decoding and metadata path. No new runtime, transport,
   bundle encoding, or extra worker. Verify byte identity and mode transitions.
3. Add a world ID to that same request path and select the original binary
   `.cubes` bytes on CubeSrv. Keep selection separate from decoding/rendering.
   Move the files and showcase only after the data path is validated.
4. Specify the platform-metadata transport separately. Preserve its existing
   semantics; do not wrap binary cube bytes in a JSON integer array. Migrate
   metadata only after identical binary-world loading has been demonstrated.
5. Validate two actual VMs, both launch orders/VM IDs, Key5/Key8 switching,
   cancellation, server absence/restart, and repeated world changes. Check host
   UI and network responsiveness during the run, not merely decoded output.

Use an independently recoverable emulator/test environment for fault reproduction
and host investigation first. Do not launch another potentially wedging build on
the physical rig without a recovery route. There is no physical recovery access
available in the current situation. The replacement Key5 migration has not started. A separate minimal Key8
preview/empty toggle now exercises selection over its existing socket and worker;
see SLIDESHOW.md.
