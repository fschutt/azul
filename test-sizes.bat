@echo off

cd %~dp0\api
python gen-api.py
cd ..

cd %~dp0\azul-dll
cargo test
cd ..
