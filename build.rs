fn main() -> Result<(), Box<dyn std::error::Error>> {
	tonic_build::configure()
		.type_attribute(".", "#[derive(serde::Deserialize, serde::Serialize)]")
		.compile(
			&["proto/api.proto"],
			&["."]
		)?;
	Ok(())
}