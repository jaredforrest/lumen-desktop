# Stream a native fixture and reject interpreter finalizer warnings even on exit 0.
def main [label: string, ...command: string] {
    if ($command | is-empty) { error make {msg: 'A native test command is required'} }
    let directory = 'artifacts/native-command-logs'
    mkdir $directory
    let log = ($directory | path join $'($label).(random chars --length 8).log')
    let program = ($command | first)
    let arguments = ($command | skip 1)
    # Test commands create their own private owned roots. A short parent avoids
    # inherited CI/Nix TMPDIR prefixes consuming Unix/Qt socket pathname budgets.
    run-external env 'TMPDIR=/tmp' $program ...$arguments out+err>| ^tee $log
    let output = (open --raw $log)
    if ($output =~ '(?m)(^|\s)ResourceWarning:|^Exception ignored (in|while finalizing)') {
        error make {msg: $'Native test emitted an unraisable warning; see ($log)'}
    }
}
