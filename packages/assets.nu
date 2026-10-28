def main [source: path, wallpaper: path, mark: string, icons: string] {
    ^cp -r $source $env.out
    ^chmod -R u+w $env.out
    ^cp --remove-destination $wallpaper ($env.out | path join shell wallpaper.png)
    if not ($mark | is-empty) {
        ^cp --remove-destination $mark ($env.out | path join shell icons lumen.svg)
    }
    if not ($icons | is-empty) {
        ^cp -r $"($icons)/." ($env.out | path join shell reference-icons)
    }
}
