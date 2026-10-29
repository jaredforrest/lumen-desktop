#!/usr/bin/env nu
def count-ink [frame bounds color] {
    let rgb = ($color | parse -r '(?<rgb>[0-9a-fA-F]{6})$' | first | get rgb)
    let target = [
        ($rgb | str substring 0..<2 | into int --radix 16)
        ($rgb | str substring 2..<4 | into int --radix 16)
        ($rgb | str substring 4..<6 | into int --radix 16)
    ]
    let x = ($bounds.x | math round | into int)
    let y = ($bounds.y | math round | into int)
    let width = ($bounds.width | math round | into int)
    let height = ($bounds.height | math round | into int)
    if $width <= 0 or $height <= 0 { error make {msg: 'Empty launcher text crop'} }
    let histogram = (^magick $frame -crop $'($width)x($height)+($x)+($y)' +repage -format '%c' histogram:info: | lines)
    mut count = 0
    for line in $histogram {
        let parsed = ($line | parse -r '^\s*(?<count>\d+): \((?<r>\d+),(?<g>\d+),(?<b>\d+)')
        if ($parsed | is-empty) { continue }
        let pixel = ($parsed | first)
        let actual = [($pixel.r | into int) ($pixel.g | into int) ($pixel.b | into int)]
        if (($actual.0 - $target.0) | math abs) <= 32 and (($actual.1 - $target.1) | math abs) <= 32 and (($actual.2 - $target.2) | math abs) <= 32 {
            $count += ($pixel.count | into int)
        }
    }
    if $count < 4 or $count >= ($width * $height * 0.6) {
        error make {msg: $'Launcher text ink count invalid: ($count) / ($width * $height), color=($color), frame=($frame)'}
    }
}
let repository = ($env.FILE_PWD | path join .. .. | path expand)
let font_config = (^nix build ($repository + '#test-owned-font-config') --no-link --print-out-paths | lines | last)
let sandbox = (^mktemp -d -t lumen-launcher-text.XXXXXX | str trim)
cp -r ($repository | path join res lumen-desktop) ($sandbox | path join assets)
for directory in [home runtime state config cache data] {
    let path = ($sandbox | path join $directory)
    mkdir $path
    ^chmod 700 $path
}
let palette = (open ($repository | path join res lumen-desktop appearance palette.json))
{search: {historyEnabled: false}, clipboard: {enabled: false}} | to json | save ($sandbox | path join settings.json)
mut identities = []
for mode in [light dark] {
    for scale in [1.0 1.15 1.3 1.5] {
        let output = ($sandbox | path join $'($mode)-($scale)')
        mkdir $output
        let cases = ([false true] | each {|small| [list grid preview] | each {|layout| {mode: $mode, scale: $scale, small: $small, layout: $layout} }} | flatten)
        (open --raw ($repository | path join tests desktop typography_fixture launcher.qml) | str replace 'CASES' ($cases | to json --raw) | str replace 'PALETTES' ($palette | to json --raw) | str replace 'OUTPUT' $output) | save --force ($sandbox | path join fixture.qml)
        {font: $palette.font, palette: ($palette | get $mode), preferences: {reduceMotion: true}} | to json | save --force ($sandbox | path join appearance.json)
        let run = (with-env {
            HOME: ($sandbox | path join home)
            XDG_RUNTIME_DIR: ($sandbox | path join runtime)
            XDG_STATE_HOME: ($sandbox | path join state)
            XDG_CONFIG_HOME: ($sandbox | path join config)
            XDG_CACHE_HOME: ($sandbox | path join cache)
            XDG_DATA_HOME: ($sandbox | path join data)
            QT_QPA_PLATFORM: offscreen
            QT_QPA_PLATFORMTHEME: generic
            QT_QUICK_BACKEND: software
            QT_ACCESSIBILITY: '0'
            NO_AT_BRIDGE: '1'
            GTK_A11Y: none
            LUMEN_APPEARANCE_STATE: ($sandbox | path join appearance.json)
            LUMEN_NIX_SETTINGS_ACTIVE: ($sandbox | path join settings.json)
            FONTCONFIG_FILE: $font_config
        } { ^timeout 25s quickshell -p ($sandbox | path join fixture.qml) | complete })
        let log = ($run.stdout + $run.stderr)
        if $run.exit_code != 0 or ($log | str contains LAUNCHER_TEXT_FAILURE) or ($log | str contains ReferenceError) or ($log | str contains TypeError) or ($log | str contains 'binding loop') {
            error make {msg: $'Launcher typography QML failed ($mode) ($scale): ($log)'}
        }
        let receipt = ($log | lines | where {|line| $line | str contains 'LAUNCHER_TEXT_RESULT '})
        if ($receipt | length) != 1 { error make {msg: $'Launcher result missing: ($log)'} }
        let rows = ($receipt | first | split row 'LAUNCHER_TEXT_RESULT ' | last | from json)
        if ($rows | length) != 18 { error make {msg: $'Launcher segment expected 18 frames, got ($rows | length)'} }
        for index in 0..17 {
            let row = ($rows | get $index)
            let entry = $row.entry
            if $entry.mode != $mode or $entry.scale != $scale or $row.resolvedFamily != $palette.font or $row.pixelSize <= 0 or ($row.roleMetrics | is-empty) {
                error make {msg: $'Launcher text metrics wrong: ($entry) ($row.phase)'}
            }
            let identity = $'($entry.mode):($entry.scale):($entry.small):($entry.layout):($row.phase)'
            if $identity in $identities { error make {msg: $'Duplicate launcher corpus case: ($identity)'} }
            $identities = ($identities | append $identity)
            let frame = ($output | path join $'(($index / 3 | math floor | into int))-(($index mod 3)).png')
            if not ($frame | path exists) { error make {msg: $'Missing launcher painted frame: ($frame)'} }
            let dimensions = (^magick identify -format '%w %h' $frame | str trim)
            let wanted = (if $entry.small { '480 360' } else { '960 720' })
            if $dimensions != $wanted { error make {msg: $'Launcher frame size ($dimensions) != ($wanted)'} }
            count-ink $frame $row.textBounds $row.color
        }
    }
}
if ($identities | length) != 144 { error make {msg: 'Launcher corpus omitted cases'} }
rm --recursive --force $sandbox
print 'PASS 144 production launcher typography frames across eight bounded segments'
