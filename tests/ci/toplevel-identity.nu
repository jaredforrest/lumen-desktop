# Actual optional identity bridge acceptance in disposable boundaries.
def main [launcher: string, basic: string, effects: string] {
    let root = $env.PWD
    ^nix build .#test-toplevel-identity-wire --out-link artifacts/toplevel-identity-wire --option abort-on-warn true
    ^nix build .#test-toplevel-identity-environment --out-link artifacts/toplevel-identity-environment --option abort-on-warn true
    ^nix build .#test-toplevel-identity-legacy-compositor --out-link artifacts/toplevel-identity-legacy-compositor --option abort-on-warn true
    let environment = ((($root | path join artifacts toplevel-identity-environment) | path expand) | path join bin env)
    let wire = ((($root | path join artifacts toplevel-identity-wire) | path expand) | path join bin lumen-toplevel-identity-wire)
    let fixture = ((($root | path join artifacts lifecycle-native-fixture) | path expand) | path join bin lumen-lifecycle-native-fixture)
    let boundary = ((($root | path join artifacts application-isolation-package) | path expand) | path join bin lumen-application-isolation)
    let font = (($root | path join artifacts owned-font-config) | path expand)
    let driver = (($root | path join artifacts software-driver) | path expand)
    let legacy = ((($root | path join artifacts toplevel-identity-legacy-compositor) | path expand) | path join bin sway)
    let assignments = [
        $'LUMEN_IDENTITY_WIRE=($wire)'
        $'FONTCONFIG_FILE=($font)'
        $'__EGL_VENDOR_LIBRARY_FILENAMES=($driver)/share/glvnd/egl_vendor.d/50_mesa.json'
        $'LIBGL_DRIVERS_PATH=($driver)/lib/dri'
        'GALLIUM_DRIVER=llvmpipe'
    ]
    for case in [
        {name: basic, provider: $basic, scenario: identity, flags: []}
        {name: effects, provider: $effects, scenario: identity, flags: [--effects]}
        {name: absent, provider: $legacy, scenario: identity-absent, flags: []}
    ] {
        let output = ($root | path join artifacts $'native-identity-($case.name)')
        mkdir $output
        ^nu tests/ci/run-native.nu $'toplevel_identity_($case.name)' -- $boundary run --source $root --output $output --software-graphics -- $environment ...$assignments $fixture $case.scenario --launcher $launcher --sway $case.provider ...$case.flags --output /artifacts/result
        let result = (open ($output | path join result result.json))
        {
            sourceRevision: (^git rev-parse HEAD | str trim)
            launcher: $launcher
            provider: $case.provider
            providerSha256: (open --raw $case.provider | hash sha256)
            quickshell: $result.quickshell
            quickshellSha256: (open --raw $result.quickshell | hash sha256)
            wire: $wire
            wireSha256: (open --raw $wire | hash sha256)
            font: $font
            softwareDriver: $driver
            scope: $result.scope
        } | to json | save -f ($output | path join exact-pins.json)
    }
}
