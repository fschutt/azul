set -e

cd ./api
python3 ./gen-api.py
cd ..

mkdir -p $CARGO_HOME/lib/azul-dll-0.0.1/target/release

# build the DLL
cd ./azul-dll
RUSTFLAGS='-C link-arg=-s' cargo build --all-features --release
# cargo build --all-features
# cargo install --path .
cd ..

cp ./target/release/libazul.dylib $CARGO_HOME/lib/azul-dll-0.0.1/target/release

if [ -d "./target/debug/examples" ]; then
    rm -f ./target/debug/examples/azul.dylib
fi

RUST_BACKTRACE=full cargo run --example public