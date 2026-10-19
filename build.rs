fn main() {
    // Embed the app icon + version metadata into the exe (Windows only).
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.set("ProductName", "Path of Titans VR Launcher");
        res.set("FileDescription", "Unofficial VR launcher for Path of Titans");
        res.set("OriginalFilename", "pot-vr-launcher.exe");
        res.set("LegalCopyright", "MIT License");
        res.set("CompanyName", "cybershrimp23");
        if let Err(e) = res.compile() {
            println!("cargo:warning=winresource failed to embed icon/version info: {e}");
        }
    }
    println!("cargo:rerun-if-changed=assets/icon.ico");
}
