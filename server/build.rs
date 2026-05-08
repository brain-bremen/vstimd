fn main() {
    prost_build::Config::new()
        .compile_protos(&["../proto/wonderlamp.proto"], &["../proto/"])
        .expect("failed to compile protobuf schema");

    compile_shader("shaders/solid.wgsl", "solid.spv");
    compile_shader("src/render/vk/egui/shaders.wgsl", "egui.spv");
}

fn compile_shader(wgsl_path: &str, output_name: &str) {
    println!("cargo:rerun-if-changed={}", wgsl_path);

    let wgsl = std::fs::read_to_string(wgsl_path)
        .unwrap_or_else(|e| panic!("failed to read {}: {}", wgsl_path, e));

    let module = naga::front::wgsl::parse_str(&wgsl)
        .unwrap_or_else(|e| panic!("failed to parse {}: {}", wgsl_path, e));

    let mut capabilities = naga::valid::Capabilities::empty();
    capabilities.insert(naga::valid::Capabilities::PUSH_CONSTANT);

    let info = naga::valid::Validator::new(naga::valid::ValidationFlags::all(), capabilities)
        .validate(&module)
        .unwrap_or_else(|e| panic!("WGSL validation failed for {}: {}", wgsl_path, e));

    let options = naga::back::spv::Options {
        lang_version: (1, 0),
        ..Default::default()
    };

    let spv_words = naga::back::spv::write_vec(&module, &info, &options, None)
        .unwrap_or_else(|e| panic!("failed to write SPIR-V for {}: {}", wgsl_path, e));

    let spv_bytes: Vec<u8> = spv_words.iter().flat_map(|&w| w.to_le_bytes()).collect();

    let out_dir = std::env::var("OUT_DIR").unwrap();
    std::fs::write(format!("{}/{}", out_dir, output_name), &spv_bytes)
        .unwrap_or_else(|e| panic!("failed to write {}: {}", output_name, e));
}
