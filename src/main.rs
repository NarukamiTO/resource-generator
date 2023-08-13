/*
 * Araumi TO - Resource server reimplementation for a certain browser tank game.
 * Copyright (C) 2023  Daniil Pryima
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as published
 * by the Free Software Foundation, either version 3 of the License, or
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

mod kind;

use std::{
  io::stdout,
  path::{Path, PathBuf},
  sync::Arc
};
use std::io::Cursor;
use std::ops::Sub;
use std::time::{Duration, Instant};

use anyhow::Result;
use araumi_protocol::{Codec, protocol_buffer::{ProtocolBuffer, FinalCodec}};
use crc::{Crc, CRC_32_ISO_HDLC};
use tokio::fs;
use tracing::{debug, info, trace};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer};
use walkdir::WalkDir;

use self::kind::ResourceDefinition;
use crate::kind::{ImageResource, LocalizedImageResource, MapResource, MultiframeTextureResource, Object3DResource, ProplibResource, ResourceInfo, SoundResource, TextureResource};

fn is_path_hidden<P: AsRef<Path>>(path: P) -> bool {
  path.as_ref().components().any(|component| {
    if let Some(name) = component.as_os_str().to_str() {
      name.starts_with('.')
    } else {
      false
    }
  })
}

fn preprocess_input_files<P: AsRef<Path>>(paths: &[P]) -> Result<Vec<&Path>> {
  let mut result = Vec::new();
  for path in paths {
    let path = path.as_ref();
    if path.try_exists()? {
      result.push(path);
    }
  }
  result.sort();

  Ok(result)
}

pub static RESOURCE_DEFINITION_FILE: &'static str = "resource.yaml";
pub static CRC: Crc<u32> = Crc::<u32>::new(&CRC_32_ISO_HDLC);

#[tokio::main]
async fn main() -> Result<()> {
  let console = tracing_subscriber::fmt::layer()
    .with_writer(Arc::new(stdout()))
    .and_then(EnvFilter::from_default_env());
  tracing_subscriber::registry().with(console).init();
  info!("Hello, world!");

  let mut input_files = 0;
  let mut output_files = 0;
  let start = Instant::now();

  info!("scanning resources...");
  let mut resources = Vec::new();
  let root = Path::new("resources");
  for entry in WalkDir::new(root) {
    let entry = entry.unwrap();
    let path = entry.path();

    let parent = path.strip_prefix(root).unwrap();
    if is_path_hidden(parent) {
      continue;
    }

    // Read full definitions
    if path.is_dir() {
      let definition_path = path.join(RESOURCE_DEFINITION_FILE);
      if !definition_path.try_exists().unwrap() {
        continue;
      }

      let definition = fs::read_to_string(&definition_path).await.unwrap();
      let mut definition: ResourceDefinition = serde_yaml::from_str(&definition).expect(&format!("failed to read definition {}", definition_path.display()));
      definition.resource_mut().init_root(path.to_path_buf());

      let name = path.strip_prefix(root)?.components().map(|component| component.as_os_str().to_str().unwrap()).collect::<Vec<_>>().join(".");
      let id = CRC.checksum(name.as_bytes());

      let mut digest = CRC.digest();
      for file in preprocess_input_files(&definition.resource().input_files().await?)? {
        if file.is_dir() {
          continue;
        }

        trace!("using {} to calculate version for {}", file.display(), id);
        digest.update(&fs::read(file).await.unwrap());
        input_files += 1;
      }
      let version = digest.finalize();

      definition
        .resource_mut()
        .init(ResourceInfo {
          name: name.clone(),
          id: id as i64,
          version: version as i64
        })
        .await?;
      debug!(
        "read resource definition {}: {:?}",
        definition_path.display(),
        definition
      );

      resources.push(definition);
    }

    // Read short definitions
    if path.is_file() {
      let file_name = path.file_name().unwrap().to_str().unwrap();
      let (file_name, extension) = file_name.rsplit_once('.').unwrap_or((&file_name, ""));
      if let Some((name, kind)) = file_name.rsplit_once('@') {
        debug!(?name, ?kind, ?extension, "discovered short resource");

        let mut definition = match kind {
          "Sound" => ResourceDefinition::Sound(SoundResource {
            root: Default::default(),
            info: None,
            sound: Some(path.to_path_buf())
          }),
          "Map" => unimplemented!("use full resource definition"),
          "Proplib" => unimplemented!("use full resource definition"),
          "Texture" => ResourceDefinition::Texture(TextureResource {
            root: Default::default(),
            info: None,
            diffuse: Some(path.to_path_buf())
          }),
          "Image" => ResourceDefinition::Image(ImageResource {
            root: Default::default(),
            info: None,
            image: Some(path.to_path_buf())
          }),
          "MultiframeTexture" => unimplemented!("use full resource definition"),
          "LocalizedImage" => unimplemented!("use full resource definition"),
          "Object3D" => unimplemented!("use full resource definition"),
          _ => unimplemented!("{} is not implemented", kind)
        };
        definition.resource_mut().init_root(path.parent().unwrap().to_path_buf());

        let name = path.strip_prefix(root)?.parent().unwrap().components().map(|component| component.as_os_str().to_str().unwrap()).collect::<Vec<_>>().join(".") + "." + name;
        let id = CRC.checksum(path.to_string_lossy().to_string().as_bytes());

        let mut digest = CRC.digest();
        for file in preprocess_input_files(&definition.resource().input_files().await?)? {
          if file.is_dir() {
            continue;
          }

          trace!("using {} to calculate version for {}", file.display(), name);
          digest.update(&fs::read(file).await.unwrap());
          input_files += 1;
        }
        let version = digest.finalize();

        definition
          .resource_mut()
          .init(ResourceInfo {
            name: name.clone(),
            id: id as i64,
            version: version as i64
          })
          .await?;
        debug!(
          "read short resource definition {}: {:?}",
          path.display(),
          definition
        );

        resources.push(definition);
      }
    }
  }

  let proplibs = resources
    .iter()
    .cloned()
    .filter(|resource| {
      if let ResourceDefinition::Proplib(_) = resource {
        true
      } else {
        false
      }
    })
    .collect::<Vec<_>>();

  let mut processed_resources = 0;
  info!("discovered {} resources", resources.len());
  for definition in &mut resources {
    if let ResourceDefinition::Map(resource) = definition {
      resource.init_proplibs(&proplibs).await?;
    }

    let info = definition.resource().get_info().as_ref().unwrap();
    let path = PathBuf::from("out")
      .join(info.encode());
    // .join(info.id.to_string())
    // .join(info.version.to_string());
    if path.try_exists()? {
      continue;
    }

    fs::create_dir_all(&path).await?;
    processed_resources += 1;

    info!("writing output files for {:?}", definition);
    for (name, data) in &definition.resource().output_files().await? {
      fs::write(path.join(name), data).await?;
      debug!("written {}:{}/{}", info.id, info.version, name);

      output_files += 1;
    }
  }

  fs::write("out/00-resources.json", serde_json::to_vec_pretty(&resources)?).await?;

  let end = Instant::now();
  info!("completed in {:?}", end - start);
  info!("processed {} ({} cached) resources: generated {} files from {} files", processed_resources, resources.len() - processed_resources, output_files, input_files);

  Ok(())
}
