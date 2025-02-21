use anyhow::anyhow;
use base64::Engine;
use tokio::io::AsyncWriteExt;

use crate::{
    egg::{ei_request, encode_to_byte, extract_contracts},
    types::BASE64,
};

pub(crate) fn build_reqwest_client() -> reqwest::Client {
    reqwest::ClientBuilder::new()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap()
}

pub(crate) async fn download_contract(ei: &str) -> anyhow::Result<()> {
    let write_file = |filename| async move {
        tokio::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(filename)
            .await
            .map_err(|e| anyhow!("Open {filename} error: {e:?}"))
    };
    let client = build_reqwest_client();
    let response = ei_request(&client, ei).await?;
    let Some(contracts) = extract_contracts(&response) else {
        println!("Contracts is empty");
        return Ok(());
    };
    let bin = BASE64.encode(encode_to_byte(contracts));
    let mut file = write_file("binary.bin").await?;
    file.write_all(bin.as_bytes())
        .await
        .map_err(|e| anyhow!("Write binary.bin error: {e:?}"))?;

    let mut file = write_file("human.txt").await?;
    file.write_all(format!("{contracts:#?}").as_bytes())
        .await
        .map_err(|e| anyhow!("Write human.txt error: {e:?}"))?;

    Ok(())
}
