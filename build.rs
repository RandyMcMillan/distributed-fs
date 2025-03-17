//fn example_proto() -> Result<(), Box<dyn std::error::Error>> {
//    tonic_build::configure()
//        .build_server(false) // optional: disable server code generation
//        .compile(&["proto/example.proto"], &["proto"])?;
//    Ok(())
//}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .type_attribute(".", "#[derive(serde::Deserialize, serde::Serialize)]")
        .protoc_arg("--experimental_allow_proto3_optional")
        .compile(&["proto/api.proto"], &["."])?;
    tonic_build::configure()
        .type_attribute(".", "#[derive(serde::Deserialize, serde::Serialize)]")
        .protoc_arg("--experimental_allow_proto3_optional")
        .compile(&["proto/example.proto"], &["."])?;
    Ok(())
}
