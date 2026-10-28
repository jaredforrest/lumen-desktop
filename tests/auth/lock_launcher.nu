def main [launcher: path, quickshell: path, swaylock: path] {
    mkdir $env.out
    let qml = ($env.out | path join lock.qml)
    'fixture' | save $qml

    let covered_runtime = ($env.out | path join covered)
    mkdir $covered_runtime
    ^chmod 700 $covered_runtime
    $env.XDG_RUNTIME_DIR = $covered_runtime
    $env.LUMEN_LOCK_FIXTURE_MODE = 'covered'
    $env.LUMEN_LOCK_FIXTURE_START = ($covered_runtime | path join started.json)
    $env.LUMEN_LOCK_FIXTURE_FALLBACK = ($covered_runtime | path join fallback.json)
    let covered = (^$launcher $quickshell $swaylock $qml | complete)
    if $covered.exit_code != 0 or not ($env.LUMEN_LOCK_FIXTURE_START | path exists) or ($env.LUMEN_LOCK_FIXTURE_FALLBACK | path exists) {
        error make {msg: $'Lock launcher accepted no secure coverage: ($covered.stderr)'}
    }

    let failed_runtime = ($env.out | path join start-failed)
    mkdir $failed_runtime
    ^chmod 700 $failed_runtime
    $env.XDG_RUNTIME_DIR = $failed_runtime
    $env.LUMEN_LOCK_FIXTURE_MODE = 'start-failed'
    $env.LUMEN_LOCK_FIXTURE_FALLBACK = ($failed_runtime | path join fallback.json)
    let failed = (^$launcher $quickshell $swaylock $qml | complete)
    if $failed.exit_code != 0 or (open $env.LUMEN_LOCK_FIXTURE_FALLBACK) != ['-f'] {
        error make {msg: $'Lock launcher did not run fallback after startup failure: ($failed.stderr)'}
    }

    let uncovered_runtime = ($env.out | path join uncovered)
    mkdir $uncovered_runtime
    ^chmod 700 $uncovered_runtime
    $env.XDG_RUNTIME_DIR = $uncovered_runtime
    $env.LUMEN_LOCK_FIXTURE_MODE = 'uncovered'
    $env.LUMEN_LOCK_FIXTURE_START = ($uncovered_runtime | path join started.json)
    $env.LUMEN_LOCK_FIXTURE_FALLBACK = ($uncovered_runtime | path join fallback.json)
    let uncovered = (^$launcher $quickshell $swaylock $qml | complete)
    if $uncovered.exit_code != 1 or not ($uncovered.stderr | str contains 'did not confirm secure coverage') or ($env.LUMEN_LOCK_FIXTURE_FALLBACK | path exists) {
        error make {msg: 'Lock launcher did not fail closed without secure coverage'}
    }

    let link_runtime = ($env.out | path join linked)
    mkdir $link_runtime
    ^chmod 700 $link_runtime
    let sentinel = ($env.out | path join sentinel)
    'untouched' | save $sentinel
    ^ln -s $sentinel ($link_runtime | path join lumen-lock-startup)
    $env.XDG_RUNTIME_DIR = $link_runtime
    $env.LUMEN_LOCK_FIXTURE_MODE = 'covered'
    let linked = (^$launcher $quickshell $swaylock $qml | complete)
    if $linked.exit_code != 1 or (open $sentinel | str trim) != 'untouched' {
        error make {msg: 'Lock launcher followed an unsafe startup-lock symlink'}
    }
}
