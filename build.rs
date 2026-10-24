fn main() {
    println!("cargo:rerun-if-changed=assets/kite.ico");

    #[cfg(target_os = "windows")]
    {
        let mut resource = winresource::WindowsResource::new();
        resource.set_icon("assets/kite.ico");
        resource.set("ProductName", "Kite");
        resource.set("FileDescription", "Kite File Manager");
        resource.set("CompanyName", "Kite contributors");
        resource.set("LegalCopyright", "Copyright (c) 2026 Kite contributors");
        resource
            .compile()
            .expect("failed to compile Windows resources");
    }
}
