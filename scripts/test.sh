set -e

cd ./api
python3 ./api/gen-api.py
cd ..

mkdir -p $CARGO_HOME/lib/azul-dll-0.0.1/target/release

cd ./azul-dll
RUSTFLAGS='-C link-arg=-s' cargo build --all-features --release
# cargo build --all-features
# cargo install --path .
cd ..

cp ./target/release/libazul.so $CARGO_HOME/lib/azul-dll-0.0.1/target/release

if [ -d "./target/debug/examples" ]; then
    rm -f ./target/debug/examples/azul.so
fi

# valgrind --track-origins=yes --leak-check=full --log-file=out.txt ./target/debug/examples/public
RUST_BACKTRACE=full cargo run --example public