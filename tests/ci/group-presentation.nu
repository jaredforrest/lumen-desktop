# The whole journey is disposable; artifact aliases are resolved before entry.
def main [launcher: string, basic: string, effects: string] {
    let root = $env.PWD
    ^nix build .#test-group-presentation-environment --out-link artifacts/group-presentation-environment --option abort-on-warn true --option eval-cache false
    let environment = (('artifacts/group-presentation-environment' | path expand) | path join bin env)
    let tools = (('artifacts/group-presentation-environment' | path expand) | path join bin)
    let boundary = (('artifacts/application-isolation-package' | path expand) | path join bin lumen-application-isolation)
    let fixture = (('artifacts/native-groups-fixture' | path expand) | path join bin lumen-native-groups-fixture)
    let font = ('artifacts/owned-font-config' | path expand)
    let driver = ('artifacts/software-driver' | path expand)
    let client = (('artifacts/window-relationships-client' | path expand) | path join bin lumen-test-window-relationships)
    let pointer = (('artifacts/pointer' | path expand) | path join bin wlrctl)
    let locker = (('artifacts/session-lock' | path expand) | path join bin lumen-test-session-lock)
    let pinned_launcher = ($launcher | path expand)
    for case in [
        {name: basic, provider: ($basic | path expand), flags: []}
        {name: effects, provider: ($effects | path expand), flags: [--effects]}
    ] {
        let output = ($root | path join artifacts $'group-presentation-($case.name)')
        mkdir $output
        ^nu tests/ci/run-native.nu $'group_presentation_($case.name)' -- $boundary run --source $root --output $output --private-pty --software-graphics -- $environment $'PATH=($tools)' $'FONTCONFIG_FILE=($font)' $'__EGL_VENDOR_LIBRARY_FILENAMES=($driver)/share/glvnd/egl_vendor.d/50_mesa.json' $'LIBGL_DRIVERS_PATH=($driver)/lib/dri' GALLIUM_DRIVER=llvmpipe $fixture presentation ...$case.flags --launcher $pinned_launcher --sway $case.provider --client $client --pointer-tool $pointer --locker $locker --output /artifacts/result
        {source: (^git rev-parse HEAD | str trim), launcher: $pinned_launcher, provider: $case.provider, fixture: $fixture, font: $font, softwareDriver: $driver, environment: $environment} | to json | save -f ($output | path join exact-pins.json)
    }
}
