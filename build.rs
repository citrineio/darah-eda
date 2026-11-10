fn main() -> std::io::Result<()> {
    println!("cargo:rerun-if-changed=packaging/darah-eda.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new()
            .set_icon("packaging/darah-eda.ico")
            .compile()?;
    }
    Ok(())
}
