# X11 late surface association

An X11 window can commit its Wayland buffer before the compositor receives the
corresponding X11 surface serial. The pinned provider installed its commit
listener during association, so that ordering missed the already completed
commit. The X server reported a mapped window while the compositor never
published its managed window.

The retained failing original relationship run stopped at the final same-process
modal remap, not initial X11 startup. A separate upstream debug build reproduced
the failure: the same XID was dissociated, then associated with a buffer present
and `mapped=false`; no later commit or map event arrived before the bounded
fixture failed. This establishes the missing replay rather than a slow startup.

The shared upstream patch replays the existing buffer-based mapping decision
after association listeners have installed their map handlers. The mapping API
already ignores an already mapped surface. A temporary dissociation listener
prevents replay if an association callback detaches or destroys either owner;
the code does not dereference the X11 owner after the callback. Ordinary commit
processing is unchanged. Diagnostic provider logging is excluded from the
release recipe.

The installed original relationship fixture retains all thirteen existing
checks and adds a fourteenth group of sixteen rapid role-change/remap cycles.
Each cycle observes the unmanaged role, then sends acknowledged unmap, managed
role, parent, modal-property and map requests without waiting for native unmap
completion. It requires the exact mapped XID, validated native parent and
physical key delivery to the modal alone. Command receipts and the exact client
and tree on mapping timeout are retained for diagnosis.

Production and fixture content integrated in `8c57b161` passed the following
sequential, isolated native matrix, with every child reaped between cases:

| Journey | Basic | Effects |
| --- | ---: | ---: |
| Original relationship checks plus sixteen rapid cycles | 14 | 14 |
| Captured managed X11 transfers | 9 | 9 |
| Original relationship fixture without X-Resource | 2 | 2 |
| Native projection compatibility | 6 | 6 |

These are 62 check groups and 32 rapid cycles, rather than 94 independent
checks. The rapid corpus exercises real permitted scheduling but does not claim
to enumerate every X11/Wayland ordering. The causal debug run separately records
the particular late-association ordering. The local evidence manifest pins the
provider, fixture, raw receipts and failed baseline; CI continues to run the
same original relationship entry point, including the rapid corpus.
