use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

static SEQ: AtomicUsize = AtomicUsize::new(0);

fn cargo_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_check-correct-path"))
}

fn skill_bin() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../bin/check-correct-path")
}

fn temp_dir(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "ccp-equiv-{}-{}-{}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::SeqCst),
        label
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn run_bin(bin: &Path, args: &[&str]) -> Output {
    Command::new(bin).args(args).output().unwrap()
}

fn assert_matches_expected(
    bin: &Path,
    args: &[&str],
    expected_code: i32,
    expected_stdout: &str,
    expected_stderr: Option<&str>,
) {
    let out = run_bin(bin, args);
    let code = out.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert_eq!(code, expected_code, "exit code for args {args:?}: stdout={stdout} stderr={stderr}");
    assert_eq!(stdout, expected_stdout, "stdout for args {args:?}");
    if let Some(exp_err) = expected_stderr {
        assert_eq!(stderr, exp_err, "stderr for args {args:?}");
    }
}

#[test]
fn valid_paths() {
    let dir = temp_dir("valid");
    let file = dir.join("한글 장면.png");
    fs::write(&file, b"image-bytes").unwrap();
    let file_str = file.to_string_lossy().into_owned();

    let expected_masked = "{\n  \"bytes_read\": 4,\n  \"decoded\": \"<redacted>/한글 장면.png\",\n  \"decoded_once\": false,\n  \"exists\": true,\n  \"issues\": [],\n  \"mapped\": \"<redacted>/한글 장면.png\",\n  \"ok\": true,\n  \"possible_double_encoding\": false,\n  \"raw\": \"<redacted>/한글 장면.png\",\n  \"readable\": true\n}\n";
    assert_matches_expected(
        &cargo_bin(),
        &[
            "path",
            "--value",
            &file_str,
            "--target",
            "linux",
            "--require-exists",
            "--require-readable",
            "--read-bytes",
            "4",
        ],
        0,
        expected_masked,
        None,
    );

    let encoded = format!("{}/%ED%95%9C%EA%B8%80%20%EC%9E%A5%EB%A9%B4.png", dir.display());
    let expected_encoded = "{\n  \"bytes_read\": 1,\n  \"decoded\": \"<redacted>/한글 장면.png\",\n  \"decoded_once\": true,\n  \"exists\": true,\n  \"issues\": [],\n  \"mapped\": \"<redacted>/한글 장면.png\",\n  \"ok\": true,\n  \"possible_double_encoding\": false,\n  \"raw\": \"<redacted>/%ED%95%9C%EA%B8%80%20%EC%9E%A5%EB%A9%B4.png\",\n  \"readable\": true\n}\n";
    assert_matches_expected(
        &cargo_bin(),
        &[
            "path",
            "--value",
            &encoded,
            "--decode",
            "once",
            "--target",
            "linux",
            "--require-exists",
            "--require-readable",
            "--read-bytes",
            "1",
        ],
        0,
        expected_encoded,
        None,
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn invalid_paths() {
    let dir = temp_dir("invalid");
    let missing = dir.join("missing.png").to_string_lossy().into_owned();
    let expected_missing = "{\n  \"bytes_read\": 0,\n  \"decoded\": \"<redacted>/missing.png\",\n  \"decoded_once\": false,\n  \"exists\": false,\n  \"issues\": [\n    \"mapped path does not exist\",\n    \"mapped path is not a readable file\",\n    \"cannot read bytes from an unreadable path\"\n  ],\n  \"mapped\": \"<redacted>/missing.png\",\n  \"ok\": false,\n  \"possible_double_encoding\": false,\n  \"raw\": \"<redacted>/missing.png\",\n  \"readable\": false\n}\n";
    assert_matches_expected(
        &cargo_bin(),
        &[
            "path",
            "--value",
            &missing,
            "--require-exists",
            "--require-readable",
            "--read-bytes",
            "2",
        ],
        1,
        expected_missing,
        None,
    );

    let empty_file = dir.join("empty.bin");
    fs::write(&empty_file, b"").unwrap();
    let empty_str = empty_file.to_string_lossy().into_owned();
    let expected_empty = "{\n  \"bytes_read\": 0,\n  \"decoded\": \"<redacted>/empty.bin\",\n  \"decoded_once\": false,\n  \"exists\": true,\n  \"issues\": [\n    \"file read returned zero bytes\"\n  ],\n  \"mapped\": \"<redacted>/empty.bin\",\n  \"ok\": false,\n  \"possible_double_encoding\": false,\n  \"raw\": \"<redacted>/empty.bin\",\n  \"readable\": true\n}\n";
    assert_matches_expected(
        &cargo_bin(),
        &["path", "--value", &empty_str, "--read-bytes", "4"],
        1,
        expected_empty,
        None,
    );

    let expected_bad_pct = "{\n  \"bytes_read\": 0,\n  \"decoded\": \"C:%ZZ\\\\x\",\n  \"decoded_once\": false,\n  \"exists\": false,\n  \"issues\": [\n    \"malformed percent escape\"\n  ],\n  \"mapped\": \"C:%ZZ\\\\x\",\n  \"ok\": false,\n  \"possible_double_encoding\": false,\n  \"raw\": \"C:%ZZ\\\\x\",\n  \"readable\": false\n}\n";
    assert_matches_expected(
        &cargo_bin(),
        &["path", "--value", "C:%ZZ\\x", "--target", "wsl", "--show-value"],
        1,
        expected_bad_pct,
        None,
    );

    let expected_double = "{\n  \"bytes_read\": 0,\n  \"decoded\": \"C:%2Ftemp\",\n  \"decoded_once\": true,\n  \"exists\": false,\n  \"issues\": [\n    \"percent escapes remain after one decode; possible double encoding\"\n  ],\n  \"mapped\": \"C:%2Ftemp\",\n  \"ok\": false,\n  \"possible_double_encoding\": true,\n  \"raw\": \"C:%252Ftemp\",\n  \"readable\": false\n}\n";
    assert_matches_expected(
        &cargo_bin(),
        &[
            "path",
            "--value",
            "C:%252Ftemp",
            "--decode",
            "once",
            "--target",
            "wsl",
            "--show-value",
        ],
        1,
        expected_double,
        None,
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn windows_paths() {
    let expected_win_to_wsl = "{\n  \"bytes_read\": 0,\n  \"decoded\": \"D:\\\\한글 폴더\\\\장면 1.png\",\n  \"decoded_once\": false,\n  \"exists\": false,\n  \"issues\": [],\n  \"mapped\": \"/mnt/d/한글 폴더/장면 1.png\",\n  \"ok\": true,\n  \"possible_double_encoding\": false,\n  \"raw\": \"D:\\\\한글 폴더\\\\장면 1.png\",\n  \"readable\": false\n}\n";
    assert_matches_expected(
        &cargo_bin(),
        &[
            "path",
            "--value",
            "D:\\한글 폴더\\장면 1.png",
            "--target",
            "wsl",
            "--show-value",
        ],
        0,
        expected_win_to_wsl,
        None,
    );

    let expected_wsl_to_win = "{\n  \"bytes_read\": 0,\n  \"decoded\": \"/mnt/d/한글 폴더/장면 1.png\",\n  \"decoded_once\": false,\n  \"exists\": false,\n  \"issues\": [],\n  \"mapped\": \"D:\\\\한글 폴더\\\\장면 1.png\",\n  \"ok\": true,\n  \"possible_double_encoding\": false,\n  \"raw\": \"/mnt/d/한글 폴더/장면 1.png\",\n  \"readable\": false\n}\n";
    assert_matches_expected(
        &cargo_bin(),
        &[
            "path",
            "--value",
            "/mnt/d/한글 폴더/장면 1.png",
            "--target",
            "windows",
            "--show-value",
        ],
        0,
        expected_wsl_to_win,
        None,
    );

    let expected_file_uri = "{\n  \"bytes_read\": 0,\n  \"decoded\": \"file:///C:/Users/tree/scene 1.png\",\n  \"decoded_once\": true,\n  \"exists\": false,\n  \"issues\": [],\n  \"mapped\": \"/mnt/c/Users/tree/scene 1.png\",\n  \"ok\": true,\n  \"possible_double_encoding\": false,\n  \"raw\": \"file:///C:/Users/tree/scene%201.png\",\n  \"readable\": false\n}\n";
    assert_matches_expected(
        &cargo_bin(),
        &[
            "path",
            "--value",
            "file:///C:/Users/tree/scene%201.png",
            "--show-value",
        ],
        0,
        expected_file_uri,
        None,
    );
}

#[test]
fn self_test_value_template_pipeline_and_installed_bin() {
    let expected_self_test = "{\n  \"checks\": [\n    {\n      \"name\": \"integer\",\n      \"passed\": true\n    },\n    {\n      \"name\": \"numeric string rejected\",\n      \"passed\": true\n    },\n    {\n      \"name\": \"error tuple rejected\",\n      \"passed\": true\n    },\n    {\n      \"name\": \"opaque string preserved\",\n      \"passed\": true\n    },\n    {\n      \"name\": \"template applied\",\n      \"passed\": true\n    },\n    {\n      \"name\": \"template unresolved\",\n      \"passed\": true\n    },\n    {\n      \"name\": \"template partial\",\n      \"passed\": true\n    },\n    {\n      \"name\": \"malformed percent rejected\",\n      \"passed\": true\n    },\n    {\n      \"name\": \"double encoding reported\",\n      \"passed\": true\n    },\n    {\n      \"name\": \"plus preserved\",\n      \"passed\": true\n    },\n    {\n      \"name\": \"windows to WSL\",\n      \"passed\": true\n    },\n    {\n      \"name\": \"WSL to windows\",\n      \"passed\": true\n    },\n    {\n      \"name\": \"unicode file byte read\",\n      \"passed\": true\n    }\n  ],\n  \"failures\": [],\n  \"ok\": true\n}\n";

    let installed = skill_bin();
    assert!(
        installed.is_file(),
        "installed skill binary missing at {}",
        installed.display()
    );

    for bin in [cargo_bin(), installed] {
        assert_matches_expected(&bin, &["self-test"], 0, expected_self_test, None);

        assert_matches_expected(
            &bin,
            &[
                "value",
                "--value-json",
                "\"17\"",
                "--expected-type",
                "integer",
                "--expect",
                "invalid",
            ],
            0,
            "{\n  \"actual_valid\": false,\n  \"expected\": \"invalid\",\n  \"issues\": [\n    \"expected integer, received str\",\n    \"numeric string was not implicitly coerced\"\n  ],\n  \"ok\": true,\n  \"received_type\": \"str\"\n}\n",
            None,
        );

        assert_matches_expected(
            &bin,
            &[
                "value",
                "--value-json",
                "[\"E\", \"boom\"]",
                "--expected-type",
                "array",
                "--expect",
                "valid",
            ],
            1,
            "{\n  \"actual_valid\": false,\n  \"expected\": \"valid\",\n  \"issues\": [\n    \"structured error value reached the consumer\"\n  ],\n  \"ok\": false,\n  \"received_type\": \"list\"\n}\n",
            None,
        );

        assert_matches_expected(
            &bin,
            &[
                "template",
                "--source",
                "결과/{{name}}.png",
                "--rendered",
                "결과/장면 1.png",
                "--variables-json",
                "{\"name\":\"장면 1\"}",
                "--expect-applied",
                "yes",
            ],
            0,
            "{\n  \"actual_applied\": true,\n  \"applied\": true,\n  \"diagnostic_ok\": true,\n  \"error_variables\": [],\n  \"expected_applied\": true,\n  \"issues\": [],\n  \"missing_variables\": [],\n  \"ok\": true,\n  \"placeholder_count\": 1,\n  \"unresolved\": []\n}\n",
            None,
        );

        assert_matches_expected(
            &bin,
            &[
                "pipeline",
                "--template",
                "D:\\\\out\\\\{{name}}.png",
                "--rendered",
                "D:\\out\\장면 1.png",
                "--variables-json",
                "{\"name\":\"장면 1\"}",
                "--target",
                "wsl",
                "--show-value",
            ],
            0,
            "{\n  \"first_failure\": null,\n  \"ok\": true,\n  \"path\": {\n    \"bytes_read\": 0,\n    \"decoded\": \"D:\\\\out\\\\장면 1.png\",\n    \"decoded_once\": false,\n    \"exists\": false,\n    \"issues\": [],\n    \"mapped\": \"/mnt/d/out/장면 1.png\",\n    \"ok\": true,\n    \"possible_double_encoding\": false,\n    \"raw\": \"D:\\\\out\\\\장면 1.png\",\n    \"readable\": false\n  },\n  \"template\": {\n    \"applied\": true,\n    \"error_variables\": [],\n    \"issues\": [],\n    \"missing_variables\": [],\n    \"ok\": true,\n    \"placeholder_count\": 1,\n    \"unresolved\": []\n  }\n}\n",
            None,
        );

        assert_matches_expected(
            &bin,
            &["path", "--value", "/tmp/x", "--read-bytes", "-1"],
            2,
            "",
            Some("{\"ok\": false, \"usage_error\": \"read-bytes must be non-negative\"}\n"),
        );

        assert_matches_expected(
            &bin,
            &[
                "template",
                "--source",
                "{{a}}",
                "--rendered",
                "1",
                "--variables-json",
                "[1,2]",
                "--expect-applied",
                "yes",
            ],
            2,
            "",
            Some("{\"ok\": false, \"usage_error\": \"variables JSON must be an object\"}\n"),
        );
    }
}

#[test]
fn verdict_changes_with_different_paths() {
    let dir = temp_dir("variation");
    let present = dir.join("alpha.txt");
    fs::write(&present, b"hello").unwrap();
    let absent = dir.join("beta.txt");

    let present_str = present.to_string_lossy().into_owned();
    let absent_str = absent.to_string_lossy().into_owned();

    assert_matches_expected(
        &skill_bin(),
        &["path", "--value", &present_str, "--require-exists", "--read-bytes", "3"],
        0,
        "{\n  \"bytes_read\": 3,\n  \"decoded\": \"<redacted>/alpha.txt\",\n  \"decoded_once\": false,\n  \"exists\": true,\n  \"issues\": [],\n  \"mapped\": \"<redacted>/alpha.txt\",\n  \"ok\": true,\n  \"possible_double_encoding\": false,\n  \"raw\": \"<redacted>/alpha.txt\",\n  \"readable\": true\n}\n",
        None,
    );

    assert_matches_expected(
        &skill_bin(),
        &["path", "--value", &absent_str, "--require-exists", "--read-bytes", "3"],
        1,
        "{\n  \"bytes_read\": 0,\n  \"decoded\": \"<redacted>/beta.txt\",\n  \"decoded_once\": false,\n  \"exists\": false,\n  \"issues\": [\n    \"mapped path does not exist\",\n    \"cannot read bytes from an unreadable path\"\n  ],\n  \"mapped\": \"<redacted>/beta.txt\",\n  \"ok\": false,\n  \"possible_double_encoding\": false,\n  \"raw\": \"<redacted>/beta.txt\",\n  \"readable\": false\n}\n",
        None,
    );

    let _ = fs::remove_dir_all(&dir);
}

