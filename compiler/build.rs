//! Puts the program icon and the version information (Properties -> Details)
//! into the Windows executable. Does nothing on other systems.
//!
//! The values come from Cargo.toml, so a version bump there is all it needs.

use std::env;
use std::fs;
use std::path::PathBuf;

const COMPANY: &str = "Cool Valley Studio";
const PRODUCT: &str = "LZ80N";

fn var(name: &str) -> String {
    env::var(name).unwrap_or_default()
}

/// A string literal for the resource compiler.
fn rc_str(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\"\""))
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../assets/lz80n.ico");
    if var("CARGO_CFG_TARGET_OS") != "windows" {
        return;
    }

    let num = |part: &str| var(&format!("CARGO_PKG_VERSION_{part}")).parse::<u16>().unwrap_or(0);
    let (major, minor, patch) = (num("MAJOR"), num("MINOR"), num("PATCH"));
    let version = var("CARGO_PKG_VERSION");
    let authors = var("CARGO_PKG_AUTHORS").replace(':', ", ");
    let year = 2026;
    let icon = PathBuf::from(var("CARGO_MANIFEST_DIR")).join("../assets/lz80n.ico");
    let exe = format!("{}.exe", var("CARGO_PKG_NAME"));

    let strings = [
        ("CompanyName", COMPANY.to_string()),
        ("FileDescription", var("CARGO_PKG_DESCRIPTION")),
        ("FileVersion", version.clone()),
        ("InternalName", var("CARGO_PKG_NAME")),
        ("LegalCopyright", format!("Copyright (c) {year} {authors}. {} License.", var("CARGO_PKG_LICENSE"))),
        ("OriginalFilename", exe),
        ("ProductName", PRODUCT.to_string()),
        ("ProductVersion", version),
        ("Comments", format!("{}  {}", var("CARGO_PKG_HOMEPAGE"), var("CARGO_PKG_REPOSITORY"))),
    ];
    let mut block = String::new();
    for (key, value) in strings {
        block.push_str(&format!("      VALUE {}, {}\n", rc_str(key), rc_str(&value)));
    }

    let rc = format!(
        "1 ICON {icon}\n\
         1 VERSIONINFO\n\
         FILEVERSION {major},{minor},{patch},0\n\
         PRODUCTVERSION {major},{minor},{patch},0\n\
         FILEFLAGSMASK 0x3fL\n\
         FILEFLAGS 0x0L\n\
         FILEOS 0x40004L\n\
         FILETYPE 0x1L\n\
         FILESUBTYPE 0x0L\n\
         BEGIN\n\
         \x20 BLOCK \"StringFileInfo\"\n\
         \x20 BEGIN\n\
         \x20   BLOCK \"040904B0\"\n\
         \x20   BEGIN\n\
         {block}\
         \x20   END\n\
         \x20 END\n\
         \x20 BLOCK \"VarFileInfo\"\n\
         \x20 BEGIN\n\
         \x20   VALUE \"Translation\", 0x409, 1200\n\
         \x20 END\n\
         END\n",
        icon = rc_str(&icon.to_string_lossy()),
    );

    let out = PathBuf::from(var("OUT_DIR")).join("lz80n.rc");
    fs::write(&out, rc).expect("couldn't write the resource script");
    embed_resource::compile(&out, embed_resource::NONE)
        .manifest_optional()
        .expect("couldn't embed the icon and version information");
}
