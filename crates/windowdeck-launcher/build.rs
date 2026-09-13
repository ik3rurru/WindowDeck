fn main() {
    println!("cargo:rerun-if-changed=../../assets/WindowDeck.ico");
    println!("cargo:rerun-if-changed=launcher.manifest");
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new()
            .set_icon("../../assets/WindowDeck.ico")
            .set_manifest_file("launcher.manifest")
            .set("FileDescription", "WindowDeck")
            .set("ProductName", "WindowDeck")
            .compile()
            .expect("No se pudieron compilar los recursos de WindowDeck");
    }
}
