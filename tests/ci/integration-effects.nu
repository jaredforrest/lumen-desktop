def main [] {
    mkdir artifacts
    ^nix build .#compositor --out-link artifacts/compositor-effects --option abort-on-warn true
    ^nu tests/ci/prepare-software-effects.nu artifacts/software-effects-kernel
    ^nix build .#test-effects-software-render --out-link artifacts/effects-software-render-bin --option abort-on-warn true
    let root = $env.PWD
    let launcher = ($root | path join artifacts shell-package bin lumen-desktop-shell)
    let effects = ($root | path join artifacts compositor-effects bin sway)
    let render = ($root | path join artifacts effects-software-render-bin bin lumen-effects-software-render)
    ^nu tests/ci/run-native.nu effects_software_render -- $render --sway $effects --output artifacts/effects-software-render
    ^nix build .#shell-runtime --out-link artifacts/shell-runtime --option abort-on-warn true
    ^nix build .#test-typography --out-link artifacts/typography-probe --option abort-on-warn true
    ^nix build .#test-typography-corpus --out-link artifacts/typography-corpus --option abort-on-warn true
    ^nix build .#test-window-layout-menu-fixture --out-link artifacts/window-layout-menu-fixture --option abort-on-warn true
    ^nix build .#test-window-overview-fixture --out-link artifacts/window-overview-fixture --option abort-on-warn true
    ^nix build .#appearance-runtime --out-link artifacts/appearance-runtime --option abort-on-warn true
    ^nix build .#test-live-appearance-fixture --out-link artifacts/live-appearance-fixture --option abort-on-warn true
    ^nix build .#test-installed-smoke-fixture --out-link artifacts/installed-smoke-fixture --option abort-on-warn true
    let probe = ($root | path join artifacts typography-probe bin typography-probe)
    let corpus = ($root | path join artifacts typography-corpus bin lumen-typography-corpus)
    ^nu tests/ci/run-native.nu typography_corpus -- $corpus --launcher $launcher --probe $probe --output artifacts/typography
    ^nix build .#compositor-basic --out-link artifacts/compositor --option abort-on-warn true
    let sway = ($root | path join artifacts compositor bin sway)
    ^nu tests/ci/run-native.nu live_appearance -- ($root | path join artifacts live-appearance-fixture bin lumen-live-appearance-fixture) --launcher $launcher --sway $sway --appearance ($root | path join artifacts appearance-runtime bin lumen-appearance) --output artifacts/live-appearance
    ^nu tests/ci/run-native.nu window_layout_menu_qml -- ($root | path join artifacts window-layout-menu-fixture bin lumen-window-layout-menu-fixture) --launcher $launcher --sway $sway --output artifacts/window-layout-menu
    ^nu tests/ci/run-native.nu window_overview_lifecycle -- ($root | path join artifacts window-overview-fixture bin lumen-window-overview-fixture) --launcher $launcher --sway $sway --output artifacts/overview-lifecycle
    ^nu tests/ci/run-native.nu smoke_installed -- ($root | path join artifacts installed-smoke-fixture bin lumen-installed-smoke-fixture) --launcher $launcher --sway $sway --output artifacts/shell
}
