#!/usr/bin/env nu
use typography_cases.nu *

def ink-count [frame bounds color] {
    let x = ($bounds.x | math round | into int)
    let y = ($bounds.y | math round | into int)
    let width = ($bounds.width | math round | into int)
    let height = ($bounds.height | math round | into int)
    if $width <= 0 or $height <= 0 { error make {msg: 'Empty typography text crop'} }
    let target = [r g b] | each {|channel| (($color | get $channel) * 255 | math round | into int) }
    let histogram = (^magick $frame -crop $'($width)x($height)+($x)+($y)' +repage -format '%c' histogram:info: | lines)
    mut pixels = 0
    for line in $histogram {
        let parsed = ($line | parse -r '^\s*(?<count>\d+): \((?<r>\d+),(?<g>\d+),(?<b>\d+)')
        if ($parsed | is-empty) { continue }
        let pixel = ($parsed | first)
        let actual = [($pixel.r | into int) ($pixel.g | into int) ($pixel.b | into int)]
        if (($actual.0 - $target.0) | math abs) <= 6 and (($actual.1 - $target.1) | math abs) <= 6 and (($actual.2 - $target.2) | math abs) <= 6 {
            $pixels += ($pixel.count | into int)
        }
    }
    if $pixels < 4 { error make {msg: $'Typography text ink missing: ($frame), pixels=($pixels)'} }
}

def main [consumer?: string, width?: int, mode?: string] {
    let repository = ($env.FILE_PWD | path join .. .. | path expand)
    let font_config = (^nix build ($repository + '#test-owned-font-config') --no-link --print-out-paths | lines | last)
    let files_package = (^nix build ($repository + '#file-surfaces-rust-wip') --no-link --print-out-paths | lines | last)
    let tags_package = (^nix build ($repository + '#file-tags') --no-link --print-out-paths | lines | last)
    let jobs_package = (^nix build ($repository + '#file-jobs-rust') --no-link --print-out-paths | lines | last)
    let sandbox = (^mktemp -d -t lumen-owned-text.XXXXXX | str trim)
    cp -r ($repository | path join res lumen-desktop shell) ($sandbox | path join desktop)
    for directory in [home runtime state config cache data] {
        let path = ($sandbox | path join $directory)
        mkdir $path
        ^chmod 700 $path
    }
    let documents = ($sandbox | path join home Documents)
    mkdir $documents
    'Synthetic private document' | save ($documents | path join 'An ordinary document.txt')
    'Second private document' | save ($documents | path join 'Another document.txt')
    let palette = (open ($repository | path join res lumen-desktop appearance palette.json))
    {font: $palette.font, palette: $palette.light, preferences: {reduceMotion: true}} | to json | save ($sandbox | path join appearance.json)
    let batches = (segments $consumer $width $mode)
    mut identities = []
    for batch in $batches {
        let output = ($sandbox | path join $'($batch.consumer)-($batch.width)-($batch.mode)')
        mkdir $output
        let expected = (cases $batch.consumer $batch.width $batch.mode)
        (open --raw ($repository | path join tests desktop owned_typography_qml.qml)
            | str replace '__CASES__' ($expected | to json --raw)
            | str replace '__FAMILY__' ($palette.font | to json --raw)
            | str replace '__PALETTES__' ($palette | to json --raw)
            | str replace --all '__DOCUMENTS__' $documents
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
            LUMEN_FILES_BIN: ($files_package | path join bin lumen-files-rust-wip)
            LUMEN_FILE_TAGS_BIN: ($tags_package | path join bin lumen-file-tags)
            LUMEN_FILE_JOBS_BIN: ($jobs_package | path join bin lumen-file-jobs)
        } { ^timeout 30s quickshell -p ($sandbox | path join fixture.qml) | complete })
        let log = ($run.stdout + $run.stderr)
        if $run.exit_code != 0 or ($log | str contains OWNED_TYPOGRAPHY_FAILURE) or ($log | str contains ReferenceError) or ($log | str contains TypeError) or ($log | str contains 'binding loop') {
            error make {msg: $'Owned typography QML failed ($batch): ($log)'}
        }
        let receipt = ($log | lines | where {|line| $line | str contains 'OWNED_TYPOGRAPHY_RESULT '})
        if ($receipt | length) != 1 { error make {msg: $'Owned typography receipt missing ($batch): ($log)'} }
        let records = ($receipt | first | split row 'OWNED_TYPOGRAPHY_RESULT ' | last | from json)
        if ($records | length) != 16 { error make {msg: $'Owned typography segment has ($records | length) records'} }
        for row in $records {
            let identity = (identity $row.entry)
            if $identity in $identities or $row.resolvedFamily != $palette.font or not $row.focusedAndRevealed or $row.focusable <= 0 {
                error make {msg: $'Owned typography identity, font or focus failed: ($identity)'}
            }
            $identities = ($identities | append $identity)
            if not ($row.capture | path exists) { error make {msg: $'Owned typography capture missing: ($row.capture)'} }
            let dimensions = (^magick identify -format '%w %h' $row.capture | str trim)
            if $dimensions != $'($row.entry.width) ($row.entry.height)' { error make {msg: $'Owned typography frame dimensions wrong: ($identity), ($dimensions)'} }
            ink-count $row.capture $row.paintedText $row.paintedText.color
            if $row.paintedEvent != null { ink-count $row.capture $row.paintedEvent $row.paintedEvent.color }
        }
    }
    if ($identities | length) != (($batches | length) * 16) { error make {msg: 'Owned typography corpus omitted cases'} }
    rm --recursive --force $sandbox
    print $'PASS ($identities | length) production Files/Calendar typography frames with focus and painted ink'
}
