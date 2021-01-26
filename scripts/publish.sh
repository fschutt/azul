# Script to automate publishing in the correct order
cd "./cargo/azul-css" && cargo check && cargo publish && cd "../.." &&
cd "./cargo/azul-css-parser" && cargo check && cargo publish && cd "../.." &&
cd "./cargo/azul-core" && cargo check && cargo publish && cd "../.." &&
cd "./cargo/azul-text-layout" && cargo check && cargo publish && cd "../.." &&
cd "./cargo/azulc" && cargo check && cargo publish && cd "../.." &&
cd "./cargo/azul-layout" && cargo check && cargo publish && cd "../.." &&
cd "./cargo/azul-desktop" && cargo check && cargo publish && cd "../.." &&
cd "./cargo/azul-web" && cargo check && cargo publish && cd "../.." &&
cd "./cargo/azul-dll" && cargo check && cargo publish && cd "../.." &&
cd "./cargo/azul" && cargo check && cargo publish && cd "../.." &&
cd "./cargo/azul-widgets" && cargo check && cargo publish && cd "../.." &&
cd "./cargo/azul-native-style" && cargo check && cargo publish && cd "../.."