# Fabric Switchboard: shell hook for zsh (SB-183). Written by Switchboard; it replaces changes.
# In a folder of a group bound to an organization, `claude` and `codex` run in that group's
# shell homes with SWITCHBOARD_CONTEXT=org:<id>; leaving the folder gives your own values back.
# Off: `switchboard shell off`. It prints nothing and never fails the prompt.
if [ -n "${ZSH_VERSION-}" ] && [ -z "${SWITCHBOARD_SESSION-}" ] && [ -z "${_switchboard_shell-}" ]; then
() {
  emulate -L zsh
  typeset -g _switchboard_shell=zsh
  typeset -g _switchboard_cli=@CLI@
  typeset -g _switchboard_data=@DATA@
@CORE@
  # The CLI's answer into the caller's _sb_line. At most ten seconds for it (the CLI answers in
  # milliseconds; ten leave room for a heavily loaded computer and for macOS checking a CLI an
  # update just replaced, the first time it runs) and two more for the CLI's
  # exit status, which must be 0: a CLI that hangs never holds the prompt longer. The CLI runs
  # disowned and holds none of the terminal's files, so nothing waits for it after a timeout.
  _switchboard_ask() {
    local _sb_status
    {
      IFS= read -r -t 10 _sb_line && IFS= read -r -t 2 _sb_status
    } < <({
      "$_switchboard_cli" --data-dir "$_switchboard_data" shell env --shell zsh --dir "$PWD"
      print -r -- "$?"
    } 2>/dev/null </dev/null &!) || return 1
    [ "$_sb_status" = 0 ]
  }
  _switchboard_chpwd() {
    emulate -L zsh
    _switchboard_apply
    return 0
  }
  export SWITCHBOARD_SHELL_HOOK=1
  autoload -Uz add-zsh-hook 2>/dev/null && add-zsh-hook chpwd _switchboard_chpwd 2>/dev/null
  _switchboard_inherit
  _switchboard_chpwd
}
fi
