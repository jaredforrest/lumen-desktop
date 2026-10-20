# Capture observations

The menu indicator and Control Centre report show bounded observations of active
capture links visible to the current PipeWire connection. Audio and video remain
separate categories. Audio can include system-audio monitoring; video does not
assert whether its source is a camera or screen. Application labels are reported
client metadata, not authenticated application identity.

The report always states its limited coverage. An empty graph means no active
capture link was observed in that graph. It does not certify that capture is
idle: direct devices, hidden graph objects and other protocols remain unassessed.
This is a read-only observer; it neither grants nor revokes permissions, changes
routing, records media, nor activates a service. It does not inspect other
applications' private portal sessions or infer permission grants from a process.

Policy bounds: one snapshot every three seconds, a two-second collection budget,
8 MiB input, 16,384 graph objects and 64 capture rows. Failed or malformed reads
clear observations. The UI expires observations after 7.5 seconds or a backwards
clock change. Snapshot data stays in memory; synthetic regression artifacts are
kept separately. These are engineering policies, not hardware timing guarantees.

The graph interpretation follows [PipeWire stream classes](https://docs.pipewire.org/page_man_pipewire-props_7.html)
and the pinned [pw-dump 1.6.9 serialization](https://github.com/PipeWire/pipewire/blob/1.6.9/src/tools/pw-dump.c).
Tests distinguish parser/transport and actual private PipeWire protocol checks
from UI and hardware coverage. The private graph uses generated audio and no
hardware, host graph, user profile, network route or device permissions.
