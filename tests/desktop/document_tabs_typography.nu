#!/usr/bin/env nu
def ink-count [frame bounds color] {
    let x = ($bounds.x | math round | into int)
    let y = ($bounds.y | math round | into int)
    let width = ($bounds.width | math round | into int)
    let height = ($bounds.height | math round | into int)
    if $width <= 0 or $height <= 0 { error make {msg: 'Empty tab caption crop'} }
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
    if $pixels < 4 or $pixels >= ($width * $height * 0.6) { error make {msg: $'Tab caption painted ink invalid: ($pixels) in ($frame)'} }
}

def main [] {
    let repository = ($env.FILE_PWD | path join .. .. | path expand)
    let font_config = (^nix build ($repository + '#test-owned-font-config') --no-link --print-out-paths | lines | last)
    let files_package = (^nix build ($repository + '#file-surfaces-rust-wip') --no-link --print-out-paths | lines | last)
    let tags_package = (^nix build ($repository + '#file-tags') --no-link --print-out-paths | lines | last)
    let jobs_package = (^nix build ($repository + '#file-jobs-rust') --no-link --print-out-paths | lines | last)
    let sandbox = (^mktemp -d -t lumen-document-tabs.XXXXXX | str trim)
    cp -r ($repository | path join res lumen-desktop shell) ($sandbox | path join desktop)
    for directory in [home runtime state config cache data] {
        let path = ($sandbox | path join $directory)
        mkdir $path
        ^chmod 700 $path
    }
    let documents = ($sandbox | path join home Documents)
    mkdir $documents
    'Disposable private item' | save ($documents | path join 'Retained selection.txt')
    let palette = (open ($repository | path join res lumen-desktop appearance palette.json))
    {font: $palette.font, palette: $palette.light, preferences: {reduceMotion: true}} | to json | save ($sandbox | path join appearance.json)
    let cases = ([light dark] | each {|mode| [480 1060] | each {|width| [1.0 1.15 1.3 1.5] | each {|scale| {mode: $mode, width: $width, height: (if $width == 480 {360} else {720}), scale: $scale} } } | flatten } | flatten)
    (open --raw ($repository | path join tests desktop document_tabs_typography.qml)
        | str replace '__CASES__' ($cases | to json --raw)
        | str replace '__PALETTES__' ($palette | to json --raw)
        | str replace --all '__DOCUMENTS__' $documents
        | str replace '__OUTPUT__' $sandbox) | save ($sandbox | path join fixture.qml)
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
        LUMEN_FILES_BIN: ($files_package | path join bin lumen-files-rust-wip)
        LUMEN_FILE_TAGS_BIN: ($tags_package | path join bin lumen-file-tags)
        LUMEN_FILE_JOBS_BIN: ($jobs_package | path join bin lumen-file-jobs)
    } { ^timeout 60s quickshell -p ($sandbox | path join fixture.qml) | complete })
    let log = ($run.stdout + $run.stderr)
    if $run.exit_code != 0 or ($log | str contains TABS_FAILURE) or ($log | str contains ReferenceError) or ($log | str contains TypeError) or ($log | str contains 'binding loop') {
        error make {msg: $'Document tabs typography QML failed: ($log)'}
    }
    let receipt = ($log | lines | where {|line| $line | str contains 'TABS_RESULT '})
    if ($receipt | length) != 1 { error make {msg: $'Document tabs result missing: ($log)'} }
    let rows = ($receipt | first | split row 'TABS_RESULT ' | last | from json)
    if ($rows | length) != 16 { error make {msg: $'Document tabs expected 16 frames, got ($rows | length)'} }
    mut identities = []
    for row in $rows {
        let entry = $row.entry
        let identity = $'($entry.mode):($entry.scale):($entry.width)'
        if $identity in $identities or $row.resolvedFamily != $palette.font or $row.pixelSize != (12 * $entry.scale | math round) {
            error make {msg: $'Document tab metrics or identity wrong: ($identity)'}
        }
        $identities = ($identities | append $identity)
        if not ($row.capture | path exists) { error make {msg: $'Missing painted tab frame: ($row.capture)'} }
        let dimensions = (^magick identify -format '%w %h' $row.capture | str trim)
        if $dimensions != $'($entry.width) ($entry.height)' { error make {msg: $'Tab frame size wrong: ($dimensions)'} }
        ink-count $row.capture $row.label $row.color
    }
    rm --recursive --force $sandbox
    print 'PASS 16 production Files tab typography frames with active identity and painted ink'
}
