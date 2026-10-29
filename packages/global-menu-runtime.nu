def main [adapter: path, exporter: path, bridge: path] {
    let directory = ($env.out | path join share global-menu)
    mkdir $directory
    ^cp $adapter ($directory | path join menu-adapter.cjs)
    ^ln -s $exporter ($directory | path join export-helper)
    ^ln -s $bridge ($directory | path join bridge)
}
