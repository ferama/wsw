# Infinite loop to log current date and time to a file
$logFile = "C:\log.txt"

while ($true) {
    $timestamp = Get-Date -Format "yyyy-MM-dd HH:mm:ss"
    "$timestamp" | Out-File -FilePath $logFile -Append
    Start-Sleep -Seconds 1 
}