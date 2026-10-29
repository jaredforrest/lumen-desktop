#!/usr/bin/env nu
let repository = ($env.FILE_PWD | path join .. .. | path expand)
let fixture_package = (^nix build ($repository + '#test-desktop-policy-fixture') --no-link --print-out-paths | lines | last)
let fixture_binary = ($fixture_package | path join bin lumen-desktop-policy-fixture)
let files_package = (^nix build ($repository + '#file-surfaces-rust-wip') --no-link --print-out-paths | lines | last)
let files_binary = ($files_package | path join bin lumen-files-rust-wip)
let tags_package = (^nix build ($repository + '#file-tags') --no-link --print-out-paths | lines | last)
let jobs_package = (^nix build ($repository + '#file-jobs-rust') --no-link --print-out-paths | lines | last)
let sandbox = (^mktemp -d -t lumen-directory-consumer.XXXXXX | str trim)
cp -r ($repository | path join res lumen-desktop) ($sandbox | path join assets)
let initial = ($sandbox | path join initial)
let target = ($sandbox | path join target)
mkdir $initial
mkdir $target
'retained' | save ($initial | path join retained.txt)
'original' | save ($target | path join original.txt)
let facts = (^$fixture_binary captured-directory-facts $target | from json)
mv $target ($sandbox | path join retired)
mkdir $target
'replacement' | save ($target | path join replacement.txt)
let template = (open --raw ($repository | path join tests desktop captured_directory_qml.qml))
($template | str replace '__INITIAL__' ($initial | to json --raw) | str replace '__TARGET__' ($target | to json --raw) | str replace '__FACTS__' ($facts | to json --raw)) | save ($sandbox | path join shell.qml)
for directory in [home runtime state config cache data] {
    let path = ($sandbox | path join $directory)
    mkdir $path
    ^chmod 700 $path
}
let palette = (open ($repository | path join res lumen-desktop appearance palette.json))
let appearance = ($sandbox | path join appearance.json)
{font: $palette.font, palette: $palette.dark, preferences: {}} | to json | save $appearance
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
    LUMEN_FILES_BIN: $files_binary
    LUMEN_FILE_TAGS_BIN: ($tags_package | path join bin lumen-file-tags)
    LUMEN_FILE_JOBS_BIN: ($jobs_package | path join bin lumen-file-jobs)
    LUMEN_APPEARANCE_STATE: $appearance
} { ^timeout 15s quickshell -p ($sandbox | path join shell.qml) | complete })
let log = ($run.stdout + $run.stderr)
if $run.exit_code != 0 or not ($log | str contains 'PASS actual Files expected-directory') or ($log | str contains 'FAIL ') {
    error make {msg: $'Captured directory QML failed: ($log)'}
}
rm --recursive --force $sandbox
print 'PASS actual Files expected-directory request rejects replacement and preserves rows, path and history'
