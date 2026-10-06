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
        ]),
    ))
    .expect("failed to run tauri-build");
}
