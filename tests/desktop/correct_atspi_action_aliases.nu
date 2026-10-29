def fail [message: string] { error make {msg: $message} }

def method_header [action: string, name: string] {
    let newline = (char newline)
    let prefix = $'<method name="($name)"($newline)              c:identifier="atspi_action_($name)"'
    let parts = ($action | split row $prefix)
    if ($parts | length) != 2 { fail $'Expected one Action method ($name)' }
    $parts | get 1 | split row '>' | first
}

def main [source: path, target: path, proof: path] {
    let original = (open --raw $source)
    let sections = ($original | split row '<interface name="Action"')
    if ($sections | length) != 2 { fail 'Expected one Action interface' }
    let action = ($sections | get 1 | split row '</interface>' | first)
    let newline = (char newline)
    mut fixed = $original
    mut changes = []

    for pair in [
        {canonical: get_action_name, legacy: get_name}
        {canonical: get_action_description, legacy: get_description}
    ] {
        let canonical = $pair.canonical
        let legacy = $pair.legacy
        let modern = (method_header $action $canonical)
        let old = (method_header $action $legacy)
        if not ($modern | str contains $'shadowed-by="($legacy)"') or ($modern | str contains 'deprecated=') {
            fail $'Unexpected metadata on Action.($canonical)'
        }
        if not ($old | str contains $'shadows="($canonical)"') or not ($old | str contains 'deprecated="1"') or not ($old | str contains 'deprecated-version="2.10"') {
            fail $'Unexpected deprecation metadata on Action.($legacy)'
        }

        let modern_prefix = $'<method name="($canonical)"($newline)              c:identifier="atspi_action_($canonical)"'
        let modern_alias = $'($modern_prefix)($newline)              shadowed-by="($legacy)"'
        if (($fixed | split row $modern_alias | length) != 2) { fail $'Expected one shadowed-by attribute on ($canonical)' }
        $fixed = ($fixed | str replace $modern_alias $modern_prefix)
        $changes = ($changes | append {method: $canonical, removed: 'shadowed-by', value: $legacy})

        let old_prefix = $'<method name="($legacy)"($newline)              c:identifier="atspi_action_($legacy)"'
        let old_alias = $'($old_prefix)($newline)              shadows="($canonical)"'
        if (($fixed | split row $old_alias | length) != 2) { fail $'Expected one shadows attribute on ($legacy)' }
        $fixed = ($fixed | str replace $old_alias $old_prefix)
        $changes = ($changes | append {method: $legacy, removed: 'shadows', value: $canonical})
    }

    $fixed | save --raw --force $target
    {sourceSha256: ($original | hash sha256), correctedSha256: ($fixed | hash sha256), changes: $changes, deprecatedAliasesRetained: true}
        | to json --indent 2 | save --force $proof
}
