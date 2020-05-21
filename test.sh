cd ./api && python3 ./gen-api.py && cd .. && \
cargo run --example public
# cargo doc --no-deps --open