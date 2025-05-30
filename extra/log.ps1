# Infinite loop to log current date and time to a file

$dir = (Get-Location).Path
$logFile = "$dir\log.txt"

while ($true) {
    $timestamp = Get-Date -Format "yyyy-MM-dd HH:mm:ss"
    "$timestamp" | Out-File -FilePath $logFile -Append
    Write-Output "$timestamp"
    Start-Sleep -Seconds 1 
}