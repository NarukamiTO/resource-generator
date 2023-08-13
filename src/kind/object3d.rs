use std::{
  collections::HashMap,
  io::{self, Cursor},
  path::PathBuf
};

use anyhow::Result;
use araumi_protocol::{
  protocol_buffer::{FinalCodec, ProtocolBuffer},
  Codec
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tara::TaraArchive;
use tokio::fs;

use super::Resource;
use crate::kind::ResourceInfo;

#[derive(Debug, Serialize)]
#[serde(rename = "images")]
pub struct ImagesXml {
  #[serde(rename = "image")]
  pub images: Vec<ImageXml>
}

#[derive(Debug, Serialize)]
pub struct ImageXml {
  #[serde(rename = "@name")]
  pub name: String,
  #[serde(rename = "@new-name")]
  pub diffuse: String,
  #[serde(rename = "@alpha")]
  #[serde(skip_serializing_if = "Option::is_none")]
  #[serde(default)]
  pub alpha: Option<String>
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Object3DImage {
  Simple(PathBuf),
  Complex { diffuse: PathBuf, alpha: PathBuf }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Object3DResource {
  #[serde(skip_deserializing)]
  pub root: PathBuf,
  #[serde(skip_deserializing)]
  pub info: Option<ResourceInfo>,
  pub object: Option<PathBuf>,
  pub images: HashMap<String, Object3DImage>
}

#[async_trait]
impl Resource for Object3DResource {
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
    let mut files = vec![self.get_object()];
    for (_, image) in &self.images {
      match image {
        Object3DImage::Simple(diffuse) => {
          files.push(self.root.join(diffuse.clone()));
        }
        Object3DImage::Complex { diffuse, alpha } => {
          files.push(self.root.join(diffuse.clone()));
          files.push(self.root.join(alpha.clone()));
        }
      }
    }

    Ok(files)
  }

  async fn output_files(&self) -> Result<HashMap<String, Vec<u8>>> {
    let mut files = HashMap::new();
    files.insert(
      "images.xml".to_owned(),
      quick_xml::se::to_string(&ImagesXml {
        images: self
          .images
          .iter()
          .map(|(name, image)| match image {
            Object3DImage::Simple(diffuse) => ImageXml {
              name: name.clone(),
              diffuse: diffuse
                .clone()
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_string(),
              alpha: None
            },
            Object3DImage::Complex { diffuse, alpha } => ImageXml {
              name: name.clone(),
              diffuse: diffuse
                .clone()
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_string(),
              alpha: Some(
                alpha
                  .clone()
                  .file_name()
                  .unwrap()
                  .to_string_lossy()
                  .to_string()
              )
            }
          })
          .collect()
      })?
      .into_bytes()
    );

    for file in self.input_files().await? {
      files.insert(
        file.file_name().unwrap().to_str().unwrap().to_owned(),
        fs::read(file).await.unwrap()
      );
    }

    Ok(files)
  }
}

impl Object3DResource {
  pub fn get_object(&self) -> PathBuf {
    self
      .object
      .clone()
      .map(|file| if file.starts_with(&self.root) { file } else { self.get_root().join(file) })
      .unwrap_or_else(|| self.get_root().join("object.3ds"))
  }
}
