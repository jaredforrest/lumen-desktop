def main [source: path, protocol: path] {
    cd $env.TMPDIR
    ^wayland-scanner client-header $protocol input-method-client.h
    ^wayland-scanner private-code $protocol input-method-protocol.c
    let flags = (^pkg-config --cflags --libs gtk+-wayland-3.0 wayland-client xkbcommon | str trim | split row ' ' | where {|flag| $flag != '' })
    ^cc -shared -fPIC -O2 -Wall -Wextra -Wno-unused-parameter -I. $source input-method-protocol.c -o libcharacter-input.so ...$flags
    let destination = ($env.out | path join lib)
    mkdir $destination
    ^cp libcharacter-input.so ($destination | path join libcharacter-input.so)
}
