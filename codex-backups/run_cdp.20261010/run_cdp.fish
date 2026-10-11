function run_cdp --description 'Open or attach to the shared WSL Chromium CDP endpoint'
    argparse \
        'p/port=' \
        'd/data-dir=' \
        'b/browser=' \
        'e/extension=' \
        'u/url=' \
        'preflight-receipt=' \
        'state-file=' \
        'active-project-test-tmp=' \
        'o/operation-id=' \
        'stale-artifact-reason=' \
        'replace-owned-endpoint' \
        'detach' \
        'headless' \
        'no-extension' \
        'h/help' \
        -- $argv
    or return 2

    if set -q _flag_help
        echo 'Usage: run_cdp --active-project-test-tmp PROJECT/test/tmp --preflight-receipt PROJECT/test/tmp/receipt.json --state-file PROJECT/test/tmp/state.json [-p PORT] [-d DATA_DIR] [-b BROWSER] [-e EXTENSION] [-u URL] [--detach] [--headless] [--no-extension]'
        echo '       run_cdp --replace-owned-endpoint --operation-id ID --stale-artifact-reason REASON [options]'
        echo 'State must contain operations[receipt.operation_id] = { endpoint_owner, endpoint_pid: "PID", launch_count: 0, launch_reserved: false, retry_used: false } before the first launch.'
        echo 'Default 9222 attaches to a healthy endpoint. Replacement is only for the active operation owner after stale-artifact evidence.'
        return 0
    end

    set -l port 9222
    set -l data_dir /home/tree/.config/codex-run-cdp
    set -l extension_paths
    set -l start_url about:blank
    set -l browser_path /home/tree/.cache/ms-playwright/chromium-1243/chrome-linux64/chrome

    if set -q _flag_port
        set port $_flag_port
    end
    if set -q _flag_data_dir
        set data_dir $_flag_data_dir
    end
    if set -q _flag_extension
        for extension_spec in $_flag_extension
            for extension_path in (string split ',' -- $extension_spec)
                if test -n "$extension_path"
                    set -a extension_paths $extension_path
                end
            end
        end
    end
    if set -q _flag_url
        set start_url $_flag_url
        if not string match --quiet --regex '^https?://[^[:space:]]+$' -- $start_url
            echo "run_cdp: --url must be an http(s) URL" >&2
            return 2
        end
    end
    if not string match --quiet --regex '^[0-9]+$' -- $port
        echo "run_cdp: invalid port: $port" >&2
        return 2
    end

    if not set -q _flag_preflight_receipt; or test -z "$_flag_preflight_receipt"
        echo 'run_cdp: --preflight-receipt is required before CDP attach or launch' >&2
        return 2
    end
    if not set -q _flag_state_file; or test -z "$_flag_state_file"
        echo 'run_cdp: --state-file is required before CDP attach or launch' >&2
        return 2
    end
    if not set -q _flag_active_project_test_tmp; or test -z "$_flag_active_project_test_tmp"
        echo 'run_cdp: --active-project-test-tmp is required before CDP attach or launch' >&2
        return 2
    end

    if set -q _flag_replace_owned_endpoint
        if test "$port" != 9222
            echo 'run_cdp: --replace-owned-endpoint is only valid for shared port 9222' >&2
            return 2
        end
        if not set -q _flag_operation_id; or test -z "$_flag_operation_id"
            echo 'run_cdp: --replace-owned-endpoint requires --operation-id' >&2
            return 2
        end
        if not set -q _flag_stale_artifact_reason; or test -z "$_flag_stale_artifact_reason"
            echo 'run_cdp: --replace-owned-endpoint requires --stale-artifact-reason' >&2
            return 2
        end
    else if set -q _flag_operation_id; or set -q _flag_stale_artifact_reason
        echo 'run_cdp: --operation-id and --stale-artifact-reason require --replace-owned-endpoint' >&2
        return 2
    end

    # Validate the runtime contract before probing, attaching, replacing, or
    # launching a browser. The helper is intentionally CDP/process-free.
    set -l function_path (functions --details (status current-function))
    set -l helper_path (path dirname "$function_path")/run_cdp_preflight.mjs
    if not test -f "$helper_path"
        echo "run_cdp: preflight helper missing: $helper_path" >&2
        return 1
    end
    set -l preflight_args --active-project-test-tmp "$_flag_active_project_test_tmp" --preflight-receipt "$_flag_preflight_receipt" --state-file "$_flag_state_file"
    if set -q _flag_replace_owned_endpoint
        set -a preflight_args --replacement
        set -a preflight_args --expected-operation-id "$_flag_operation_id"
    end
    set -l preflight_result (command node "$helper_path" $preflight_args)
    if test $status -ne 0
        return 1
    end
    if set -q _flag_headless
        set -l declared_visibility (command node -e 'const fs=require("fs"); const p=process.argv[1]; const r=JSON.parse(fs.readFileSync(p,"utf8")); process.stdout.write(String(r.browser_visibility ?? r.runtime_target?.browser_identity?.visibility ?? ""));' -- "$_flag_preflight_receipt" 2>/dev/null)
        if test "$declared_visibility" != "headless-only"
            echo 'run_cdp: --headless is allowed only for an explicit headless-only runtime contract' >&2
            return 1
        end
    end
    set -l retry_attach_only false
    if string match --quiet '*"action":"retry-attach-only"*' -- "$preflight_result"
        set retry_attach_only true
    end

    set -l endpoint "http://127.0.0.1:$port"
    set -l version_url "$endpoint/json/version"
    set -l version_response (command curl --fail --silent --show-error --max-time 1 $version_url 2>/dev/null)
    if test $status -eq 0; and string match --quiet '*webSocketDebuggerUrl*' -- "$version_response"
        if not set -q _flag_replace_owned_endpoint
            if test "$retry_attach_only" = true
                command node "$helper_path" --active-project-test-tmp "$_flag_active_project_test_tmp" --preflight-receipt "$_flag_preflight_receipt" --state-file "$_flag_state_file" --record-retry >/dev/null
                or return 1
            end
            echo "run_cdp: attached endpoint=$endpoint"
            echo "$version_response" | string match --regex '"Browser"[[:space:]]*:[[:space:]]*"[^"]+"'
            return 0
        end
        echo "run_cdp: replacing endpoint=$endpoint operation=$_flag_operation_id reason=$_flag_stale_artifact_reason"
    end

    if test "$retry_attach_only" = true
        echo "run_cdp: retry is attach/replay-only and no healthy endpoint is available; refusing a second browser launch" >&2
        return 1
    end

    # On shared 9222, a listener that is not a verified CDP endpoint is still
    # someone else's runtime. Default behavior is attach-only/refusal; only
    # the active operation owner may replace it with explicit stale evidence.
    if test "$port" = 9222
        set -l listener_lines (command ss -H -ltnp "sport = :$port" 2>/dev/null)
        if test (count $listener_lines) -gt 0
            if not set -q _flag_replace_owned_endpoint
                echo "run_cdp: shared listener exists on $port but is not a verified CDP endpoint; refusing replacement without --replace-owned-endpoint, --operation-id, and --stale-artifact-reason" >&2
                return 1
            end
            set -l listener_pids (string join \n -- $listener_lines | string match -ra --groups-only 'pid=([0-9]+)' | sort -u)
            if test (count $listener_pids) -eq 0
                echo "run_cdp: cannot identify TCP listener PID on $port; refusing replacement" >&2
                return 1
            end
            if test (count $listener_pids) -ne 1
                echo "run_cdp: shared port $port has multiple listener PIDs; refusing replacement" >&2
                return 1
            end
            command node "$helper_path" --active-project-test-tmp "$_flag_active_project_test_tmp" --preflight-receipt "$_flag_preflight_receipt" --state-file "$_flag_state_file" --replacement --expected-operation-id "$_flag_operation_id" --current-endpoint-pid "$listener_pids[1]" >/dev/null
            or begin
                echo 'run_cdp: replacement listener PID does not match the receipt/state owner record' >&2
                return 1
            end
            echo "run_cdp: closing owned stale TCP listener(s) on $port: $listener_pids"
            command kill -TERM $listener_pids 2>/dev/null
            for attempt in (seq 1 10)
                sleep 0.2
                set listener_lines (command ss -H -ltnp "sport = :$port" 2>/dev/null)
                test (count $listener_lines) -eq 0; and break
            end
            set listener_lines (command ss -H -ltnp "sport = :$port" 2>/dev/null)
            if test (count $listener_lines) -gt 0
                echo "run_cdp: owned listener remains on $port after graceful termination; refusing force-kill" >&2
                return 1
            end
        else if set -q _flag_replace_owned_endpoint
            echo "run_cdp: replacement requires the current shared listener PID on $port" >&2
            return 1
        end
    end

    if set -q _flag_browser
        set browser_path $_flag_browser
    else if not test -x "$browser_path"
        set browser_path (command -v google-chrome-stable; or command -v chromium; or true)
    end
    if test -z "$browser_path"; or not test -x "$browser_path"
        echo 'run_cdp: executable browser not found' >&2
        return 1
    end

    # An absent endpoint can leave only this launcher's own profile behind.
    # Clear it only while starting a new session, never on attach.
    set -l profile_pids
    for process in (ps -eo pid=,args=)
        set -l fields (string split -m 1 ' ' (string trim -- $process))
        set -l process_pid $fields[1]
        set -l process_args $fields[2]
        if test -n "$process_pid"; and test "$process_pid" != "$fish_pid"; and string match --quiet "*--user-data-dir=$data_dir*" -- "$process_args"
            set -a profile_pids $process_pid
        end
    end
    if test (count $profile_pids) -gt 0
        echo "run_cdp: closing stale launcher-profile processes: $data_dir"
        command kill -TERM $profile_pids 2>/dev/null
        for attempt in (seq 1 10)
            set -l remaining
            for process_pid in $profile_pids
                if command kill -0 $process_pid 2>/dev/null
                    set -a remaining $process_pid
                end
            end
            test (count $remaining) -eq 0; and break
            sleep 0.2
        end
        set -l remaining
        for process_pid in $profile_pids
            if command kill -0 $process_pid 2>/dev/null
                set -a remaining $process_pid
            end
        end
        if test (count $remaining) -gt 0
            echo "run_cdp: stale launcher-profile processes remain: $remaining" >&2
            return 1
        end
    end

    if not set -q _flag_no_extension
        for extension_path in $extension_paths
            if not test -f "$extension_path/manifest.json"
                echo "run_cdp: extension manifest not found: $extension_path" >&2
                return 1
            end
        end
    end
    command mkdir -p "$data_dir"
    or return 1

    set -l browser_args \
        --remote-debugging-address=127.0.0.1 \
        --remote-debugging-port=$port \
        '--remote-allow-origins=*' \
        --disable-features=DisableLoadExtensionCommandLineSwitch \
        --user-data-dir=$data_dir \
        --no-first-run \
        --no-default-browser-check \
        --disable-dev-shm-usage \
        --no-sandbox \
        $start_url
    if not set -q _flag_no_extension; and test (count $extension_paths) -gt 0
        set -l extension_csv (string join , -- $extension_paths)
        set browser_args --load-extension=$extension_csv --disable-extensions-except=$extension_csv $browser_args
    end
    if set -q _flag_headless
        set browser_args --headless=new $browser_args
    end

    set -l log_path "/tmp/run_cdp-$port.log"
    set -l reserve_args --active-project-test-tmp "$_flag_active_project_test_tmp" --preflight-receipt "$_flag_preflight_receipt" --state-file "$_flag_state_file" --reserve-launch
    if set -q _flag_replace_owned_endpoint
        set -a reserve_args --replacement --expected-operation-id "$_flag_operation_id"
    end
    command node "$helper_path" $reserve_args >/dev/null
    or begin
        echo 'run_cdp: launch reservation was rejected; refusing to spawn Chrome' >&2
        return 1
    end
    command setsid $browser_path $browser_args >$log_path 2>&1 &
    set -l browser_pid $last_pid
    set -g _run_cdp_browser_pid $browser_pid
    trap 'if test -n "$_run_cdp_browser_pid"; and command kill -0 $_run_cdp_browser_pid 2>/dev/null; command kill -TERM $_run_cdp_browser_pid 2>/dev/null; end' INT TERM
    for attempt in (seq 1 20)
        sleep 0.5
        if not command kill -0 $browser_pid 2>/dev/null
            echo "run_cdp: browser exited before CDP became ready; log=$log_path" >&2
            return 1
        end
        set version_response (command curl --fail --silent --show-error --max-time 1 $version_url 2>/dev/null)
        if test $status -eq 0; and string match --quiet '*webSocketDebuggerUrl*' -- "$version_response"
            echo "run_cdp: ready pid=$browser_pid endpoint=$endpoint"
            echo "$version_response" | string match --regex '"Browser"[[:space:]]*:[[:space:]]*"[^"]+"'
            if set -q _flag_detach
                echo "run_cdp: detached pid=$browser_pid; attach through $endpoint"
                trap - INT TERM
                set -e -g _run_cdp_browser_pid
                return 0
            end
            echo "run_cdp: waiting for Chrome pid=$browser_pid; interrupt to stop this session"
            wait $browser_pid
            set -l wait_status $status
            if test $wait_status -ne 0; and command kill -0 $browser_pid 2>/dev/null
                echo "run_cdp: stopping interrupted Chrome pid=$browser_pid" >&2
                command kill -TERM $browser_pid 2>/dev/null
                wait $browser_pid 2>/dev/null
            end
            trap - INT TERM
            set -e -g _run_cdp_browser_pid
            return $wait_status
        end
    end
    if command kill -0 $browser_pid 2>/dev/null
        command kill -TERM $browser_pid 2>/dev/null
        wait $browser_pid 2>/dev/null
    end
    trap - INT TERM
    set -e -g _run_cdp_browser_pid
    echo "run_cdp: CDP did not become ready at $endpoint; log=$log_path" >&2
    return 1
end
