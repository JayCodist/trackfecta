// Temporary diagnostic: write the cached app token to a 600 file for local API checks.
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;

fn main() {
    let e = keyring::Entry::new("com.togglinux.app", "toggl_api_token").unwrap();
    match e.get_password() {
        Ok(t) => {
            let mut f = std::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .mode(0o600)
                .open("/tmp/.tl_token")
                .unwrap();
            f.write_all(t.as_bytes()).unwrap();
            println!("ok len={}", t.len());
        }
        Err(err) => println!("no token: {err}"),
    }
}
