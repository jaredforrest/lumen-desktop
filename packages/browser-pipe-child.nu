def main [source: path, invocation: path] {
    let target = ($env.out | path join bin)
    mkdir $target
    ^rustc --edition=2024 -C opt-level=2 $source -o ($target | path join lumen-browser-pipe-child)
    let test_binary = ($env.TMPDIR | path join browser-invocation-tests)
    ^rustc --edition=2024 --test $invocation -o $test_binary
    ^$test_binary
    ^rustc --edition=2024 -C opt-level=2 $invocation -o ($target | path join lumen-browser-invocation)
}
