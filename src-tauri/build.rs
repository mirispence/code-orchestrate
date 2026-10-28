fn main() {
    // The bundle icons are embedded in the binary (window, taskbar, exe resource), but
    // tauri_build doesn't tell Cargo to watch them, so swapping an icon left a stale build.
    println!("cargo:rerun-if-changed=icons");
    tauri_build::build()
}
