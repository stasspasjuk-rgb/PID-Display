$ErrorActionPreference = 'Stop'

if ($env:PID_AUDIO) {
    try {
        if ([IO.Path]::GetExtension($env:PID_AUDIO) -eq '.wav') {
            $player = New-Object Media.SoundPlayer $env:PID_AUDIO
            try { $player.PlaySync() } finally { $player.Dispose() }
            exit 0
        }
        Add-Type -AssemblyName PresentationCore
        Add-Type -AssemblyName System.Windows.Forms
        $player = New-Object Windows.Media.MediaPlayer
        try {
            $player.Open([Uri]::new($env:PID_AUDIO))
            $deadline = [DateTime]::UtcNow.AddSeconds(4)
            while (-not $player.NaturalDuration.HasTimeSpan -and [DateTime]::UtcNow -lt $deadline) {
                [Windows.Forms.Application]::DoEvents()
                Start-Sleep -Milliseconds 50
            }
            if (-not $player.NaturalDuration.HasTimeSpan) { throw 'Cannot open recording' }
            $player.Play()
            Start-Sleep -Milliseconds ([int]$player.NaturalDuration.TimeSpan.TotalMilliseconds + 100)
        } finally { $player.Close() }
        exit 0
    } catch {
        # Try the local Czech voice when a recording cannot be decoded.
    }
}

try {
    if ($env:PID_GONG -eq 'next') {
        [Console]::Beep(740, 90)
        [Console]::Beep(988, 135)
    } else {
        [Console]::Beep(988, 90)
        [Console]::Beep(740, 135)
    }
} catch {}

try {
    Add-Type -AssemblyName System.Runtime.WindowsRuntime
    $null = [Windows.Media.SpeechSynthesis.SpeechSynthesizer, Windows.Media.SpeechSynthesis, ContentType=WindowsRuntime]
    $streamType = [Windows.Media.SpeechSynthesis.SpeechSynthesisStream, Windows.Media.SpeechSynthesis, ContentType=WindowsRuntime]
    $voice = [Windows.Media.SpeechSynthesis.SpeechSynthesizer]::AllVoices |
        Where-Object { $_.Language -like 'cs-*' } | Select-Object -First 1
    if (-not $voice) { throw 'No Czech WinRT voice' }
    $synth = New-Object Windows.Media.SpeechSynthesis.SpeechSynthesizer
    try {
        $synth.Voice = $voice
        $asTask = [System.WindowsRuntimeSystemExtensions].GetMethods() | Where-Object {
            $_.Name -eq 'AsTask' -and $_.IsGenericMethod -and $_.GetParameters().Count -eq 1 -and
            $_.GetParameters()[0].ParameterType.Name -eq ('IAsyncOperation' + [char]96 + '1')
        } | Select-Object -First 1
        $operation = $synth.SynthesizeTextToStreamAsync($env:PID_PHRASE)
        $task = $asTask.MakeGenericMethod($streamType).Invoke($null, @($operation))
        $wave = $task.GetAwaiter().GetResult()
        $stream = [System.IO.WindowsRuntimeStreamExtensions]::AsStreamForRead($wave)
        try {
            $player = New-Object Media.SoundPlayer
            try { $player.Stream = $stream; $player.PlaySync() } finally { $player.Dispose() }
        } finally { $stream.Dispose() }
    } finally { $synth.Dispose() }
    exit 0
} catch {
    try {
        $sapi = New-Object -ComObject SAPI.SpVoice
        $voice = $sapi.GetVoices('Language=405') | Select-Object -First 1
        if (-not $voice) { exit 2 }
        $sapi.Voice = $voice
        $null = $sapi.Speak($env:PID_PHRASE)
        exit 0
    } catch { exit 2 }
}
