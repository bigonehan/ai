function codex-plan --description "Run codex with Plan mode intent, then pass all args"
    if not type -q codex
        echo "codex command not found"
        return 127
    end

    # Try explicit plan-mode override first.
    codex -c 'collaboration_mode="plan"' $argv
    set -l status_code $status
    if test $status_code -eq 0
        return 0
    end

    # Fallback: run codex normally with the same args.
    codex $argv
    return $status
end

