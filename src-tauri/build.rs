fn main() {
    // rc.exe cannot handle some characters (e.g. the apostrophe in "Drive'ım")
    // in resource paths, so hand it a copy of the icon from the ASCII-only OUT_DIR.
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let ico = out.join("app-icon.ico");
    std::fs::copy("icons/icon.ico", &ico).expect("icons/icon.ico kopyalanamadı");
    println!("cargo:rerun-if-changed=icons/icon.ico");

    let attrs = tauri_build::Attributes::new().windows_attributes(tauri_build::WindowsAttributes::new().window_icon_path(&ico));
    tauri_build::try_build(attrs).expect("tauri-build başarısız");
}
