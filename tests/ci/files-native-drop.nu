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
    build default files-native-drop-shell
    build test-desktop-surface-fixture files-native-drop-fixture
    build test-pointer files-native-drop-pointer
    build test-application-isolation files-native-drop-isolation
    build compositor-basic files-native-drop-basic
    build compositor files-native-drop-effects
    let source = (^realpath -- $env.PWD | str trim)
    let launcher = (executable 'files-native-drop-shell/bin/lumen-desktop-shell')
    let fixture = (executable 'files-native-drop-fixture/bin/lumen-desktop-surface-fixture')
    let pointer = (executable 'files-native-drop-pointer/bin/wlrctl')
    let isolation = (executable 'files-native-drop-isolation/bin/lumen-application-isolation')
    let backends = (if $backend == all { [basic effects] } else { [$backend] })
    for backend in $backends {
        let compositor = (executable $'files-native-drop-($backend)/bin/sway')
        for peer in [qt gtk] {
            let scenario = (if $peer == gtk { 'files-drag-gtk' } else { 'files-drag' })
            let flags = (if $backend == effects { ['--effects'] } else { [] })
            let label = $'files-native-drop-($id)-($backend)-($peer)'
            let output = (artifact $label)
            if ($output | path exists) { error make {msg: $'Fresh evidence directory required: ($output)'} }
            mkdir $output
            ^chmod 700 $output
            ^nu tests/ci/run-native.nu $label -- $isolation run --source $source --output $output --software-graphics -- $fixture $scenario ...$flags --launcher $launcher --sway $compositor --pointer-tool $pointer --output /artifacts/run
        }
    }
}
