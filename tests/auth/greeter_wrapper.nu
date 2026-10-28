def main [greeter: path] {
    mkdir $env.out
    $env.HOME = $env.out
    $env.TMPDIR = $env.out
    $env.LUMEN_TEST_LOG = ($env.out | path join fallback.json)

    $env.LUMEN_TEST_SWAY_SUCCESS = "1"
    let accepted = (^$greeter | complete)
    if $accepted.exit_code != 0 or ($env.LUMEN_TEST_LOG | path exists) {
        error make {msg: $'Greeter did not accept a confirmed compositor session: ($accepted.stderr)'}
    }

    $env.LUMEN_TEST_SWAY_SUCCESS = "0"
    let fallback = (^$greeter | complete)
    if $fallback.exit_code != 0 or not ($env.LUMEN_TEST_LOG | path exists) {
        error make {msg: $'Greeter did not fall back after compositor failure: ($fallback.stderr)'}
    }
    let args = (open $env.LUMEN_TEST_LOG)
    if ($args | first) != "--time" or "--asterisks" not-in $args or "--cmd" not-in $args {
        error make {msg: 'Fallback greeter did not receive the expected options'}
    }
}
