fn main() {
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=include");

    cc::Build::new()
        .cpp(true)
        .std("c++17")
        .file("src/vector_store/index.cpp")
        .file("src/embedding/engine.cpp")
        .file("src/embedding/tokenizer.cpp")
        .file("src/document/chunker.cpp")
        .file("src/ffi/remem.cpp")
        .include("include")
        .include("src")
        .flag_if_supported("-Wno-unused-parameter")
        .compile("remem");
}
