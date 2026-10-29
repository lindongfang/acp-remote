use std::{env, error::Error, fs, path::PathBuf};

fn main() -> Result<(), Box<dyn Error>> {
    let directory = env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .ok_or("usage: generate_test_certificate <output-directory>")?;
    fs::create_dir_all(&directory)?;
    let certified = rcgen::generate_simple_self_signed(vec!["localhost".to_owned()])?;
    fs::write(directory.join("server.pem"), certified.cert.pem())?;
    fs::write(
        directory.join("server-key.pem"),
        certified.signing_key.serialize_pem(),
    )?;
    Ok(())
}
