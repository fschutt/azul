set -e

cd ./api
python3 ./gen-api.py
cd ..

cd ./azul-dll
cargo build --all-features --release # build the DLL
# cargo build --all-features # build the DLL
cd ..

cd ./target/release
strip ./libazul.so
cd ../..

# cargo doc --no-deps --open
RUST_BACKTRACE=full cargo run --example public