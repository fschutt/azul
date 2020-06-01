cd ./api && python3 ./gen-api.py && cd .. && \
RUST_BACKTRACE=full cargo check --verbose --example public
# cargo doc --no-deps --open