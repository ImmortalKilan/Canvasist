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
            "start_gradescope_login",
            "get_auth_status",
            "sign_out",
            "get_snapshot",
            "refresh",
            "open_external",
            "get_marks",
            "mark_done",
            "dismiss",
            "restore",
            "get_preferences",
            "set_reminder_offsets",
            "set_course_hidden",
            "set_show_unsubmittable",
        ]),
    ))
    .expect("failed to run tauri-build");
}
