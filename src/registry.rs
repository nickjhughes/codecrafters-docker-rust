use anyhow::Result;
use reqwest::blocking::Client;
use serde::Deserialize;
use std::path::Path;

pub struct Registry {
    client: Client,
    image_name: String,
    image_tag: String,
    token: Option<String>,
    manifest: Option<Manifest>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    schema_version: usize,
    media_type: String,
    config: Config,
    layers: Vec<Layer>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    media_type: String,
    size: usize,
    digest: String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Layer {
    media_type: String,
    size: usize,
    digest: String,
}

impl Registry {
    const SERVICE: &'static str = "registry.docker.io";
    const AUTH_URL: &'static str = "https://auth.docker.io/token";
    const REGISTRY_API_URL: &'static str = "https://registry-1.docker.io/v2";

    pub fn new(image_name: &str) -> Self {
        let (image_name, image_tag) = image_name.split_once(':').unwrap();

        Registry {
            image_name: image_name.to_owned(),
            image_tag: image_tag.to_owned(),
            client: Client::new(),
            token: None,
            manifest: None,
        }
    }

    pub fn auth(&mut self) -> Result<()> {
        let scope = format!("repository:library/{}:pull", self.image_name);
        let request = self
            .client
            .get(Self::AUTH_URL)
            .query(&[("service", Self::SERVICE), ("scope", &scope)]);
        let resp = request.send()?;
        let body: serde_json::Value = serde_json::from_str(&resp.text()?)?;
        if let Some(token_value) = body.get("token") {
            self.token = Some(token_value.as_str().unwrap().to_string());
            // println!("Authed");
        } else {
            anyhow::bail!("auth failed, no token in response");
        }
        Ok(())
    }

    pub fn get_manifests(&mut self) -> Result<()> {
        assert!(self.token.is_some());

        let manifest_url = format!(
            "{}/library/{}/manifests/{}",
            Self::REGISTRY_API_URL,
            self.image_name,
            self.image_tag
        );
        let request = self
            .client
            .get(manifest_url)
            .header(
                "Authorization",
                format!("Bearer {}", self.token.as_ref().unwrap()),
            )
            .header(
                "Accept",
                "application/vnd.docker.distribution.manifest.v2+json",
            );
        let resp = request.send()?;
        if resp.status().is_success() {
            let text = resp.text()?;
            let manifest: Manifest = serde_json::from_str(&text)?;
            self.manifest = Some(manifest);
            // println!(
            //     "Got manifest, with {} layers",
            //     self.manifest.as_ref().unwrap().layers.len()
            // );
        } else {
            anyhow::bail!("request failed: {:?}", resp.text());
        }

        Ok(())
    }

    pub fn download_layers(&self, path: &Path) -> Result<()> {
        assert!(self.token.is_some());
        assert!(self.manifest.is_some());

        for layer in self.manifest.as_ref().unwrap().layers.iter() {
            let blob_url = format!(
                "{}/library/{}/blobs/{}",
                Self::REGISTRY_API_URL,
                self.image_name,
                layer.digest
            );
            let request = self.client.get(blob_url).header(
                "Authorization",
                format!("Bearer {}", self.token.as_ref().unwrap()),
            );
            let resp = request.send()?;

            if resp.status().is_success() {
                let layer_bytes = resp.bytes()?;
                assert_eq!(layer_bytes.len(), layer.size);

                let tar = flate2::read::GzDecoder::new(&layer_bytes[..]);
                let mut archive = tar::Archive::new(tar);
                archive.unpack(path)?;
            } else {
                anyhow::bail!("request failed: {:?}", resp.text());
            }
        }

        Ok(())
    }
}
