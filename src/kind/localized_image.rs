use std::{collections::HashMap, path::PathBuf};

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::fs;
use walkdir::WalkDir;

use super::Resource;
use crate::{kind::ResourceInfo, RESOURCE_DEFINITION_FILE};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LocalizedImageResource {
  #[serde(skip_deserializing)]
  pub root: PathBuf,
  #[serde(skip_deserializing)]
  pub info: Option<ResourceInfo>
}

#[async_trait]
impl Resource for LocalizedImageResource {
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
    let mut files = HashMap::new();
    for file in self.input_files().await? {
      let file_name = file.file_name().unwrap().to_str().unwrap().to_owned();
      let (name, _) = file_name.rsplit_once('.').unwrap_or((&file_name, ""));
      files.insert(
        format!("{}.tnk", name),
        fs::read(file).await.unwrap()
      );
    }

    Ok(files)
  }
}
