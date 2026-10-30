# Real full-shell preview and dispatch decisions in disposable owned sessions.
def main [launcher: string, basic: string, effects: string] {
    let root = $env.PWD
    ^nix build .#test-window-inventory-environment --out-link artifacts/window-inventory-environment --option abort-on-warn true
    let environment = (($root | path join artifacts window-inventory-environment) | path expand | path join bin env)
    let boundary = (($root | path join artifacts application-isolation-package) | path expand | path join bin lumen-application-isolation)
    let fixture = (($root | path join artifacts lifecycle-native-fixture) | path expand | path join bin lumen-lifecycle-native-fixture)
    let font = (($root | path join artifacts owned-font-config) | path expand)
    let driver = (($root | path join artifacts software-driver) | path expand)
    let assignments = [
        $'PATH=($environment | path dirname)'
        $'FONTCONFIG_FILE=($font)'
        $'__EGL_VENDOR_LIBRARY_FILENAMES=($driver)/share/glvnd/egl_vendor.d/50_mesa.json'
        $'LIBGL_DRIVERS_PATH=($driver)/lib/dri'
        'GALLIUM_DRIVER=llvmpipe'
    ]
    for case in [
        {name: basic, scenario: window-mru-inventory, provider: ($basic | path expand), flags: []}
        {name: effects, scenario: window-mru-inventory, provider: ($effects | path expand), flags: [--effects]}
        {name: window-mru-basic, scenario: window-mru, provider: ($basic | path expand), flags: []}
        {name: window-mru-effects, scenario: window-mru, provider: ($effects | path expand), flags: [--effects]}
    ] {
        let output = ($root | path join artifacts $'native-window-inventory-($case.name)')
        mkdir $output
        ^nu tests/ci/run-native.nu $'window_inventory_($case.name)' -- $boundary run --source $root --output $output --software-graphics -- $environment ...$assignments $fixture $case.scenario --launcher ($launcher | path expand) --sway $case.provider ...$case.flags --output /artifacts/result
        if not (open ($output | path join result result.json)).passed { error make {msg: 'Window inventory decisions failed'} }
        let absent = [
            'WARN: PulseAudioService: pa_context_connect() failed'
            'ERROR quickshell.service.pipewire.loop: Failed to connect pipewire context. Errno: 112'
            'WARN quickshell.bluetooth: Could not connect to DBus. Bluetooth integration is not available.'
        ]
        let diagnostics = (glob ($output | path join result 'desktop*.log') | each {|path|
            let normalized = (open --raw $path | ansi strip | lines | where {|line| $line =~ '\b(WARN|ERROR)\b' } | each {|line| $line | str trim | str replace --regex '\s+:' ':' })
            for diagnostic in $normalized {
                if not ($diagnostic in $absent) { error make {msg: $'Unexpected desktop diagnostic in ($path): ($diagnostic)'} }
            }
            for diagnostic in $absent {
                if ($normalized | where {|line| $line == $diagnostic } | length) > 1 { error make {msg: 'Repeated unavailable hardware diagnostics'} }
            }
            {source: $path, diagnostics: $normalized}
        })
        {logs: $diagnostics, allowed: $absent, reason: 'The disposable namespace has no audio graph, audio server or Bluetooth system service. These are unrelated to the native window identity and dispatch assertions; no host service is accessed.'} | to json | save -f ($output | path join unavailable-hardware-diagnostics.json)
        let renderer = (open --raw ($output | path join result sway.log))
        if ('--effects' in $case.flags) and not ($renderer | str contains 'llvmpipe') { error make {msg: 'Effects inventory fixture did not prove software renderer'} }
        {
            sourceRevision: (^git rev-parse HEAD | str trim)
            launcher: ($launcher | path expand)
            provider: $case.provider
            providerSha256: (open --raw $case.provider | hash sha256)
            fixture: $fixture
            fixtureSha256: (open --raw $fixture | hash sha256)
            font: $font
            softwareDriver: $driver
        } | to json | save -f ($output | path join exact-pins.json)
    }
}
