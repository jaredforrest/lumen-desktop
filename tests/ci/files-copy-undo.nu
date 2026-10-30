def artifact [path: string] { $env.PWD | path join artifacts $path }
def build [attribute: string, output: string] {
    ^nix build $'.#($attribute)' --out-link (artifact $output) --option abort-on-warn true --option eval-cache false --max-jobs 1 --cores 2
}
def executable [path: string] { ^realpath -- (artifact $path) | str trim }
def main [--run-id: string, --backend: string = 'all'] {
    let id = ($run_id | default (random chars --length 8))
    if not ($id =~ '^[a-zA-Z0-9_-]{1,48}$') { error make {msg: 'Use a short alphanumeric evidence run ID'} }
    if $backend not-in [all basic effects] { error make {msg: 'Backend must be all, basic or effects'} }
    mkdir artifacts
    build default files-copy-undo-shell
    build test-desktop-surface-fixture files-copy-undo-fixture
    build test-pointer files-copy-undo-pointer
    build test-application-isolation files-copy-undo-isolation
    build compositor-basic files-copy-undo-basic
    build compositor files-copy-undo-effects
    let source = (^realpath -- $env.PWD | str trim)
    let launcher = (executable 'files-copy-undo-shell/bin/lumen-desktop-shell')
    let fixture = (executable 'files-copy-undo-fixture/bin/lumen-desktop-surface-fixture')
    let pointer = (executable 'files-copy-undo-pointer/bin/wlrctl')
    let isolation = (executable 'files-copy-undo-isolation/bin/lumen-application-isolation')
    let backends = (if $backend == all { [basic effects] } else { [$backend] })
    for backend in $backends {
        let compositor = (executable $'files-copy-undo-($backend)/bin/sway')
        let flags = (if $backend == effects { ['--effects'] } else { [] })
        let label = $'files-copy-undo-($id)-($backend)'
        let output = (artifact $label)
        if ($output | path exists) { error make {msg: $'Fresh evidence directory required: ($output)'} }
        mkdir $output
        ^chmod 700 $output
        ^nu tests/ci/run-native.nu $label -- $isolation run --source $source --output $output --software-graphics -- $fixture files-drag-copy-undo ...$flags --launcher $launcher --sway $compositor --pointer-tool $pointer --output /artifacts/run
        let result = (open ($output | path join run acceptance.json))
        if $result.passed != true or ($result.cases | length) != 9 { error make {msg: 'Incomplete native Copy Undo acceptance'} }
    }
}
