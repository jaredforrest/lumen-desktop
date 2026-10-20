# Captured folder stack actions

A stack row activation captures its row object, popup generation, local path and six lossless source facts. Replacement, mutation, reload and a different popup invocation invalidate that action. Sorting and presentation persist against the captured folder invocation. Symbolic-link and special-file actions are explicitly unsupported.

Regular-file Open uses the fixed default handler after source validation. Once dispatched, an external handler may reopen the mutable path later; this is not an immutable external handoff or a cancellable operation. An uncertain helper outcome is reported without retry.

Directory activation carries its captured facts into the first owned Files listing. The provider opens metadata without following links, validates a regular directory, scans its retained directory descriptor and uses descriptor-relative metadata paths. It validates that descriptor and the original pathname again before returning any rows. Failure leaves existing Files navigation and history intact. This does not freeze subsequent directory mutations or ordinary later navigation.

Stack listing is limited to 5000 rows. The folder action remains available with an explicit limit message; opening Files does not promise an unlimited listing. A failed or terminated listing exposes an error and permits an explicit reload.

The isolated native fixture uses real stack controls, the production directory provider and a private default-handler observer. Its narrow listing-input gate only holds the initial stdin request to reproduce child death before any reply. No host clipboard, default handler, files, devices or accounts are used.
