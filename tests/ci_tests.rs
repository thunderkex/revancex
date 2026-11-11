use std::process::Command;

#[test]
fn test_cli_help_and_version() {
    let bin = env!("CARGO_BIN_EXE_revancex");
    let help = Command::new(bin)
        .arg("--help")
        .output()
        .expect("run revancex --help");
    assert!(help.status.success());
    let stdout = String::from_utf8_lossy(&help.stdout);
    assert!(stdout.contains("validate"));
    assert!(stdout.contains("build"));

    let ver = Command::new(bin)
        .arg("--version")
        .output()
        .expect("run revancex --version");
    assert!(ver.status.success());
}
