use std::collections::HashMap;
use std::io;
use std::io::{Cursor, Read, Write};
use std::path::PathBuf;

use anyhow::Result;
use araumi_protocol::protocol_buffer::{ProtocolBuffer, ProtocolBufferCompressedExt};
use araumi_protocol::Codec;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::fs;
use tracing::info;
use walkdir::WalkDir;

use super::Resource;
use crate::kind::ResourceInfo;
use crate::RESOURCE_DEFINITION_FILE;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LocalizationResource {
  #[serde(skip_deserializing)]
  pub root: PathBuf,
  #[serde(skip_deserializing)]
  pub info: Option<ResourceInfo>,
  #[serde(skip_serializing)]
  pub images: HashMap<String, PathBuf>,
  #[serde(skip_serializing)]
  pub strings: HashMap<String, String>,
}

#[derive(Debug, Codec)]
struct Localization {
  pub images: Vec<LocalizationImage>,
  pub strings: Vec<LocalizationString>,
}

#[derive(Debug, Codec)]
struct LocalizationImage {
  pub key: String,
  pub value: Vec<u8>,
}

#[derive(Debug, Codec)]
struct LocalizationString {
  pub key: String,
  pub value: String,
}

#[async_trait]
impl Resource for LocalizationResource {
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

    let mut images = Vec::new();
    for (key, value) in &self.images {
      let file_path = value.parent().unwrap().join("images").join(value.file_name().unwrap());
      let file_path = self.root.join(file_path);

      images.push(LocalizationImage {
        key: key.clone(),
        value: fs::read(file_path).await.unwrap(),
      });
    }

    let localization = Localization {
      images: vec![],
      strings: self
        .strings
        .iter()
        .map(|(key, value)| LocalizationString {
          key: key.clone(),
          value: value.clone(),
        })
        .collect(),
    };
    let mut protocol_buffer = ProtocolBuffer::new();
    localization.encode(&mut protocol_buffer).unwrap();

    info!("Encoded protocol buffer: {:?}", protocol_buffer.data.get_ref().len());

    let mut data = Cursor::new(Vec::new());
    protocol_buffer.encode_compressed(&mut data).unwrap();

    let position = data.position();
    let mut data = data.into_inner();
    data.drain(..position as usize);

    {
      let mut data = Cursor::new(data.clone());
      let mut protocol_buffer = ProtocolBuffer::decode_compressed(&mut data).unwrap();
      info!("Decoded protocol buffer: {:?}", protocol_buffer.data.get_ref().len());

      let localization = Localization::decode(&mut protocol_buffer).unwrap();
      info!("Decoded localization: {:?}", localization);
    }

    let (_, name) = self.info.as_ref().unwrap().name.rsplit_once(".").unwrap();
    files.insert(format!("{}.l18n", name), data);

    Ok(files)
  }
}
