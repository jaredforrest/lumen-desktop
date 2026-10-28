def main [action: string, key?: string, value?: string] {
    let file = $env.LUMEN_NIX_SETTINGS_ACTIVE
    if $action == 'read' {
        print (open --raw $file)
        return
    }

    let denied = ($file | str replace -r '\.json$' '.deny')
    if ($denied | path exists) {
        print ({error: 'Fixture persistence refused'} | to json --raw)
        exit 1
    }

    let values = (open $file)
    let updated = ($values | upsert $key ($value | from json))
    let pending = ($file | str replace -r '\.json$' '.new')
    $updated | to json --raw | save --force $pending
    mv --force $pending $file
    print ($updated | to json --raw)
}
