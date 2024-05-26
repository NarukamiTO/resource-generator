use std::{collections::HashMap, path::PathBuf};

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tara::TaraArchive;
use tokio::fs;
use walkdir::WalkDir;

use super::Resource;
use crate::{kind::ResourceInfo, RESOURCE_DEFINITION_FILE};

#[derive(Debug, Deserialize)]
#[serde(rename = "library")]
pub struct LibraryXml {
  #[serde(rename = "@name")]
  pub name: String
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProplibResource {
  #[serde(skip_deserializing)]
  pub root: PathBuf,
  #[serde(skip_deserializing)]
  pub info: Option<ResourceInfo>,
  #[serde(skip_deserializing)]
  pub name: Option<String>,

  pub namespace: Option<String>,

  #[serde(skip)]
  pub library: Option<Library>,
  #[serde(skip)]
  pub images: Option<Images>,
  #[serde(skip)]
  pub used_files: Vec<PathBuf>
}

pub const DEFAULT_TEXTURE_NAME: &str = "$$$_DEFAULT_TEXTURE_$$$";

#[async_trait]
impl Resource for ProplibResource {
  fn init_root(&mut self, root: PathBuf) {
    self.root = root;
  }

  async fn init(&mut self, info: ResourceInfo) -> Result<()> {
    self.info = Some(info);

    let library = self.get_root().join("library.xml");
    let library = fs::read_to_string(library).await.unwrap();
    let library: LibraryXml = quick_xml::de::from_str(&library)?;
    self.name = Some(library.name);

    Ok(())
  }

  fn get_root(&self) -> PathBuf {
    self.root.clone()
  }

  fn get_info(&self) -> &Option<ResourceInfo> {
    &self.info
  }

  async fn input_files(&self) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in WalkDir::new(&self.get_root()) {
      let entry = entry?;
      if entry.file_type().is_dir() {
        continue;
      }
      if entry.file_name() == RESOURCE_DEFINITION_FILE {
        continue;
      }

      files.push(entry.path().to_path_buf())
    }
    Ok(files)
  }

  async fn output_files(&self) -> Result<HashMap<String, Vec<u8>>> {
    let mut archive = TaraArchive::new();
    for file in self.input_files().await? {
      archive.add_entry(
        file.file_name().unwrap().to_str().unwrap().to_owned(),
        fs::read(file).await.unwrap()
      );
    }

    let mut data = Vec::new();
    archive.write(&mut data)?;

    Ok(HashMap::from([("library.tara".to_owned(), data)]))
  }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename = "library")]
pub struct Library {
  #[serde(rename = "@name")]
  pub name: String,
  #[serde(rename = "prop-group")]
  pub prop_groups: Vec<PropGroup>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct PropGroup {
  #[serde(rename = "@name")]
  pub name: String,
  #[serde(rename = "prop")]
  pub props: Vec<Prop>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Prop {
  #[serde(rename = "@name")]
  pub name: String,
  // Cannot use an enum, see https://github.com/tafia/quick-xml/issues/286
  pub mesh: Option<Mesh>,
  pub sprite: Option<Sprite>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Sprite {
}

#[derive(Clone, Debug, Deserialize)]
pub struct Mesh {
  #[serde(rename = "@file")]
  pub file: String,
  #[serde(rename = "texture", default)]
  pub textures: Vec<Texture>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Texture {
  #[serde(rename = "@name")]
  pub name: String,
  #[serde(rename = "@diffuse-map")]
  pub diffuse_map: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename = "images")]
pub struct Images {
  #[serde(rename = "image")]
  pub images: Vec<Image>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Image {
  #[serde(rename = "@name")]
  pub name: String,
  #[serde(rename = "@new-name")]
  pub diffuse: String,
  #[serde(rename = "@alpha")]
  pub alpha: Option<String>,
}
