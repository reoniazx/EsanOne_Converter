fn main() {
    // Keep the development executable and bundled installers in sync with tray/window icons.
    println!("cargo:rerun-if-changed=icons");
    tauri_build::build()
}
