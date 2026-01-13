fn main() {
    // 环境变量 MCU 和 ESP_IDF_SDKCONFIG_DEFAULTS 
    // 已由构建脚本 (build_esp32s2.ps1 / build_esp32s3.ps1) 设置
    embuild::espidf::sysenv::output();
    
    // 复制 partitions.csv 到 ESP-IDF 构建输出目录
    // ESP-IDF 会在 OUT_DIR 中查找 partitions.csv
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let out_dir = std::env::var("OUT_DIR").unwrap();
    
    let src_partitions = std::path::Path::new(&manifest_dir).join("partitions.csv");
    let dst_partitions = std::path::Path::new(&out_dir).join("partitions.csv");
    
    if src_partitions.exists() {
        std::fs::copy(&src_partitions, &dst_partitions)
            .expect("Failed to copy partitions.csv to OUT_DIR");
        println!("cargo:rerun-if-changed=partitions.csv");
    }
}
