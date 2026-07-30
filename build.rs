fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .type_attribute(".", "#[derive(serde::Deserialize, serde::Serialize)]")
        .protoc_arg("--experimental_allow_proto3_optional")
        .compile(&["proto/api.proto"], &["."])?;
    Ok(())
}
