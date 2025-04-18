/*
 * Narukami TO - a server software reimplementation for a certain browser tank game.
 * Copyright (c) 2025  Daniil Pryima
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::fs;

use super::Resource;
use crate::kind::ResourceInfo;

#[derive(Debug, Serialize)]
#[serde(rename = "images")]
pub struct ImagesXml {
  #[serde(rename = "image")]
  pub images: Vec<ImageXml>,
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
  pub alpha: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Object3DImage {
  Simple(PathBuf),
  Complex { diffuse: PathBuf, alpha: PathBuf },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Object3DResource {
  #[serde(skip_deserializing)]
  pub root: PathBuf,
  #[serde(skip_deserializing)]
  pub info: Option<ResourceInfo>,
  pub id: Option<u32>,
  pub object: Option<PathBuf>,
  pub images: HashMap<String, Object3DImage>,
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
    for image in self.images.values() {
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
              diffuse: diffuse.clone().file_name().unwrap().to_string_lossy().to_string(),
              alpha: None,
            },
            Object3DImage::Complex { diffuse, alpha } => ImageXml {
              name: name.clone(),
              diffuse: diffuse.clone().file_name().unwrap().to_string_lossy().to_string(),
              alpha: Some(alpha.clone().file_name().unwrap().to_string_lossy().to_string()),
            },
          })
          .collect(),
      })?
      .into_bytes(),
    );

    for file in self.input_files().await? {
      files.insert(
        file.file_name().unwrap().to_str().unwrap().to_owned(),
        fs::read(file).await.unwrap(),
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
      .map(|file| {
        if file.starts_with(&self.root) {
          file
        } else {
          self.get_root().join(file)
        }
      })
      .unwrap_or_else(|| self.get_root().join("object.3ds"))
  }
}
