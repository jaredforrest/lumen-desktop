#!@NU@
def --wrapped main [...args] {
  # Relaunches can arrive without the graphical login environment.
  if ($env.XDG_SESSION_TYPE? | default "") != "x11" and ($env.WAYLAND_DISPLAY? | default "" | is-empty) {
    let manager = (do { ^@SYSTEMCTL@ --user show-environment } | complete)
    if $manager.exit_code == 0 {
      for line in ($manager.stdout | lines) {
        let parts = ($line | split row '=')
        let key = ($parts | first)
        if $key in [WAYLAND_DISPLAY SWAYSOCK DISPLAY XDG_RUNTIME_DIR XDG_SESSION_TYPE] and ($env | get -o $key | default "" | is-empty) {
          load-env {($key): ($parts | skip 1 | str join '=')}
        }
      }
    }
  }
  let display = ($env.WAYLAND_DISPLAY? | default "")
  let socket = if ($display | str starts-with '/') { $display } else { [($env.XDG_RUNTIME_DIR? | default "") $display] | path join }
  let explicit = ($args | any {|arg| $arg == '--ozone-platform' or ($arg | str starts-with '--ozone-platform=') })
  let cli = (($args.0? | default "") in [--cli --diagnose --help -h])
  let flags = if not $cli and not $explicit and $display != "" and (($socket | path type) == "socket") { [--ozone-platform=wayland] } else { [] }
  exec @WRAPPED@ ...$flags ...$args
}
