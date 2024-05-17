use std::{fs::{File, self}, io::Write, path::Path};

fn main() {
    let build_path = Path::new("./target/idl/");
    fs::create_dir_all(build_path).expect("Creating build path failed");

    let idl_path = Path::new("src/VideoDDS.idl");
    let idl_src = std::fs::read_to_string(idl_path).expect("Couldn't read IDL source file!");

    let compiled_idl = dust_dds_gen::compile_idl(&idl_src).expect("Couldn't parse IDL file");
    let compiled_idl_path = build_path.join("video_dds.rs");
    let mut file = File::create(compiled_idl_path).expect("Failed to create file");

    file.write_all(compiled_idl.as_bytes())
        .expect("Failed to write to file");
}
