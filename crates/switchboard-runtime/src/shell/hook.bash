# Fabric Switchboard: shell hook for bash (SB-183). Written by Switchboard; it replaces changes.
# In a folder of a group bound to an organization, `claude` and `codex` run in that group's
# shell homes with SWITCHBOARD_CONTEXT=org:<id>; leaving the folder gives your own values back.
# Off: `switchboard shell off`. It prints nothing and never fails the prompt.
if [ -n "${BASH_VERSION-}" ] && [ -z "${SWITCHBOARD_SESSION-}" ] && [ -z "${_switchboard_shell-}" ]; then
  # `set -a` goes off before the hook assigns anything, so none of its own variables is
  # exported; it comes back at the end.
  case $- in *a*) set +a; _switchboard_allexport=1 ;; *) _switchboard_allexport= ;; esac
  _switchboard_shell=bash
  _switchboard_cli=@CLI@
  _switchboard_data=@DATA@
  _switchboard_pwd=$PWD
@CORE@
  # The CLI's answer into the caller's _sb_line; fails when the CLI fails. (bash has no timeout
  # for a command's output that also works in its POSIX mode.)
  _switchboard_ask() {
    _sb_line=$("$_switchboard_cli" --data-dir "$_switchboard_data" shell env --shell bash --dir "$PWD" 2>/dev/null </dev/null)
  }
  # Runs before every prompt; acts only when the folder changed, and keeps the status the
  # prompt shows.
  _switchboard_prompt() {
    local _sb_status=$? _sb_a=
    case $- in *a*) _sb_a=1; set +a ;; esac
    if [ "${_switchboard_pwd-}" != "$PWD" ]; then
      _switchboard_pwd=$PWD
      _switchboard_apply
    fi
    [ -z "$_sb_a" ] || set -a
    return $_sb_status
  }
  case ";${PROMPT_COMMAND-};" in
    *";_switchboard_prompt;"*) ;;
    *) PROMPT_COMMAND="_switchboard_prompt${PROMPT_COMMAND:+;$PROMPT_COMMAND}" ;;
  esac
  export SWITCHBOARD_SHELL_HOOK=1
  _switchboard_inherit
  _switchboard_apply
  [ -z "$_switchboard_allexport" ] || set -a
fi
