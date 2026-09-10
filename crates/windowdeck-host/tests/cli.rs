use std::process::Command;

#[test]
fn help_and_invalid_arguments_exit_without_starting_the_host() {
    for (args, success, expected) in [
        (vec!["--help"], true, "Uso recomendado"),
        (vec!["diag"], true, "Diagnosticos"),
        (vec!["diag", "--help"], true, "gpu-encode"),
        (vec!["--version"], true, "windowdeck-host"),
        (vec!["--gpu-self-test"], false, "diag"),
        (
            vec!["diag", "gpu-encode", "auto", "extra"],
            false,
            "sobran argumentos",
        ),
        (vec!["diag", "capture", "0"], false, "mayor que cero"),
        (vec!["--version", "extra"], false, "sobran argumentos"),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_windowdeck-host"))
            .args(&args)
            .output()
            .expect("host executable");
        let log = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.status.success(), success, "{args:?}: {log}");
        assert!(log.contains(expected), "{args:?}: {log}");
        assert!(
            !log.contains("event=host_build"),
            "command dispatched a runtime mode: {log}"
        );
        assert!(
            !log.contains("windowdeck_state="),
            "command started a server: {log}"
        );
    }
}
