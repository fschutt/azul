@echo off

cd %~dp0\api
python gen-api.py
cd ..

SET CARGO_HOME=%USERPROFILE%/.cargo
if not exist "%CARGO_HOME%\lib\azul-dll-0.0.1\target\release" mkdir "%CARGO_HOME%\lib\azul-dll-0.0.1\target\release"

cd "%~dp0\azul-dll"
rem taskkill /im "cargo.exe"
rem SET CC=clang-cl
rem SET CXX=clang-cl
rem SET RUSTFLAGS=-C target-feature=+crt-static -C link-arg=-s
rem cargo build --all-features --release
cd ..

copy "%~dp0\target\release\azul.dll" "%CARGO_HOME%\lib\azul-dll-0.0.1\target\release"

if exist "%~dp0\target\debug\examples" del "%~dp0\target\debug\examples\azul.dll"
if exist "%~dp0\target\release\examples" del "%~dp0\target\release\examples\azul.dll"

if exist "%~dp0\target\release\examples\" del "%~dp0\target\release\examples\azul.dll"
copy "%~dp0\target\release\azul.dll" "%~dp0\target\release\examples\"

cd "%~dp0\target\release\examples\"
public.exe
cd "../../../.."
