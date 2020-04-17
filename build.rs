fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .format(false)
        .compile(&["proto/tgcd.proto"], &["proto"])?;
    Ok(())
}
