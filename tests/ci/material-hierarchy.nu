def main [] {
    mkdir artifacts
    ^nix build .#shell --out-link artifacts/material-shell --option abort-on-warn true
    ^nix build .#compositor-basic --out-link artifacts/material-basic --option abort-on-warn true
    ^nix build .#compositor --out-link artifacts/material-effects --option abort-on-warn true
    ^nix build .#test-application-font --out-link artifacts/material-font --option abort-on-warn true
    ^nix build .#test-application-software-driver --out-link artifacts/material-driver --option abort-on-warn true
    ^nix build .#test-material-hierarchy-fixture --out-link artifacts/material-hierarchy-fixture --option abort-on-warn true
    ^nu tests/ci/prepare-software-effects.nu artifacts/material-software-effects
    let root = $env.PWD
    let launcher = ($root | path join artifacts material-shell bin lumen-desktop-shell)
    let font = ($root | path join artifacts material-font share fonts truetype)
    let driver = ($root | path join artifacts material-driver)
    for variant in [basic effects] {
        let flags = if $variant == 'effects' { ['--effects'] } else { [] }
        let sway = ($root | path join artifacts $'material-($variant)' bin sway)
        let output = $'artifacts/material-hierarchy-($variant)'
        ^nu tests/ci/run-native.nu $'material_hierarchy_($variant)' -- ($root | path join artifacts material-hierarchy-fixture bin lumen-material-hierarchy-fixture) --source $root --launcher $launcher --sway $sway --font-directory $font --software-driver $driver ...$flags --output $output
    }
}
