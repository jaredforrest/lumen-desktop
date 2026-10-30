# Exercise the actual card/handle and guarded family transfer in a disposable seat.
def main [launcher: string, basic: string, effects: string, --scope: string = 'all'] {
    if not ($scope in [all unsupported projection-legacy]) {error make {msg:'Unknown transfer acceptance scope'}}
    let root = $env.PWD
    ^nix build .#test-space-transfer-environment --out-link artifacts/space-transfer-environment --option abort-on-warn true --option eval-cache false
    ^nix build .#test-native-focus-legacy-compositor --out-link artifacts/space-transfer-legacy-focus-provider --option abort-on-warn true --option eval-cache false
    ^nix build .#test-display-spaces-legacy-compositor --out-link artifacts/space-transfer-legacy-display-provider --option abort-on-warn true --option eval-cache false
    ^nix build .#test-all-desktops-legacy-compositor --out-link artifacts/space-transfer-legacy-projection-provider --option abort-on-warn true --option eval-cache false
    let tools = ('artifacts/space-transfer-environment/bin' | path expand)
    let environment = ($tools | path join env)
    let boundary = ('artifacts/application-isolation-package/bin/lumen-application-isolation' | path expand)
    let fixture = ('artifacts/native-groups-fixture/bin/lumen-native-groups-fixture' | path expand)
    let font = ('artifacts/owned-font-config' | path expand)
    let driver = ('artifacts/software-driver' | path expand)
    let client = ('artifacts/window-relationships-client/bin/lumen-test-window-relationships' | path expand)
    let pointer = ('artifacts/pointer/bin/wlrctl' | path expand)
    let locker = ('artifacts/session-lock/bin/lumen-test-session-lock' | path expand)
    let pinned_launcher = ($launcher | path expand)
    for helper in [$environment $boundary $fixture $font $client $pointer $locker $pinned_launcher ($tools | path join dbus-daemon) ($tools | path join grim) ($tools | path join wtype) ($tools | path join sh)] {
        if not ($helper | path exists) {error make {msg:$'Missing immutable transfer helper ($helper)'}}
    }
    for case in [
        {name: basic, provider: ($basic | path expand), flags: [], scenario: desktop-transfer}
        {name: effects, provider: ($effects | path expand), flags: [--effects], scenario: desktop-transfer}
        {name: layer-basic, provider: ($basic | path expand), flags: [], scenario: desktop-transfer-layer}
        {name: layer-effects, provider: ($effects | path expand), flags: [--effects], scenario: desktop-transfer-layer}
        {name: legacy-projection, provider: ('artifacts/space-transfer-legacy-projection-provider/bin/sway' | path expand), flags: [], scenario: desktop-transfer}
        {name: legacy-focus, provider: ('artifacts/space-transfer-legacy-focus-provider/bin/sway' | path expand), flags: [], scenario: desktop-transfer-unsupported}
        {name: legacy-display, provider: ('artifacts/space-transfer-legacy-display-provider/bin/sway' | path expand), flags: [], scenario: desktop-transfer-unsupported}
    ] {
        if $scope == unsupported and $case.scenario != desktop-transfer-unsupported {continue}
        if $scope == projection-legacy and $case.name != legacy-projection {continue}
        if not ($case.provider | path exists) {error make {msg:'Missing immutable transfer provider'}}
        let output = ($root | path join artifacts $'desktop-transfer-($case.name)')
        if ($output | path exists) {error make {msg:'Transfer output must be fresh'}}
        mkdir $output
        ^nu tests/ci/run-native.nu $'desktop_transfer_($case.name)' -- $boundary run --source $root --output $output --private-pty --software-graphics -- $environment $'PATH=($tools)' $'FONTCONFIG_FILE=($font)' $'__EGL_VENDOR_LIBRARY_FILENAMES=($driver)/share/glvnd/egl_vendor.d/50_mesa.json' $'LIBGL_DRIVERS_PATH=($driver)/lib/dri' GALLIUM_DRIVER=llvmpipe $fixture $case.scenario ...$case.flags --launcher $pinned_launcher --sway $case.provider --client $client --pointer-tool $pointer --locker $locker --output /artifacts/result
        {source: (^git rev-parse HEAD | str trim), launcher: $pinned_launcher, provider: $case.provider, fixture: $fixture, font: $font, softwareDriver: $driver, environment: $environment, client: $client, pointer: $pointer, locker: $locker} | to json | save -f ($output | path join exact-pins.json)
    }
}
