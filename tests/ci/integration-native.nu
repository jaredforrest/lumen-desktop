def artifact [path: string] {
    $env.PWD | path join artifacts $path
}

def build [attribute: string, output: string] {
    ^nix build $'.#($attribute)' --out-link $'artifacts/($output)' --option abort-on-warn true
}

def main [] {
    build app-launcher app-launcher
    build desktop-action desktop-action
    build desktop-state desktop-state
    build checks.x86_64-linux.shortcut-hints shortcut-fixtures
    build test-shortcut-hints-fixture shortcut-hints-fixture
    build test-batch-rename-fixture batch-rename-fixture
    build test-trash-confirmation-fixture trash-confirmation-fixture
    build test-files-layout-fixture files-layout-fixture
    build test-appearance-controls-fixture appearance-controls-fixture
    build test-desktop-surface-fixture desktop-surface-fixture
    build test-pointer pointer
    build test-status-clock-fixture status-clock-fixture
    build test-input-settings-fixture input-settings-fixture
    build test-screen-saver-fixture screen-saver-fixture
    build test-wallpaper-settings-fixture wallpaper-settings-fixture
    build test-power-provider-fixture power-provider-fixture
    build test-calendar-qa calendar-qa
    build test-calendar-runtime calendar-runtime
    build test-calendar-widget-fixture calendar-widget-fixture
    build test-widget-visibility-fixture widget-visibility-fixture
    build test-widget-catalogue-fixture widget-catalogue-fixture
    build test-fullscreen-menu-fixture fullscreen-menu-fixture
    build test-surface-matrix-fixture surface-matrix-fixture
    build test-icon-render-fixture icon-render-fixture
    build test-window-frames-fixture window-frames-fixture
    build test-content-clipping-fixture content-clipping-fixture
    build test-display-visual-fixtures display-visual-fixtures
    build test-auth-render-fixture auth-render-fixture
    build test-fixture-infrastructure fixture-infrastructure
    build test-native-input-fixtures native-input-fixtures
    build test-preview-input-fixtures preview-input-fixtures
    build test-search-native-fixture search-native-fixture
    build test-capture-native-fixture capture-native-fixture
    build test-preferred-zoom-fixture preferred-zoom-fixture
    build test-portal-permission-provider-fixture portal-permission-provider-fixture
    build test-portal-permissions-native-fixture portal-permissions-native-fixture
    build test-text-sizing-fixture text-sizing-fixture
    build test-mpris-mock-fixture mpris-mock-fixture
    build test-media-control-fixture media-control-fixture
    build test-system-popover-fixture system-popover-fixture
    build test-owned-window-context-fixture owned-window-context-fixture
    build test-search-history-fixture search-history-fixture
    build test-consumer-fit-fixture consumer-fit-fixture
    build test-settings-search-fixture settings-search-fixture
    build test-native-groups-fixture native-groups-fixture
    build test-switcher-fixture switcher-fixture
    build shell-runtime switcher-runtime
    build test-offscreen-font-config switcher-font-config
    build test-spaces-placement-fixture spaces-placement-fixture
    build test-notification-native-fixture notification-native-fixture
    build test-desktop-policy-fixture desktop-policy-fixture
    build file-surfaces-rust-wip file-surfaces-rust-wip
    build test-owned-font-config owned-font-config
    build test-owned-geometry-legacy-compositor compositor-legacy
    build test-menu-exporter-fixture menu-exporter-fixture
    build test-menu-native-fixture menu-native-fixture
    build test-lifecycle-native-fixture lifecycle-native-fixture
    build test-settings-runtime settings-runtime
    build test-nix-settings-fixture nix-settings-fixture
    build test-capture-observation-shim capture-observation-shim
    build test-application-isolation application-isolation-package
    build test-force-quit-native force-quit-native
    build test-application-control application-control
    build test-application-software-driver software-driver
    build test-transport-frame-fixture transport-frame-fixture
    build network-connectivity network-connectivity-client
    build test-network-connectivity-provider-fixture network-connectivity-provider-fixture
    let shell = (artifact 'shell-package/bin/lumen-desktop-shell')
    ^(artifact 'switcher-fixture/bin/lumen-switcher-coverage-fixture') --quickshell (artifact 'switcher-runtime/bin/quickshell') --source ($env.PWD | path join res lumen-desktop shell) --fontconfig (artifact 'switcher-font-config') --output (artifact 'switcher-coverage')
    ^nu tests/ci/run-native.nu nix_settings_transaction -- (artifact 'nix-settings-fixture/bin/lumen-nix-settings-fixture') (artifact 'settings-runtime/bin/lumen-nix-settings') (nix eval --raw .#packages.x86_64-linux.settings-adapter.schema) (nix eval --raw --expr '(builtins.getFlake (toString ./.)).inputs.nixpkgs.outPath' --impure)
    let basic = (artifact 'compositor/bin/sway')
    let effects = (artifact 'compositor-effects/bin/sway')
    let legacy = (artifact 'compositor-legacy/bin/sway')
    let pointer = (artifact 'pointer/bin/wlrctl')
    let lock = (artifact 'session-lock/bin/lumen-test-session-lock')
    let client = (artifact 'window-relationships-client/bin/lumen-test-window-relationships')
    let pipewire = (artifact 'capture-pipewire/bin')
    let atspi = (artifact 'screen-reader-atspi-package')
    ^nu tests/ci/run-native.nu auth_render -- (artifact 'auth-render-fixture/bin/lumen-auth-render-fixture') --source $env.PWD --launcher $shell --sway $basic --output artifacts/auth-render
    ^nu tests/ci/run-native.nu app_switcher_model -- (artifact 'switcher-fixture/bin/lumen-switcher-fixture') model --sway $basic --wtype wtype --output artifacts/app-switcher-model
    ^nu tests/ci/run-native.nu application_isolation_native -- (artifact 'application-isolation-package/bin/lumen-application-isolation') probe --source $env.PWD --output artifacts/application-isolation
    build test-capture-pipewire capture-pipewire
    build test-capture-observer capture-observer
    build test-capture-protocol-fixture capture-protocol-fixture
    build test-dialog-lifetime-fixture dialog-lifetime-fixture
    build test-hot-corners-fixture hot-corners-fixture
    build test-dock-matrix-fixture dock-matrix-fixture
    build test-dock-interaction-fixture dock-interaction-fixture
    build test-paired-picker-fixture paired-picker-fixture
    build test-paired-layouts-fixture paired-layouts-fixture
    build test-edge-layouts-fixture edge-layouts-fixture
    build test-window-relationships-fixture window-relationships-fixture
    build test-xwayland-relationships-fixture xwayland-relationships-fixture
    ^nu tests/ci/run-native.nu capture_protocol -- (artifact 'capture-protocol-fixture/bin/lumen-capture-protocol-fixture') --pipewire $pipewire --observer (artifact 'capture-observer/bin/lumen-capture-observation') --output artifacts/capture-protocol
    ^nu tests/ci/run-native.nu live_transparency_native -- (artifact 'display-visual-fixtures/bin/lumen-live-transparency-fixture') --launcher $shell --sway $basic --pointer-tool $pointer --source res/lumen-desktop --output artifacts/live-transparency-basic
    ^nu tests/ci/run-native.nu live_transparency_native_effects -- (artifact 'display-visual-fixtures/bin/lumen-live-transparency-fixture') --launcher $shell --sway $effects --pointer-tool $pointer --source res/lumen-desktop --effects --output artifacts/live-transparency-effects
    build reader-command reader-command-package
    build test-reader-command reader-command-enabled-package
    build test-reader-commands-provider-fixture reader-commands-provider-fixture
    ^nu tests/ci/run-native.nu reader_commands_provider -- (artifact 'reader-commands-provider-fixture/bin/lumen-reader-commands-provider-fixture') --disabled (artifact 'reader-command-package') --enabled (artifact 'reader-command-enabled-package') --output artifacts/reader-command-provider
    build test-screen-reader screen-reader-package
    build test-screen-reader-atspi screen-reader-atspi-package
    build test-screen-reader-tools screen-reader-tools-package
    build test-speech-dispatcher speech-dispatcher-package
    build test-speech-source speech-dispatcher-source
    build test-speech-transport-fixture speech-transport-fixture
    build test-accessibility-workflows-fixture accessibility-workflows-fixture
    build test-accessibility-protocol-fixture accessibility-protocol-fixture
    build test-gi-binding-native-fixture gi-binding-native-fixture
    ^nu tests/ci/run-native.nu screen_reader_transport -- (artifact 'speech-transport-fixture/bin/lumen-speech-transport-fixture') --dispatcher (artifact 'speech-dispatcher-package') --dispatcher-source (artifact 'speech-dispatcher-source') --output artifacts/screen-reader-transport
    ^nu tests/ci/run-native.nu atspi_action_metadata_rust -- (artifact 'accessibility-workflows-fixture/bin/lumen-accessibility-workflows-fixture') action-metadata --atspi $atspi --launcher $shell --sway $basic --output artifacts/screen-reader-action-metadata-rust
    ^nu tests/ci/run-native.nu gi_closure_callbacks -- (artifact 'gi-binding-native-fixture/bin/lumen-gi-binding-native-fixture') closures --output artifacts/screen-reader-closures
    ^nu tests/ci/run-native.nu atspi_action_metadata -- (artifact 'gi-binding-native-fixture/bin/lumen-gi-binding-native-fixture') action-metadata --atspi $atspi --launcher $shell --sway $basic --output artifacts/screen-reader-action-metadata
    build test-session-lock session-lock
    build test-constraint-client constraint-client
    build test-window-relationships window-relationships-client
    build test-physical-keyboard physical-keyboard
    build test-xwayland-relationships xwayland-relationships-client
    build test-xwayland-server xwayland-server
    build test-clipboard-producer clipboard-producer
    build clipboard-observer clipboard-observer

    for compositor in [compositor compositor-effects] {
        let flags = if $compositor == 'compositor-effects' { ['--effects'] } else { [] }
        let sway = (artifact $'($compositor)/bin/sway')
        ^nu tests/ci/run-native.nu $'owned_preview_input_($compositor)' -- (artifact 'preview-input-fixtures/bin/lumen-owned-preview-fixture') ...$flags --launcher $shell --sway $sway --output $'artifacts/owned-preview-($compositor)'
        ^nu tests/ci/run-native.nu $'preview_actions_input_($compositor)' -- (artifact 'preview-input-fixtures/bin/lumen-preview-actions-fixture') ...$flags --launcher $shell --sway $sway --pointer-tool $pointer --output $'artifacts/preview-actions-($compositor)'
        ^nu tests/ci/run-native.nu $'image_markup_input_($compositor)' -- (artifact 'preview-input-fixtures/bin/lumen-image-markup-fixture') ...$flags --launcher $shell --sway $sway --pointer-tool $pointer --keyboard (artifact 'physical-keyboard/bin/lumen-test-physical-keyboard') --output $'artifacts/image-markup-($compositor)'
        ^nu tests/ci/run-native.nu $'files_history_($compositor)' -- (artifact 'files-layout-fixture/bin/lumen-files-layout-fixture') --history ...$flags --launcher $shell --sway $sway --pointer-tool $pointer --output $'artifacts/files-history-($compositor)'
        ^nu tests/ci/run-native.nu $'batch_rename_($compositor)' -- (artifact 'batch-rename-fixture/bin/lumen-batch-rename-fixture') ...$flags --launcher $shell --sway $sway --pointer-tool $pointer --output $'artifacts/batch-rename-($compositor)'
        ^nu tests/ci/run-native.nu $'trash_confirmation_($compositor)' -- (artifact 'trash-confirmation-fixture/bin/lumen-trash-confirmation-fixture') ...$flags --launcher $shell --sway $sway --pointer-tool $pointer --output $'artifacts/trash-confirmation-($compositor)'
        ^nu tests/ci/run-native.nu $'files_selection_($compositor)' -- (artifact 'files-layout-fixture/bin/lumen-files-layout-fixture') --selection ...$flags --launcher $shell --sway $sway --pointer-tool $pointer --lock-client $lock --output $'artifacts/files-selection-($compositor)'
        ^nu tests/ci/run-native.nu $'installed_switcher_($compositor)' -- (artifact 'switcher-fixture/bin/lumen-switcher-fixture') installed ...$flags --launcher $shell --action (artifact 'desktop-action/bin/lumen-desktop-action') --sway $sway --output $'artifacts/installed-switcher-($compositor)'
        ^nu tests/ci/run-native.nu $'clipboard_native_($compositor)' -- (artifact 'native-input-fixtures/bin/lumen-clipboard-native-fixture') ...$flags --launcher $shell --sway $sway --producer (artifact 'clipboard-producer/bin/lumen-test-clipboard-producer') --observer (artifact 'clipboard-observer/bin/lumen-clipboard-observer') --pointer-tool $pointer --output $'artifacts/clipboard-native-($compositor)'
        ^nu tests/ci/run-native.nu $'search_history_($compositor)' -- (artifact 'search-history-fixture/bin/lumen-search-history-fixture') ...$flags --launcher $shell --app-launcher (artifact 'app-launcher/bin/lumen-app-launcher') --sway $sway --pointer-tool $pointer --output $'artifacts/search-history-($compositor)'
        ^nu tests/ci/run-native.nu $'screen_reader_input_($compositor)' -- (artifact 'accessibility-workflows-fixture/bin/lumen-accessibility-workflows-fixture') screen-reader-input ...$flags --launcher $shell --sway $sway --orca (artifact 'screen-reader-package') --atspi $atspi --dispatcher (artifact 'speech-dispatcher-package') --dispatcher-source (artifact 'speech-dispatcher-source') --speech-tool (artifact 'speech-transport-fixture/bin/lumen-speech-transport-fixture') --schema-compiler (artifact 'screen-reader-tools-package/bin/glib-compile-schemas') --isolation-tool (artifact 'screen-reader-tools-package/bin/unshare') --reader-command (artifact 'reader-command-enabled-package/bin/lumen-reader-command') --lock-client $lock --output $'artifacts/screen-reader-input-($compositor)'
        ^nu tests/ci/run-native.nu $'capture_native_input_($compositor)' -- (artifact 'capture-native-fixture/bin/lumen-capture-native-fixture') --source $env.PWD ...$flags --launcher $shell --sway $sway --pointer-tool $pointer --pipewire $pipewire --capture-shim (artifact 'capture-observation-shim/bin/lumen-capture-observation-shim') --at-spi-prefix $atspi --output $'artifacts/capture-native-($compositor)'
        ^nu tests/ci/run-native.nu $'clock_native_($compositor)' -- (artifact 'native-input-fixtures/bin/lumen-clock-native-fixture') ...$flags --launcher $shell --sway $sway --pointer-tool $pointer --at-spi-prefix $atspi --output $'artifacts/clock-native-($compositor)'
        ^nu tests/ci/run-native.nu $'settings_search_($compositor)' -- (artifact 'settings-search-fixture/bin/lumen-settings-search-fixture') ...$flags --launcher $shell --sway $sway --pointer-tool $pointer --output $'artifacts/settings-search-($compositor)'
        ^nu tests/ci/run-native.nu $'search_native_input_($compositor)' -- (artifact 'search-native-fixture/bin/lumen-search-native-fixture') ...$flags --launcher $shell --sway $sway --pointer-tool $pointer --output $'artifacts/search-native-($compositor)'
        ^nu tests/ci/run-native.nu $'group_membership_($compositor)' -- (artifact 'native-groups-fixture/bin/lumen-native-groups-fixture') membership ...$flags --launcher $shell --sway $sway --client $client --pointer-tool $pointer --locker $lock --output $'artifacts/group-membership-($compositor)'
        ^nu tests/ci/run-native.nu $'group_selector_($compositor)' -- (artifact 'native-groups-fixture/bin/lumen-native-groups-fixture') selector ...$flags --launcher $shell --sway $sway --client $client --pointer-tool $pointer --locker $lock --output $'artifacts/group-selector-($compositor)'
        ^nu tests/ci/run-native.nu $'window_groups_($compositor)' -- (artifact 'native-groups-fixture/bin/lumen-native-groups-fixture') window-groups ...$flags --launcher $shell --sway $sway --client $client --keyboard wtype --pointer-tool $pointer --locker $lock --output $'artifacts/window-groups-($compositor)'
        ^nu tests/ci/run-native.nu $'widget_catalogue_($compositor)' -- (artifact 'widget-catalogue-fixture/bin/lumen-widget-catalogue-fixture') ...$flags --launcher $shell --sway $sway --pointer-tool $pointer --source $env.PWD --output $'artifacts/widget-catalogue-($compositor)'
        ^nu tests/ci/run-native.nu $'dialog_lifetime_($compositor)' -- (artifact 'dialog-lifetime-fixture/bin/lumen-dialog-lifetime-fixture') ...$flags --sway $sway --client $client --output $'artifacts/dialog-manager-lifetime-($compositor)'
        ^nu tests/ci/run-native.nu $'desktop_lifecycle_($compositor)' -- (artifact 'desktop-policy-fixture/bin/lumen-desktop-policy-fixture') lifecycle ...$flags --launcher $shell --sway $sway --pointer-tool $pointer --output $'artifacts/desktop-lifecycle-($compositor)'
        for scenario in [owned-inactive owned-inactive-native] {
            ^nu tests/ci/run-native.nu $'($scenario)_($compositor)' -- (artifact 'desktop-policy-fixture/bin/lumen-desktop-policy-fixture') $scenario ...$flags --launcher $shell --sway $sway --keyboard (artifact 'physical-keyboard/bin/lumen-test-physical-keyboard') --output $'artifacts/($scenario)-($compositor)'
        }
        ^nu tests/ci/run-native.nu $'window_relationships_($compositor)' -- (artifact 'window-relationships-fixture/bin/lumen-window-relationships-fixture') ...$flags --sway $sway --client $client --keyboard wtype --state-bin (artifact 'desktop-state/bin/lumen-desktop-state') --output $'artifacts/window-relationships-($compositor)'
        ^nu tests/ci/run-native.nu $'xwayland_relationships_($compositor)' -- (artifact 'xwayland-relationships-fixture/bin/lumen-xwayland-relationships-fixture') ...$flags --sway $sway --client (artifact 'xwayland-relationships-client/bin/lumen-test-xwayland-relationships') --keyboard (artifact 'physical-keyboard/bin/lumen-test-physical-keyboard') --xwayland (artifact 'xwayland-server/bin/Xwayland') --output $'artifacts/xwayland-relationships-($compositor)'
        ^nu tests/ci/run-native.nu $'xwayland_no_xres_($compositor)' -- (artifact 'xwayland-relationships-fixture/bin/lumen-xwayland-relationships-fixture') ...$flags --sway $sway --client (artifact 'xwayland-relationships-client/bin/lumen-test-xwayland-relationships') --keyboard (artifact 'physical-keyboard/bin/lumen-test-physical-keyboard') --xwayland (artifact 'xwayland-server/bin/Xwayland') --without-xres --output $'artifacts/xwayland-no-xres-($compositor)'
        ^nu tests/ci/run-native.nu $'owned_window_context_($compositor)' -- (artifact 'owned-window-context-fixture/bin/lumen-owned-window-context-fixture') ...$flags --full-shell --launcher $shell --sway $sway --output $'artifacts/owned-window-context-($compositor)'
        ^nu tests/ci/run-native.nu $'menubar_($compositor)' -- (artifact 'menu-native-fixture/bin/lumen-menu-native-fixture') menubar ...$flags --launcher $shell --sway $sway --pointer-tool $pointer --at-spi-prefix $atspi --output $'artifacts/menubar-($compositor)'
        ^nu tests/ci/run-native.nu $'menu_keyboard_($compositor)' -- (artifact 'menu-native-fixture/bin/lumen-menu-native-fixture') keyboard ...$flags --launcher $shell --sway $sway --pointer-tool $pointer --at-spi-prefix $atspi --exporter (artifact 'menu-exporter-fixture/bin/lumen-menu-exporter-fixture') --output $'artifacts/menu-keyboard-($compositor)'
        ^nu tests/ci/run-native.nu $'shortcut_hints_($compositor)' -- (artifact 'shortcut-hints-fixture/bin/lumen-shortcut-hints-fixture') ...$flags --launcher $shell --sway $sway --font-config ((artifact 'owned-font-config') | path expand) --software-driver ((artifact 'software-driver') | path expand) --fixtures (artifact 'shortcut-fixtures') --output $'artifacts/shortcut-hints-($compositor)'
    }
    ^nu tests/ci/run-native.nu menu_typography_qml -- (artifact 'desktop-policy-fixture/bin/menu-typography') --launcher $shell --font-config (artifact 'owned-font-config') --output artifacts/menu-typography
    ^nu tests/ci/run-native.nu essential_dialog_typography_qml -- (artifact 'desktop-policy-fixture/bin/essential-dialog-qml') --launcher $shell --font-config (artifact 'owned-font-config') --output artifacts/essential-dialog-qml
    ^nu tests/ci/run-native.nu widget_catalogue_light -- (artifact 'widget-catalogue-fixture/bin/lumen-widget-catalogue-fixture') --launcher $shell --sway $basic --pointer-tool $pointer --source $env.PWD --theme light --output artifacts/widget-catalogue-light
    ^nu tests/ci/run-native.nu network_connectivity_provider -- (artifact 'network-connectivity-provider-fixture/bin/lumen-network-connectivity-provider-fixture') --output artifacts/network-connectivity-provider --client (artifact 'network-connectivity-client/bin/lumen-network-connectivity')
    ^nu tests/ci/run-native.nu battery_provider -- (artifact 'power-provider-fixture/bin/lumen-power-provider-fixture') --output artifacts/power-provider
    ^nu tests/ci/run-native.nu status_clock_input -- (artifact 'status-clock-fixture/bin/lumen-status-clock-fixture') --launcher $shell --sway $basic --pointer-tool $pointer --output artifacts/status-clock
    ^nu tests/ci/run-native.nu input_settings_native -- (artifact 'input-settings-fixture/bin/lumen-input-settings-fixture') --launcher $shell --sway $basic --pointer-tool $pointer --output artifacts/input-settings
    ^nu tests/ci/run-native.nu wallpaper_settings_native -- (artifact 'wallpaper-settings-fixture/bin/lumen-wallpaper-settings-fixture') --launcher $shell --sway $basic --pointer-tool $pointer --output artifacts/wallpaper-settings
    ^nu tests/ci/run-native.nu screen_saver_native -- (artifact 'screen-saver-fixture/bin/lumen-screen-saver-fixture') --launcher $shell --sway $basic --pointer-tool $pointer --lock $lock --output artifacts/screen-saver
    ^nu tests/ci/run-native.nu screen_saver_native_effects -- (artifact 'screen-saver-fixture/bin/lumen-screen-saver-fixture') --effects --launcher $shell --sway $effects --pointer-tool $pointer --lock $lock --output artifacts/screen-saver-effects
    ^nu tests/ci/run-native.nu calendar_widget_native -- (artifact 'calendar-widget-fixture/bin/lumen-calendar-widget-fixture') --launcher $shell --sway $basic --pointer-tool $pointer --calendar-runtime (artifact 'calendar-runtime/bin/desktop-calendar') --output artifacts/calendar-widget
    ^nu tests/ci/run-native.nu calendar_widget_native_effects -- (artifact 'calendar-widget-fixture/bin/lumen-calendar-widget-fixture') --effects --launcher $shell --sway $effects --pointer-tool $pointer --calendar-runtime (artifact 'calendar-runtime/bin/desktop-calendar') --output artifacts/calendar-widget-effects
    ^nu tests/ci/run-native.nu calendar_card -- (artifact 'calendar-qa/bin/lumen-calendar-qa') card --assets res/lumen-desktop/shell --launcher $shell --output artifacts/calendar-card
    ^nu tests/ci/run-native.nu calendar_window -- (artifact 'calendar-qa/bin/lumen-calendar-qa') window --assets res/lumen-desktop/shell --launcher $shell --output artifacts/calendar-window
    ^nu tests/ci/run-native.nu calendar_popover -- (artifact 'calendar-qa/bin/lumen-calendar-qa') popover --assets res/lumen-desktop/shell --launcher $shell --output artifacts/calendar-popover
    ^nu tests/ci/run-native.nu widget_visibility_native -- (artifact 'widget-visibility-fixture/bin/lumen-widget-visibility-fixture') --launcher $shell --sway $basic --pointer-tool $pointer --calendar-runtime (artifact 'calendar-runtime/bin/desktop-calendar') --output artifacts/widget-visibility
    ^nu tests/ci/run-native.nu widget_visibility_native_effects -- (artifact 'widget-visibility-fixture/bin/lumen-widget-visibility-fixture') --effects --launcher $shell --sway $effects --pointer-tool $pointer --calendar-runtime (artifact 'calendar-runtime/bin/desktop-calendar') --output artifacts/widget-visibility-effects
    ^nu tests/ci/run-native.nu fullscreen_menu_native -- (artifact 'fullscreen-menu-fixture/bin/lumen-fullscreen-menu-fixture') --launcher $shell --sway $basic --pointer-tool $pointer --source $env.PWD --output artifacts/fullscreen-menu
    ^nu tests/ci/run-native.nu fullscreen_menu_native_effects -- (artifact 'fullscreen-menu-fixture/bin/lumen-fullscreen-menu-fixture') --effects --launcher $shell --sway $effects --pointer-tool $pointer --source $env.PWD --output artifacts/fullscreen-menu-effects
    ^nu tests/ci/run-native.nu media_control_native -- (artifact 'media-control-fixture/bin/lumen-media-control-fixture') --launcher $shell --sway $basic --pointer-tool $pointer --provider (artifact 'mpris-mock-fixture/bin/lumen-mpris-mock-fixture') --atspi $atspi --output artifacts/media-controls
    ^nu tests/ci/run-native.nu media_control_native_effects -- (artifact 'media-control-fixture/bin/lumen-media-control-fixture') --effects --launcher $shell --sway $effects --pointer-tool $pointer --provider (artifact 'mpris-mock-fixture/bin/lumen-mpris-mock-fixture') --atspi $atspi --output artifacts/media-controls-effects
    ^nu tests/ci/run-native.nu dock_drag_out -- (artifact 'dock-interaction-fixture/bin/lumen-dock-interaction-fixture') drag --launcher $shell --sway $basic --pointer-tool $pointer --source $env.PWD --output artifacts/dock-drag-out
    ^nu tests/ci/run-native.nu dock_drag_out_effects -- (artifact 'dock-interaction-fixture/bin/lumen-dock-interaction-fixture') drag --effects --launcher $shell --sway $effects --pointer-tool $pointer --source $env.PWD --output artifacts/dock-drag-out-effects
    ^nu tests/ci/run-native.nu system_popover_reflow_native -- (artifact 'system-popover-fixture/bin/lumen-system-popover-fixture') --launcher $shell --sway $basic --mpris-provider (artifact 'mpris-mock-fixture/bin/lumen-mpris-mock-fixture') --output artifacts/system-popover-reflow
    ^nu tests/ci/run-native.nu system_popover_reflow_native_effects -- (artifact 'system-popover-fixture/bin/lumen-system-popover-fixture') --effects --theme light --native-frame on --launcher $shell --sway $effects --mpris-provider (artifact 'mpris-mock-fixture/bin/lumen-mpris-mock-fixture') --output artifacts/system-popover-reflow-effects
    ^nu tests/ci/run-native.nu text_sizing_input -- (artifact 'text-sizing-fixture/bin/lumen-text-sizing-fixture') --launcher $shell --sway $basic --pointer-tool $pointer --output artifacts/text-sizing
    ^nu tests/ci/run-native.nu portal_permissions_input -- (artifact 'portal-permissions-native-fixture/bin/lumen-portal-permissions-native-fixture') --launcher $shell --sway $basic --pointer-tool $pointer --provider (artifact 'portal-permission-provider-fixture/bin/lumen-portal-permission-provider-fixture') --output artifacts/portal-permissions
    ^nu tests/ci/run-native.nu files_layout_rust -- (artifact 'files-layout-fixture/bin/lumen-files-layout-fixture') --launcher $shell --sway $basic --pointer-tool $pointer --output artifacts/files-layout
    ^nu tests/ci/run-native.nu files_layout_rust_effects -- (artifact 'files-layout-fixture/bin/lumen-files-layout-fixture') --effects --launcher $shell --sway $effects --pointer-tool $pointer --output artifacts/files-layout-effects
    ^nu tests/ci/run-native.nu files_split_panes -- (artifact 'files-layout-fixture/bin/lumen-files-layout-fixture') --split-panes --launcher $shell --sway $basic --pointer-tool $pointer --output artifacts/files-split-panes
    ^nu tests/ci/run-native.nu appearance_controls -- (artifact 'appearance-controls-fixture/bin/lumen-appearance-controls-fixture') --launcher $shell --sway $basic --output artifacts/appearance-controls
    for mode in [base layout multi] {
        let flags = (if $mode == 'base' { [] } else if $mode == 'layout' { ['--layout'] } else { ['--layout' '--multi-drag'] })
        ^nu tests/ci/run-native.nu $'desktop_files_($mode)' -- (artifact 'desktop-surface-fixture/bin/lumen-desktop-surface-fixture') files --launcher $shell --sway $basic --pointer-tool $pointer --output $'artifacts/desktop-files-($mode)' ...$flags
    }
    ^nu tests/ci/run-native.nu edge_layouts_input -- (artifact 'edge-layouts-fixture/bin/lumen-edge-layouts-fixture') --sway $basic --pointer $pointer --keyboard wtype --lock $lock --quickshell (artifact 'shell-runtime/bin/quickshell') --constraint-client (artifact 'constraint-client/bin/lumen-test-constraint-client') --output artifacts/edge-layouts
    ^nu tests/ci/run-native.nu paired_layouts_input -- (artifact 'paired-layouts-fixture/bin/lumen-paired-layouts-fixture') --sway $basic --pointer $pointer --keyboard wtype --constraint-client (artifact 'constraint-client/bin/lumen-test-constraint-client') --state-bin (artifact 'desktop-state/bin/lumen-desktop-state') --output artifacts/paired-layouts
    ^nu tests/ci/run-native.nu paired_layouts_input_effects -- (artifact 'paired-layouts-fixture/bin/lumen-paired-layouts-fixture') --sway $effects --pointer $pointer --keyboard wtype --constraint-client (artifact 'constraint-client/bin/lumen-test-constraint-client') --state-bin (artifact 'desktop-state/bin/lumen-desktop-state') --effects --output artifacts/paired-layouts-effects
    ^nu tests/ci/run-native.nu paired_picker_qml -- (artifact 'paired-picker-fixture/bin/lumen-paired-picker-fixture') --launcher $shell --sway $basic --assets ($env.PWD | path join res lumen-desktop) --output artifacts/paired-picker
    ^nu tests/ci/run-native.nu notification_recovery_basic -- (artifact 'notification-native-fixture/bin/lumen-notification-native-fixture') recovery --launcher $shell --sway $basic --output artifacts/notification-recovery-basic
    ^nu tests/ci/run-native.nu notification_recovery_effects -- (artifact 'notification-native-fixture/bin/lumen-notification-native-fixture') recovery --launcher $shell --sway $effects --effects --output artifacts/notification-recovery-effects
    ^nu tests/ci/run-native.nu notification_lock_basic -- (artifact 'notification-native-fixture/bin/lumen-notification-native-fixture') lock --launcher $shell --sway $basic --lock-client $lock --frame-tool (artifact 'transport-frame-fixture/bin/lumen-transport-frame-fixture') --output artifacts/notification-lock-basic
    ^nu tests/ci/run-native.nu notification_lock_effects -- (artifact 'notification-native-fixture/bin/lumen-notification-native-fixture') lock --launcher $shell --sway $effects --lock-client $lock --frame-tool (artifact 'transport-frame-fixture/bin/lumen-transport-frame-fixture') --effects --output artifacts/notification-lock-effects
    ^nu tests/ci/run-native.nu focus_presentation_basic -- (artifact 'notification-native-fixture/bin/lumen-notification-native-fixture') focus-presentation --launcher $shell --sway $basic --output artifacts/focus-expiry-presentation-basic
    ^nu tests/ci/run-native.nu focus_presentation_effects -- (artifact 'notification-native-fixture/bin/lumen-notification-native-fixture') focus-presentation --launcher $shell --sway $effects --effects --output artifacts/focus-expiry-presentation-effects
    ^nu tests/ci/run-native.nu desktop_native_drag -- (artifact 'desktop-surface-fixture/bin/lumen-desktop-surface-fixture') drag --launcher $shell --sway $basic --pointer-tool $pointer --output artifacts/file-drag
    for mode in [light dark] {
        for frame in [off on] {
            ^nu tests/ci/run-native.nu $'surface_matrix_($mode)_($frame)' -- (artifact 'surface-matrix-fixture/bin/lumen-surface-matrix-fixture') --launcher $shell --sway $basic --theme $mode --native-frame $frame --source res/lumen-desktop --output $'artifacts/surfaces-($mode)-($frame)'
        }
    }
    ^nu tests/ci/run-native.nu native_hot_corners -- (artifact 'hot-corners-fixture/bin/lumen-hot-corners-fixture') --sway $basic --pointer $pointer --keyboard wtype --lock $lock --output artifacts/hot-corners
    ^nu tests/ci/run-native.nu dock_matrix -- (artifact 'dock-matrix-fixture/bin/lumen-dock-matrix-fixture') --sway $basic --pointer $pointer --assets ($env.PWD | path join res lumen-desktop shell) --output artifacts/dock-matrix
    ^nu tests/ci/run-native.nu window_frames_render -- (artifact 'window-frames-fixture/bin/lumen-window-frames-fixture') --core-only --launcher $shell --sway $basic --pointer-tool $pointer --output artifacts/native-layouts
    ^nu tests/ci/run-native.nu icon_render_bootstrap -- (artifact 'icon-render-fixture/bin/lumen-icon-render-fixture') --bootstrap-check
    ^nu tests/ci/run-native.nu icon_render -- (artifact 'icon-render-fixture/bin/lumen-icon-render-fixture') --assets ($env.PWD | path join res lumen-desktop shell) --launcher $shell --sway $basic --output artifacts/icon-grid
    ^nu tests/ci/run-native.nu mixed_output_scale_native -- (artifact 'display-visual-fixtures/bin/lumen-mixed-output-scale-fixture') --launcher $shell --sway $basic --pointer-tool $pointer --source res/lumen-desktop --output artifacts/mixed-output-scale-basic
    ^nu tests/ci/run-native.nu mixed_output_scale_native_effects -- (artifact 'display-visual-fixtures/bin/lumen-mixed-output-scale-fixture') --launcher $shell --sway $effects --pointer-tool $pointer --source res/lumen-desktop --effects --output artifacts/mixed-output-scale-effects
    for variant in [basic effects] {
        let sway = if $variant == 'effects' { $effects } else { $basic }
        let flags = if $variant == 'effects' { ['--effects'] } else { [] }
        ^nu tests/ci/run-native.nu $'preferred_zoom_native_($variant)' -- (artifact 'preferred-zoom-fixture/bin/lumen-preferred-zoom-fixture') --source $env.PWD --rapid --launcher $shell --sway $sway --pointer-tool $pointer ...$flags --output $'artifacts/preferred-zoom-($variant)'
        for backend in [software opengl] {
            for frame in [on off] {
                ^nu tests/ci/run-native.nu $'content_clipping_($variant)_($backend)_($frame)' -- (artifact 'content-clipping-fixture/bin/lumen-content-clipping-fixture') --source $env.PWD --launcher $shell --sway $sway --pointer-tool $pointer ...$flags --native-frame $frame --content-backend $backend --output $'artifacts/content-clipping-($variant)-($backend)-($frame)'
            }
        }
        for frame in [on off] {
            ^nu tests/ci/run-native.nu $'consumer_fit_($variant)_($frame)' -- (artifact 'consumer-fit-fixture/bin/lumen-consumer-fit-fixture') --launcher $shell --sway $sway ...$flags --native-frame $frame --output $'artifacts/consumer-fit-($variant)-($frame)'
        }
    }
    ^nu tests/ci/run-native.nu interaction_regression -- (artifact 'dock-interaction-fixture/bin/lumen-dock-interaction-fixture') interaction --launcher $shell --sway $basic --pointer-tool $pointer --source $env.PWD --output artifacts/interactions
    ^nu tests/ci/run-native.nu accessibility_protocol -- (artifact 'accessibility-protocol-fixture/bin/lumen-accessibility-protocol-fixture') --launcher $shell --sway $basic --at-spi-prefix $atspi --output artifacts/accessibility --full-shell
    ^nu tests/ci/run-native.nu force_quit_surface -- (artifact 'force-quit-native/bin/lumen-force-quit-native') --sway $basic --wtype wtype --shell-source ($env.PWD | path join res lumen-desktop shell) --output artifacts/force-quit
    ^nu tests/ci/run-native.nu force_quit_broker -- (artifact 'force-quit-native/bin/lumen-force-quit-native') --acceptance --broker (artifact 'application-control/bin/lumen-application-control') --sway $basic --wtype wtype --keyboard (artifact 'physical-keyboard/bin/lumen-test-physical-keyboard') --shell-source ($env.PWD | path join res lumen-desktop shell) --output artifacts/force-quit-native-basic
    ^nu tests/ci/run-native.nu force_quit_broker_effects -- (artifact 'force-quit-native/bin/lumen-force-quit-native') --acceptance --broker (artifact 'application-control/bin/lumen-application-control') --sway $effects --wtype wtype --keyboard (artifact 'physical-keyboard/bin/lumen-test-physical-keyboard') --shell-source ($env.PWD | path join res lumen-desktop shell) --software-driver (artifact 'software-driver') --effects --output artifacts/force-quit-native-effects
    ^nu tests/ci/run-native.nu spaces_placement -- (artifact 'spaces-placement-fixture/bin/lumen-spaces-placement-fixture') --launcher $shell --sway $basic --output artifacts/spaces
    ^nu tests/ci/run-native.nu application_desktops_basic -- (artifact 'desktop-policy-fixture/bin/lumen-desktop-policy-fixture') application --launcher $shell --sway $basic --client $client --output artifacts/application-desktops-basic
    ^nu tests/ci/run-native.nu application_desktops_effects -- (artifact 'desktop-policy-fixture/bin/lumen-desktop-policy-fixture') application --launcher $shell --sway $effects --client $client --effects --output artifacts/application-desktops-effects
    ^nu tests/ci/run-native.nu display_spaces_native -- (artifact 'desktop-policy-fixture/bin/lumen-desktop-policy-fixture') display --launcher $shell --sway $basic --client $client --output artifacts/display-spaces-native
    for variant in [basic effects] {
        let sway = if $variant == 'effects' { $effects } else { $basic }
        let flags = if $variant == 'effects' { ['--effects'] } else { [] }
        ^nu tests/ci/run-native.nu $'pointer_retirement_($variant)' -- (artifact 'desktop-policy-fixture/bin/lumen-desktop-policy-fixture') pointer-retirement --launcher $shell --sway $sway --pointer-tool $pointer ...$flags --output $'artifacts/pointer-retirement-($variant)'
        let motion_output = ($env.PWD | path join artifacts $'reduced-motion-($variant)')
        mkdir $motion_output
        ^chmod 700 $motion_output
        let isolated_graphics = if $variant == 'effects' { ['--software-graphics'] } else { [] }
        ^nu tests/ci/run-native.nu $'reduced_motion_($variant)' -- (artifact 'application-isolation-package/bin/lumen-application-isolation') run --source $env.PWD --output $motion_output ...$isolated_graphics -- ((artifact 'desktop-policy-fixture/bin/lumen-desktop-policy-fixture') | path expand) reduced-motion --launcher ($shell | path expand) --sway ($sway | path expand) --keyboard ((artifact 'physical-keyboard/bin/lumen-test-physical-keyboard') | path expand) --pointer-tool ($pointer | path expand) --font-config ((artifact 'owned-font-config') | path expand) --software-driver ((artifact 'software-driver') | path expand) ...$flags --output /artifacts
        ^nu tests/ci/run-native.nu $'window_controls_($variant)' -- (artifact 'desktop-policy-fixture/bin/lumen-desktop-policy-fixture') window-controls --launcher $shell --sway $sway --pointer-tool $pointer --client (artifact 'constraint-client/bin/lumen-test-constraint-client') ...$flags --output $'artifacts/window-controls-($variant)'
        ^nu tests/ci/run-native.nu $'minimized_dock_($variant)' -- (artifact 'desktop-policy-fixture/bin/lumen-desktop-policy-fixture') minimized-dock --launcher $shell --sway $sway --pointer-tool $pointer ...$flags --output $'artifacts/minimized-dock-($variant)'
        ^nu tests/ci/run-native.nu $'owned_typography_($variant)' -- (artifact 'desktop-policy-fixture/bin/lumen-desktop-policy-fixture') typography --launcher $shell --sway $sway --font-config (artifact 'owned-font-config') ...$flags --output $'artifacts/owned-typography-native-($variant)'
        ^nu tests/ci/run-native.nu $'desktop_typography_($variant)' -- (artifact 'desktop-policy-fixture/bin/lumen-desktop-policy-fixture') desktop-typography --launcher $shell --sway $sway --pointer-tool $pointer --font-config (artifact 'owned-font-config') ...$flags --output $'artifacts/desktop-typography-native-($variant)'
        ^nu tests/ci/run-native.nu $'semantic_state_($variant)' -- (artifact 'desktop-policy-fixture/bin/lumen-desktop-policy-fixture') semantic-state --launcher $shell --sway $sway --pointer-tool $pointer --font-config (artifact 'owned-font-config') ...$flags --output $'artifacts/semantic-state-native-($variant)'
        ^nu tests/ci/run-native.nu $'live_overlay_appearance_($variant)' -- (artifact 'desktop-policy-fixture/bin/lumen-desktop-policy-fixture') live-overlay-appearance --launcher $shell --sway $sway --font-config (artifact 'owned-font-config') ...$flags --output $'artifacts/live-overlay-appearance-($variant)'
        ^nu tests/ci/run-native.nu $'utility_typography_($variant)' -- (artifact 'desktop-policy-fixture/bin/lumen-desktop-policy-fixture') utility-typography --launcher $shell --sway $sway --font-config (artifact 'owned-font-config') ...$flags --output $'artifacts/owned-utility-native-($variant)'
        ^nu tests/ci/run-native.nu $'calendar_detail_typography_($variant)' -- (artifact 'desktop-policy-fixture/bin/lumen-desktop-policy-fixture') calendar-detail-typography --launcher $shell --sway $sway --font-config (artifact 'owned-font-config') ...$flags --output $'artifacts/calendar-detail-native-($variant)'
        ^nu tests/ci/run-native.nu $'overview_typography_($variant)' -- (artifact 'desktop-policy-fixture/bin/lumen-desktop-policy-fixture') overview-typography --launcher $shell --sway $sway --font-config (artifact 'owned-font-config') ...$flags --output $'artifacts/overview-typography-native-($variant)'
        ^nu tests/ci/run-native.nu $'dock_motion_($variant)' -- (artifact 'desktop-policy-fixture/bin/lumen-desktop-policy-fixture') dock-motion --launcher $shell --sway $sway --pointer-tool $pointer --font-config (artifact 'owned-font-config') ...$flags --output $'artifacts/dock-motion-($variant)'
        ^nu tests/ci/run-native.nu $'folder_stack_($variant)' -- (artifact 'desktop-policy-fixture/bin/lumen-desktop-policy-fixture') folder-stack --launcher $shell --sway $sway --client (artifact 'file-surfaces-rust-wip/bin/lumen-files-rust-wip') --pointer-tool $pointer --font-config (artifact 'owned-font-config') ...$flags --output $'artifacts/folder-stack-($variant)'
        ^nu tests/ci/run-native.nu $'launcher_typography_($variant)' -- (artifact 'desktop-policy-fixture/bin/lumen-desktop-policy-fixture') launcher-typography --launcher $shell --sway $sway --keyboard (artifact 'physical-keyboard/bin/lumen-test-physical-keyboard') --pointer-tool $pointer --font-config (artifact 'owned-font-config') ...$flags --output $'artifacts/launcher-typography-native-($variant)'
        ^nu tests/ci/run-native.nu $'shared_control_typography_($variant)' -- (artifact 'desktop-policy-fixture/bin/lumen-desktop-policy-fixture') shared-control-typography --launcher $shell --sway $sway --keyboard (artifact 'physical-keyboard/bin/lumen-test-physical-keyboard') --pointer-tool $pointer --font-config (artifact 'owned-font-config') ...$flags --output $'artifacts/shared-control-native-($variant)'
        ^nu tests/ci/run-native.nu $'essential_dialog_($variant)' -- (artifact 'desktop-policy-fixture/bin/lumen-desktop-policy-fixture') essential-dialog --launcher $shell --sway $sway --keyboard (artifact 'physical-keyboard/bin/lumen-test-physical-keyboard') --font-config (artifact 'owned-font-config') ...$flags --output $'artifacts/essential-dialog-native-($variant)'
        for scenario in [resize-hit-targets resize-hit-targets-rapid] {
            ^nu tests/ci/run-native.nu $'($scenario)_($variant)' -- (artifact 'desktop-policy-fixture/bin/lumen-desktop-policy-fixture') $scenario --launcher $shell --sway $sway --client (artifact 'force-quit-native/bin/resize-hit-target-client') --pointer-tool $pointer --font-config (artifact 'owned-font-config') ...$flags --output $'artifacts/($scenario)-($variant)'
        }
        for scenario in [dock-offer dock-offer-busy] {
            ^nu tests/ci/run-native.nu $'($scenario)_($variant)' -- (artifact 'desktop-policy-fixture/bin/lumen-desktop-policy-fixture') $scenario --launcher $shell --sway $sway --pointer-tool $pointer ...$flags --output $'artifacts/($scenario)-($variant)'
        }
    }
    for variant in [modern legacy] {
        let sway = if $variant == 'legacy' { $legacy } else { $basic }
        let suffix = if $variant == 'legacy' { '-legacy' } else { '' }
        ^nu tests/ci/run-native.nu $'display_recovery_($variant)' -- (artifact 'desktop-policy-fixture/bin/lumen-desktop-policy-fixture') $'display-recovery($suffix)' --launcher $shell --sway $sway --output $'artifacts/display-spaces-recovery-($variant)'
        ^nu tests/ci/run-native.nu $'owned_geometry_($variant)' -- (artifact 'desktop-policy-fixture/bin/lumen-desktop-policy-fixture') $'owned-geometry($suffix)' --launcher $shell --sway $sway --pointer-tool $pointer --output $'artifacts/owned-geometry-($variant)'
        ^nu tests/ci/run-native.nu $'owned_components_($variant)' -- (artifact 'desktop-policy-fixture/bin/lumen-desktop-policy-fixture') $'owned-components($suffix)' --launcher $shell --sway $sway --output $'artifacts/owned-components-($variant)'
    }
    ^nu tests/ci/run-native.nu application_lifecycle -- (artifact 'lifecycle-native-fixture/bin/lumen-lifecycle-native-fixture') lifecycle --launcher $shell --sway $basic --output artifacts/application-lifecycle
    ^nu tests/ci/window-inventory.nu $shell $basic $effects
    ^nu tests/ci/run-native.nu session_end -- (artifact 'lifecycle-native-fixture/bin/lumen-lifecycle-native-fixture') session --launcher $shell --sway $basic --output artifacts/session-end
    ^nu tests/ci/run-native.nu session_end_effects -- (artifact 'lifecycle-native-fixture/bin/lumen-lifecycle-native-fixture') session --launcher $shell --sway $effects --effects --output artifacts/session-end-effects
    ^nu tests/ci/run-native.nu session_recovery_basic -- (artifact 'lifecycle-native-fixture/bin/lumen-lifecycle-native-fixture') recovery --launcher $shell --sway $basic --output artifacts/session-recovery-basic
    ^nu tests/ci/run-native.nu session_recovery_effects -- (artifact 'lifecycle-native-fixture/bin/lumen-lifecycle-native-fixture') recovery --launcher $shell --sway $effects --effects --output artifacts/session-recovery-effects
    ^nu tests/ci/toplevel-identity.nu $shell $basic $effects
    ^nu tests/ci/native-focus-guard.nu
    ^nu tests/ci/files-native-drop.nu
    ^nu tests/ci/files-copy-undo.nu
}
