def main [] {
    let response = (^dbus-send --session --print-reply --dest=org.freedesktop.DBus /org/freedesktop/DBus org.freedesktop.DBus.ListActivatableNames | complete)
    if $response.exit_code != 0 {
        error make {msg: $'Private bus query failed: ($response.stderr)'}
    }
    let names = ($response.stdout | lines | each {|line| $line | str trim } | where {|line| $line | str starts-with 'string "' } | each {|line| $line | parse 'string "{name}"' | get 0.name })
    if $names != ['org.freedesktop.DBus'] {
        error make {msg: $'Private bus exposed activatable services: ($names | to json)'}
    }
    mkdir $env.out
}
