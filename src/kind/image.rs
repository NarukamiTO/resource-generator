use std::{collections::HashMap, path::PathBuf};

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::fs;

use super::Resource;
use crate::kind::ResourceInfo;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ImageResource {
  #[serde(skip_deserializing)]
  pub root: PathBuf,
  #[serde(skip_deserializing)]
  pub info: Option<ResourceInfo>,
  pub image: Option<PathBuf>
}

#[async_trait]
impl Resource for ImageResource {
  fn init_root(&mut self, root: PathBuf) {
    self.root = root;
  }

  async fn init(&mut self, info: ResourceInfo) -> Result<()> {
    self.info = Some(info);
    Ok(())
  }

  fn get_root(&self) -> PathBuf {
    self.root.clone()
  }

  fn get_info(&self) -> &Option<ResourceInfo> {
    &self.info
  }

  async fn input_files(&self) -> Result<Vec<PathBuf>> {
    Ok(vec![self.get_image()])
  }

  async fn output_files(&self) -> Result<HashMap<String, Vec<u8>>> {
    Ok(HashMap::from([(
      "image.tnk".to_owned(),
      fs::read(self.get_image()).await.unwrap()
    )]))
  }
}

impl ImageResource {
  pub fn get_image(&self) -> PathBuf {
    self
      .image
      .clone()
      .map(|file| {
        if file.starts_with(&self.root) {
          file
        } else {
          self.get_root().join(file)
        }
      })
      .unwrap_or_else(|| self.get_root().join("image.jpg"))
  }
}
