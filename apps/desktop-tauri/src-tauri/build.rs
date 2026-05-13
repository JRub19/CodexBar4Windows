fn main() {
    // Phase 3 B1: ship an explicit Windows application manifest so per
    // monitor DPI v2 awareness, GDI scaling off, and long path support
    // are opt in, not inherited from Tauri defaults. The manifest also
    // documents the supported OS GUIDs and the Common Controls v6
    // dependency required by tauri-runtime-wry's TaskDialogIndirect
    // import (without it the EXE fails to load with
    // STATUS_ENTRYPOINT_NOT_FOUND).
    //
    // CRITICAL: tell cargo to rerun this build script when the manifest
    // changes. Without this directive cargo caches the embedded
    // resource.rc; editing app.manifest then rebuilding does NOT
    // re-embed the new content, even though the Rust code recompiles.
    println!("cargo:rerun-if-changed=app.manifest");
    // Re-embed bundled frontend assets when dist/ changes. Without
    // this, cargo's incremental cache keeps the old JS/CSS baked into
    // the EXE even after `npm run build` regenerates dist/.
    println!("cargo:rerun-if-changed=../dist/index.html");
    println!("cargo:rerun-if-changed=../dist/assets");

    let manifest = std::fs::read_to_string("app.manifest")
        .expect("app.manifest must be present alongside build.rs");
    let attributes = tauri_build::Attributes::new()
        .windows_attributes(tauri_build::WindowsAttributes::new().app_manifest(manifest));
    tauri_build::try_build(attributes).expect("tauri build must succeed");
}
