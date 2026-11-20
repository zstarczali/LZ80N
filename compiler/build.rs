//! Puts the program icon into the Windows executable. Does nothing on other systems.

fn main() {
    println!("cargo:rerun-if-changed=../assets/lz80n.rc");
    println!("cargo:rerun-if-changed=../assets/lz80n.ico");
    embed_resource::compile("../assets/lz80n.rc", embed_resource::NONE)
        .manifest_optional()
        .expect("couldn't embed the program icon");
}
