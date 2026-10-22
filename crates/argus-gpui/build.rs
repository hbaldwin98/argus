//! Puts the Tauri window's icon into the Windows executable as resource 1, which the GPUI
//! platform loads for the window's class: the taskbar, Alt-Tab and a shortcut show it.

fn main() {
    println!("cargo:rerun-if-changed=argus-gpui.rc");
    println!("cargo:rerun-if-changed=../argus-desktop/icons/icon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile("argus-gpui.rc", embed_resource::NONE).manifest_optional().expect("the icon resource compiles");
    }
}
