fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    #[cfg(windows)]
    {
        winresource::WindowsResource::new()
            .set("ProductName", "clipi")
            .set("FileDescription", "Minimal clipboard history manager")
            .compile()
            .expect("embed Windows version resource");
    }
}
