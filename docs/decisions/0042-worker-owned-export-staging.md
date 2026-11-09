# 0042 — Worker-owned export staging

Date2026-11-09. Status candidate architecture under implementation and verification. Root selects the bounded candidate after the source review. Milestone acceptance remains pending.

## Context

`Prepared` currently transfers a TempDir through the bounded result channel. Owner polling, stale/cancelled completion, failed publication and buffered-result destruction can recursively delete staging on the editor thread. Root's focused regression observes the actual destructor on the owner thread after the producer has exited. It fails its required worker-affinity assertion. Foreground hitch duration is unmeasured, so no speedup is established.

## Candidate decision

Keep staging exclusively in the existing export worker. Send metadata-only preparation to the owner. Preserve owner-side identity, cancellation, DirectoryGrant revalidation and atomic no-clobber publication. Latch the publication outcome and receipt before releasing staging through one bounded acknowledgment. Receiver/controller destruction disconnects that acknowledgment and releases the worker for cleanup.

Retain the job through worker cleanup and completion. Normal close waits through the existing finished contract. A new export/configuration change remains blocked while that owner is occupied. Nonblocking completion observation replaces any UI-thread join.

Once publication commits, later cancellation, replacement or cleanup failure preserves committed status and the receipt. Expose cleanup progress and secondary warnings through existing operation inspection and ordinary export UI. Remove cancellation authority after commit. Preparation errors continue to dispose on the worker.

## Alternatives and consequences

Returning TempDir to a worker after owner publication leaves additional buffered-receiver/drop transfer cases. Worker-local ownership avoids those recursive filesystem owners in UI-held messages. A general cleanup service, second worker pool or detached unbounded tasks would add unnecessary lifetime and capacity contracts.

The worker remains occupied longer, preserving boundedness at the cost of delaying the next export until cleanup finishes. Abrupt process termination cannot guarantee cleanup. Successful publication with cleanup failure remains successful with a warning.

## Required evidence

Exercise success, stale/cancelled completion, buffered-result/controller drop, worker disconnect and deterministic cleanup failure. Observe actual destructor thread identity. Preserve publication authority, pinned resources, colliding destinations, truthful receipt retention and helper descendant reaping. Extend existing owner tests economically and run changed ordinary/native MCP export paths before acceptance. Measure actual completion/cleanup spans before claiming a foreground timing benefit.
