cd ./api && python3 ./gen-api.py && cd .. && \
RUST_BACKTRACE=full cargo check --example public
# cargo doc --no-deps --open