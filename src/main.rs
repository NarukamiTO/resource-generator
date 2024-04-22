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
  collections::HashMap,
  io::stdout,
  path::Path,
  sync::Arc,
  time::{Instant, UNIX_EPOCH}
};
use std::collections::HashSet;

use anyhow::Result;
use araumi_protocol::{protocol_buffer::FinalCodec, Codec};
use crc::{Crc, CRC_32_ISO_HDLC};
use tokio::{fs, fs::File, io::AsyncWriteExt};
use tracing::{debug, info, trace};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer};
use walkdir::WalkDir;

use self::kind::ResourceDefinition;
use crate::kind::{ImageResource, ResourceInfo, SoundResource, SwfLibraryResource, TextureResource};

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

  let out = Path::new("out");
  let root = Path::new("resources");

  if !out.try_exists().unwrap() {
    fs::create_dir_all(out).await.unwrap();
  }

  let mtimes_file = out.join("mtimes");
  let mut resource_cached_mtimes = HashMap::new();
  let mut resource_actual_mtimes = HashMap::new();
  let mut unchanged_resources = HashSet::new();

  let mut mtime_skip_files = 0;
  let mut input_files = 0;
  let mut output_files = 0;
  let start = Instant::now();

  if mtimes_file.try_exists().unwrap() {
    info!("loading resource mtimes...");
    for entry in fs::read_to_string(&mtimes_file).await.unwrap().split('\n') {
      let entry = entry.trim();
      if let Some((file, time)) = entry.split_once(": ") {
        let time = time.parse::<u128>().unwrap();

        debug!("{}: {}", file, time);
        resource_cached_mtimes.insert(file.to_owned(), time);
      }
    }
  }

  info!("scanning resources...");
  let mut resources = Vec::new();
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
      let mut definition: ResourceDefinition = serde_yaml::from_str(&definition).expect(&format!(
        "failed to read definition {}",
        definition_path.display()
      ));
      definition.resource_mut().init_root(path.to_path_buf());

      let name = path
        .strip_prefix(root)?
        .components()
        .map(|component| component.as_os_str().to_str().unwrap())
        .collect::<Vec<_>>()
        .join(".");
      let mut id = CRC.checksum(name.as_bytes());
      if let ResourceDefinition::Object3D(resource) = &definition {
        if let Some(forced_id) = resource.id {
          id = forced_id;
        }
      }

      let mut raw_input_files = definition.resource().input_files().await?;
      raw_input_files.push(definition_path.clone());
      let preprocessed_input_files = preprocess_input_files(&raw_input_files)?;

      let mtime_input_files = preprocessed_input_files.clone();

      let mut changed = false;
      for file in &mtime_input_files {
        if file.is_dir() {
          continue;
        }

        let cache_path = file.strip_prefix(root).unwrap().to_str().unwrap();

        let actual_mtime = fs::metadata(file)
          .await
          .unwrap()
          .modified()
          .map(|time| time.duration_since(UNIX_EPOCH).unwrap().as_millis())
          .expect("unsupported platform");
        resource_actual_mtimes.insert(cache_path.to_owned(), actual_mtime);

        if let Some(cached_mtime) = resource_cached_mtimes.get(cache_path) {
          if actual_mtime == *cached_mtime {
            debug!("{} has not changed", file.display());
            continue;
          }

          debug!("{} has changed", file.display());
          changed = true;
        } else {
          debug!("new file {}", file.display());
          changed = true;
        }
      }

      if !changed {
        debug!("skipping {} as no files have been changed", name);
        mtime_skip_files += 1;
        unchanged_resources.insert(id as i64);
        // continue;
      }

      let mut digest = CRC.digest();
      for file in &preprocessed_input_files {
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
          "SwfLibrary" => ResourceDefinition::SwfLibrary(SwfLibraryResource {
            root: Default::default(),
            info: None,
            file: Some(path.to_path_buf())
          }),
          _ => unimplemented!("{} is not implemented", kind)
        };
        definition
          .resource_mut()
          .init_root(path.parent().unwrap().to_path_buf());

        let name = path
          .strip_prefix(root)?
          .parent()
          .unwrap()
          .components()
          .map(|component| component.as_os_str().to_str().unwrap())
          .collect::<Vec<_>>()
          .join(".")
          + "."
          + name;
        let id = CRC.checksum(path.to_string_lossy().to_string().as_bytes());

        let mut raw_input_files = definition.resource().input_files().await?;
        raw_input_files.push(path.to_owned());
        let preprocessed_input_files = preprocess_input_files(&raw_input_files)?;

        let mtime_input_files = preprocessed_input_files.clone();

        let mut changed = false;
        for file in &mtime_input_files {
          if file.is_dir() {
            continue;
          }

          let cache_path = file.strip_prefix(root).unwrap().to_str().unwrap();

          let actual_mtime = fs::metadata(file)
            .await
            .unwrap()
            .modified()
            .map(|time| time.duration_since(UNIX_EPOCH).unwrap().as_millis())
            .expect("unsupported platform");
          resource_actual_mtimes.insert(cache_path.to_owned(), actual_mtime);

          if let Some(cached_mtime) = resource_cached_mtimes.get(cache_path) {
            if actual_mtime == *cached_mtime {
              debug!("{} has not changed", file.display());
              continue;
            }

            debug!("{} has changed", file.display());
            changed = true;
          } else {
            debug!("new file {}", file.display());
            changed = true;
          }
        }

        if !changed {
          debug!("skipping {} as no files have been changed", name);
          mtime_skip_files += 1;
          unchanged_resources.insert(id as i64);
          // continue;
        }

        let mut digest = CRC.digest();
        for file in &preprocessed_input_files {
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

  info!("discovered {} resources", resources.len());

  {
    debug!("writing mtimes file...");
    let mut mtimes_file = File::create(mtimes_file).await.unwrap();
    for (file, mtime) in resource_actual_mtimes {
      mtimes_file
        .write_all(format!("{}: {}\n", file, mtime).as_bytes())
        .await
        .unwrap();
    }
    mtimes_file.flush().await.unwrap();
  }

  let mut processed_resources = 0;
  for definition in &mut resources {
    let info = definition.resource().get_info().as_ref().unwrap();
    if unchanged_resources.contains(&info.id) {
      continue;
    }

    if let ResourceDefinition::Map(resource) = definition {
      resource.init_proplibs(&proplibs).await?;
    }

    let info = definition.resource().get_info().as_ref().unwrap();
    let path = out.join(info.encode());
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

  fs::write(
    "out/00-resources.json",
    serde_json::to_vec_pretty(&resources)?
  )
  .await?;

  let end = Instant::now();
  info!("completed in {:?}", end - start);
  info!(
    "processed {} resources ({} cached, {} not changed): generated {} files from {} files",
    processed_resources,
    resources.len() - processed_resources,
    mtime_skip_files,
    output_files,
    input_files
  );

  Ok(())
}
