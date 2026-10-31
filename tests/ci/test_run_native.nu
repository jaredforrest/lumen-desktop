def require [condition: bool, message: string] {
    if not $condition { error make {msg: $message} }
}

def main [] {
    let root = ($env.FILE_PWD | path dirname | path dirname)
    let runner = ($root | path join tests ci run-native.nu)
    let sources = [
        ($root | path join .github workflows check.yml)
        ($root | path join tests ci integration-smoke.nu)
        ($root | path join tests ci integration-effects.nu)
        ($root | path join tests ci integration-network.nu)
        ($root | path join tests ci integration-native.nu)
        ($root | path join tests ci material-hierarchy.nu)
    ]
    let invocations = ($sources | each {|source| open --raw $source | lines } | flatten | where { |line| ($line | str contains 'run-native.nu ') and ($line | str contains ' -- ') })
    require (($invocations | length) >= 40) 'Direct native workflow inventory is unexpectedly small'
    for line in $invocations {
        require (($line | str contains 'nix develop --command nu tests/ci/run-native.nu ') or ($line | str contains '^nu tests/ci/run-native.nu ')) 'Native invocation skipped the diagnostic guard'
        require ($line | str contains ' -- ') 'Native invocation lost the guarded command boundary'
    }
    let native = (open --raw ($root | path join tests ci integration-native.nu))
    require ($native | str contains '^nu tests/ci/run-native.nu gi_closure_callbacks -- (artifact') 'Rust Gio closure fixture lost its diagnostic guard'
    require ($native | str contains '^nu tests/ci/run-native.nu atspi_action_metadata -- (artifact') 'Rust GI action fixture lost its diagnostic guard'
    require ($native | str contains '^nu tests/ci/run-native.nu accessibility_protocol -- (artifact') 'Rust AT-SPI protocol fixture lost its diagnostic guard'
    require ($native | str contains "-- (artifact 'accessibility-workflows-fixture/bin/lumen-accessibility-workflows-fixture') screen-reader-input") 'Rust Orca fixture lost its diagnostic guard'
    require ($native | str contains '^nu tests/ci/run-native.nu battery_provider -- (artifact') 'Rust power fixture lost its diagnostic guard'
    require ($native | str contains '^nu tests/ci/run-native.nu network_connectivity_provider -- (artifact') 'Rust connectivity fixture lost its diagnostic guard'
    require ($native | str contains '^nu tests/ci/run-native.nu reader_commands_provider -- (artifact') 'Rust reader fixture lost its diagnostic guard'
    require ($native | str contains '^nu tests/ci/run-native.nu screen_reader_transport -- (artifact') 'Rust speech transport fixture lost its diagnostic guard'
    require ($native | str contains "-- (artifact 'settings-search-fixture/bin/lumen-settings-search-fixture')") 'Rust settings search fixture lost its diagnostic guard'
    require ($native | str contains '^nu tests/ci/run-native.nu force_quit_surface -- (artifact') 'Rust Force Quit fixture lost its diagnostic guard'
    require ($native | str contains '^nu tests/ci/run-native.nu calendar_card -- (artifact') 'Rust Calendar card fixture lost its diagnostic guard'
    require ($native | str contains '^nu tests/ci/run-native.nu calendar_window -- (artifact') 'Rust Calendar window fixture lost its diagnostic guard'
    require ($native | str contains '^nu tests/ci/run-native.nu calendar_popover -- (artifact') 'Rust Calendar popover fixture lost its diagnostic guard'
    let smoke = (open --raw ($root | path join tests ci integration-smoke.nu))
    require ($smoke | str contains '^nu tests/ci/run-native.nu calendar_toggle_contrast -- ($root | path join artifacts calendar-qa bin lumen-calendar-qa) toggle') 'Rust Calendar toggle fixture lost its diagnostic guard'
    let workspace = (^mktemp -d | str trim)
    for case in [
        {name: success, source: 'print "Synthetic success"', code: 0, expected: 'Synthetic success'}
        {name: failure, source: 'exit 7', code: 7, expected: ''}
        {name: warning, source: 'print "ResourceWarning: leaked"', code: 1, expected: 'ResourceWarning:'}
        {name: unraisable, source: 'print --stderr "Exception ignored while finalizing synthetic fixture"', code: 1, expected: 'Exception ignored while finalizing'}
    ] {
        let folder = ($workspace | path join $case.name)
        mkdir $folder
        cd $folder
        let result = (^nu $runner synthetic -- nu -c $case.source | complete)
        require ($result.exit_code == $case.code) $'Wrong native guard exit for ($case.name): ($result.exit_code)'
        let logs = (glob 'artifacts/native-command-logs/*.log')
        require (($logs | length) == 1) $'Wrong log count for ($case.name)'
        let log = (open --raw ($logs | first))
        require ($log | str contains $case.expected) $'Missing retained output for ($case.name)'
    }
    let inherited = ($workspace | path join ('long-inherited-temp-parent-' | fill --width 100 --character x))
    mkdir $inherited
    let folder = ($workspace | path join short-temp-policy)
    mkdir $folder
    cd $folder
    with-env {TMPDIR: $inherited} {
        let result = (^nu $runner short-temp -- nu -c 'print $env.TMPDIR' | complete)
        require ($result.exit_code == 0) 'Short native temp policy failed'
        require (($result.stdout | str trim) == '/tmp') 'Native child inherited an overlong temporary parent'
        require ($env.TMPDIR == $inherited) 'Native guard changed its caller temporary parent'
        let log = (glob 'artifacts/native-command-logs/*.log' | first | open --raw)
        require (($log | str trim) == '/tmp') 'Short native temp policy output was not retained'
    }
    cd $root
    rm --recursive $workspace
    print 'PASS native diagnostic guard and workflow coverage'
}
