# Copy target/codegen/Azul.psd1 + Azul.psm1 and libazul here, then: pwsh ./main.ps1
Import-Module ./Azul.psd1
. ./styles.ps1

$styleBtn = Get-StyleBtn
Write-Host "Get-StyleBtn: $($styleBtn.len) properties"
