@echo off

cd %~dp0\api
python gen-api.py
cd ..

cd %~dp0\azul-dll
SET CC=clang-cl
SET CXX=clang-cl
SET RUSTFLAGS=-C target-feature=+crt-static -C link-arg=-s
cargo test --all-features --release
cd ..
