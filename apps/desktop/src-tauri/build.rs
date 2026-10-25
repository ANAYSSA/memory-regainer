fn main() {
    println!("cargo:rerun-if-changed=windows-app.manifest");
    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(
        tauri_build::WindowsAttributes::new().app_manifest(include_str!("windows-app.manifest")),
    ))
    .expect("Unable to build application resources");
}
