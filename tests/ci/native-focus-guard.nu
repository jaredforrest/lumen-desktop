def artifact [path: string] { $env.PWD | path join artifacts $path }
def build [attribute: string, output: string] {
    ^nix build $'.#($attribute)' --out-link (artifact $output) --option abort-on-warn true --option eval-cache false --max-jobs 1 --cores 2
}
def main [] {
    mkdir artifacts
    build default guard-shell
    build test-lifecycle-native-fixture guard-fixture
    build test-window-relationships guard-client
    build compositor-basic guard-basic
    build compositor guard-effects
    build test-native-focus-legacy-compositor guard-legacy
    for case in [
        {name: basic, provider: guard-basic, scenario: focus-guard, flags: []}
        {name: effects, provider: guard-effects, scenario: focus-guard, flags: ['--effects']}
        {name: unsupported, provider: guard-legacy, scenario: focus-guard-unsupported, flags: []}
    ] {
        ^nu tests/ci/run-native.nu $'native_focus_guard_($case.name)' -- (artifact 'guard-fixture/bin/lumen-lifecycle-native-fixture') $case.scenario ...$case.flags --launcher (artifact 'guard-shell/bin/lumen-desktop-shell') --sway (artifact $'($case.provider)/bin/sway') --relationships-client (artifact 'guard-client/bin/lumen-test-window-relationships') --output (artifact $'native-focus-guard-($case.name)')
    }
}
