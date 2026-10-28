def main [assets: path] {
    mkdir $env.out
    for file in (glob $"($assets)/auth/*.qml") {
        ^cp $file $env.out
    }
    ^ln -s ($assets | path join shell wallpaper.png) ($env.out | path join wallpaper.png)
}
