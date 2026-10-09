<#
.SYNOPSIS
Starts the deployed glazewm in the interactive session; run through the vmtest-sut-glazewm scheduled task.
#>
$env:GLAZEWM_CONFIG_PATH = 'C:\vmtest\glazewm\config.yaml'
Start-Process -FilePath 'C:\vmtest\glazewm\glazewm.exe' -ArgumentList 'start' -WorkingDirectory 'C:\vmtest\glazewm' `
    -RedirectStandardOutput 'C:\vmtest\glazewm\stdout.log' -RedirectStandardError 'C:\vmtest\glazewm\stderr.log'
