def main [root: path] {
    let root = ($root | path expand)
    let brand = (([('a' + 'pple') ('m' + 'acos') ('m' + 'acintosh') ('(^|[^a-z])m' + 'ac([^a-z]|$)')] | str join '|'))
    let forbidden = '(?i)' + $brand
    let machine_path = '/home/' + 'jalio'
    mut errors = []
    for path in (glob $'($root)/**/*' --no-dir --no-symlink --exclude [**/.git/** **/__pycache__/** **/artifacts/** **/target/**]) {
        let relative = ($path | str replace $'($root)/' '')
        if ($relative =~ $forbidden) {
            $errors = ($errors | append $'Branded path: ($relative)')
        }
        let content = (try { open --raw $path | decode utf-8 } catch { null })
        if $content == null { continue }
        if ($content =~ $forbidden) {
            $errors = ($errors | append $'Branded text: ($relative)')
        }
        if ($content | str contains $machine_path) {
            $errors = ($errors | append $'Machine-specific path: ($relative)')
        }
        if ($path | str ends-with '.py') {
            $errors = ($errors | append $'First-party Python source restored: ($relative)')
        }
        if (['.sh' '.bash' '.fish' '.ps1' '.lua' '.rb' '.pl'] | any {|suffix| $path | str ends-with $suffix }) {
            $errors = ($errors | append $'Unapproved script source restored: ($relative)')
        }
        if ($path | str ends-with '.cjs') and $relative != 'res/lumen-desktop/global-menu/menu-adapter.cjs' {
            $errors = ($errors | append $'JavaScript outside the Electron adapter: ($relative)')
        }
        if ($path | str ends-with '.js') and not ($relative | str starts-with 'res/lumen-desktop/shell/') {
            $errors = ($errors | append $'JavaScript outside QML models: ($relative)')
        }
        if (['.cpp' '.cc' '.cxx' '.hpp' '.hh' '.hxx'] | any {|suffix| $path | str ends-with $suffix }) {
            $errors = ($errors | append $'First-party C++ source restored: ($relative)')
        }
        for hit in ($content | parse -r '(?m)^\s*\^?rustc\s+(?<arguments>[^\r\n]+)') {
            if $hit.arguments !~ '-D(?:warnings|\s+warnings)(?:\s|$)' {
                $errors = ($errors | append $'Standalone Rust compiler lacks warning denial: ($relative)')
            }
        }
        for hit in ($content | parse -r '(?s)Command::new\("rustc"\)\s*\.args\(\[(?<arguments>[^\]]*)\]') {
            if $hit.arguments !~ '"-Dwarnings"|"-D"\s*,\s*"warnings"' {
                $errors = ($errors | append $'Nested Rust compiler lacks warning denial: ($relative)')
            }
        }
        if ($path | str ends-with '.nix') {
            let upstream_rust_builders = [packages/wordcraft-source.nix packages/gridcraft-source.nix packages/deckcraft-source.nix]
            if ($content | str contains 'pkgs.rustPlatform.buildRustPackage') and $relative != 'packages/owned-rust.nix' and $relative not-in $upstream_rust_builders {
                $errors = ($errors | append $'Owned Rust package bypasses the shared warning policy: ($relative)')
            }
            for hit in ($content | parse -r '(?:import|callPackage)\s+(?<target>\.{1,2}/[\w./-]+)') {
                let imported = (($path | path dirname) | path join $hit.target)
                if not ($imported | path exists) {
                    $errors = ($errors | append $'Missing Nix import: ($relative) -> ($hit.target)')
                }
            }
        }
    }
    let capture_source = ($root | path join tests desktop capture-native-rust src main.rs)
    if not ($capture_source | path exists) or not ((open --raw $capture_source) | str contains 'lumen-fixture-providers') {
        $errors = ($errors | append 'Full-shell Rust fixture lost its private provider boundary: capture-native-rust')
    }
    let clock_source = ($root | path join tests desktop native-input-rust src bin clock.rs)
    if not ($clock_source | path exists) or not ((open --raw $clock_source) | str contains 'lumen-fixture-providers') {
        $errors = ($errors | append 'Full-shell Rust fixture lost its private provider boundary: native-input-rust clock')
    }
    for name in [fullscreen-menu-rust surface-matrix-rust icon-render-rust] {
        let path = ($root | path join tests desktop $name src main.rs)
        if not ($path | path exists) or not ((open --raw $path) | str contains 'lumen-fixture-providers') {
            $errors = ($errors | append $'Full-shell Rust fixture lost its private provider boundary: ($name)')
        }
    }
    let lifecycle_source = ($root | path join tests desktop lifecycle-native-rust src harness.rs)
    if not ($lifecycle_source | path exists) or not ((open --raw $lifecycle_source) | str contains 'lumen-fixture-providers') {
        $errors = ($errors | append 'Full-shell Rust lifecycle fixture lost its private provider boundary')
    }
    let popover_source = ($root | path join tests desktop system-popover-rust src main.rs)
    if not ($popover_source | path exists) or not ((open --raw $popover_source) | str contains 'lumen-fixture-providers') {
        $errors = ($errors | append 'System-popover fixture lost its private provider boundary')
    }
    let owned_context_source = ($root | path join tests desktop owned-window-context-rust src main.rs)
    if not ($owned_context_source | path exists) or not ((open --raw $owned_context_source) | str contains 'lumen-fixture-providers') {
        $errors = ($errors | append 'Owned window context fixture lost its private provider boundary')
    }
    let search_history_source = ($root | path join tests desktop search-history-rust src main.rs)
    if not ($search_history_source | path exists) or not ((open --raw $search_history_source) | str contains 'lumen-fixture-providers') {
        $errors = ($errors | append 'Search-history fixture lost its private provider boundary')
    }
    let consumer_fit_source = ($root | path join tests desktop consumer-fit-rust src main.rs)
    if not ($consumer_fit_source | path exists) or not ((open --raw $consumer_fit_source) | str contains 'calendar_helper') {
        $errors = ($errors | append 'Consumer-fit fixture lost its Rust calendar responder')
    }
    let switcher_source = ($root | path join tests desktop switcher-rust src installed.rs)
    if not ($switcher_source | path exists) or not ((open --raw $switcher_source) | str contains 'lumen-fixture-providers') {
        $errors = ($errors | append 'Installed switcher fixture lost its private provider boundary')
    }
    let shortcut_source = ($root | path join tests desktop shortcut-hints-rust src main.rs)
    let menu_source = ($root | path join tests desktop menu-native-rust src harness.rs)
    if not ($menu_source | path exists) or not ((open --raw $menu_source) | str contains 'lumen-fixture-providers') {
        $errors = ($errors | append 'Full-shell Rust menu fixture lost its private provider boundary')
    }
    if not ($shortcut_source | path exists) or not ((open --raw $shortcut_source) | str contains 'lumen-fixture-providers') {
        $errors = ($errors | append 'Full-shell Rust shortcut fixture lost its private provider boundary')
    }
    let smoke_source = ($root | path join tests desktop smoke-rust src main.rs)
    if not ($smoke_source | path exists) or not ((open --raw $smoke_source) | str contains 'lumen-fixture-providers') or not ((open --raw $smoke_source) | str contains 'LUMEN_FILE_JOBS_BIN') {
        $errors = ($errors | append 'Full-shell Rust smoke fixture lost its private provider or File Jobs boundary')
    }
    let module = (open --raw ($root | path join modules desktop.nix))
    for policy in ['boot.kernel' 'swapDevices' 'hardware.i2c' 'services.udev' 'environment.persistence' 'xdg.userDirs' 'kb_layout' 'max-items'] {
        if ($module | str contains $policy) {
            $errors = ($errors | append $'Host policy in desktop module: ($policy)')
        }
    }
    for obsolete in ['pkgs.hyprland' 'baseBar =' 'programs.waybar.style'] {
        if ($module | str contains $obsolete) {
            $errors = ($errors | append 'Obsolete session stack dependency')
        }
    }
    if ($module | str contains 'lumen-night.png') {
        $errors = ($errors | append 'Removed wallpaper referenced by desktop module')
    }
    let status = (open --raw ($root | path join res lumen-desktop shell StatusMessage.qml))
    for required in ['Accessible.description: severity' '"Error: "' '"Success: "' '"Warning: "' 'Theme.errorText' 'Theme.successText' 'Theme.warningText'] {
        if not ($status | str contains $required) {
            $errors = ($errors | append $'Status message lost textual severity: ($required)')
        }
    }
    let force_quit = (open --raw ($root | path join res lumen-desktop shell ForceQuitDialog.qml))
    if not ($force_quit | str contains 'parent.enabled?Theme.errorText') {
        $errors = ($errors | append 'Force Quit lost its error foreground')
    }
    for workflow in (glob ($root | path join .github workflows '*.yml')) {
        let lines = (open --raw $workflow | lines)
        let retained = ($lines | where { |line| $line | str starts-with '            artifacts/' } | each { |line| $line | str trim })
        if ($retained | length) != ($retained | uniq | length) {
            $errors = ($errors | append $'Duplicate artifact retention in ($workflow)')
        }
        mut declared = []
        for line in ($lines | where { |line| $line | str contains '--output ' }) {
            for match in ($line | parse -r '--output\s+"?(?<output>(?:\$PWD/)?artifacts/[^\s"]+)') {
                let output = ($match.output | str replace '$PWD/' '' | str replace -a -r '\$\{[^}]+\}|\$[A-Za-z_][A-Za-z_0-9]*' 'fixture')
                $declared = ($declared | append $output)
            }
        }
        if ($declared | is-empty) {
            $errors = ($errors | append $'No native outputs declared in ($workflow)')
        }
        for output in $declared {
            let covered = ($retained | any { |pattern|
                if ($pattern | str ends-with '*') {
                    $output | str starts-with ($pattern | str replace -r '\*$' '')
                } else {
                    $output == $pattern
                }
            })
            if not $covered {
                $errors = ($errors | append $'Missing diagnostic retention in ($workflow): ($output)')
            }
        }
    }
    if not ($errors | is-empty) {
        error make {msg: ($errors | str join (char nl))}
    }
    'PASS: neutral source, Rust/Nushell runtime with scoped QML and Electron JavaScript, no first-party Python or C++, local Nix imports, semantic labels, artifact retention and publication boundary' | print
    'passed' | save --force $env.out
}
