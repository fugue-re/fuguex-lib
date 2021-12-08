// use protoc_rust::Customize;
// compile the proto spec
fn main() {
    let tbb_proto_path = "src/utils/tbb.proto";
    // only re-compile if it has been changed
    println!("cargo:rerun-if-changed={}", tbb_proto_path);
    // recompile using protoc_rust and geterate .rs file
    protoc_rust::Codegen::new()
        .out_dir("src/utils/")
        .inputs(&[tbb_proto_path])
        .include("src/utils/")
        .run()
        .expect("protoc compiling error");
}