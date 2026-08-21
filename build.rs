fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=Cargo.toml");

    #[cfg(windows)]
    {
        let version = std::env::var("CARGO_PKG_VERSION").expect("CARGO_PKG_VERSION");
        let file_version = format!("{version}.0");
        winresource::WindowsResource::new()
            .set("ProductName", "clipi")
            .set("FileDescription", "Minimal clipboard history manager")
            .set("FileVersion", &file_version)
            .set("ProductVersion", &file_version)
            .set("OriginalFilename", "clipi.exe")
            .set("InternalName", "clipi")
            .set("LegalCopyright", "Copyright 2026")
            .compile()
            .expect("embed Windows version resource");
    }
}
