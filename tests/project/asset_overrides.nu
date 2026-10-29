def assert_same [source: path, installed: path] {
    let result = (^cmp $source $installed | complete)
    if $result.exit_code != 0 {
        error make {msg: $'Asset differs from its override: ($installed)'}
    }
}

def assert_nonempty [file: path] {
    let result = (^test -s $file | complete)
    if $result.exit_code != 0 {
        error make {msg: $'Missing or empty asset: ($file)'}
    }
}

def main [icons: path, mark: path, wallpaper: path, assets: path] {
    assert_same ($icons | path join files.svg) ($assets | path join shell reference-icons files.svg)
    assert_same $mark ($assets | path join shell icons lumen.svg)
    assert_same $wallpaper ($assets | path join shell wallpaper.png)
    assert_nonempty ($assets | path join shell reference-icons calendar.svg)
    assert_nonempty ($assets | path join auth lock.qml)
    mkdir $env.out
}
