def main [root: path, base_path: string] {
    let root = ($root | path expand)
    let active = ($root | path join settings.json)
    '{"inputs":{}}' | save --force $active
    let bin = ($root | path join bin)
    mkdir $bin
    let command = ($bin | path join lumen-nix-settings)
    let interpreter = (which nu | get path | first)
    let adapter = ($env.FILE_PWD | path join settings_adapter.nu)
    [$'#!($interpreter)' (open --raw $adapter)] | str join (char nl) | save --force $command
    ^chmod 755 $command
    {LUMEN_NIX_SETTINGS_ACTIVE: $active, PATH: $'($bin):($base_path)'} | to json --raw
}
