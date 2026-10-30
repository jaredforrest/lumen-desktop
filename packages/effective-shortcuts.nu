def main [validator: path, defaults: path, declarations: path, requested: path] {
    let base = (^$validator $defaults | complete)
    if $base.exit_code != 0 {
        error make {msg: $'Default shortcut registry failed validation: ($base.stderr)'}
    }
    let proposed = (^$validator $requested | complete)
    if $proposed.exit_code != 0 { error make {msg: $'Requested shortcut changes failed validation: ($proposed.stderr)'} }
    ^$validator $declarations --output $env.out
    if $env.LAST_EXIT_CODE != 0 {
        error make {msg: 'Effective shortcut registry failed validation'}
    }
}
