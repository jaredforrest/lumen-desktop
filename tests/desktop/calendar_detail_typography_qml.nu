#!/usr/bin/env nu
def ink-count [frame bounds color] {
    let x = ($bounds.x | math round | into int)
    let y = ($bounds.y | math round | into int)
    let width = ($bounds.width | math round | into int)
    let height = ($bounds.height | math round | into int)
    if $width <= 0 or $height <= 0 { error make {msg: 'Empty Calendar detail text crop'} }
    let rgb = ($color | parse -r '(?<rgb>[0-9a-fA-F]{6})$' | first | get rgb)
    let target = [($rgb | str substring 0..<2 | into int --radix 16) ($rgb | str substring 2..<4 | into int --radix 16) ($rgb | str substring 4..<6 | into int --radix 16)]
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
    if $pixels < 4 { error make {msg: $'Calendar detail painted ink missing: ($frame) ($pixels)'} }
}

def main [] {
    let repository = ($env.FILE_PWD | path join .. .. | path expand)
    let font_config = (^nix build ($repository + '#test-owned-font-config') --no-link --print-out-paths | lines | last)
    let provider_package = (^nix build ($repository + '#test-desktop-policy-fixture') --no-link --print-out-paths | lines | last)
    let sandbox = (^mktemp -d -t lumen-calendar-detail-text.XXXXXX | str trim)
    cp -r ($repository | path join res lumen-desktop shell) ($sandbox | path join desktop)
    for directory in [home runtime state config cache data bin] {
        let path = ($sandbox | path join $directory)
        mkdir $path
        ^chmod 700 $path
    }
    ln -s ($provider_package | path join bin calendar_detail_provider) ($sandbox | path join bin lumen-calendar)
    let palette = (open ($repository | path join res lumen-desktop appearance palette.json))
    mut identities = []
    for mode in [light dark] {
        for scale in [1.0 1.15 1.3 1.5] {
            for readonly in [false true] {
                let output = ($sandbox | path join $'($mode)-($scale)-($readonly)')
                mkdir $output
                for directory in [home runtime state config cache data] {
                    let path = ($output | path join $directory)
                    mkdir $path
                    ^chmod 700 $path
                }
                let cases = ([false true] | each {|small| {mode: $mode, scale: $scale, readonly: $readonly, small: $small} })
                (open --raw ($repository | path join tests desktop calendar_detail_typography_qml.qml)
                    | str replace '__CASES__' ($cases | to json --raw)
                    | str replace '__PALETTES__' ($palette | to json --raw)
                    | str replace '__OUTPUT__' $output) | save --force ($sandbox | path join fixture.qml)
                {font: $palette.font, palette: ($palette | get $mode), preferences: {reduceMotion: true}} | to json | save --force ($output | path join appearance.json)
                let run = (with-env {
                    HOME: ($output | path join home)
                    XDG_RUNTIME_DIR: ($output | path join runtime)
                    XDG_STATE_HOME: ($output | path join state)
                    XDG_CONFIG_HOME: ($output | path join config)
                    XDG_CACHE_HOME: ($output | path join cache)
                    XDG_DATA_HOME: ($output | path join data)
                    PATH: ($env.PATH | prepend ($sandbox | path join bin))
                    QT_QPA_PLATFORM: offscreen
                    QT_QPA_PLATFORMTHEME: generic
                    QT_QUICK_BACKEND: software
                    QT_ACCESSIBILITY: '0'
                    NO_AT_BRIDGE: '1'
                    GTK_A11Y: none
                    FONTCONFIG_FILE: $font_config
                    LUMEN_FIXTURE_CALENDAR_READONLY: ($readonly | into string)
                    LUMEN_APPEARANCE_STATE: ($output | path join appearance.json)
                } { ^timeout 40s quickshell -p ($sandbox | path join fixture.qml) | complete })
                let log = ($run.stdout + $run.stderr)
                if $run.exit_code != 0 or ($log | str contains DETAIL_TEXT_FAILURE) or ($log | str contains ReferenceError) or ($log | str contains TypeError) or ($log | str contains 'binding loop') {
                    error make {msg: $'Calendar detail typography failed ($mode) ($scale) ($readonly): ($log)'}
                }
                let receipt = ($log | lines | where {|line| $line | str contains 'DETAIL_TEXT_RESULT '})
                if ($receipt | length) != 1 { error make {msg: $'Calendar detail result missing: ($log)'} }
                let rows = ($receipt | first | split row 'DETAIL_TEXT_RESULT ' | last | from json)
                if ($rows | length) != 12 { error make {msg: $'Calendar detail segment expected 12 frames, got ($rows | length)'} }
                for index in 0..11 {
                    let row = ($rows | get $index)
                    let entry = $row.entry
                    let identity = $'($entry.mode):($entry.scale):($entry.small):($entry.readonly):($row.phase)'
                    if $identity in $identities or $row.resolvedFamily != $palette.font or $row.pixelSize <= 0 {
                        error make {msg: $'Calendar detail role or identity wrong: ($identity)'}
                    }
                    $identities = ($identities | append $identity)
                    let frame = ($output | path join $'(($index / 6 | math floor | into int))-($row.phase).png')
                    if not ($frame | path exists) { error make {msg: $'Missing Calendar detail frame: ($frame)'} }
                    ink-count $frame $row.textBounds $row.color
                }
            }
        }
    }
    if ($identities | length) != 192 { error make {msg: 'Calendar detail corpus omitted cases'} }
    rm --recursive --force $sandbox
    print 'PASS 192 production Calendar detail typography frames with readonly states and painted ink'
}
