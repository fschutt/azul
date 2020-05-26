cd ./api && python3 ./gen-api.py && cd .. && \
cargo check --example public
# cargo doc --no-deps --open