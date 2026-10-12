//! On Windows, build the app icon into the program, so Explorer, shortcuts and the taskbar show
//! it (other systems get it from the window, or on macOS from the app bundle).

fn main() {
    let icon = "../../packaging/icon/microcosm.ico";
    println!("cargo:rerun-if-changed={icon}");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon(icon);
        res.compile().expect("embedding the Windows icon");
    }
}
