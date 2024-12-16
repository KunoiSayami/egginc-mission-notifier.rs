fn main() -> std::io::Result<()> {
    prost_build::compile_protos(&["src/ei.proto"], &["src/"])?;
    Ok(())
}
