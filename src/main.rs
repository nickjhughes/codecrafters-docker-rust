use anyhow::{Context, Result};
use std::path::PathBuf;

use registry::Registry;

mod registry;

// Usage: your_docker.sh run <image> <command> <arg1> <arg2> ...
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    let image_name = &args[2];
    let command = &args[3];
    let command_args = &args[4..];

    // Create temporary directory
    let temp_dir = tempfile::tempdir().context("failed to create temp directory")?;

    let valid_image_name = image_name != "<some_image>";
    if valid_image_name {
        // Connect to registry and get image manifests
        let mut registry = Registry::new(image_name);
        registry.auth()?;
        registry.get_manifests()?;

        // Download image layers
        registry.download_layers(temp_dir.path())?;
    }

    // Create /dev/null in temporary directory
    std::fs::create_dir_all(temp_dir.path().join("dev/null"))
        .context("failed to create /dev/null")?;

    let command_path = if valid_image_name {
        PathBuf::from(command)
    } else {
        // Copy command binary into the temporary directory
        let command_path = PathBuf::from(command);
        let command_binary = command_path.file_name().unwrap();
        let temp_command_path = temp_dir.path().join(command_binary);

        std::fs::copy(&command_path, &temp_command_path).context(format!(
            "failed to copy command binary from {:?} to {:?}",
            &command_path, &temp_command_path,
        ))?;

        PathBuf::from("/").join(command_binary)
    };

    // chroot and chdir into the temporary directory
    std::os::unix::fs::chroot(&temp_dir).context("failed to chroot")?;
    std::env::set_current_dir("/").context("failed to chdir")?;

    // Split this process off into its own new PID namespace
    unsafe {
        libc::unshare(libc::CLONE_NEWPID);
    }

    let output = std::process::Command::new(&command_path)
        .args(command_args)
        .output()
        .with_context(|| {
            format!(
                "Tried to run {:?} with arguments {:?}",
                command_path, command_args
            )
        })?;

    let std_out = std::str::from_utf8(&output.stdout)?;
    let std_err = std::str::from_utf8(&output.stderr)?;
    print!("{}", std_out);
    eprint!("{}", std_err);

    if let Some(status_code) = output.status.code() {
        std::process::exit(status_code);
    }
    Ok(())
}
