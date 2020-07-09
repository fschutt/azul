set -e

# generate the DLL C-API
cd ./api
python3 ./gen-api.py
cd ..

mkdir -p ~/.cargo/lib/azul-dll-0.0.1/target/release

# build the DLL
cd ./azul-dll
RUSTFLAGS='-C link-arg=-s' cargo build --all-features --release
# cargo build --all-features
# cargo install --path .
cd ..

cp ./target/release/libazul.dylib ~/.cargo/lib/azul-dll-0.0.1/target/release

if [ -d "./target/debug/examples" ]; then
    # remove the stale azul.dylib object
    cd ./target/debug/examples
    rm -f ./azul.dylib
    cd ../..
fi

# run the opengl example
RUST_BACKTRACE=full cargo build --example public
# valgrind --track-origins=yes --leak-check=full --log-file=out.txt ./target/debug/examples/public
RUST_BACKTRACE=full cargo run --example public