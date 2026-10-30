def main [] {
    mkdir artifacts
    ^nix build .#shell --out-link artifacts/shell-package --option abort-on-warn true
    ^nix build .#test-offscreen-font-config --out-link artifacts/offscreen-font-config --option abort-on-warn true
    ^nix build .#test-calendar-qa --out-link artifacts/calendar-qa --option abort-on-warn true
    ^nix build .#test-files-share-menu-fixture --out-link artifacts/files-share-menu-fixture --option abort-on-warn true
    ^nix build .#test-shortcut-editor-data --out-link artifacts/shortcut-editor-data --option abort-on-warn true
    ^nix build .#test-owned-font-config --out-link artifacts/shortcut-font-config --option abort-on-warn true
    let root = $env.PWD
    let launcher = ($root | path join artifacts shell-package bin lumen-desktop-shell)
    let font_config = ($root | path join artifacts offscreen-font-config)
    ^nu tests/ci/run-native.nu files_share_menu_qml -- ($root | path join artifacts files-share-menu-fixture bin lumen-files-share-menu-fixture) --launcher $launcher --fontconfig $font_config --output artifacts/files-share-menu
    ^nu tests/ci/run-native.nu share_destinations_qml -- ($root | path join artifacts files-share-menu-fixture bin lumen-share-destinations-fixture) --launcher $launcher --fontconfig $font_config --output artifacts/share-destinations
    ^nu tests/ci/run-native.nu settings_poll_qml -- ($root | path join artifacts files-share-menu-fixture bin lumen-settings-poll-fixture) --launcher $launcher --fontconfig $font_config --output artifacts/settings-poll
    ^nu tests/ci/run-native.nu tooltip_lifecycle_qml -- ($root | path join artifacts files-share-menu-fixture bin lumen-tooltip-fixture) --launcher $launcher --fontconfig $font_config --output artifacts/tooltip-lifecycle
    ^nu tests/ci/run-native.nu shortcut_editor_qml -- ($root | path join artifacts files-share-menu-fixture bin lumen-shortcut-editor-fixture) --launcher $launcher --fontconfig ($root | path join artifacts shortcut-font-config) --window-source ($root | path join artifacts shortcut-editor-data) --output artifacts/shortcut-editor
    ^nu tests/ci/run-native.nu shortcut_settings_typography_qml -- ($root | path join artifacts files-share-menu-fixture bin lumen-shortcut-settings-typography-fixture) --launcher $launcher --fontconfig ($root | path join artifacts shortcut-font-config) --output artifacts/shortcut-settings-typography
    ^nu tests/ci/run-native.nu calendar_toggle_contrast -- ($root | path join artifacts calendar-qa bin lumen-calendar-qa) toggle --assets ($root | path join res lumen-desktop shell) --launcher $launcher --font-config $font_config --output artifacts/calendar-toggle-contrast
}
