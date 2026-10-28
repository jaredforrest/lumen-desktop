def main [source: path] {
    $env.HOME = ($env.TMPDIR | path join 'home')
    mkdir $env.HOME

    let launcher = (which lumen-desktop-shell | get path | first)
    ^lumen-input-pane-fixture --launcher $launcher --assets ($source | path join res lumen-desktop)
    let provider = (which lumen-portal-permission-provider-fixture | get path | first)
    ^lumen-portal-pane-fixture --launcher $launcher --assets ($source | path join res lumen-desktop) --provider $provider
    let permission_client = (which lumen-portal-permissions | get path | first)
    ^lumen-portal-permission-protocol-fixture --provider $provider --client $permission_client

    mkdir $env.out
}
