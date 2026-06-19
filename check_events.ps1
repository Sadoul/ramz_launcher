Get-WinEvent -FilterHashtable @{LogName='Application'; StartTime=(Get-Date).AddMinutes(-15)} -ErrorAction SilentlyContinue |
  Where-Object { $_.Message -match 'pd-launcher|doomsday|WebView|ramz' } |
  Select-Object -First 10 TimeCreated, LevelDisplayName, Message |
  Format-List
