def main [configuration: path, catalog: path] {
    $env.XML_CATALOG_FILES = $catalog
    let validation = (^xmllint --valid --noout $configuration | complete)
    if $validation.exit_code != 0 or not ($validation.stderr | str trim | is-empty) {
        error make {msg: $'Font configuration validation failed: ($validation.stderr)'}
    }
    mkdir $env.out
}
