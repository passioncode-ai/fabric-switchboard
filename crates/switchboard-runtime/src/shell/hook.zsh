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
