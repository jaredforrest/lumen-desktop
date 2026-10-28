def main [] {
    ^nix build .#test-network-provider --out-link artifacts/network-provider --option abort-on-warn true
    ^nix build .#network-locations --out-link artifacts/network-locations-client --option abort-on-warn true
    ^nix build .#test-network-provider-fixture --out-link artifacts/network-provider-fixture --option abort-on-warn true
    let provider = ($env.PWD | path join artifacts network-provider)
    let client = ($env.PWD | path join artifacts network-locations-client bin lumen-network-locations)
    let fixture = ($env.PWD | path join artifacts network-provider-fixture bin lumen-network-provider-fixture)
    let result = (^nu tests/ci/run-native.nu network_locations_provider -- $fixture --provider $provider --client $client | complete)
    let output = $result.stdout + $result.stderr
    $output | save --raw --force artifacts/network-provider.log
    print --no-newline $output
    if $result.exit_code != 0 { exit $result.exit_code }
}
