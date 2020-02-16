fn main() -> Result<(), Box<dyn std::error::Error>> {
    let protoc = which::which("protoc").map_err(|e| format!("`protoc` not found, please install it: {}", e))?;
    std::env::set_var("PROTOC", &protoc);
    tonic_build::compile_protos("proto/tgcd.proto")?;
    Ok(())
}
