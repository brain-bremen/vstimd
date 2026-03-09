fn main() {
    prost_build::Config::new()
        .compile_protos(&["proto/wonderlamp.proto"], &["proto/"])
        .expect("failed to compile protobuf schema");
}
