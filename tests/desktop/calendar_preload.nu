#!/usr/bin/env nu
def main [quickshell: path] {
    let repository = ($env.FILE_PWD | path join .. .. | path expand)
    let sandbox = (^mktemp -d -t lumen-calendar-preload.XXXXXX | str trim)
    for directory in [bin runtime state home] { mkdir ($sandbox | path join $directory) }
    ^chmod 700 ($sandbox | path join runtime)
    let desktop = ('file:' + ($repository | path join res lumen-desktop shell))
    open --raw ($env.FILE_PWD | path join calendar_preload.qml) | str replace '__DESKTOP__' $desktop | save ($sandbox | path join check.qml)
    let shebang = ('#!' + $nu.current-exe + "\n")
    ($shebang + 'def main [] { print ({events: [], monthEvents: [], configured: true, lastSuccess: "2026-11-12T00:00:00Z", providers: []} | to json --raw) }') | save ($sandbox | path join bin lumen-calendar)
    ($shebang + 'def --wrapped main [...args] { "notification" | save --append $env.CALENDAR_NOTIFICATION_LOG }') | save ($sandbox | path join bin gdbus)
    ^chmod +x ($sandbox | path join bin lumen-calendar) ($sandbox | path join bin gdbus)
    let result = (with-env {
        QT_QPA_PLATFORM: offscreen, QSG_RHI_BACKEND: software,
        HOME: ($sandbox | path join home), XDG_STATE_HOME: ($sandbox | path join state),
        XDG_RUNTIME_DIR: ($sandbox | path join runtime),
        CALENDAR_NOTIFICATION_LOG: ($sandbox | path join notifications),
        PATH: ([($sandbox | path join bin)] | append $env.PATH)
    } { ^$quickshell --path ($sandbox | path join check.qml) | complete })
    if $result.exit_code != 0 or not (($result.stderr + $result.stdout) | str contains CALENDAR_PRELOAD_PASS) { error make {msg: ($result.stderr + $result.stdout)} }
    if (open --raw ($sandbox | path join notifications)) != 'notification' { error make {msg: 'Calendar failure notification was missing or duplicated'} }
    print 'PASS: hidden startup preload, immediate opening, one notification per repeated failure'
}
