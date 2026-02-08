fn main() {
    let output = std::process::Command::new("date")
        .args(["+%Y-%m-%d %H:%M"])
        .output()
        .expect("failed to get date");
    let date = String::from_utf8(output.stdout).unwrap().trim().to_string();
    println!("cargo:rustc-env=BUILD_DATE={}", date);
}
