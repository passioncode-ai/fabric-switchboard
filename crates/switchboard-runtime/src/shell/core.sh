# The transition both zsh and bash run (SB-183, node A-2). POSIX words only, and bash 3.2's
# (macOS's /bin/bash). Variables of the hook start with _switchboard_ and are never exported.
#
# Per managed variable N (claude, codex, context): _switchboard_m_N is 1 while the hook holds
# it, _switchboard_v_N is the value it set, _switchboard_w_N is 1 when the person's own value
# was set and _switchboard_s_N is that value.

# $1 N, $2 the variable, $3 the value wanted here (empty: none of the hook's here).
_switchboard_var() {
  local _sb_n _sb_var _sb_want _sb_isset _sb_cur _sb_m _sb_v _sb_w _sb_s
  _sb_n=$1
  _sb_var=$2
  _sb_want=$3
  eval "_sb_isset=\${$_sb_var+1}; _sb_cur=\${$_sb_var-}"
  eval "_sb_m=\${_switchboard_m_$_sb_n-}; _sb_v=\${_switchboard_v_$_sb_n-}"
  if [ "$_sb_m" = 1 ]; then
    if [ -z "$_sb_isset" ] || [ "$_sb_cur" != "$_sb_v" ]; then
      # The person changed it while the hook held it: theirs from now on.
      eval "_switchboard_m_$_sb_n=; _switchboard_v_$_sb_n=; _switchboard_w_$_sb_n=; _switchboard_s_$_sb_n="
      return 0
    fi
  fi
  if [ -n "$_sb_want" ]; then
    if [ "$_sb_m" != 1 ]; then
      eval "_switchboard_s_$_sb_n=\$_sb_cur; _switchboard_w_$_sb_n=\${_sb_isset:-0}; _switchboard_m_$_sb_n=1"
    fi
    eval "_switchboard_v_$_sb_n=\$_sb_want"
    export "$_sb_var=$_sb_want"
  elif [ "$_sb_m" = 1 ]; then
    eval "_sb_w=\${_switchboard_w_$_sb_n-0}; _sb_s=\${_switchboard_s_$_sb_n-}"
    if [ "$_sb_w" = 1 ]; then
      export "$_sb_var=$_sb_s"
    else
      unset "$_sb_var"
    fi
    eval "_switchboard_m_$_sb_n=; _switchboard_v_$_sb_n=; _switchboard_w_$_sb_n=; _switchboard_s_$_sb_n="
  fi
  return 0
}

# A home inherited from a hooked terminal this shell was started in (its parent saved the
# person's values, which a child never sees): held, with "not set" as the person's own, so
# leaving the folder never keeps a group's home.
_switchboard_inherit() {
  local _sb_shell _sb_held _sb_n _sb_var _sb_cur
  _sb_shell="$_switchboard_data/shell/"
  _sb_held=
  for _sb_n in claude:CLAUDE_CONFIG_DIR codex:CODEX_HOME; do
    _sb_var=${_sb_n#*:}
    _sb_n=${_sb_n%%:*}
    eval "_sb_cur=\${$_sb_var-}"
    case $_sb_cur in
      "$_sb_shell"*)
        eval "_switchboard_m_$_sb_n=1; _switchboard_v_$_sb_n=\$_sb_cur; _switchboard_w_$_sb_n=0; _switchboard_s_$_sb_n="
        _sb_held=1
        ;;
    esac
  done
  case ${SWITCHBOARD_CONTEXT-} in
    org:*)
      if [ -n "$_sb_held" ]; then
        _switchboard_m_context=1
        _switchboard_v_context=$SWITCHBOARD_CONTEXT
        _switchboard_w_context=0
        _switchboard_s_context=
      fi
      ;;
  esac
  return 0
}

# Splits the answer in the caller's _sb_line into its _sb_context, _sb_claude and _sb_codex.
# Fails on an answer of another form.
_switchboard_parse() {
  _sb_tab=$(printf '\t')
  case $_sb_line in
    *"$_sb_tab"*"$_sb_tab"*"$_sb_tab"*) ;;
    *) return 1 ;;
  esac
  _sb_pool=${_sb_line%%"$_sb_tab"*}
  _sb_rest=${_sb_line#*"$_sb_tab"}
  _sb_context=${_sb_rest%%"$_sb_tab"*}
  _sb_rest=${_sb_rest#*"$_sb_tab"}
  _sb_claude=${_sb_rest%%"$_sb_tab"*}
  _sb_codex=${_sb_rest#*"$_sb_tab"}
  case $_sb_codex in *"$_sb_tab"*) return 1 ;; esac
  [ -n "$_sb_pool" ] && [ -n "$_sb_context" ] || return 1
  [ "$_sb_claude" != - ] || _sb_claude=
  [ "$_sb_codex" != - ] || _sb_codex=
  case $_sb_claude in ''|/*) ;; *) return 1 ;; esac
  case $_sb_codex in ''|/*) ;; *) return 1 ;; esac
  return 0
}

# Asks Switchboard what applies in this folder and moves the three variables. Prints nothing.
# A CLI that is missing, older, failing or slow, or an answer of another form, reads as `none`:
# what the hook holds is given back, and nothing it does not hold changes.
_switchboard_apply() {
  local _sb_line _sb_tab _sb_rest _sb_pool _sb_context _sb_claude _sb_codex
  [ -z "${SWITCHBOARD_SESSION-}" ] || return 0
  _sb_line=
  if [ ! -x "$_switchboard_cli" ] || ! _switchboard_ask; then
    _sb_line=none
  fi
  _sb_context=
  _sb_claude=
  _sb_codex=
  if [ "$_sb_line" != none ] && ! _switchboard_parse; then
    _sb_context=
    _sb_claude=
    _sb_codex=
  fi
  _switchboard_var claude CLAUDE_CONFIG_DIR "$_sb_claude"
  _switchboard_var codex CODEX_HOME "$_sb_codex"
  _switchboard_var context SWITCHBOARD_CONTEXT "$_sb_context"
  return 0
}
