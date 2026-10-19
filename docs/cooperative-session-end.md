# Cooperative session termination

Log Out, Restart and Shut Down use an observed-application preflight before the
existing host command. Sleep is unchanged and remains consumer-owned policy.

The application observer captures its current compositor epoch, exact native
window IDs and pidfd-bound process identities, including retained zero-window
applications it previously observed. Every app must advertise an enabled,
parameterless `org.gtk.Actions` Quit action owned by that exact process. All
endpoints are checked before the first Quit is sent. Unsupported, Xwayland,
shared or ambiguous ownership blocks the request with an explanation. Users can
quit those applications with their own controls and retry; there is no close,
signal, force-quit or inferred dirty-state fallback.

Quit requests are sequential. The app owns Save, Discard and Cancel prompts and
all document writes. An app remaining alive means waiting, never successful
termination. Cancel closes the shell preflight immediately and clears its queue
and one-use confirmation identity. An already delivered app Quit cannot be
recalled; users should Cancel its own prompt too if they want to keep it open.
Each capability check has a bounded D-Bus timeout; cancellation is processed
between checks. There is no waiting timeout that proceeds to shutdown.

Native process replacement, another primary window, protocol owner replacement,
compositor replacement or observation failure stops the preflight. Actual
transient/modal prompts belonging to a captured process are allowed. Only the
observer's actual parent shell process (PID, executable and start identity) is
excluded; another client advertising the shell app ID cannot exclude itself.

After every captured app has actually exited, the dialog asks for a second
explicit confirmation. Authorization rechecks the current native tree and
observations, consumes a one-use token and goes to the same visible dialog.
Closing/cancelling the dialog invalidates pending replies. No timer or backend
can launch a delayed host action. The existing normal host command still owns
permissions and inhibitors; no force/inhibitor-bypass flags are added.

This is a supported cooperative protocol, not a universal dirty-document API.
Apps never observed with native windows and application background services are
not discoverable as document applications here. Apps may implement their own
Quit incorrectly; the shell cannot authenticate or reconstruct their buffers.
Previously closed applications are not automatically reopened after cancellation.
