def main [...launchers: string] {
    for launcher in $launchers {
        let result = (^nu --ide-check 100 $launcher | complete)
        let diagnostics = ($result.stdout | lines | each {|line| $line | from json } | where type != hint)
        if $result.exit_code != 0 or not ($diagnostics | is-empty) {
            error make {msg: $'Nushell launcher did not parse: ($launcher) ($result.stdout) ($result.stderr)'}
        }
    }
    mkdir $env.out
    let private = ($env.out | path join private)
    mkdir $private
    $env.HOME = $private
    $env.XDG_STATE_HOME = ($private | path join state)
    $env.XDG_CONFIG_HOME = ($private | path join config)
    let runtime = ($private | path join runtime)
    mkdir $runtime
    ^chmod 700 $runtime
    $env.XDG_RUNTIME_DIR = $runtime
    let clipboard = ($launchers | where {|launcher| $launcher | str ends-with '/lumen-clipboard-history'} | first)
    let probe = (^$clipboard status | complete)
    if $probe.exit_code != 0 {
        error make {msg: $'Clipboard launcher status failed: ($probe.stdout) ($probe.stderr)'}
    }
    let status = ($probe.stdout | from json)
    if $status.enabled or $status.retentionSeconds != 3600 or $status.maxItems != 100 {
        error make {msg: 'Clipboard launcher did not forward the private status command and configured defaults'}
    }
    let audio = ($launchers | where {|launcher| $launcher | str ends-with '/lumen-audio-key'} | first)
    if (^$audio invalid | complete).exit_code != 2 {
        error make {msg: 'Audio launcher accepted an unsupported key'}
    }
    let action = ($launchers | where {|launcher| $launcher | str ends-with '/lumen-desktop-action'} | first)
    if (^$action invalid | complete).exit_code != 2 {
        error make {msg: 'Desktop action launcher accepted an unsupported action'}
    }
    let reader = ($launchers | where {|launcher| $launcher | str ends-with '/lumen-reader-command'} | first)
    let rejection = (^$reader invalid | complete)
    if $rejection.exit_code != 1 or ($rejection.stdout | from json).reason != 'unsupported_command' {
        error make {msg: 'Reader launcher did not reject an unsupported command with the expected response'}
    }
    if (^id -u | str trim) != '0' {
        let activation = ($launchers | where {|launcher| $launcher | str ends-with '/lumen-auth-activate-login'} | first)
        let refusal = (^$activation | complete)
        if $refusal.exit_code != 1 or not ($refusal.stderr | str contains 'administrator privileges') {
            error make {msg: 'Login activation did not refuse an unprivileged caller before writing service configuration'}
        }
    }
}
