# Fabric Switchboard: shell hook for fish (SB-183). Written by Switchboard; it replaces changes.
# In a folder of a group bound to an organization, `claude` and `codex` run in that group's
# shell homes with SWITCHBOARD_CONTEXT=org:<id>; leaving the folder gives your own values back.
# Off: `switchboard shell off`. It prints nothing and never fails the prompt.
if not set -q SWITCHBOARD_SESSION; and not set -q _switchboard_shell
    set -g _switchboard_shell fish
    set -g _switchboard_cli @CLI@
    set -g _switchboard_data @DATA@

    # $argv: N, the variable, the value wanted here (empty: none of the hook's here).
    function _switchboard_var --argument-names n var want
        set -l m _switchboard_m_$n
        set -l v _switchboard_v_$n
        set -l w _switchboard_w_$n
        set -l s _switchboard_s_$n
        set -l held 0
        set -q $m; and test "$$m" = 1; and set held 1
        if test $held = 1
            if not set -q $var; or test "$$var" != "$$v"
                # The person changed it while the hook held it: theirs from now on.
                set -e -g $m
                set -e -g $v
                set -e -g $w
                set -e -g $s
                return 0
            end
        end
        if test -n "$want"
            if test $held != 1
                if set -q $var
                    set -g $s "$$var"
                    set -g $w 1
                else
                    set -g $s ""
                    set -g $w 0
                end
                set -g $m 1
            end
            set -g $v "$want"
            set -gx $var "$want"
        else if test $held = 1
            if test "$$w" = 1
                set -gx $var "$$s"
            else
                set -e -g $var
            end
            set -e -g $m
            set -e -g $v
            set -e -g $w
            set -e -g $s
        end
        return 0
    end

    function _switchboard_apply
        set -q SWITCHBOARD_SESSION; and return 0
        test -x "$_switchboard_cli"; or return 0
        set -l line ("$_switchboard_cli" --data-dir "$_switchboard_data" shell env --shell fish --dir "$PWD" 2>/dev/null </dev/null)
        or return 0
        test (count $line) -eq 1; or return 0
        set -l context ""
        set -l claude ""
        set -l codex ""
        if test "$line" != none
            set -l parts (string split \t -- $line)
            test (count $parts) -eq 4; or return 0
            test -n "$parts[1]"; and test -n "$parts[2]"; or return 0
            set context $parts[2]
            set claude $parts[3]
            set codex $parts[4]
            test "$claude" = -; and set claude ""
            test "$codex" = -; and set codex ""
            string match -q -r '^(|/.*)$' -- $claude; or return 0
            string match -q -r '^(|/.*)$' -- $codex; or return 0
        end
        _switchboard_var claude CLAUDE_CONFIG_DIR "$claude"
        _switchboard_var codex CODEX_HOME "$codex"
        _switchboard_var context SWITCHBOARD_CONTEXT "$context"
        return 0
    end

    function _switchboard_pwd --on-variable PWD
        _switchboard_apply
    end

    # A home inherited from a hooked terminal this shell was started in: held, with "not set"
    # as the person's own, so leaving the folder never keeps a group's home.
    set -l held 0
    for pair in claude:CLAUDE_CONFIG_DIR codex:CODEX_HOME
        set -l n (string split -m1 : -- $pair)[1]
        set -l var (string split -m1 : -- $pair)[2]
        if set -q $var; and string match -q -- "$_switchboard_data/shell/*" "$$var"
            set -g _switchboard_m_$n 1
            set -g _switchboard_v_$n "$$var"
            set -g _switchboard_w_$n 0
            set -g _switchboard_s_$n ""
            set held 1
        end
    end
    if test $held = 1; and set -q SWITCHBOARD_CONTEXT; and string match -q -- 'org:*' "$SWITCHBOARD_CONTEXT"
        set -g _switchboard_m_context 1
        set -g _switchboard_v_context "$SWITCHBOARD_CONTEXT"
        set -g _switchboard_w_context 0
        set -g _switchboard_s_context ""
    end
    set -gx SWITCHBOARD_SHELL_HOOK 1
    _switchboard_apply
end
