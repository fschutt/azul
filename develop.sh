cd azul-dll && \
cargo build --release && \
cd .. && \
sudo cp ./target/release/libazul.so /usr/lib && \
AZUL_LINK_PATH=/usr/lib cargo run --release --manifest-path ./examples/Cargo.toml --bin widgets