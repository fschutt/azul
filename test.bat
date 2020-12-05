@echo off

cd %~dp0\api
python gen-api.py
cd ..

set CARGO_HOME=%USERPROFILE%/.cargo
if not exist "%CARGO_HOME%\lib\azul-dll-0.0.1\target\release" mkdir "%CARGO_HOME%\lib\azul-dll-0.0.1\target\release"

cd "%~dp0\azul-dll"
taskkill /im "cargo.exe"
rem cargo build --all-features --release
RUSTFLAGS='-C link-arg=-s'
cargo build --all-features
cargo install --path .
cd ..

rem copy "%~dp0\target\release\azul.dll" "%CARGO_HOME%\lib\azul-dll-0.0.1\target\release"

if exist "%~dp0\target\debug\examples" del "%~dp0\target\debug\examples\azul.dll"

cargo run --release --example public

pause >nul
