fn main() {
    // Declaring the app's commands in the manifest makes each one permission-gated:
    // a window can only call a command its capability explicitly allows.
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "get_settings",
            "set_language",
            "get_autostart",
            "set_autostart",
            "get_app_info",
            "search_schools",
            "check_canvas_url",
            "start_canvas_login",
            "cancel_canvas_login",
            "get_auth_status",
            "sign_out",
            "get_canvas_snapshot",
            "refresh_canvas",
            "open_external",
        ]),
    ))
    .expect("failed to run tauri-build");
}
