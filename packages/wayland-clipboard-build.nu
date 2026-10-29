def main [kind: string, source: path, protocol: path] {
    cd $env.TMPDIR
    let header = if $kind == 'file' { 'data-control.h' } else if $kind == 'observer' { 'wlr-data-control-client-protocol.h' } else { error make {msg: 'Unknown clipboard component'} }
    let generated = if $kind == 'file' { 'data-control.c' } else { 'protocol.c' }
    let binary = if $kind == 'file' { 'lumen-file-clipboard' } else { 'lumen-clipboard-observer' }

    ^wayland-scanner client-header $protocol $header
    ^wayland-scanner private-code $protocol $generated
    let flags = (^pkg-config --cflags --libs wayland-client | str trim | split row ' ' | where {|flag| $flag != '' })
    if $kind == 'file' {
        ^cc -O2 -Wall -I. $source $generated ...$flags -o $binary
    } else {
        ^cc -std=c11 -Wall -Wextra -Werror -I. $source $generated ...$flags -o $binary
    }
    let destination = ($env.out | path join bin)
    mkdir $destination
    ^cp $binary ($destination | path join $binary)
}
