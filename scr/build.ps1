cargo build --release

espflash save-image --chip esp32s2 --partition-table partitions.csv target/xtensa-esp32s2-espidf/release/esp32-wifi-screen esp32-wifi-screen.bin