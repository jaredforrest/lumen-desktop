def --wrapped main [...args: string] {
    let input = $in
    let root = $env.FIXTURE_ROOT
    if $args == ['--terse' '--fields' 'UUID,TYPE' 'connection' 'show'] {
        if not ($root | path join no-local | path exists) { print '22222222-2222-2222-2222-222222222222:802-11-wireless' }
    } else if ($args | any {|a| $a == '--get-values'}) {
        let field = $args.4
        match $field {
            '802-11-wireless.ssid' => { print 'Fixture Wi-Fi' }
            '802-11-wireless-security.key-mgmt' => { print 'wpa-psk' }
            '802-11-wireless-security.psk' => { open --raw ($root | path join cached.txt) | print --no-newline }
            _ => { error make {msg:'Unexpected property'} }
        }
    } else if $args.2 == 'connection' and $args.3 == 'edit' {
        let commands = ($input | lines)
        if $commands.1 != 'save persistent' or $commands.2 != 'quit' { error make {msg:'Unexpected editor commands'} }
        $commands.0 | str replace 'set 802-11-wireless-security.psk ' '' | save --raw --force ($root | path join cached.txt)
    } else if $args.0 == 'connection' and $args.1 == 'add' {
        $args.5 | save --raw ($root | path join created-profile)
    } else if $args.0 == 'connection' and $args.1 == 'modify' {
        if $args.4 != 'connection.autoconnect' or $args.5 != 'yes' { error make {msg:'Unexpected profile change'} }
        'enabled' | save --raw ($root | path join autoconnect)
    } else { error make {msg:'Unexpected network operation'} }
}
