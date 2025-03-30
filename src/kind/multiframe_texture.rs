/*
 * Araumi TO - a server software reimplementation for a certain browser tank game.
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
use std::io::{self, Cursor};
use std::path::PathBuf;

use anyhow::Result;
use araumi_protocol::protocol_buffer::{FinalCodec, ProtocolBuffer};
use araumi_protocol::Codec;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tara::TaraArchive;
use tokio::fs;

use super::Resource;
use crate::kind::ResourceInfo;

#[derive(Clone, Debug, Serialize, Deserialize, Codec)]
pub struct MultiframeTextureProperties {
  pub fps: f32,
  pub frame_height: i32,
  pub frame_width: i32,
  pub image_height: i32,
  pub image_width: i32,
  pub frames: i16,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MultiframeTextureResource {
  #[serde(skip_deserializing)]
  pub root: PathBuf,
  #[serde(skip_deserializing)]
  pub info: Option<ResourceInfo>,
  pub diffuse: Option<PathBuf>,
  pub alpha: Option<PathBuf>,
  pub properties: MultiframeTextureProperties,
}

#[async_trait]
impl Resource for MultiframeTextureResource {
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
    Ok(vec![self.get_diffuse(), self.get_alpha()])
  }

  async fn output_files(&self) -> Result<HashMap<String, Vec<u8>>> {
    let mut archive = TaraArchive::new();

    // Follow original order: p, a, i
    archive.add_entry("p".to_owned(), self.get_properties_file()?);

    let alpha = self.get_alpha();
    if alpha.try_exists()? {
      archive.add_entry("a".to_owned(), fs::read(alpha).await.unwrap());
    }

    let diffuse = self.get_diffuse();
    if diffuse.try_exists()? {
      archive.add_entry("i".to_owned(), fs::read(diffuse).await.unwrap());
    }

    let mut data = Vec::new();
    archive.write(&mut data)?;

    Ok(HashMap::from([("image.tara".to_owned(), data)]))
  }
}

impl MultiframeTextureResource {
  pub fn get_diffuse(&self) -> PathBuf {
    self
      .diffuse
      .clone()
      .map(|file| {
        if file.starts_with(&self.root) {
          file
        } else {
          self.get_root().join(file)
        }
      })
      .unwrap_or_else(|| self.get_root().join("diffuse.jpg"))
  }

  pub fn get_alpha(&self) -> PathBuf {
    self
      .diffuse
      .clone()
      .map(|file| {
        if file.starts_with(&self.root) {
          file
        } else {
          self.get_root().join(file)
        }
      })
      .unwrap_or_else(|| self.get_root().join("alpha.jpg"))
  }

  fn get_properties_file(&self) -> io::Result<Vec<u8>> {
    let mut buffer = ProtocolBuffer::new();
    self.properties.encode(&mut buffer)?;

    let mut data = Cursor::new(Vec::new());
    buffer.encode(&mut data)?;

    let position = data.position() as usize;
    Ok(data.get_ref()[position..].to_vec())
  }
}
