def ensure [condition: bool, message: string] {
    if not $condition { error make {msg: $message} }
}

def fullscreen_actions [document: record] {
    $document.bindings | where { |row| $row.action == 'window.fullscreen.toggle' }
}

def fullscreen_hints [document: record] {
    $document.hints | where { |row| $row.action == 'window.fullscreen.toggle' }
}

def main [default_bundle: path, rebound_bundle: path, removed_bundle: path, replaced_bundle: path] {
    let output = $env.out
    mkdir $output
    cp $default_bundle ($output | path join default.json)
    cp $rebound_bundle ($output | path join rebound.json)
    cp $removed_bundle ($output | path join removed.json)
    cp $replaced_bundle ($output | path join replaced.json)

    let default = (open $default_bundle)
    let old = (open $default.registry)
    let rebound = (open $rebound_bundle)
    let new = (open $rebound.registry)
    ensure ($old.valid and $new.valid) 'Generated shortcut registries must be valid'
    let old_action = (fullscreen_actions $old)
    let new_action = (fullscreen_actions $new)
    ensure (($old_action | length) == 1 and $old_action.0.chord == 'Mod4+Control+f') 'Default fullscreen chord changed'
    ensure (($new_action | length) == 1 and $new_action.0.chord == 'Mod4+Control+x') 'Rebound fullscreen chord missing'
    ensure (($default.dispatch | get 'Mod4+Control+f') == ($rebound.dispatch | get 'Mod4+Control+x')) 'Rebind changed command identity'
    ensure (not ('Mod4+Control+f' in ($rebound.dispatch | columns))) 'Rebound registry retained old dispatch'
    let new_hints = (fullscreen_hints $new)
    ensure (($new_hints | length) == 1 and $new_hints.0.bindings.0.chord == 'Mod4+Control+x') 'Rebound hint is stale'
    for bundle in [$removed_bundle $replaced_bundle] {
        let value = (open $bundle)
        ensure ($value.valid and ((fullscreen_actions $value) | is-empty) and ((fullscreen_hints $value) | is-empty)) 'Removed or replaced shortcut retained authority'
    }
    print 'Actual merged binding identities and generated hints agree after rebind, disable and replacement'
}
