#!/usr/bin/env nu
let repository = ($env.FILE_PWD | path join .. .. | path expand)
let sandbox = (^mktemp -d -t lumen-display-wire.XXXXXX | str trim)
let assets = ($sandbox | path join assets)
cp -r ($repository | path join res lumen-desktop) $assets
let source = ($assets | path join shell DisplaySpacesState.qml)
let original = (open --raw $source)
let command_lines = ($original | lines | where {|line| $line | str trim | str starts-with 'command:'})
if ($command_lines | length) != 1 {
    error make {msg: 'Expected one production observer command'}
}
let modified = ($original | str replace ($command_lines | first) '        command:["cat"]' | str replace 'running:true' 'running:true;stdinEnabled:true')
if $modified == $original or ($modified | str contains 'stdinEnabled:true') == false {
    error make {msg: 'Disposable observer substitution failed'}
}
$modified | save --force $source
cp ($repository | path join tests desktop display_spaces_wire.qml) ($sandbox | path join shell.qml)
for directory in [home runtime state config cache data] {
    let path = ($sandbox | path join $directory)
    mkdir $path
    ^chmod 700 $path
}
let run = (with-env {
    HOME: ($sandbox | path join home)
    XDG_RUNTIME_DIR: ($sandbox | path join runtime)
    XDG_STATE_HOME: ($sandbox | path join state)
    XDG_CONFIG_HOME: ($sandbox | path join config)
    XDG_CACHE_HOME: ($sandbox | path join cache)
    XDG_DATA_HOME: ($sandbox | path join data)
    QT_QPA_PLATFORM: offscreen
    QT_QUICK_BACKEND: software
    QT_ACCESSIBILITY: '0'
    NO_AT_BRIDGE: '1'
    GTK_A11Y: none
    QT_QPA_PLATFORMTHEME: generic
} { ^timeout 10s quickshell -p ($sandbox | path join shell.qml) | complete })
let log = ($run.stdout + $run.stderr)
if $run.exit_code != 0 or not ($log | str contains DISPLAY_WIRE_PASS) or ($log | str contains DISPLAY_WIRE_FAIL) {
    error make {msg: $'Display observer wire test failed: ($log)'}
}
rm --recursive --force $sandbox
print 'PASS actual Qt observer frames: stale generation, fresh session, malformed data and routing fallback'
