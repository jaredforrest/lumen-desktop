def --wrapped main [...args: string] {
    let input = $in
    let root = $env.FIXTURE_ROOT
    match $args.0 {
        'unlocked' => { if ($root | path join locked | path exists) { exit 1 } }
        'sync' => {}
        'list' => { if ($root | path join item.json | path exists) { print "11111111-1111-1111-1111-111111111111\tWi-Fi" } }
        'get' => { open --raw ($root | path join item.json) | print --no-newline }
        'add' => {
            let data = ($input | lines)
            {password: $data.0, notes: ($data | skip 1 | str join "\n")} | to json | save ($root | path join uploaded.json)
        }
        _ => { error make {msg:'Unexpected vault operation'} }
    }
}
