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
    if $width <= 0 or $height <= 0 { error make {msg: 'Empty utility text crop'} }
    let histogram = (^magick $frame -crop $'($width)x($height)+($x)+($y)' +repage -format '%c' histogram:info: | lines)
    mut pixels = 0
    for line in $histogram {
        let parsed = ($line | parse -r '^\s*(?<count>\d+): \((?<r>\d+),(?<g>\d+),(?<b>\d+)')
        if ($parsed | is-empty) { continue }
        let pixel = ($parsed | first)
        let actual = [($pixel.r | into int) ($pixel.g | into int) ($pixel.b | into int)]
        if (($actual.0 - $target.0) | math abs) <= 2 and (($actual.1 - $target.1) | math abs) <= 2 and (($actual.2 - $target.2) | math abs) <= 2 {
            $pixels += ($pixel.count | into int)
        }
    }
    if $pixels < 4 { error make {msg: $'Utility text ink missing: ($frame), pixels=($pixels)'} }
}

let repository = ($env.FILE_PWD | path join .. .. | path expand)
let font_config = (^nix build ($repository + '#test-owned-font-config') --no-link --print-out-paths | lines | last)
let sandbox = (^mktemp -d -t lumen-utility-text.XXXXXX | str trim)
cp -r ($repository | path join res lumen-desktop shell) ($sandbox | path join desktop)
for directory in [home runtime state config cache data] {
    let path = ($sandbox | path join $directory)
    mkdir $path
    ^chmod 700 $path
}
let palette = (open ($repository | path join res lumen-desktop appearance palette.json))
{font: $palette.font, palette: $palette.light, preferences: {reduceMotion: true}} | to json | save ($sandbox | path join appearance.json)
mut count = 0
for consumer in [TextTools Remote Info] {
    let views = (if $consumer == TextTools { [Dictionary Dictation Shortcuts] } else if $consumer == Remote { [Connections] } else { [Information] })
    for mode in [light dark] {
        let output = ($sandbox | path join $'($consumer)-($mode)')
        mkdir $output
        mut cases = []
        for scale in [1.0 1.15 1.3 1.5] {
            for small in [false true] {
                for view in $views {
                    $cases = ($cases | append {consumer: $consumer, mode: $mode, scale: $scale, small: $small, view: $view})
                }
            }
        }
        (open --raw ($repository | path join tests desktop owned_utility_typography_qml.qml)
            | str replace '__CASES__' ($cases | to json --raw)
            | str replace '__PALETTE__' ($palette | to json --raw)
            | str replace '__OUTPUT__' $output) | save --force ($sandbox | path join fixture.qml)
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
            FONTCONFIG_FILE: $font_config
            LUMEN_APPEARANCE_STATE: ($sandbox | path join appearance.json)
            LUMEN_UNIFIED_OWNED_HEADERS: '1'
            LUMEN_NATIVE_WINDOW_CONTROLS: '0'
        } { ^timeout 30s quickshell -p ($sandbox | path join fixture.qml) | complete })
        let log = ($run.stdout + $run.stderr)
        if $run.exit_code != 0 or ($log | str contains UTILITY_TYPOGRAPHY_FAILURE) or ($log | str contains ReferenceError) or ($log | str contains TypeError) or ($log | str contains 'binding loop') {
            error make {msg: $'Utility typography QML failed ($consumer) ($mode): ($log)'}
        }
        let receipt = ($log | lines | where {|line| $line | str contains 'UTILITY_TYPOGRAPHY_RESULT '})
        if ($receipt | length) != 1 { error make {msg: $'Utility typography receipt missing: ($log)'} }
        let rows = ($receipt | first | split row 'UTILITY_TYPOGRAPHY_RESULT ' | last | from json)
        if ($rows | length) != ($cases | length) { error make {msg: $'Utility typography case count wrong: ($consumer) ($mode)'} }
        for index in 0..(($rows | length) - 1) {
            let row = ($rows | get $index)
            if $row.entry != ($cases | get $index) or $row.resolvedFamily != $palette.font or $row.pixelSize <= 0 or $row.target.width <= 0 or $row.target.height <= 0 {
                error make {msg: $'Utility text metrics wrong: ($row)'}
            }
            let frame = ($output | path join $'($index).png')
            if not ($frame | path exists) { error make {msg: $'Utility painted frame missing: ($frame)'} }
            count-ink $frame $row.textBounds $row.color
            $count += 1
        }
    }
}
if $count != 80 { error make {msg: $'Expected 80 utility typography cases, got ($count)'} }
rm --recursive --force $sandbox
print 'PASS 80 production utility typography frames with focus, roles and painted ink'
