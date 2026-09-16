/// Test utilities and conditional skipping macros.

#[macro_export]
macro_rules! skip_unless {
    (java) => {
        if std::process::Command::new("java")
            .arg("-version")
            .output()
            .is_err()
        {
            println!("SKIP: Java not installed");
            return;
        }
    };
    (adb) => {
        if std::process::Command::new("adb")
            .arg("version")
            .output()
            .is_err()
        {
            println!("SKIP: ADB not installed");
            return;
        }
    };
    (network) => {
        if std::env::var("OFFLINE").is_ok() {
            println!("SKIP: Offline mode requested");
            return;
        }
    };
}
