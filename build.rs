fn main() -> std::io::Result<()> {
    /* let mut cfg = prost_build::Config::new();
    cfg.type_attribute(
        "package.ei",
        "#[derive(serde::Serialize, serde::Deserialize)]",
    ); */
    prost_build::compile_protos(&["src/ei.proto"], &["src/"])?;
    Ok(())
}
