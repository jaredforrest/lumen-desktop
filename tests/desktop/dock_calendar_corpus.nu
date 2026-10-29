#!/usr/bin/env nu
def count-color [capture geometry target] {
    let histogram = (^magick $capture -crop $geometry +repage -format '%c' histogram:info: | lines)
    mut count = 0
    for line in $histogram {
        let parsed = ($line | parse -r '^\s*(?<count>\d+): \((?<r>\d+),(?<g>\d+),(?<b>\d+)')
        if ($parsed | is-empty) { continue }
        let pixel = ($parsed | first)
        let actual = [($pixel.r | into int) ($pixel.g | into int) ($pixel.b | into int)]
        if (($actual.0 - $target.0) | math abs) <= 4 and (($actual.1 - $target.1) | math abs) <= 4 and (($actual.2 - $target.2) | math abs) <= 4 {
            $count += ($pixel.count | into int)
        }
    }
    $count
}
let repository = ($env.FILE_PWD | path join .. .. | path expand)
let sandbox = (^mktemp -d -t lumen-calendar-corpus.XXXXXX | str trim)
cp -r ($repository | path join res lumen-desktop) ($sandbox | path join assets)
cp -r ($repository | path join res lumen-desktop) ($sandbox | path join broken)
for directory in [home runtime state config cache data] {
    let path = ($sandbox | path join $directory)
    mkdir $path
    ^chmod 700 $path
}
let palettes = (open ($repository | path join res lumen-desktop appearance palette.json))
let instants = [
    '2026-10-20T23:59:59Z' '2026-10-21T00:00:00Z'
    '2026-10-31T23:59:59Z' '2026-11-01T00:00:00Z'
    '2026-12-31T23:59:59Z' '2027-01-01T00:00:00Z'
    '2028-02-28T23:59:59Z' '2028-02-29T00:00:00Z'
    '2028-03-01T00:00:00Z'
]
(open --raw ($repository | path join tests desktop dock_calendar_corpus.qml) | str replace '__PALETTES__' ($palettes | to json --raw) | str replace '__DATES__' ($instants | to json --raw)) | save ($sandbox | path join corpus.qml)
{font: $palettes.font, palette: $palettes.light, preferences: {reduceMotion: true}} | to json | save ($sandbox | path join appearance.json)
let cases = [
    {en: Tue, fr: 'mar.', day: '20'} {en: Wed, fr: 'mer.', day: '21'}
    {en: Sat, fr: 'sam.', day: '31'} {en: Sun, fr: 'dim.', day: '1'}
    {en: Thu, fr: 'jeu.', day: '31'} {en: Fri, fr: 'ven.', day: '1'}
    {en: Mon, fr: 'lun.', day: '28'} {en: Tue, fr: 'mar.', day: '29'}
    {en: Wed, fr: 'mer.', day: '1'}
]
mut verified = 0
for locale in [en_GB fr_FR] {
    rm --force ($sandbox | path join broken shell reference-icons calendar.svg)
    rm --force ($sandbox | path join broken shell reference-icons dark calendar.svg)
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
        TZ: UTC
        LANG: ($locale + '.UTF-8')
        LC_ALL: ($locale + '.UTF-8')
        LUMEN_CALENDAR_CAPTURE_DIR: $sandbox
        LUMEN_APPEARANCE_STATE: ($sandbox | path join appearance.json)
    } { ^timeout 30s quickshell -p ($sandbox | path join corpus.qml) | complete })
    let log = ($run.stdout + $run.stderr)
    if $run.exit_code != 0 or ($log | str contains 'CALENDAR_CORPUS_FAILURE') {
        error make {msg: $'Calendar QML failed for ($locale): ($log)'}
    }
    let receipt = ($log | lines | where {|line| $line | str contains 'CALENDAR_CORPUS_RESULT '})
    if ($receipt | length) != 1 { error make {msg: $'Missing calendar receipt for ($locale): ($log)'} }
    let payload = ($receipt | first | split row 'CALENDAR_CORPUS_RESULT ' | last | from json)
    if $payload.locale != $locale or ($payload.records | length) != 18 or ($payload.recoveries | length) != 2 {
        error make {msg: $'Calendar case count or locale mismatch: ($locale)'}
    }
    for recovery in $payload.recoveries {
        if $recovery.failed or not ($recovery.source | str ends-with $'/reference-icons/(if $recovery.mode == dark {"dark/"} else {""})calendar.svg') or not ($recovery.capture | path exists) {
            error make {msg: $'Calendar icon recovery failed: ($locale) ($recovery.mode)'}
        }
    }
    for index in 0..17 {
        let row = ($payload.records | get $index)
        let expected = ($cases | get ($index / 2 | math floor | into int))
        let weekday = (if $locale == en_GB { $expected.en } else { $expected.fr })
        let labels = ($row.labels | get text)
        let fallback_labels = ($row.unavailableLabels | get text)
        let wanted = [$weekday $expected.day]
        if $labels != $wanted or $fallback_labels != $wanted or ($row.labels | any {|label| $label.width > 128 }) {
            error make {msg: $'Calendar date label mismatch: ($locale) ($row.iso) ($labels) ($fallback_labels)'}
        }
        if not $row.unavailableFailed or ($row.unavailableSource | str ends-with '/icons/application.svg') or not ($row.genericSource | str ends-with '/icons/application.svg') or not ($row.genericLabels | is-empty) or not ($row.capture | path exists) {
            error make {msg: $'Calendar fallback contract failed: ($locale) ($row.iso)'}
        }
        let dimensions = (^magick identify -format '%w %h' $row.capture | str trim | split row ' ')
        if ($dimensions.0 | into int) <= 380 or ($dimensions.1 | into int) != 128 {
            error make {msg: $'Calendar frame dimensions wrong: ($locale) ($row.iso)'}
        }
        let background = (if $row.mode == light { [245 245 247] } else { [69 69 75] })
        let ink = (if $row.mode == light { [23 23 23] } else { [245 245 247] })
        let red = (if $row.mode == light { [185 57 49] } else { [241 109 104] })
        for offset in [0 148] {
            let full = (count-color $row.capture $'128x128+($offset)+0' $background)
            let day = (count-color $row.capture $'108x79+($offset + 10)+37' $ink)
            let weekday = (count-color $row.capture $'118x30+($offset + 5)+14' $red)
            if $full <= 1000 or $day <= 30 or $weekday <= 8 {
                error make {msg: $'Calendar ink missing: ($locale) ($row.iso) ($row.mode) offset=($offset) background=($full) day=($day) weekday=($weekday)'}
            }
        }
        $verified += 1
    }
}
rm --recursive --force $sandbox
print $'PASS ($verified) live calendar dates, locales, fallback labels and captured frames'
