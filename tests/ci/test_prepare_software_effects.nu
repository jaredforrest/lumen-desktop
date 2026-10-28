def require [condition: bool, message: string] {
    if not $condition { error make {msg: $message} }
}

def main [] {
    let root = ($env.FILE_PWD | path dirname | path dirname)
    let script = ($root | path join tests ci prepare-software-effects.nu)
    let workspace = (^mktemp -d | str trim)
    for case in [
        {name: builtin, present: true, appears: false, success: true}
        {name: missing, present: false, appears: false, success: false}
        {name: late, present: false, appears: true, success: true}
    ] {
        let folder = ($workspace | path join $case.name)
        mkdir $folder
        let device = ($folder | path join device)
        let calls = ($folder | path join calls)
        let sudo = ($folder | path join sudo)
        let artifacts = ($folder | path join artifacts)
        let stub = [
            '#!/usr/bin/env nu'
            'def main [...args: string] {'
            '  ($args | str join " ") | save --append $env.CALLS'
            '  "\n" | save --append $env.CALLS'
            '  if ($args | first) == "modprobe" {'
            '    if $env.APPEARS == "1" { ^ln -s /dev/null $env.DEVICE }'
            '    exit 1'
            '  }'
            '}'
        ] | str join (char nl)
        $stub | save $sudo
        ^chmod '0755' $sudo
        if $case.present { ^ln -s /dev/null $device }
        let environment = {PATH: ($env.PATH | prepend $folder), CALLS: $calls, DEVICE: $device, APPEARS: (if $case.appears {'1'} else {'0'})}
        let result = (with-env $environment { ^nu $script $artifacts --device $device | complete })
        require (($artifacts | path join kernel.txt | path exists)) $'Missing kernel diagnostic for ($case.name)'
        require (($result.exit_code == 0) == $case.success) $'Wrong provisioning result for ($case.name): ($result.exit_code)'
        let invoked = (if ($calls | path exists) { open --raw $calls } else { '' })
        if $case.present {
            require (not ($invoked | str contains 'modprobe')) 'Built-in device unexpectedly loaded a module'
        } else {
            require ($invoked | str contains 'modprobe udmabuf') 'Missing device did not attempt a module load'
        }
        if $case.success {
            require ($invoked | str contains 'chmod 0600') 'Device mode was not secured'
        } else {
            require (not ($invoked | str contains 'chmod')) 'Missing device was modified'
        }
    }
    rm --recursive $workspace
    print 'PASS disposable software-effects provisioning branches'
}
