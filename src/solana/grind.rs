use std::process::{Command, Output};

pub fn grind(owner: String) -> Result<String, String> {
    // Execute the external script and capture the output
    let output: Output = Command::new("bash")
        .arg("./grind.sh")
        .arg(&owner)
        .output()
        .expect("Failed to execute script");

    // Check if the script executed successfully
    if output.status.success() {
        // Convert the stdout to String
        let output_str = String::from_utf8(output.stdout)
            .map_err(|e| e.to_string())?;

        // Optional: Log or process the output string
        println!("Script output: {}", output_str);

        // Extract the filename from the output
        if let Some(line) = output_str.lines().find(|line| line.ends_with("json")) {
            let mut path_str = line.replace(".json", "");
            path_str = path_str.replace("Wrote keypair to", "");
            Ok(path_str.trim().to_string())
        } else {
            Err("No keypair file path found in script output.".to_string())
        }
    } else {
        println!("Script failed to execute. we are in else condition");
        // If script failed, convert stderr to String and return it as an error
        let error_message = String::from_utf8(output.stderr)
            .unwrap_or_else(|_| "Unknown error executing script.".to_string());
        Err(error_message)
    }
}
