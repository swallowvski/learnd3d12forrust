fn main() {
    println!("!cargo:rerun-if-changed=src/BasicVertexShader.hlsl");
    std::fs::copy(
        "src/BasicVertexShader.hlsl",
        std::env::var("OUT_DIR").unwrap() + "/../../../BasicVertexShader.hlsl",
    )
    .expect("Copy");

    let path = std::env::var("OUT_DIR").unwrap();
    println!("{}", path + "/../../../BasicVertexShader.hlsl");

    std::fs::copy(
        "src/BasicPixelShader.hlsl",
        std::env::var("OUT_DIR").unwrap() + "/../../../BasicPixelShader.hlsl",
    )
    .expect("Copy");

    let path = std::env::var("OUT_DIR").unwrap();
    println!("{}", path + "/../../../BasicPixelShader.hlsl");
}
