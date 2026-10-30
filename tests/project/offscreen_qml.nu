def main [root: path] {
    let output = $env.out
    mkdir $output
    let provider = ($output | path join nearby-provider-stub)
    ^rustc --edition=2024 -Dwarnings ($root | path join tests desktop nearby_provider_stub.rs) -o $provider
    $env.LUMEN_NEARBY_PROVIDER_STUB = $provider
    let native_focus_stub = ($output | path join native-focus-transport-stub)
    ^rustc --edition=2024 -Dwarnings ($root | path join tests desktop native_focus_transport_stub.rs) -o $native_focus_stub
    $env.LUMEN_NATIVE_FOCUS_STUB = $native_focus_stub
    for name in ['app_launch_qml' 'dock_model_qml' 'file_share_intent_qml' 'menu_layout_qml' 'settings_search_qml' 'quickshell_json_adapter' 'capture_state_qml' 'focus_profile_qml' 'network_state_qml' 'battery_qml' 'biometric_status_qml' 'semantic_states_qml' 'status_clock_qml' 'text_sizing_qml' 'selected_surface_contrast_qml' 'shell_request_lifecycle_qml' 'nearby_sharing_defaults_qml' 'trash_state_qml' 'shortcut_state_qml' 'javascript_models_qml' 'file_link_labels_qml' 'captured_action_intent_qml' 'desktop_drop_qml' 'desktop_layout_state_qml' 'widget_preferences_qml' 'hot_corners_qml' 'batch_rename_qml' 'capture_lifecycle_qml' 'widget_catalogue_qml' 'force_quit_qml' 'control_centre_qml' 'network_locations_qml' 'settings_window_search_qml' 'desktop_files_actions_qml' 'files_layout_qml' 'preview_retirement_qml' 'clipboard_launcher_qml' 'native_focus_transport_qml'] {
        let source = ($root | path join tests desktop $'($name).rs')
        let binary = ($output | path join $name)
        ^rustc --edition=2024 -Dwarnings --test $source -o $binary
        ^$binary --nocapture
    }
}
