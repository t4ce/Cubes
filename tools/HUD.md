# Movable microfont counter

The counter owns a separate dirty/double-buffered UI4 window, initially at the
cube window's lower-right corner. UI4 manages its movement and composition;
the app does not pin it back to that corner after a drag. It is an ordinary
window, not a cursor overlay or a reserved hardware display plane.

Microfont rasterizes the tiny sprite on the CPU. Sprite composition and
`publish_compute` use only the counter's own frame and completion receipt.
The cube frame publishes first and never receives HUD writes. Counter updates
are capped at 10 Hz and skipped when the displayed text is unchanged. A busy
publish is retried without redrawing or acquiring another lease. Resizing
invalidates the cached text. Closing/failing the counter disables it without
stopping the cube renderer; its owned Frame is dropped with the demo.

The panel is serviced cooperatively from the existing demo loop. It does not
introduce Tokio or an independent execution task, so it cannot update during
a blocked scene render. This keeps the change scoped to window ownership.
