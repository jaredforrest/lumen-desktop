# Production projection journeys on disposable displays and private providers.
def main [launcher: string, basic: string, effects: string, --only: string = 'all', --run-id: string = ''] {
    let scenarios = [projection projection-interaction projection-focus projection-protocol projection-lock projection-family projection-family-bounds projection-drag]
    if $only != 'all' and $only not-in $scenarios {error make {msg:'Unknown projection acceptance scenario'}}
    if $run_id != '' and ($run_id !~ '^[a-z0-9][a-z0-9-]{0,63}$') {error make {msg:'Invalid projection run identifier'}}
    let root = $env.PWD
    ^nix build .#test-projection-environment --out-link artifacts/projection-environment --option abort-on-warn true
    if $env.LAST_EXIT_CODE != 0 {error make {msg:'Projection environment build failed'}}
    let environment = (($root | path join artifacts projection-environment) | path expand)
    let fixture = (($root | path join artifacts lifecycle-native-fixture bin lumen-lifecycle-native-fixture) | path expand)
    let boundary = (($root | path join artifacts application-isolation-package bin lumen-application-isolation) | path expand)
    let font = (($root | path join artifacts owned-font-config) | path expand)
    let driver = (($root | path join artifacts software-driver) | path expand)
    let shell = ($launcher | path expand)
    for backend in [
        {name: basic, provider: ($basic | path expand), flags: []}
        {name: effects, provider: ($effects | path expand), flags: [--effects]}
    ] {
        for scenario in ($scenarios | where {|scenario| $only == 'all' or $scenario == $only}) {
            let suffix = if $run_id == '' {''} else {$'-($run_id)'}
            let output = ($root | path join artifacts $'native-projection-($backend.name)-($scenario)($suffix)')
            if ($output | path exists) {error make {msg:$'Projection output already exists: ($output)'}}
            mkdir $output
            ^nu tests/ci/run-native.nu $'projection_($backend.name)_($scenario)' -- $boundary run --source $root --output $output --software-graphics -- ($environment | path join bin env) $'PATH=($environment)/bin' $'FONTCONFIG_FILE=($font)' $'LIBGL_DRIVERS_PATH=($driver)/lib/dri' $'__EGL_VENDOR_LIBRARY_FILENAMES=($driver)/share/glvnd/egl_vendor.d/50_mesa.json' GALLIUM_DRIVER=llvmpipe $fixture $scenario --launcher $shell --sway $backend.provider ...$backend.flags --output /artifacts/result
            if $env.LAST_EXIT_CODE != 0 {error make {msg:$'Projection boundary or fixture failed: ($scenario) / ($backend.name)'}}
            let result = (open ($output | path join result result.json))
            if $result.passed != true {error make {msg:'Projection acceptance did not pass'}}
            {
                sourceRevision: (^git rev-parse HEAD | str trim)
                scenario: $scenario
                backend: $backend.name
                launcher: $shell
                launcherSha256: (open --raw $shell | hash sha256)
                provider: $backend.provider
                providerSha256: (open --raw $backend.provider | hash sha256)
                fixture: $fixture
                fixtureSha256: (open --raw $fixture | hash sha256)
                environment: $environment
                font: $font
                softwareDriver: $driver
                result: $result
            } | to json | save -f ($output | path join exact-pins.json)
        }
    }
}
