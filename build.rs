// build.rs — embeds the Windows manifest (requireAdministrator + DPI awareness)
// and an application icon when assets/icon.ico is present.

use std::env;
use std::path::Path;

fn main() {
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let manifest = Path::new("purge.manifest");
        let icon = Path::new("assets/icon.ico");

        if manifest.exists() {
            println!("cargo:rerun-if-changed=purge.manifest");
        }
        if icon.exists() {
            println!("cargo:rerun-if-changed=assets/icon.ico");
        }

        let mut res = winresource::WindowsResource::new();
        if manifest.exists() {
            res.set_manifest_file("purge.manifest");
        }
        if icon.exists() {
            res.set_icon("assets/icon.ico");
        }
        res.compile().expect("failed to compile windows resources");
    }
}
