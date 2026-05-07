fn main() {
    println!("cargo:rerun-if-changed=../libremem/src");
    println!("cargo:rerun-if-changed=../libremem/include");

    cc::Build::new()
        .cpp(true)
        .std("c++17")
        .file("../libremem/src/vector_store/index.cpp")
        .file("../libremem/src/embedding/engine.cpp")
        .file("../libremem/src/embedding/tokenizer.cpp")
        .file("../libremem/src/ffi/remem.cpp")
        .include("../libremem/include")
        .include("../libremem/src")
        .flag_if_supported("-Wno-unused-parameter")
        .compile("remem");
}
