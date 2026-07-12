fn main() {
    println!("cargo:rerun-if-changed=app.rc");
    println!("cargo:rerun-if-changed=assets/app.ico");

    if cfg!(target_os = "windows") {
        let _ = embed_resource::compile("app.rc", embed_resource::NONE);
    }
}
