fn main() {
    // 环境变量 MCU 和 ESP_IDF_SDKCONFIG_DEFAULTS 
    // 已由构建脚本 (build_esp32s2.ps1 / build_esp32s3.ps1) 设置
    embuild::espidf::sysenv::output();
    // // 指定库的查找路径
    // println!("cargo:rustc-link-search=C:/s/scr/library");

    // // 指定要链接的库
    // println!("cargo:rustc-link-lib=static=tjpgd");
}
