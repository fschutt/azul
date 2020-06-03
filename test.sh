set -e

cd ./api
python3 ./gen-api.py
cd ..

cd ./azul-dll
cargo build --all-features --release # build the DLL
cd ..

cd ./target/release
strip ./libazul.so
cd ../..

# RUST_BACKTRACE=full cargo check --example public
# cargo doc --no-deps --open