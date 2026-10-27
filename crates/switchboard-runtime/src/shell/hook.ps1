# Fabric Switchboard: shell hook for PowerShell (SB-183). Written by Switchboard; it replaces changes.
# In a folder of a group bound to an organization, `claude` and `codex` run in that group's
# shell homes with SWITCHBOARD_CONTEXT=org:<id>; leaving the folder gives your own values back.
# Off: `switchboard shell off`. It prints nothing and never fails the prompt.
# Windows PowerShell 5.1 and PowerShell 7: the prompt is wrapped and acts when the location
# changed. Everything the hook keeps is a global variable (never in a child's environment), and
# it works the same whatever scope this file is read in.
if (-not [Environment]::GetEnvironmentVariable('SWITCHBOARD_SESSION') -and
    $null -eq (Get-Variable -Name '_switchboard_shell' -Scope Global -ValueOnly -ErrorAction Ignore)) {
    try {
        $global:_switchboard_shell = 'pwsh'
        $global:_switchboard_cli = @CLI@
        $global:_switchboard_data = @DATA@
        # Per managed variable (claude, codex, context): the value the hook set (v), whether the
        # person's own value was set (w) and that value (s).
        $global:_switchboard_held = @{}
        $global:_switchboard_pwd = $null

        # $n, the variable, the value wanted here (empty: none of the hook's here).
        function global:_switchboard_var([string]$n, [string]$name, [string]$want) {
            $held = $global:_switchboard_held
            $cur = [Environment]::GetEnvironmentVariable($name)
            if ($held.ContainsKey($n)) {
                if ($null -eq $cur -or $cur -cne $held[$n]['v']) {
                    # The person changed it while the hook held it: theirs from now on.
                    $held.Remove($n)
                    return
                }
            }
            if ($want) {
                if (-not $held.ContainsKey($n)) {
                    $held[$n] = @{ 'v' = $null; 'w' = ($null -ne $cur); 's' = $cur }
                }
                $held[$n]['v'] = $want
                [Environment]::SetEnvironmentVariable($name, $want)
            } elseif ($held.ContainsKey($n)) {
                if ($held[$n]['w']) {
                    [Environment]::SetEnvironmentVariable($name, $held[$n]['s'])
                } else {
                    [Environment]::SetEnvironmentVariable($name, $null)
                }
                $held.Remove($n)
            }
        }

        # One argument as the command line's parser reads it back: in double quotes, with a quote
        # escaped and the backslashes before a quote (or the end) doubled.
        function global:_switchboard_quote([string]$text) {
            '"' + (($text -replace '(\\*)"', '$1$1\"') -replace '(\\+)$', '$1$1') + '"'
        }

        # The CLI's answer, or $null. At most ten seconds for it (the CLI answers in milliseconds;
        # ten leave room for a heavily loaded computer) and two more for its output; its exit
        # status must be 0. No window opens and nothing it writes reaches the terminal.
        function global:_switchboard_ask([string]$dir) {
            $info = New-Object System.Diagnostics.ProcessStartInfo
            $info.FileName = $global:_switchboard_cli
            $info.Arguments = '--data-dir ' + (_switchboard_quote $global:_switchboard_data) +
                ' shell env --shell pwsh --dir ' + (_switchboard_quote $dir)
            $info.UseShellExecute = $false
            $info.CreateNoWindow = $true
            $info.RedirectStandardInput = $true
            $info.RedirectStandardOutput = $true
            $info.RedirectStandardError = $true
            $info.StandardOutputEncoding = New-Object System.Text.UTF8Encoding($false)
            $process = [System.Diagnostics.Process]::Start($info)
            try {
                $process.StandardInput.Close()
                $out = $process.StandardOutput.ReadToEndAsync()
                $null = $process.StandardError.ReadToEndAsync()
                if (-not $process.WaitForExit(10000)) {
                    try { $process.Kill() } catch { }
                    return $null
                }
                if (-not $out.Wait(2000)) { return $null }
                if ($process.ExitCode -ne 0) { return $null }
                return $out.Result
            } finally {
                $process.Dispose()
            }
        }

        # Asks Switchboard what applies in this folder and moves the three variables. Prints
        # nothing. A CLI that is missing, older, failing or slow, a location that is not a
        # folder, or an answer of another form, reads as `none`: what the hook holds is given
        # back, and nothing it does not hold changes.
        function global:_switchboard_apply {
            if ([Environment]::GetEnvironmentVariable('SWITCHBOARD_SESSION')) { return }
            $context = ''
            $claude = ''
            $codex = ''
            $line = $null
            $location = Get-Location
            if ($location.Provider.Name -eq 'FileSystem' -and
                [System.IO.File]::Exists($global:_switchboard_cli)) {
                try { $line = _switchboard_ask $location.ProviderPath } catch { $line = $null }
            }
            if ($line -is [string]) {
                $line = $line -replace '\r?\n\z', ''
                $parts = $line.Split([char]9)
                $ok = ($line -notmatch '[\r\n]') -and $parts.Count -eq 4 -and $parts[0] -and $parts[1]
                if ($ok) {
                    $context = $parts[1]
                    $claude = $parts[2]
                    $codex = $parts[3]
                    if ($claude -eq '-') { $claude = '' }
                    if ($codex -eq '-') { $codex = '' }
                    foreach ($folder in @($claude, $codex)) {
                        if ($folder -and -not [System.IO.Path]::IsPathRooted($folder)) { $ok = $false }
                    }
                }
                if (-not $ok) {
                    $context = ''
                    $claude = ''
                    $codex = ''
                }
            }
            _switchboard_var 'claude' 'CLAUDE_CONFIG_DIR' $claude
            _switchboard_var 'codex' 'CODEX_HOME' $codex
            _switchboard_var 'context' 'SWITCHBOARD_CONTEXT' $context
        }

        # A home inherited from a hooked terminal this shell was started in (its parent saved the
        # person's values, which a child never sees): held, with "not set" as the person's own,
        # so leaving the folder never keeps a group's home.
        function global:_switchboard_inherit {
            $shell = [System.IO.Path]::Combine($global:_switchboard_data, 'shell') +
                [System.IO.Path]::DirectorySeparatorChar
            $found = $false
            foreach ($pair in @('claude:CLAUDE_CONFIG_DIR', 'codex:CODEX_HOME')) {
                $n, $name = $pair -split ':', 2
                $value = [Environment]::GetEnvironmentVariable($name)
                if ($value -and $value.StartsWith($shell, [System.StringComparison]::OrdinalIgnoreCase)) {
                    $global:_switchboard_held[$n] = @{ 'v' = $value; 'w' = $false; 's' = $null }
                    $found = $true
                }
            }
            $context = [Environment]::GetEnvironmentVariable('SWITCHBOARD_CONTEXT')
            if ($found -and $context -and $context.StartsWith('org:', [System.StringComparison]::Ordinal)) {
                $global:_switchboard_held['context'] = @{ 'v' = $context; 'w' = $false; 's' = $null }
            }
        }

        # Runs before every prompt; acts only when the location changed, then hands the person's
        # own prompt the status and exit code it would have seen.
        $global:_switchboard_prompt = $function:prompt
        function global:prompt {
            $sb_ok = $global:?
            $sb_code = Get-Variable -Name 'LASTEXITCODE' -Scope Global -ValueOnly -ErrorAction Ignore
            try {
                $sb_here = (Get-Location).Path
                if ($global:_switchboard_pwd -cne $sb_here) {
                    $global:_switchboard_pwd = $sb_here
                    _switchboard_apply
                }
            } catch { }
            $sb_prompt = $global:_switchboard_prompt
            if ($null -ne $sb_code) { $global:LASTEXITCODE = $sb_code }
            if (-not $sb_prompt) {
                return "PS $($ExecutionContext.SessionState.Path.CurrentLocation)$('>' * ($NestedPromptLevel + 1)) "
            }
            # A command that fails quietly sets `$?` back to False, as the last thing before the
            # person's prompt runs.
            if (-not $sb_ok) { Write-Error '' -ErrorAction Ignore }
            & $sb_prompt
        }

        [Environment]::SetEnvironmentVariable('SWITCHBOARD_SHELL_HOOK', '1')
        _switchboard_inherit
        $global:_switchboard_pwd = (Get-Location).Path
        _switchboard_apply
    } catch { }
}
