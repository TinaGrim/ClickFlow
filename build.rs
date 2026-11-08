fn main() {
    println!("cargo:rerun-if-changed=assets/icon/ClickFlow.ico");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut resource = winresource::WindowsResource::new();
        resource.set_icon("assets/icon/ClickFlow.ico");
        resource.set("ProductName", "ClickFlow");
        resource.set("FileDescription", "ClickFlow - Auto Clicker");
        resource.set("OriginalFilename", "ClickFlow.exe");
        resource.set("LegalCopyright", "Copyright 2026 ClickFlow");
        if let Err(error) = resource.compile() {
            panic!("could not compile Windows resources: {error}");
        }
    }
}
