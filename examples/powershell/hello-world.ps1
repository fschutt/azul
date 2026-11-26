Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

Import-Module (Join-Path $PSScriptRoot 'Azul.psd1') -Force
Set-AzulLibraryPath -Path $PSScriptRoot

# The data model is any object. The callbacks are script blocks, converted to the typed
# delegates of the binding (the data first, then the engine's value in its wrapper class), and
# the binding hands the model back to them as it was.
$model = [PSCustomObject]@{ Counter = 5 }

$onClick = [Azul.HostInvoker+ButtonOnClickCallbackWithData[object]] {
    param($m, $info)
    $m.Counter = $m.Counter + 1
    return [Azul.Update]::RefreshDom
}

$layoutBlock = {
    param($m, $info)
    $label = [Azul.Dom]::CreatePWithText([string]$m.Counter).WithCss('font-size: 32px; margin: 0;')

    $button = [Azul.Button]::Create('Increase counter').WithButtonType([Azul.ButtonType]::Primary)
    $button.SetOnClick[object]($m, $onClick)

    return [Azul.Dom]::CreateBody().WithChild($label).WithChild($button.Dom())
}.GetNewClosure()
$layout = [Azul.HostInvoker+LayoutCallbackWithData[object]]$layoutBlock

$window = [Azul.WindowCreateOptions]::Create[object]($layout)
$app = [Azul.App]::Create[object]($model, [Azul.AppConfig]::Create())
$app.Run($window)
