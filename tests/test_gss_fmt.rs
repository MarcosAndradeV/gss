use std::fs;
use std::process::Command;

#[test]
fn test_fmt_stdout_valid_file() {
    let output = Command::new(env!("CARGO_BIN_EXE_gss-fmt"))
        .arg("tests/test3.gss")
        .output()
        .expect("failed to execute gss-fmt");
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("hex = 50,"));
    assert!(stdout.contains("key = 1,"));
    assert!(stdout.contains("other = 2,"));
}

#[test]
fn test_fmt_check_already_formatted_file() {
    let temp_path = "temp/test_fmt_already_formatted.gss";
    let formatted_content = "test = {\n    hex = 50,\n    key = 1,\n    other = 2,\n},\n";
    let _ = fs::create_dir_all("temp");
    fs::write(temp_path, formatted_content).expect("failed to create temp file");

    let output = Command::new(env!("CARGO_BIN_EXE_gss-fmt"))
        .args(["--check", temp_path])
        .output()
        .expect("failed to execute gss-fmt");

    let _ = fs::remove_file(temp_path);

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
}

#[test]
fn test_fmt_check_unformatted_file() {
    let temp_path = "temp/test_fmt_unformatted.gss";
    let unformatted_content = "test={other=2,key=1,hex=0x32}";
    let _ = fs::create_dir_all("temp");
    fs::write(temp_path, unformatted_content).expect("failed to create temp file");

    let output = Command::new(env!("CARGO_BIN_EXE_gss-fmt"))
        .args(["-c", temp_path])
        .output()
        .expect("failed to execute gss-fmt");

    let _ = fs::remove_file(temp_path);

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("File not formatted: temp/test_fmt_unformatted.gss"));
}

#[test]
fn test_fmt_write_in_place() {
    let temp_path = "temp/test_fmt_write.gss";
    let unformatted_content = "a={b=1,c=2}\nz=99";
    let _ = fs::create_dir_all("temp");
    fs::write(temp_path, unformatted_content).expect("failed to create temp file");

    let output = Command::new(env!("CARGO_BIN_EXE_gss-fmt"))
        .args(["-w", temp_path])
        .output()
        .expect("failed to execute gss-fmt");

    assert_eq!(output.status.code(), Some(0));

    let formatted_file_content =
        fs::read_to_string(temp_path).expect("failed to read formatted file");
    let _ = fs::remove_file(temp_path);

    let expected = "a = {\n    b = 1,\n    c = 2,\n},\nz = 99,\n";
    assert_eq!(formatted_file_content, expected);
}

#[test]
fn test_fmt_syntax_error() {
    let temp_path = "temp/test_fmt_syntax_err.gss";
    let _ = fs::create_dir_all("temp");
    fs::write(temp_path, "invalid syntax here").expect("failed to create temp file");

    let output = Command::new(env!("CARGO_BIN_EXE_gss-fmt"))
        .arg(temp_path)
        .output()
        .expect("failed to execute gss-fmt");

    let _ = fs::remove_file(temp_path);

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.is_empty());
}

#[test]
fn test_fmt_nonexistent_file() {
    let output = Command::new(env!("CARGO_BIN_EXE_gss-fmt"))
        .arg("non_existent_file_99999.gss")
        .output()
        .expect("failed to execute gss-fmt");
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.is_empty());
}

#[test]
fn test_fmt_help() {
    let output = Command::new(env!("CARGO_BIN_EXE_gss-fmt"))
        .arg("--help")
        .output()
        .expect("failed to execute gss-fmt");
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Usage: gss-fmt"));
    assert!(stdout.contains("--write"));
    assert!(stdout.contains("--check"));
    assert!(stdout.contains("--diff"));
    assert!(stdout.contains("--indent-width"));
    assert!(stdout.contains("--use-tabs"));
}

#[test]
fn test_fmt_version() {
    let output = Command::new(env!("CARGO_BIN_EXE_gss-fmt"))
        .arg("--version")
        .output()
        .expect("failed to execute gss-fmt");
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains(&format!("gss-fmt {}", env!("CARGO_PKG_VERSION"))));
}

#[test]
fn test_fmt_diff() {
    let temp_path = "temp/test_fmt_diff.gss";
    let unformatted_content = "b=2,a=1";
    let _ = fs::create_dir_all("temp");
    fs::write(temp_path, unformatted_content).expect("failed to create temp file");

    let output = Command::new(env!("CARGO_BIN_EXE_gss-fmt"))
        .args(["--diff", "--color", "never", temp_path])
        .output()
        .expect("failed to execute gss-fmt");

    let _ = fs::remove_file(temp_path);

    // Diff mode returns exit code 1 when differences exist
    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--- a/temp/test_fmt_diff.gss"));
    assert!(stdout.contains("+++ b/temp/test_fmt_diff.gss"));
    assert!(stdout.contains("-b=2,a=1"));
    assert!(stdout.contains("+a = 1,"));
    assert!(stdout.contains("+b = 2,"));
}

#[test]
fn test_fmt_check_with_diff() {
    let temp_path = "temp/test_fmt_check_diff.gss";
    let unformatted_content = "foo = 10, bar = 20";
    let _ = fs::create_dir_all("temp");
    fs::write(temp_path, unformatted_content).expect("failed to create temp file");

    let output = Command::new(env!("CARGO_BIN_EXE_gss-fmt"))
        .args(["-c", "-d", "--color", "never", temp_path])
        .output()
        .expect("failed to execute gss-fmt");

    let _ = fs::remove_file(temp_path);

    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stdout.contains("--- a/temp/test_fmt_check_diff.gss"));
    assert!(stderr.contains("File not formatted: temp/test_fmt_check_diff.gss"));
}

#[test]
fn test_fmt_custom_indent_and_tabs_cli() {
    let temp_path = "temp/test_fmt_indent.gss";
    let content = "obj = { x = 1 }";
    let _ = fs::create_dir_all("temp");
    fs::write(temp_path, content).expect("failed to create temp file");

    // 2-space indent
    let output_2 = Command::new(env!("CARGO_BIN_EXE_gss-fmt"))
        .args(["--indent-width", "2", temp_path])
        .output()
        .expect("failed to execute gss-fmt");
    assert_eq!(output_2.status.code(), Some(0));
    let stdout_2 = String::from_utf8_lossy(&output_2.stdout);
    assert_eq!(stdout_2, "obj = {\n  x = 1,\n},\n");

    // tabs indent
    let output_tabs = Command::new(env!("CARGO_BIN_EXE_gss-fmt"))
        .args(["--use-tabs", temp_path])
        .output()
        .expect("failed to execute gss-fmt");
    assert_eq!(output_tabs.status.code(), Some(0));
    let stdout_tabs = String::from_utf8_lossy(&output_tabs.stdout);
    assert_eq!(stdout_tabs, "obj = {\n\tx = 1,\n},\n");

    let _ = fs::remove_file(temp_path);
}

#[test]
fn test_fmt_recursive_directory() {
    let temp_dir = "temp/test_fmt_rec";
    let file1 = format!("{temp_dir}/a.gss");
    let file2 = format!("{temp_dir}/sub/b.gss");
    let _ = fs::create_dir_all(format!("{temp_dir}/sub"));
    fs::write(&file1, "z=1,a=2").expect("write a");
    fs::write(&file2, "y=3,x=4").expect("write b");

    let output = Command::new(env!("CARGO_BIN_EXE_gss-fmt"))
        .args(["-w", temp_dir])
        .output()
        .expect("failed to execute gss-fmt");

    assert_eq!(output.status.code(), Some(0));

    let content1 = fs::read_to_string(&file1).expect("read a");
    let content2 = fs::read_to_string(&file2).expect("read b");
    let _ = fs::remove_dir_all(temp_dir);

    assert_eq!(content1, "a = 2,\nz = 1,\n");
    assert_eq!(content2, "x = 4,\ny = 3,\n");
}
