use std::process::Command;

#[test]
fn informational_commands_exit_without_a_panel_or_broker() {
    for flag in ["--help", "--version"] {
        let output = Command::new(env!("CARGO_BIN_EXE_windowdeck-launcher"))
            .arg(flag)
            .output()
            .unwrap();
        assert!(output.status.success());
        assert!(String::from_utf8_lossy(&output.stdout).contains("WindowDeck"));
    }
    let output = Command::new(env!("CARGO_BIN_EXE_windowdeck-launcher"))
        .args(["--native", "extra"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("Argumentos inválidos"));
}
