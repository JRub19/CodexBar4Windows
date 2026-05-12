fn main() {
    // Phase 3 B1: ship an explicit Windows application manifest so per
    // monitor DPI v2 awareness, GDI scaling off, and long path support
    // are opt in, not inherited from Tauri defaults. The manifest also
    // documents the supported OS GUIDs.
    let manifest = std::fs::read_to_string("app.manifest")
        .expect("app.manifest must be present alongside build.rs");
    let attributes = tauri_build::Attributes::new()
        .windows_attributes(tauri_build::WindowsAttributes::new().app_manifest(manifest));
    tauri_build::try_build(attributes).expect("tauri build must succeed");
}
