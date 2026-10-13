//! On Windows, build the app icon into the program, so Explorer, shortcuts and the taskbar show
//! it (other systems get it from the window, or on macOS from the app bundle).
//!
//! Also list the manual's figures (`docs/manual/figures/*.png|jpg`) for `manual.rs` to build in.

fn main() {
    manual_figures();
    let icon = "../../packaging/icon/microcosm.ico";
    println!("cargo:rerun-if-changed={icon}");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon(icon);
        res.compile().expect("embedding the Windows icon");
    }
}

/// Write `manual_figures.rs` into OUT_DIR: `(file name, bytes)` for every PNG and JPEG figure.
fn manual_figures() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/manual/figures");
    println!("cargo:rerun-if-changed={}", dir.display());
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok()?.file_name().into_string().ok())
                .filter(|n| n.ends_with(".png") || n.ends_with(".jpg"))
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    let mut code = String::from("&[\n");
    for name in &names {
        let path = dir.join(name);
        println!("cargo:rerun-if-changed={}", path.display());
        code += &format!("    ({name:?}, include_bytes!({:?})),\n", path.display().to_string());
    }
    code += "]\n";
    let out = std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("manual_figures.rs");
    std::fs::write(out, code).expect("writing the manual's figure list");
}
