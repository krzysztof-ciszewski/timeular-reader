use std::{io, thread};

use anyhow::{Context as _, Result};
use tokio::sync::oneshot;

pub async fn read_line() -> Result<String> {
    run_on_input_thread(|| {
        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .map_err(anyhow::Error::from)?;
        Ok(input)
    })
    .await
}

pub async fn read_password(prompt: String) -> Result<String> {
    run_on_input_thread(move || rpassword::prompt_password(prompt).map_err(anyhow::Error::from))
        .await
        .context("interactive input thread stopped unexpectedly")
}

pub(crate) async fn run_on_input_thread<T, F>(read: F) -> Result<T>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T> + Send + 'static,
{
    let (sender, receiver) = oneshot::channel();
    thread::spawn(move || {
        let _ = sender.send(read());
    });
    receiver
        .await
        .context("interactive input thread stopped unexpectedly")?
}
