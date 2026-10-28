def --wrapped main [...args: string] {
    let extra = if (($env.LUMEN_XWAYLAND_NO_XRES? | default '0') == '1') { ['-extension' 'X-Resource'] } else { [] }
    exec $env.LUMEN_XWAYLAND_REAL ...$extra ...$args
}
