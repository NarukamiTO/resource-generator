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
use walkdir::WalkDir;

use super::Resource;
use crate::kind::ResourceInfo;
use crate::RESOURCE_DEFINITION_FILE;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LocalizedImageResource {
  #[serde(skip_deserializing)]
  pub root: PathBuf,
  #[serde(skip_deserializing)]
  pub info: Option<ResourceInfo>,
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
    for entry in WalkDir::new(self.get_root()) {
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
      files.insert(format!("{}.tnk", name), fs::read(file).await.unwrap());
    }

    Ok(files)
  }
}
