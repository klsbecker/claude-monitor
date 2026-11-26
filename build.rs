use embed_manifest::{embed_manifest, new_manifest};

fn main() {
    // Layered child windows (the strip inside the taskbar) need the Windows 8+ compatibility entries.
    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        embed_manifest(new_manifest("ClaudeMonitor")).expect("unable to embed manifest");
    }
    println!("cargo:rerun-if-changed=build.rs");
}
