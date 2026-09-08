# Movable microfont counter

The counter owns a separate dirty/double-buffered UI4 window, initially at the
cube window's lower-right corner. UI4 manages its movement and composition;
the app does not pin it back to that corner after a drag. It is an ordinary
window, not a cursor overlay or a reserved hardware display plane.

Microfont rasterizes the tiny sprite on the CPU. Sprite composition and
`publish_compute` use only the counter's own frame and completion receipt.
The cube frame publishes and sends counts through a latest-value watch channel;
it never receives HUD writes or waits for the HUD task. Counter updates
are capped at 10 Hz and skipped when the displayed text is unchanged. A busy
publish is retried without redrawing or acquiring another lease. Resizing
invalidates the cached text. Closing/failing the counter disables it without
stopping the cube renderer; its owned Frame is dropped with the demo.

The panel is owned by a Tokio local task on the platform current-thread runtime,
using pinned Tokio 1.52.3 through `trueos`'s `tokio-runtime` feature. Its 100 ms
timer skips missed ticks. Count updates coalesce without an unbounded queue;
the task continues polling resize and pending publication when counts are idle.
The scene uses asynchronous sleep between renders, allowing the HUD task to run.
This is cooperative task scheduling, not a multithread worker pool: synchronous
driver calls can still delay both tasks until they return.

On scene failure, the sender closes and the HUD task is joined before exit.
The task drops its own Frame. Dropping the worker unexpectedly aborts its task;
the owning LocalSet/runtime provides cancellation cleanup. HUD errors remain
local to the task and do not terminate the cube scene. The std-enabled runtime
owns panic handling, replacing the app's no-std default-panic-handler feature.
