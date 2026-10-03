# THREADS8 progress (wave 8) - widget threads that never stop

Branch: wt/threads8 (base 45c6bf98b)

## DONE
- (none yet)

## IN PROGRESS
- Reading: how a widget owns a Thread; what happens on unmount (DOM diff / remap_node_ids); run_all_threads reaping.

## NEXT
- Find the thread map (layout), the dll's reaping, the video/camera/screencap workers.
- RED test: a DOM that drops a widget owning a thread -> the thread is told to stop and gone after the next frame.

## Decisions / open questions
