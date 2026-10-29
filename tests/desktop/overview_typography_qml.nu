#!/usr/bin/env nu
def ink-count [frame bounds color] {
    let x = ($bounds.x | math round | into int)
    let y = ($bounds.y | math round | into int)
    let width = ($bounds.width | math round | into int)
    let height = ($bounds.height | math round | into int)
    if $width <= 0 or $height <= 0 { error make {msg: 'Empty Overview text crop'} }
    let rgb = ($color | parse -r '(?<rgb>[0-9a-fA-F]{6})$' | first | get rgb)
    let target = [($rgb | str substring 0..<2 | into int --radix 16) ($rgb | str substring 2..<4 | into int --radix 16) ($rgb | str substring 4..<6 | into int --radix 16)]
    let histogram = (^magick $frame -crop $'($width)x($height)+($x)+($y)' +repage -format '%c' histogram:info: | lines)
    mut pixels = 0
    for line in $histogram {
        let parsed = ($line | parse -r '^\s*(?<count>\d+): \((?<r>\d+),(?<g>\d+),(?<b>\d+)')
        if ($parsed | is-empty) { continue }
        let pixel = ($parsed | first)
        let actual = [($pixel.r | into int) ($pixel.g | into int) ($pixel.b | into int)]
        if (($actual.0 - $target.0) | math abs) <= 32 and (($actual.1 - $target.1) | math abs) <= 32 and (($actual.2 - $target.2) | math abs) <= 32 {
            $pixels += ($pixel.count | into int)
        }
    }
    if $pixels < 4 or $pixels >= ($width * $height * 0.6) { error make {msg: $'Overview painted ink invalid: ($pixels) in ($frame)'} }
}

def main [] {
    let repository = ($env.FILE_PWD | path join .. .. | path expand)
    let font_config = (^nix build ($repository + '#test-owned-font-config') --no-link --print-out-paths | lines | last)
    let provider_package = (^nix build ($repository + '#test-desktop-policy-fixture') --no-link --print-out-paths | lines | last)
    let sandbox = (^mktemp -d -t lumen-overview-text.XXXXXX | str trim)
    cp -r ($repository | path join res lumen-desktop shell) ($sandbox | path join desktop)
    for directory in [home runtime state config cache data bin] {
        let path = ($sandbox | path join $directory)
        mkdir $path
        ^chmod 700 $path
    }
    ln -s ($provider_package | path join bin overview_typography_provider) ($sandbox | path join bin lumen-window-overview)
    let palette = (open ($repository | path join res lumen-desktop appearance palette.json))
    {font: $palette.font, palette: $palette.light, preferences: {reduceMotion: true}} | to json | save ($sandbox | path join appearance.json)
    let cases = ([light dark] | each {|mode| [560 880] | each {|width| [1.0 1.15 1.3 1.5] | each {|scale| {mode: $mode, width: $width, height: (if $width == 560 {480} else {560}), scale: $scale} } } | flatten } | flatten)
    (open --raw ($repository | path join tests desktop overview_typography_qml.qml)
        | str replace '__CASES__' ($cases | to json --raw)
        | str replace '__PALETTES__' ($palette | to json --raw)
        | str replace '__OUTPUT__' $sandbox) | save ($sandbox | path join fixture.qml)
    let run = (with-env {
        HOME: ($sandbox | path join home)
        XDG_RUNTIME_DIR: ($sandbox | path join runtime)
        XDG_STATE_HOME: ($sandbox | path join state)
        XDG_CONFIG_HOME: ($sandbox | path join config)
        XDG_CACHE_HOME: ($sandbox | path join cache)
        XDG_DATA_HOME: ($sandbox | path join data)
        PATH: ($env.PATH | prepend ($sandbox | path join bin))
        QT_QPA_PLATFORM: offscreen
        QT_QPA_PLATFORMTHEME: generic
        QT_QUICK_BACKEND: software
        QT_ACCESSIBILITY: '0'
        NO_AT_BRIDGE: '1'
        GTK_A11Y: none
        FONTCONFIG_FILE: $font_config
        LUMEN_APPEARANCE_STATE: ($sandbox | path join appearance.json)
    } { ^timeout 60s quickshell -p ($sandbox | path join fixture.qml) | complete })
    let log = ($run.stdout + $run.stderr)
    if $run.exit_code != 0 or ($log | str contains OVERVIEW_TEXT_FAILURE) or ($log | str contains ReferenceError) or ($log | str contains TypeError) or ($log | str contains 'binding loop') {
        error make {msg: $'Overview typography QML failed: ($log)'}
    }
    let receipt = ($log | lines | where {|line| $line | str contains 'OVERVIEW_TEXT_RESULT '})
    if ($receipt | length) != 1 { error make {msg: $'Overview typography result missing: ($log)'} }
    let rows = ($receipt | first | split row 'OVERVIEW_TEXT_RESULT ' | last | from json)
    if ($rows | length) != 48 { error make {msg: $'Overview expected 48 frames, got ($rows | length)'} }
    mut identities = []
    for row in $rows {
        let entry = $row.entry
        let identity = $'($entry.mode):($entry.scale):($entry.width):($row.phase)'
        if $identity in $identities or $row.font != (12 * $entry.scale | math round) or ($row.focusName | is-empty) {
            error make {msg: $'Overview font, focus or identity wrong: ($identity)'}
        }
        $identities = ($identities | append $identity)
        if not ($row.capture | path exists) { error make {msg: $'Missing Overview frame: ($row.capture)'} }
        ink-count $row.capture $row.paintedText $row.paintedColor
    }
    rm --recursive --force $sandbox
    print 'PASS 48 production Overview typography frames with focus and painted ink'
}
