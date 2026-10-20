# Capture observations

The optional status indicator reports active PipeWire graph links into audio/video input streams. It does not establish permission, authenticated application identity, global inactivity, or camera versus screen origin. Client names are reported metadata. Missing, invalid, oversized, expired or disconnected observations remove the positive indicator.

The read-only collector has a two-second CLOCK_BOOTTIME budget, including blocking reads, process completion and JSON parsing, with an 8 MiB reply, 16,384 graph objects and 64 displayed rows. The UI expires observations after 7.5 seconds and rejects backwards wall-clock movement. Disabling the collector clears its state and invalidates canceled process generations; late stdout cannot republish retired observations.

The native acceptance fixture uses the installed full shell, physical private pointer/keyboard input, a separate return-focus window and a generated-audio-only PipeWire server in disposable filesystem/network/process namespaces. It tests both report entry points, accessible text, actual viewport geometry, narrow output, unlink, natural stale expiry and provider loss. It never opens host capture devices or the host graph.

Only the effects variant explicitly requests the test boundary's default-off `software_graphics` capability. This binds the existing accessible non-symlink `/dev/udmabuf` character device after checking kernel sysfs identity and checks the bound device again. It does not provision devices or expose `/dev/dri`, `/dev/snd` or video devices. Other application fixtures retain their original device-free boundary. Genuine software GLES effects remain required; this is not a basic-renderer fallback.
