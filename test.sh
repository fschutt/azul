cd ./api && python3 ./gen-api.py && cd .. && \
cargo doc --no-deps --open
# cargo check --example public