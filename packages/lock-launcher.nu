def main [source: path] {
    let target = ($env.out | path join bin)
    mkdir $target
    ^rustc --edition=2024 -C opt-level=2 $source -o ($target | path join lumen-lock-launcher)
}
